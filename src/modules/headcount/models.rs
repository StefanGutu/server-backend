use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// The newest photo of a bus, when the model has not counted it yet.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Uncounted {
    pub bus_id: i32,
    pub file_path: String,
}

/// What the model saw in the newest photo of a bus that was counted.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct LatestCount {
    pub people_count: i32,
    pub counted_at: DateTime<Utc>,
    pub file_path: String,
}

/// What `GET /buses/{bus_id}/headcount` answers: the people the camera counted in the bus and the users of the app heard on it.
/// (What the passengers paid is not part of it: `GET /buses/{bus_id}/occupancy` has the paid passengers.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BusHeadcount {
    pub bus_id: i32,
    /// When the newest counted photo was counted (even when it is too old to be shown as `people_counted`, so the
    /// screen can say how old it is), and which photo it was (the id of `GET /buses/{bus_id}/frames`).
    #[serde(serialize_with = "crate::extract::utc_seconds_opt")]
    pub counted_at: Option<DateTime<Utc>>,
    pub frame_id: Option<Uuid>,
    /// What the model saw in the newest counted photo. `None` when there is none, or it is too old to be true now.
    pub people_counted: Option<i32>,
    /// Users of the app heard on the bus now.
    pub devices_heard: i64,
}
