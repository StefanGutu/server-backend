use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::models::{BusInfo, DeviceInfo, FrameRow, Occupancy, TelemetryInfo};

/// The columns of a `TelemetryInfo` (the route comes from the bus).
macro_rules! telemetry_columns {
    () => {
        "t.bus_id, b.route_id, t.device_id, t.boot_id, t.seq, t.ts, t.uptime_ms, t.sim, t.fix_ok, t.lat, t.lon, t.speed_kmh, t.received_at"
    };
}

pub async fn bus_exists(db: &PgPool, bus_id: i32) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM buses WHERE bus_id = $1)")
        .bind(bus_id)
        .fetch_one(db)
        .await
}

pub async fn list_buses(db: &PgPool) -> Result<Vec<BusInfo>, sqlx::Error> {
    sqlx::query_as("SELECT bus_id, route_id FROM buses ORDER BY bus_id")
        .fetch_all(db)
        .await
}

pub async fn devices_of_bus(db: &PgPool, bus_id: i32) -> Result<Vec<DeviceInfo>, sqlx::Error> {
    sqlx::query_as(
        "SELECT device_id, bus_id, role FROM devices WHERE bus_id = $1 ORDER BY role, device_id",
    )
    .bind(bus_id)
    .fetch_all(db)
    .await
}

pub async fn find_device(db: &PgPool, device_id: &str) -> Result<Option<DeviceInfo>, sqlx::Error> {
    sqlx::query_as("SELECT device_id, bus_id, role FROM devices WHERE device_id = $1")
        .bind(device_id)
        .fetch_optional(db)
        .await
}

/// The messages of one bus, newest first, between `from` and `to` (the ones given).
pub async fn telemetry_history(
    db: &PgPool,
    bus_id: i32,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<TelemetryInfo>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        telemetry_columns!(),
        " FROM telemetry t JOIN buses b USING (bus_id)
          WHERE t.bus_id = $1 AND ($2::timestamptz IS NULL OR t.received_at >= $2)
            AND ($3::timestamptz IS NULL OR t.received_at <= $3)
          ORDER BY t.received_at DESC, t.id DESC LIMIT $4"
    ))
    .bind(bus_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// The newest message of a bus that has a GPS position.
pub async fn latest_position(
    db: &PgPool,
    bus_id: i32,
) -> Result<Option<TelemetryInfo>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        telemetry_columns!(),
        " FROM telemetry t JOIN buses b USING (bus_id)
          WHERE t.bus_id = $1 AND t.fix_ok AND t.lat IS NOT NULL AND t.lon IS NOT NULL
          ORDER BY t.received_at DESC, t.id DESC LIMIT 1"
    ))
    .bind(bus_id)
    .fetch_optional(db)
    .await
}

/// The newest position of every bus (one each), only the buses of the route when one is given.
pub async fn latest_positions(
    db: &PgPool,
    route_id: Option<i32>,
) -> Result<Vec<TelemetryInfo>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT DISTINCT ON (t.bus_id) ",
        telemetry_columns!(),
        " FROM telemetry t JOIN buses b USING (bus_id)
          WHERE t.fix_ok AND t.lat IS NOT NULL AND t.lon IS NOT NULL AND ($1::int IS NULL OR b.route_id = $1)
          ORDER BY t.bus_id, t.received_at DESC, t.id DESC"
    ))
    .bind(route_id)
    .fetch_all(db)
    .await
}

/// The place of a photo in the list (`received_at`), or None if there is no such photo of this bus.
pub async fn frame_received_at(
    db: &PgPool,
    bus_id: i32,
    file_path: &str,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    sqlx::query_scalar("SELECT received_at FROM frames WHERE bus_id = $1 AND file_path = $2")
        .bind(bus_id)
        .bind(file_path)
        .fetch_optional(db)
        .await
}

/// The photos of one bus, newest first. With `after`: only those older than that photo.
pub async fn list_frames(
    db: &PgPool,
    bus_id: i32,
    older_than: Option<(DateTime<Utc>, String)>,
    limit: i64,
) -> Result<Vec<FrameRow>, sqlx::Error> {
    let (received_at, file_path) = match older_than {
        Some((at, path)) => (Some(at), Some(path)),
        None => (None, None),
    };
    sqlx::query_as(
        "SELECT file_path, bus_id, route_id, frame_seq, capture_ts, width, height, received_at
         FROM frames
         WHERE bus_id = $1 AND ($2::timestamptz IS NULL OR (received_at, file_path) < ($2, $3))
         ORDER BY received_at DESC, file_path DESC LIMIT $4",
    )
    .bind(bus_id)
    .bind(received_at)
    .bind(file_path)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// The file of the newest photo of a bus.
pub async fn latest_frame_path(db: &PgPool, bus_id: i32) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT file_path FROM frames WHERE bus_id = $1 ORDER BY received_at DESC, file_path DESC LIMIT 1")
        .bind(bus_id)
        .fetch_optional(db)
        .await
}

/// The passengers of the paid rides that are going on in a bus.
pub async fn occupancy(db: &PgPool, bus_id: i32) -> Result<Occupancy, sqlx::Error> {
    sqlx::query_as(
        "SELECT $1::int AS bus_id, COALESCE(SUM(passengers), 0)::bigint AS passengers
         FROM ride_sessions WHERE bus_id = $1 AND status = 'confirmed'",
    )
    .bind(bus_id)
    .fetch_one(db)
    .await
}
