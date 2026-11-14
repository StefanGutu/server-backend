//! Rules of the devices module. Other modules and the extractor call these, never `repo`.

use sqlx::PgPool;

use super::{
    models::{BusLine, Device, DeviceRole, Route},
    repo, signature,
};
use crate::error::AppError;

/// The route with this id, or NOT_FOUND.
pub async fn get_route(db: &PgPool, route_id: i32) -> Result<Route, AppError> {
    repo::find_route(db, route_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// Line name and destination of a bus. None when the bus does not exist.
pub async fn bus_line(db: &PgPool, bus_id: i32) -> Result<Option<BusLine>, AppError> {
    Ok(repo::find_route_of_bus(db, bus_id)
        .await?
        .map(BusLine::from))
}

/// A device may only call for its own bus, and only the calls of its role: Node A (the beacon) sends
/// telemetry, Node B (the camera) sends photos. Anything else is 403 `FORBIDDEN`. Pure.
pub fn require_device(device: &Device, role: DeviceRole, url_bus_id: i32) -> Result<(), AppError> {
    if device.role == role && device.bus_id == url_bus_id {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// The device says which route it is configured for. If that is not the planned route of the bus,
/// someone reconfigured the bus: a warning is written in the log (the call still goes on).
pub async fn warn_if_route_differs(
    db: &PgPool,
    bus_id: i32,
    reported_route_id: i32,
) -> Result<(), AppError> {
    let planned = planned_route_id(db, bus_id).await?;
    if planned != Some(reported_route_id) {
        tracing::warn!(
            bus_id,
            planned_route_id = ?planned,
            reported_route_id,
            "bus reports a route different from the planned one"
        );
    }
    Ok(())
}

/// The route a bus runs, with all its points (stops and bends). None when the bus does not exist.
pub async fn route_of_bus(db: &PgPool, bus_id: i32) -> Result<Option<Route>, AppError> {
    Ok(repo::find_route_of_bus(db, bus_id).await?)
}

/// The route a bus is planned to run (`buses.route_id`).
pub async fn planned_route_id(db: &PgPool, bus_id: i32) -> Result<Option<i32>, AppError> {
    Ok(repo::find_bus_route_id(db, bus_id).await?)
}

/// The beacon's secret is the key of the sighting tokens its bus broadcasts.
/// None when the bus does not exist or has no beacon.
pub async fn beacon_secret(db: &PgPool, bus_id: i32) -> Result<Option<String>, AppError> {
    Ok(repo::find_beacon_secret(db, bus_id).await?)
}

/// Checks the three signature headers against the secret stored for the device.
/// Returns the device, or AUTH_INVALID for an unknown device, a wrong signature
/// or a timestamp too far from our clock.
pub async fn authenticate(
    db: &PgPool,
    device_id: &str,
    timestamp: &str,
    signature_hex: &str,
    body: &[u8],
) -> Result<Device, AppError> {
    // Every refusal says why in the log (never the secret): a device that gets a 401 can be told what to fix.
    let Some(device) = repo::find_device(db, device_id).await? else {
        tracing::warn!(
            device_id,
            "device refused: this device id is not in the table `devices`"
        );
        return Err(AppError::AuthInvalid);
    };

    if !signature::verify_signature(&device.secret, timestamp, device_id, body, signature_hex) {
        tracing::warn!(
            device_id,
            timestamp,
            body_bytes = body.len(),
            signature_chars = signature_hex.len(),
            "device refused: the signature does not match (wrong secret, or the device signed other bytes than the body it sent)"
        );
        return Err(AppError::AuthInvalid);
    }

    let Ok(timestamp_secs) = timestamp.parse::<i64>() else {
        tracing::warn!(
            device_id,
            timestamp,
            "device refused: X-Timestamp is not a number"
        );
        return Err(AppError::AuthInvalid);
    };
    let now = chrono::Utc::now().timestamp();
    if !signature::timestamp_ok(timestamp_secs, now) {
        tracing::warn!(
            device_id,
            timestamp_secs,
            server_time = now,
            "device refused: X-Timestamp is more than 5 minutes from the server clock"
        );
        return Err(AppError::AuthInvalid);
    }
    Ok(device)
}
