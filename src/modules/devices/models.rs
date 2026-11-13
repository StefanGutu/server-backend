use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::Json};

/// One point of a route. A point with a `name` is a stop, without one it is a bend.
#[derive(Serialize, Deserialize)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Same shape as the JSON of GET /routes/{route_id} (server.md).
#[derive(Serialize, FromRow)]
pub struct Route {
    pub route_id: i32,
    pub name: String,
    pub points: Json<Vec<Point>>,
}

/// What a device does on the bus. Stored as text in `devices.role`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum DeviceRole {
    /// Node A: BLE broadcast and GPS telemetry.
    Beacon,
    /// Node B: cabin photos.
    Camera,
}

/// A bus device (Node A beacon or Node B camera). Never sent to clients: it holds the secret.
#[derive(FromRow)]
pub struct Device {
    pub device_id: String,
    pub bus_id: i32,
    pub role: DeviceRole,
    pub secret: String,
}
