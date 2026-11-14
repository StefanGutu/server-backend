use chrono::{NaiveDate, NaiveTime};
use serde::{Serialize, Serializer};
use sqlx::FromRow;
use uuid::Uuid;

use crate::extract::ron_from_bani;

/// A time of day as `HH:MM` (server-api.md: `time?` of `RecentTrip`).
fn hours_and_minutes<S: Serializer>(time: &NaiveTime, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&time.format("%H:%M").to_string())
}

/// A trip as the app sees it (`RecentTrip` in server-api.md). The row is also the JSON.
#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RecentTrip {
    pub id: Uuid,
    #[serde(rename = "from")]
    pub from_stop: String,
    #[serde(rename = "to")]
    pub to_stop: String,
    /// The day the user got on, `YYYY-MM-DD` (UTC).
    pub date: NaiveDate,
    /// The time the user got on, `HH:MM` (UTC).
    #[serde(serialize_with = "hours_and_minutes")]
    pub time: NaiveTime,
    pub line: String,
    /// Kept in bani, sent as lei in `fareRon`.
    #[serde(rename = "fareRon", serialize_with = "ron_from_bani")]
    pub fare_bani: i32,
    pub distance_km: f64,
    pub duration_min: i32,
}

/// A route the user keeps (`SavedRoute` in server-api.md).
#[derive(Debug, Serialize, FromRow)]
pub struct SavedRoute {
    pub id: Uuid,
    #[serde(rename = "from")]
    pub from_stop: String,
    #[serde(rename = "to")]
    pub to_stop: String,
    pub line: String,
}

/// A trip that is about to be stored.
#[derive(Debug, Clone, PartialEq)]
pub struct NewTrip {
    pub user_id: Uuid,
    pub charge_id: Uuid,
    pub from_stop: String,
    pub to_stop: String,
    pub date: NaiveDate,
    pub time: NaiveTime,
    pub line: String,
    pub fare_bani: i32,
    pub distance_km: f64,
    pub duration_min: i32,
}
