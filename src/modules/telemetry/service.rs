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
    boot_id.len() == 8 && boot_id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Checks the message against the signed device and the bus in the URL.
pub fn check(device: &Device, url_bus_id: i32, t: &Telemetry) -> Result<(), AppError> {
    // Only the beacon sends telemetry, and only for its own bus.
    if device.role != DeviceRole::Beacon || device.bus_id != url_bus_id {
        return Err(AppError::Forbidden);
    }
    // The body must agree with the URL and with the device that signed it.
    if t.bus_id != url_bus_id || t.device_id != device.device_id {
        return Err(AppError::Validation(
            "bus_id or device_id does not match the request.".into(),
        ));
    }
    if !boot_id_ok(&t.boot_id) {
        return Err(AppError::Validation("boot_id must be 8 lowercase hex characters.".into()));
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

/// The device reports its own route_id. If it is not the planned one, someone
/// reconfigured the bus: write a warning in the log (the message is still stored).
async fn warn_if_route_differs(db: &PgPool, t: &Telemetry) -> Result<(), AppError> {
    let planned = devices::service::planned_route_id(db, t.bus_id).await?;
    if planned != Some(t.route_id) {
        tracing::warn!(
            bus_id = t.bus_id,
            planned_route_id = ?planned,
            reported_route_id = t.route_id,
            "bus reports a route different from the planned one"
        );
    }
    Ok(())
}

/// Whole flow: parse, check, store. Duplicates are fine (stored once, still Ok).
pub async fn handle(db: &PgPool, device: &Device, url_bus_id: i32, body: &[u8]) -> Result<(), AppError> {
    let telemetry = parse(body)?;
    check(device, url_bus_id, &telemetry)?;
    warn_if_route_differs(db, &telemetry).await?;
    repo::insert_if_new(db, &telemetry).await?;
    Ok(())
}
