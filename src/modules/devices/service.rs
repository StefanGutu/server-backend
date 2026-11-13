//! Rules of the devices module. Other modules and the extractor call these, never `repo`.

use sqlx::PgPool;

use super::{models::{Device, Route}, repo, signature};
use crate::error::AppError;

/// The route with this id, or NOT_FOUND.
pub async fn get_route(db: &PgPool, route_id: i32) -> Result<Route, AppError> {
    repo::find_route(db, route_id).await?.ok_or(AppError::NotFound)
}

/// The route a bus is planned to run (`buses.route_id`).
pub async fn planned_route_id(db: &PgPool, bus_id: i32) -> Result<Option<i32>, AppError> {
    Ok(repo::find_bus_route_id(db, bus_id).await?)
}

fn unix_now() -> Result<i64, AppError> {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AppError::Server)?;
    Ok(since_epoch.as_secs() as i64)
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
    let device = repo::find_device(db, device_id)
        .await?
        .ok_or(AppError::AuthInvalid)?;

    if !signature::verify_signature(&device.secret, timestamp, device_id, body, signature_hex) {
        return Err(AppError::AuthInvalid);
    }

    let timestamp: i64 = timestamp.parse().map_err(|_| AppError::AuthInvalid)?;
    if !signature::timestamp_ok(timestamp, unix_now()?) {
        return Err(AppError::AuthInvalid);
    }
    Ok(device)
}
