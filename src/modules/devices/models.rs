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

impl Route {
    /// The last stop of the line. A circular line repeats its first stop at the end to close
    /// the loop; that repeat is skipped, so the destination is the stop before it.
    pub fn destination(&self) -> Option<&str> {
        let stops: Vec<&Point> = self.points.iter().filter(|p| p.name.is_some()).collect();
        let (first, last) = (stops.first()?, stops.last()?);
        let closes_loop = stops.len() > 1 && last.lat == first.lat && last.lon == first.lon;
        let destination = if closes_loop { stops[stops.len() - 2] } else { last };
        destination.name.as_deref()
    }
}

/// What the app shows about a bus: its line and where it goes.
pub struct BusLine {
    pub line: String,
    pub destination: Option<String>,
}

impl From<Route> for BusLine {
    fn from(route: Route) -> Self {
        let destination = route.destination().map(str::to_string);
        Self { line: route.name, destination }
    }
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
