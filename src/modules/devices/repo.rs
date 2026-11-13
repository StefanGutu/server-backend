use sqlx::PgPool;

use super::models::{Device, Route};

/// Returns None when the route does not exist.
pub async fn find_route(db: &PgPool, route_id: i32) -> Result<Option<Route>, sqlx::Error> {
    sqlx::query_as("SELECT route_id, name, points FROM routes WHERE route_id = $1")
        .bind(route_id)
        .fetch_optional(db)
        .await
}

/// The route a bus is planned to run. None when the bus does not exist.
pub async fn find_bus_route_id(db: &PgPool, bus_id: i32) -> Result<Option<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT route_id FROM buses WHERE bus_id = $1")
        .bind(bus_id)
        .fetch_optional(db)
        .await
}

/// Returns None when no device has this id.
pub async fn find_device(db: &PgPool, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
    sqlx::query_as("SELECT device_id, bus_id, role, secret FROM devices WHERE device_id = $1")
        .bind(device_id)
        .fetch_optional(db)
        .await
}
