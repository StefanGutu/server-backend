//! The rules for a telemetry message. `parse` and `check` are pure (no database).

use sqlx::PgPool;

use super::{models::Telemetry, repo};
use crate::{
    error::AppError,
    modules::devices::{
        self,
        models::{Device, DeviceRole},
    },
};

/// Raw body -> Telemetry. Malformed JSON or a missing field is a 400.
pub fn parse(body: &[u8]) -> Result<Telemetry, AppError> {
    serde_json::from_slice(body)
        .map_err(|e| AppError::Validation(format!("Invalid telemetry body: {e}")))
}

/// `boot_id` is 8 lowercase hex characters (server.md).
fn boot_id_ok(boot_id: &str) -> bool {
    boot_id.len() == 8
        && boot_id
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Checks the message against the signed device and the bus in the URL.
pub fn check(device: &Device, url_bus_id: i32, t: &Telemetry) -> Result<(), AppError> {
    // Only the beacon sends telemetry, and only for its own bus.
    devices::service::require_device(device, DeviceRole::Beacon, url_bus_id)?;
    // The body must agree with the URL and with the device that signed it.
    if t.bus_id != url_bus_id || t.device_id != device.device_id {
        return Err(AppError::Validation(
            "bus_id or device_id does not match the request.".into(),
        ));
    }
    if !boot_id_ok(&t.boot_id) {
        return Err(AppError::Validation(
            "boot_id must be 8 lowercase hex characters.".into(),
        ));
    }
    // seq starts at 0 (server.md)
    if t.seq < 0 {
        return Err(AppError::Validation("seq must not be negative.".into()));
    }
    if t.sim != 0 && t.sim != 1 {
        return Err(AppError::Validation("sim must be 0 or 1.".into()));
    }
    Ok(())
}

const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Distance in metres between two (lat, lon) points in degrees (haversine formula).
pub fn distance_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat_a, lat_b) = (a.0.to_radians(), b.0.to_radians());
    let d_lat = lat_b - lat_a;
    let d_lon = (b.1 - a.1).to_radians();
    let h = (d_lat / 2.0).sin().powi(2) + lat_a.cos() * lat_b.cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// How far the bus got from its first position. The furthest point is used (not the last
/// one), so a bus that went out and came back still counts as moving, and GPS noise
/// while it stands still does not add up. 0 for fewer than two positions.
pub fn max_distance_from_first_m(positions: &[(f64, f64)]) -> f64 {
    let Some(&first) = positions.first() else {
        return 0.0;
    };
    positions
        .iter()
        .map(|&p| distance_m(first, p))
        .fold(0.0, f64::max)
}

/// Positions kept for each bus: the newest 5000 (about 2 hours 45 minutes at one message every 2 seconds).
/// The positions are read by the boarding decision (the last minutes), by the end of a ride (since the user was
/// last heard, up to an hour), by the trip of a ride (the whole ride) and by reports (the last 5 minutes), so
/// this must be more than the longest ride: with 1000 (33 minutes) a long ride would lose where it started.
pub const MAX_POSITIONS_PER_BUS: i64 = 5000;

/// Deletes the oldest positions beyond `MAX_POSITIONS_PER_BUS` for each bus (called by the background task).
pub async fn delete_old_positions(db: &PgPool) -> Result<u64, AppError> {
    Ok(repo::delete_beyond_newest(db, MAX_POSITIONS_PER_BUS).await?)
}

/// The GPS positions (lat, lon) of a bus received between two unix times, oldest first. Only messages with a
/// GPS fix; `allow_sim`: also simulator positions (test mode only).
pub async fn bus_positions(
    db: &PgPool,
    bus_id: i32,
    from_epoch: i64,
    to_epoch: i64,
    allow_sim: bool,
) -> Result<Vec<(f64, f64)>, AppError> {
    Ok(repo::fix_positions(db, bus_id, from_epoch, to_epoch, allow_sim).await?)
}

/// How many metres a bus moved between two unix times (by the time the server received
/// the positions). `allow_sim`: also count simulator positions (test mode only, because
/// nobody is charged on a simulated position in production).
pub async fn bus_movement_m(
    db: &PgPool,
    bus_id: i32,
    from_epoch: i64,
    to_epoch: i64,
    allow_sim: bool,
) -> Result<f64, AppError> {
    let positions = bus_positions(db, bus_id, from_epoch, to_epoch, allow_sim).await?;
    Ok(max_distance_from_first_m(&positions))
}

/// Whole flow: parse, check, store. Duplicates are fine (stored once, still Ok).
pub async fn handle(
    db: &PgPool,
    device: &Device,
    url_bus_id: i32,
    body: &[u8],
) -> Result<(), AppError> {
    let telemetry = parse(body)?;
    check(device, url_bus_id, &telemetry)?;
    devices::service::warn_if_route_differs(db, telemetry.bus_id, telemetry.route_id).await?;
    repo::insert_if_new(db, &telemetry).await?;
    Ok(())
}
