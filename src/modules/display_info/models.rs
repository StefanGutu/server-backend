use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::extract::utc_seconds;

/// A device of a bus. **The secret is never part of it.**
#[derive(Debug, Serialize, FromRow)]
pub struct DeviceInfo {
    pub device_id: String,
    pub bus_id: i32,
    /// `beacon` (Node A) or `camera` (Node B).
    pub role: String,
}

/// A bus and the route it is planned to run.
#[derive(Debug, Serialize, FromRow)]
pub struct BusInfo {
    pub bus_id: i32,
    pub route_id: i32,
}

/// One telemetry message, as the beacon sent it. `route_id` is the planned route of the bus (the message does not keep
/// the route the device reported). `received_at` is when the server got it.
#[derive(Debug, Serialize, FromRow)]
pub struct TelemetryInfo {
    pub bus_id: i32,
    pub route_id: i32,
    pub device_id: String,
    pub boot_id: String,
    pub seq: i64,
    pub ts: i64,
    pub uptime_ms: i64,
    pub sim: i16,
    pub fix_ok: bool,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub speed_kmh: Option<f64>,
    #[serde(serialize_with = "utc_seconds")]
    pub received_at: DateTime<Utc>,
}

/// The query of `GET /buses/{bus_id}/telemetry`: `from` and `to` are times (`2026-11-14T14:00:00Z`) of `received_at`.
#[derive(Debug, Deserialize)]
pub struct TelemetryQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

/// The query of `GET /buses/positions`.
#[derive(Debug, Deserialize)]
pub struct PositionsQuery {
    pub route_id: Option<i32>,
}

/// The query of `GET /buses/{bus_id}/frames`: `before` is the `id` of the last photo the caller has.
#[derive(Debug, Deserialize)]
pub struct FramesQuery {
    pub limit: Option<i64>,
    pub before: Option<Uuid>,
}

/// A stored photo row, as the database has it.
#[derive(Debug, FromRow)]
pub struct FrameRow {
    pub file_path: String,
    pub bus_id: i32,
    pub route_id: Option<i32>,
    pub frame_seq: i64,
    pub capture_ts: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub received_at: DateTime<Utc>,
}

/// A photo as the display sees it (the file itself is `GET /buses/{bus_id}/frames/latest` for the newest one).
/// `route_id` and `frame_size` are `null` for a photo stored before the server kept them.
#[derive(Debug, Serialize, PartialEq)]
pub struct FrameInfo {
    pub id: Uuid,
    pub bus_id: i32,
    pub route_id: Option<i32>,
    pub frame_seq: i64,
    pub capture_ts: i64,
    pub frame_size: Option<String>,
    #[serde(serialize_with = "utc_seconds")]
    pub received_at: DateTime<Utc>,
}

/// How many people a bus carries now.
#[derive(Debug, Serialize, FromRow)]
pub struct Occupancy {
    pub bus_id: i32,
    pub passengers: i64,
}

/// How long the camera of a bus waits between photos.
#[derive(Debug, Serialize)]
pub struct CaptureInterval {
    pub bus_id: i32,
    pub next_capture_ms: u32,
}
