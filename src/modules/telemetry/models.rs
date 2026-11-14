use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::FromRow;

/// The JSON Node A sends (server.md). Fields we do not store (heading, sats, ...)
/// are ignored by serde.
#[derive(Debug, Deserialize)]
pub struct Telemetry {
    pub bus_id: i32,
    pub route_id: i32,
    pub device_id: String,
    pub boot_id: String,
    pub seq: i64,
    pub ts: i64,
    pub uptime_ms: i64,
    pub sim: i16,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub speed_kmh: Option<f64>,
    pub fix_ok: bool,
}

/// A GPS position of a bus with when the server received it, how fast the bus went and whether it is simulated.
/// (`at` is the time of the server: the clock of the device is often 0.)
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct Fix {
    pub lat: f64,
    pub lon: f64,
    pub speed_kmh: Option<f64>,
    pub simulated: bool,
    pub at: DateTime<Utc>,
}
