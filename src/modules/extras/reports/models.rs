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

/// Body of POST /reports (server-api.md): `{ "kind", "line" }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportInput {
    pub kind: ReportKind,
    pub line: String,
}
