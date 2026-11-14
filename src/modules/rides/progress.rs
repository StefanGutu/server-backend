//! The progress of a ride along its route (doc/PLAN.md, part 4): where the bus is, the stop it has just left, the next stop,
//! the distance to it and the time. Everything here is pure: no database, no clock (the caller gives `now`).
//!
//! The routes are **loops** and **every one of them drives some streets both ways**, so the nearest segment of the line is
//! not enough to tell where a bus is: at the same spot the bus can be on the way out or on the way back. The direction is
//! taken from where the bus moved in its last positions (`movement`), and `project` picks the segment that agrees with it.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{
    extract::utc_seconds,
    modules::{devices::models::Point, telemetry::models::Fix},
};

/// A position farther than this from the line is not on the route: no projection (metres).
pub const MAX_OFF_ROUTE_M: f64 = 60.0;
/// The segments within this much of the nearest one are candidates, and the direction chooses among them (metres).
const CANDIDATE_SLACK_M: f64 = 15.0;
/// The bus must have moved this far for its direction to be known (metres).
pub const MIN_MOVEMENT_M: f64 = 20.0;
/// A bus slower than this is taken as going this fast for the time to the stop, so a bus at a traffic light does not
/// get a huge time (km/h).
pub const MIN_ETA_SPEED_KMH: f64 = 15.0;
/// A position older than this is not shown (seconds).
pub const POSITION_FRESH_SECS: i64 = 30;

/// A stop of the route and how far along the route it is.
#[derive(Debug, Clone, PartialEq)]
pub struct StopOnRoute {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// Metres from the start of the route.
    pub along_m: f64,
}

/// A route as a line in metres, to measure along it. Built once for a ride from `Route.points`.
#[derive(Debug, Clone)]
pub struct RouteLine {
    origin: (f64, f64),
    kx: f64,
    xy: Vec<(f64, f64)>,
    /// `cum[i]`: metres along the route up to point `i`.
    cum: Vec<f64>,
    /// The named points in route order. A closed route repeats its first stop at the end: that repeat is not in the list.
    pub stops: Vec<StopOnRoute>,
    pub total_m: f64,
}

/// Metres in a degree of latitude (a flat map of a city is exact enough: the routes are a few kilometres across).
const M_PER_DEG_LAT: f64 = 110_540.0;
const M_PER_DEG_LON_AT_EQUATOR: f64 = 111_320.0;

impl RouteLine {
    /// `None` for fewer than 2 points (there is no line).
    pub fn new(points: &[Point]) -> Option<Self> {
        let first = points.first()?;
        if points.len() < 2 {
            return None;
        }
        let origin = (first.lat, first.lon);
        let kx = M_PER_DEG_LON_AT_EQUATOR * origin.0.to_radians().cos();
        let xy: Vec<(f64, f64)> = points
            .iter()
            .map(|p| ((p.lon - origin.1) * kx, (p.lat - origin.0) * M_PER_DEG_LAT))
            .collect();
        let mut cum = vec![0.0];
        for pair in xy.windows(2) {
            cum.push(cum.last().copied().unwrap_or(0.0) + dist(pair[0], pair[1]));
        }
        let total_m = cum.last().copied().unwrap_or(0.0);
        let mut stops: Vec<StopOnRoute> = points
            .iter()
            .zip(&cum)
            .filter_map(|(p, along)| {
                p.name.as_ref().map(|name| StopOnRoute {
                    name: name.clone(),
                    lat: p.lat,
                    lon: p.lon,
                    along_m: *along,
                })
            })
            .collect();
        // a closed route repeats its first stop as its last: that is the same stop, not another one
        if let (Some(first), Some(last)) = (stops.first(), stops.last())
            && stops.len() > 1
            && (first.lat, first.lon) == (last.lat, last.lon)
        {
            stops.pop();
        }
        Some(Self {
            origin,
            kx,
            xy,
            cum,
            stops,
            total_m,
        })
    }

    /// A latitude and longitude as metres on the line's flat map.
    pub fn local(&self, lat: f64, lon: f64) -> (f64, f64) {
        (
            (lon - self.origin.1) * self.kx,
            (lat - self.origin.0) * M_PER_DEG_LAT,
        )
    }
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

/// Where a position is on the route.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    /// Metres from the start of the route to the point of the line nearest to the position (in the direction chosen).
    pub along_m: f64,
    /// How far the position is from the line.
    pub off_route_m: f64,
}

/// The direction the bus is going, as a unit vector on the line's map: from the newest position back to the first older one
/// that is at least `MIN_MOVEMENT_M` away (the shortest piece of history that shows movement, so a turn does not
/// spoil it). `None` when the bus has not moved that far (a bus at a stop). `newest_first` is (lat, lon), newest first. Pure.
pub fn movement(line: &RouteLine, newest_first: &[(f64, f64)]) -> Option<(f64, f64)> {
    let (newest, older) = newest_first.split_first()?;
    let newest = line.local(newest.0, newest.1);
    older
        .iter()
        .map(|p| line.local(p.0, p.1))
        .find_map(|before| {
            let length = dist(before, newest);
            (length >= MIN_MOVEMENT_M).then(|| {
                (
                    (newest.0 - before.0) / length,
                    (newest.1 - before.1) / length,
                )
            })
        })
}

/// Where a position (lat, lon) is on the route, or `None` when it is more than `MAX_OFF_ROUTE_M` from the line.
///
/// Of the segments within `CANDIDATE_SLACK_M` of the nearest one, the one whose direction agrees with `movement` is chosen
/// (the nearest of those). Without a movement, or when no candidate agrees with it, it is the nearest segment. Pure.
pub fn project(
    line: &RouteLine,
    position: (f64, f64),
    movement: Option<(f64, f64)>,
) -> Option<Projection> {
    let point = line.local(position.0, position.1);
    // (distance to the line, along the route, direction of the segment) of every segment
    let segments: Vec<(f64, f64, (f64, f64))> = line
        .xy
        .windows(2)
        .zip(&line.cum)
        .filter_map(|(pair, start)| {
            let (a, b) = (pair[0], pair[1]);
            let length = dist(a, b);
            if length == 0.0 {
                return None;
            }
            let direction = ((b.0 - a.0) / length, (b.1 - a.1) / length);
            let t = (((point.0 - a.0) * direction.0 + (point.1 - a.1) * direction.1) / length)
                .clamp(0.0, 1.0);
            let nearest = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
            Some((dist(point, nearest), start + t * length, direction))
        })
        .collect();
    let best = segments.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    if !best.is_finite() || best > MAX_OFF_ROUTE_M {
        return None;
    }
    let candidates = segments.iter().filter(|s| s.0 <= best + CANDIDATE_SLACK_M);
    let nearest_of = |items: Vec<&(f64, f64, (f64, f64))>| {
        items
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|s| Projection {
                along_m: s.1,
                off_route_m: s.0,
            })
    };
    let all: Vec<_> = candidates.collect();
    let agreeing: Vec<_> = match movement {
        Some(m) => all
            .iter()
            .copied()
            .filter(|s| s.2.0 * m.0 + s.2.1 * m.1 > 0.0)
            .collect(),
        None => Vec::new(),
    };
    nearest_of(agreeing).or_else(|| nearest_of(all))
}

/// The next stop in front of the bus.
#[derive(Debug, Clone, PartialEq)]
pub struct NextStop {
    /// Its place in `RouteLine::stops`.
    pub index: usize,
    pub name: String,
    /// Metres along the route from the bus to the stop.
    pub distance_m: f64,
}

/// The first stop at or after `along_m`. After the last stop the next one is the first (the route is a loop): its distance
/// is the rest of the loop plus the way from the start to that stop. `None` when the route has no stops. Pure.
pub fn next_stop(line: &RouteLine, along_m: f64) -> Option<NextStop> {
    let found = line
        .stops
        .iter()
        .enumerate()
        .find(|(_, s)| s.along_m >= along_m);
    let (index, stop, distance_m) = match found {
        Some((index, stop)) => (index, stop, stop.along_m - along_m),
        None => {
            let first = line.stops.first()?;
            (0, first, (line.total_m - along_m) + first.along_m)
        }
    };
    Some(NextStop {
        index,
        name: stop.name.clone(),
        distance_m,
    })
}

/// For each stop: is it behind the bus (`along_m` is the place of the bus)? Pure.
pub fn passed_flags(line: &RouteLine, along_m: Option<f64>) -> Vec<bool> {
    line.stops
        .iter()
        .map(|s| along_m.is_some_and(|along| s.along_m < along))
        .collect()
}

/// Seconds to cover `distance_m` at the speed of the bus, but never at less than `MIN_ETA_SPEED_KMH`. A missing speed is
/// the minimum. Rounded. Pure.
pub fn eta_sec(distance_m: f64, speed_kmh: Option<f64>) -> i64 {
    let kmh = speed_kmh.unwrap_or(0.0).max(MIN_ETA_SPEED_KMH);
    (distance_m / (kmh / 3.6)).round() as i64
}

// ---- what the ride shows (the JSON of the contract) ----

/// The newest position of the bus.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPosition {
    pub lat: f64,
    pub lng: f64,
    pub speed_kmh: Option<f64>,
    /// When the server received it.
    #[serde(serialize_with = "utc_seconds")]
    pub at: DateTime<Utc>,
    pub simulated: bool,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNextStop {
    pub index: usize,
    pub name: String,
    pub distance_m: i64,
    pub eta_sec: i64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ProgressStop {
    pub name: String,
    pub lat: f64,
    pub lng: f64,
    pub passed: bool,
}

/// `progress` of a confirmed ride (server-api.md, section 2).
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub position: Option<ProgressPosition>,
    pub boarded_stop: Option<String>,
    pub next_stop: Option<ProgressNextStop>,
    pub stops: Vec<ProgressStop>,
}

/// The `progress` of a ride from what is known. Pure.
///
/// - `recent` are the positions of the bus, newest first; the newest is shown as `position` only if it is at most
///   `POSITION_FRESH_SECS` old at `now` (otherwise `position` and `next_stop` are `null` and no stop is `passed`).
/// - `line` is the route (`None` or no stops: `stops` is `[]` and there is no `next_stop`).
/// - `boarded_stop` is the stop nearest to where the bus was when the user got on (found by the caller).
pub fn build_progress(
    line: Option<&RouteLine>,
    recent: &[Fix],
    boarded_stop: Option<String>,
    now: DateTime<Utc>,
) -> Progress {
    let newest = recent
        .first()
        .filter(|f| (now - f.at).num_seconds() <= POSITION_FRESH_SECS);
    let position = newest.map(|f| ProgressPosition {
        lat: f.lat,
        lng: f.lon,
        speed_kmh: f.speed_kmh,
        at: f.at,
        simulated: f.simulated,
    });
    // where the bus is on the line: only for a fresh position that is on the route
    let along = match (line, newest) {
        (Some(line), Some(newest)) => {
            let trail: Vec<(f64, f64)> = recent.iter().map(|f| (f.lat, f.lon)).collect();
            project(line, (newest.lat, newest.lon), movement(line, &trail)).map(|p| p.along_m)
        }
        _ => None,
    };
    let next_stop = match (line, along) {
        (Some(line), Some(along)) => next_stop(line, along).map(|n| ProgressNextStop {
            index: n.index,
            name: n.name,
            distance_m: n.distance_m.round() as i64,
            eta_sec: eta_sec(n.distance_m, newest.and_then(|f| f.speed_kmh)),
        }),
        _ => None,
    };
    let stops = match line {
        Some(line) => {
            let passed = passed_flags(line, along);
            line.stops
                .iter()
                .zip(passed)
                .map(|(s, passed)| ProgressStop {
                    name: s.name.clone(),
                    lat: s.lat,
                    lng: s.lon,
                    passed,
                })
                .collect()
        }
        None => Vec::new(),
    };
    Progress {
        position,
        boarded_stop,
        next_stop,
        stops,
    }
}
