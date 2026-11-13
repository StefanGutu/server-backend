use serde::Deserialize;

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
