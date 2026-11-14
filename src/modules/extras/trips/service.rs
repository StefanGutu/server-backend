//! Trips. `nearest_stop`, `path_length_m` and `build_trip` are pure.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{NewTrip, RecentTrip, SavedRoute},
    repo,
};
use crate::{
    error::AppError,
    extract::ListQuery,
    modules::{devices::models::Point, telemetry::service::distance_m},
};

/// GPS hops shorter than this are noise (a bus that stands still jumps a few metres), not movement.
const MIN_HOP_M: f64 = 5.0;

/// What is known about a ride that ended, to make its trip.
pub struct TripFacts<'a> {
    pub user_id: Uuid,
    pub charge_id: Uuid,
    pub line: &'a str,
    pub fare_bani: i32,
    /// When the user got on and when they got off (unix seconds): the time they were heard near the bus.
    pub started_at: i64,
    pub ended_at: i64,
    /// The points of the route: the ones with a name are the stops.
    pub route: &'a [Point],
    /// Where the bus was during the ride, oldest first.
    pub positions: &'a [(f64, f64)],
}

/// The name of the stop nearest to a position. Only points with a name are stops; a point without one is
/// a bend in the road. None if the route has no stop. Pure.
pub fn nearest_stop(route: &[Point], position: (f64, f64)) -> Option<&str> {
    route
        .iter()
        .filter_map(|point| {
            point
                .name
                .as_deref()
                .map(|name| (name, distance_m(position, (point.lat, point.lon))))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(name, _)| name)
}

/// How far the bus drove: the sum of the distances between one position and the next. A hop under `MIN_HOP_M`
/// is not counted: it is GPS noise of a bus that stands still. In metres. Pure.
pub fn path_length_m(positions: &[(f64, f64)]) -> f64 {
    positions
        .windows(2)
        .map(|pair| distance_m(pair[0], pair[1]))
        .filter(|hop| *hop >= MIN_HOP_M)
        .sum()
}

/// Rounds to one decimal.
fn one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// The trip of a ride that ended. `from` is the stop nearest to the first position of the bus during the ride,
/// `to` the stop nearest to the last one. The distance is what the bus drove (kilometres, one decimal), the
/// duration is the time the user was on the bus (minutes, rounded, never negative), the date and time are when
/// they got on (UTC). `None` when the trip cannot be described: no position of the bus, or a route with no stop.
/// Pure.
pub fn build_trip(facts: &TripFacts) -> Option<NewTrip> {
    let (first, last) = (*facts.positions.first()?, *facts.positions.last()?);
    let from_stop = nearest_stop(facts.route, first)?.to_string();
    let to_stop = nearest_stop(facts.route, last)?.to_string();
    let got_on = DateTime::<Utc>::from_timestamp(facts.started_at, 0)?;
    let seconds = (facts.ended_at - facts.started_at).max(0);
    Some(NewTrip {
        user_id: facts.user_id,
        charge_id: facts.charge_id,
        from_stop,
        to_stop,
        date: got_on.date_naive(),
        time: got_on.time(),
        line: facts.line.to_string(),
        fare_bani: facts.fare_bani,
        distance_km: one_decimal(path_length_m(facts.positions) / 1000.0),
        duration_min: ((seconds as f64) / 60.0).round() as i32,
    })
}

/// Writes the trip of a ride that ended. Returns false when there was nothing to write (see `build_trip`)
/// or the charge already had its trip.
pub async fn record_trip(db: &PgPool, facts: &TripFacts<'_>) -> Result<bool, AppError> {
    let Some(trip) = build_trip(facts) else {
        tracing::warn!(user_id = %facts.user_id, charge_id = %facts.charge_id, "no trip written: the ride has no position of the bus or its route has no stop");
        return Ok(false);
    };
    let stored = repo::insert_trip(db, Uuid::now_v7(), &trip).await?;
    if stored {
        tracing::info!(user_id = %trip.user_id, from = %trip.from_stop, to = %trip.to_stop, km = trip.distance_km, minutes = trip.duration_min, "trip written");
    }
    Ok(stored)
}

/// GET /trips/recent: the user's trips, newest first, `limit` at a time. `before` is the id of the last trip the
/// app has; an id that is not one of the user's trips is a 404 (the same rule as `GET /charges`).
pub async fn list_trips(
    db: &PgPool,
    user_id: Uuid,
    query: &ListQuery,
) -> Result<Vec<RecentTrip>, AppError> {
    let limit = query.page_size()?;
    if let Some(trip_id) = query.before
        && !repo::trip_exists(db, user_id, trip_id).await?
    {
        return Err(AppError::NotFound);
    }
    Ok(repo::list_trips(db, user_id, query.before, limit).await?)
}

/// GET /trips/saved: the routes the user keeps. Nothing creates them yet, so the list is empty.
pub async fn list_saved_routes(db: &PgPool, user_id: Uuid) -> Result<Vec<SavedRoute>, AppError> {
    Ok(repo::list_saved_routes(db, user_id).await?)
}

/// The user's trips of one month, oldest first (`first_day` to `end_day`, the end not included).
pub async fn trips_between(
    db: &PgPool,
    user_id: Uuid,
    first_day: NaiveDate,
    end_day: NaiveDate,
) -> Result<Vec<RecentTrip>, AppError> {
    Ok(repo::list_trips_between(db, user_id, first_day, end_day).await?)
}
