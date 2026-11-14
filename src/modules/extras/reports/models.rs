use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::extract::utc_seconds;

/// What is wrong (server-api.md). Stored as text in `reports.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum ReportKind {
    Traffic,
    Accident,
    Breakdown,
    Works,
    Crowding,
    Detour,
}

impl ReportKind {
    /// The name as it is stored and sent (`traffic`, `accident`...).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Traffic => "traffic",
            Self::Accident => "accident",
            Self::Breakdown => "breakdown",
            Self::Works => "works",
            Self::Crowding => "crowding",
            Self::Detour => "detour",
        }
    }
}

/// A report as the app sees it (`Report` in server-api.md). The row is also the JSON.
#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub id: Uuid,
    pub kind: ReportKind,
    pub line: String,
    pub stop_name: String,
    pub confirmations: i64,
    #[serde(serialize_with = "utc_seconds")]
    pub expires_at: DateTime<Utc>,
    #[serde(serialize_with = "utc_seconds")]
    pub created_at: DateTime<Utc>,
}

/// The query of `GET /reports`: both parts are optional. `line` is matched in any letter case.
#[derive(Debug, Default, Deserialize)]
pub struct ReportFilter {
    pub line: Option<String>,
    pub kind: Option<ReportKind>,
    /// Only the display reads these two (the app's list is at most 100 reports): `limit` and `offset`.
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Body of POST /reports (server-api.md): `{ "kind", "line" }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportInput {
    pub kind: ReportKind,
    pub line: String,
}

/// Where a report is: the line (as the route calls it), the bus it was made on and the stop nearest to that bus.
pub struct StopFound {
    pub line: String,
    pub stop_name: String,
    pub bus_id: i32,
    pub lat: f64,
    pub lon: f64,
}
