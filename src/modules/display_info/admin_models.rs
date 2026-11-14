//! What the display calls show about users, sightings, rides, charges, trips, passes, reports and ticket checks.
//! Names are the ones of the display info table: most are `snake_case`, a few `camelCase` where the app's own models
//! already use them (`fareRon`, `stopName`, `startedAt`...).

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

use crate::{
    extract::{ron_from_bani, utc_seconds, utc_seconds_opt},
    modules::{
        charges::models::{Charge, ChargeStatus},
        devices::models::Point,
        rides::models::{Candidate, PaymentReason, RideStatus},
        wallet::models::PassLines,
    },
};

/// `limit` (default 100, at most 1000) and `offset` (default 0) of every list.
#[derive(Debug, Default)]
pub struct Paging {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UsersQuery {
    pub tier: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// The filters shared by the lists of one user's things (`user_id`) and of one bus (`bus_id`).
#[derive(Debug, Deserialize)]
pub struct OwnerQuery {
    pub user_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SightingsQuery {
    pub user_id: Option<Uuid>,
    pub bus_id: Option<i32>,
    pub accepted: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct RidesQuery {
    pub user_id: Option<Uuid>,
    pub bus_id: Option<i32>,
    pub status: Option<RideStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ChargesQuery {
    pub user_id: Option<Uuid>,
    pub status: Option<ChargeStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct VerificationsQuery {
    pub inspector_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// A query that carries `limit` and `offset` (they are plain fields: a `flatten` would not read numbers from a URL).
impl UsersQuery {
    pub fn paging(&self) -> Paging {
        Paging {
            limit: self.limit,
            offset: self.offset,
        }
    }
}

impl OwnerQuery {
    pub fn paging(&self) -> Paging {
        Paging {
            limit: self.limit,
            offset: self.offset,
        }
    }
}

impl SightingsQuery {
    pub fn paging(&self) -> Paging {
        Paging {
            limit: self.limit,
            offset: self.offset,
        }
    }
}

impl RidesQuery {
    pub fn paging(&self) -> Paging {
        Paging {
            limit: self.limit,
            offset: self.offset,
        }
    }
}

impl ChargesQuery {
    pub fn paging(&self) -> Paging {
        Paging {
            limit: self.limit,
            offset: self.offset,
        }
    }
}

impl VerificationsQuery {
    pub fn paging(&self) -> Paging {
        Paging {
            limit: self.limit,
            offset: self.offset,
        }
    }
}

// ---- users ----

/// A row of `GET /users`.
#[derive(Debug, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub tier: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub default_source_id: Option<Uuid>,
    pub settings: Json<Value>,
    pub cards_count: i64,
    pub passes_count: i64,
    pub open_ride_status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UserListItem {
    pub id: Uuid,
    pub tier: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub roles: Vec<String>,
    #[serde(serialize_with = "utc_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "utc_seconds_opt")]
    pub last_seen_at: Option<DateTime<Utc>>,
    pub cards_count: i64,
    pub passes_count: i64,
    pub open_ride_status: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Identity {
    pub provider: String,
    pub subject: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PushDeviceInfo {
    pub platform: String,
    pub push_token: String,
}

/// A refresh token row, to build the sessions.
#[derive(Debug, FromRow)]
pub struct TokenRow {
    pub id: Uuid,
    pub device_id: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
}

/// One phone's login: when it began, when it was last refreshed, and why it ended.
#[derive(Debug, Serialize, PartialEq)]
pub struct Session {
    pub device_id: String,
    #[serde(serialize_with = "utc_seconds_opt")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "utc_seconds_opt")]
    pub last_refresh_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "utc_seconds")]
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
    /// `logout` or `token_reuse` (`deleted`: the user is gone, so there is no session to show).
    pub revoked_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UserDetail {
    pub id: Uuid,
    pub tier: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub roles: Vec<String>,
    #[serde(serialize_with = "utc_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "utc_seconds_opt")]
    pub last_seen_at: Option<DateTime<Utc>>,
    pub identities: Vec<Identity>,
    pub settings: Value,
    pub default_source_id: Option<Uuid>,
    pub sessions: Vec<Session>,
    pub push_devices: Vec<PushDeviceInfo>,
    pub cards_count: i64,
    pub passes_count: i64,
    pub open_ride_status: Option<String>,
}

// ---- sightings ----

#[derive(Debug, Serialize, FromRow)]
pub struct SightingInfo {
    pub id: i64,
    #[serde(serialize_with = "utc_seconds")]
    pub received_at: DateTime<Utc>,
    pub user_id: Uuid,
    pub bus_id: i32,
    pub slot: i64,
    pub slot_offset: i64,
    pub rssi: i32,
    pub clock_synced: bool,
    pub accepted: bool,
    pub reject_reason: Option<String>,
}

// ---- rides ----

/// A row of `GET /rides`. `candidates` is the same `[{ busId, line }]` as the app's ride.
#[derive(Debug, FromRow)]
pub struct RideRow {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub status: RideStatus,
    pub bus_id: i32,
    pub candidates: Json<Vec<Candidate>>,
    pub passengers: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub end_reason: Option<String>,
    pub payment_reason: Option<PaymentReason>,
    pub manual: bool,
    pub qualification: Option<Json<Value>>,
    pub charge_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct RideListItem {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub status: RideStatus,
    pub bus_id: i32,
    pub line: Option<String>,
    pub destination: Option<String>,
    pub candidates: Vec<Candidate>,
    pub passengers: i32,
    #[serde(serialize_with = "utc_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "utc_seconds")]
    pub updated_at: DateTime<Utc>,
    #[serde(serialize_with = "utc_seconds_opt")]
    pub confirmed_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "utc_seconds_opt")]
    pub ended_at: Option<DateTime<Utc>>,
    /// `declined`, `timeout`, `not_boarded`, `silence_90s` or `silence_60min`.
    pub end_reason: Option<String>,
    pub payment_reason: Option<PaymentReason>,
    pub charge_id: Option<Uuid>,
    /// The user said "I am on the bus".
    pub manual: bool,
}

#[derive(Debug, Serialize, FromRow)]
pub struct StatusChange {
    pub status: String,
    #[serde(serialize_with = "utc_seconds")]
    pub at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct RideSighting {
    pub slot: i64,
    pub bus_id: i32,
    pub rssi: i32,
    #[serde(serialize_with = "utc_seconds")]
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct RideDetail {
    #[serde(flatten)]
    pub ride: RideListItem,
    pub status_history: Vec<StatusChange>,
    pub sightings: Vec<RideSighting>,
    /// What made the server offer the ride: `{ "slots_in_row", "distance_m" }`. `null` for a manual ride.
    pub qualification: Option<Value>,
    pub charge: Option<Charge>,
}

// ---- charges ----

#[derive(Debug, FromRow)]
pub struct ChargeAdminRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub session_id: Option<Uuid>,
    pub line: String,
    pub destination: Option<String>,
    pub amount_bani: i32,
    pub status: ChargeStatus,
    pub passengers: i32,
    pub paid_with: Uuid,
    pub source_kind: Option<String>,
    pub pass_product_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub qr_token: String,
}

#[derive(Debug, Serialize)]
pub struct ChargeAdminItem {
    pub id: Uuid,
    pub user_id: Uuid,
    pub session_id: Option<Uuid>,
    /// `ride`, or `pass_renewal` (a pass was bought: no ride).
    pub kind: &'static str,
    pub line: String,
    pub destination: Option<String>,
    #[serde(serialize_with = "ron_from_bani")]
    pub amount: i32,
    pub currency: &'static str,
    pub status: ChargeStatus,
    pub passengers: i32,
    #[serde(rename = "paidWith")]
    pub paid_with: Uuid,
    /// `card` or `pass`; `null` when what paid was deleted since.
    pub source_kind: Option<String>,
    pub pass_product_id: Option<Uuid>,
    #[serde(rename = "startedAt", serialize_with = "utc_seconds")]
    pub started_at: DateTime<Utc>,
    #[serde(rename = "endedAt", serialize_with = "utc_seconds_opt")]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(rename = "qrToken")]
    pub qr_token: String,
}

// ---- trips ----

#[derive(Debug, Serialize, FromRow)]
pub struct TripAdminItem {
    pub id: Uuid,
    pub user_id: Uuid,
    pub bus_id: Option<i32>,
    pub session_id: Option<Uuid>,
    pub charge_id: Uuid,
    #[serde(rename = "from")]
    pub from_stop: String,
    #[serde(rename = "to")]
    pub to_stop: String,
    pub date: NaiveDate,
    #[serde(serialize_with = "hours_and_minutes")]
    pub time: NaiveTime,
    pub line: String,
    #[serde(rename = "fareRon", serialize_with = "ron_from_bani")]
    pub fare_bani: i32,
    #[serde(rename = "distanceKm")]
    pub distance_km: f64,
    #[serde(rename = "durationMin")]
    pub duration_min: i32,
}

fn hours_and_minutes<S: serde::Serializer>(
    time: &NaiveTime,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&time.format("%H:%M").to_string())
}

// ---- passes ----

#[derive(Debug, FromRow)]
pub struct PassAdminRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub holder: String,
    pub card_number: String,
    pub valid_from: NaiveDate,
    pub valid_until: NaiveDate,
    pub lines: Json<PassLines>,
    pub theme: String,
    pub verified: bool,
    pub last_renewed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct PassAdminItem {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub holder: String,
    #[serde(rename = "cardNumber")]
    pub card_number: String,
    #[serde(rename = "validFrom")]
    pub valid_from: NaiveDate,
    #[serde(rename = "validUntil")]
    pub valid_until: NaiveDate,
    pub lines: PassLines,
    pub theme: String,
    pub verified: bool,
    /// When the pass was added (the time inside its id).
    #[serde(serialize_with = "utc_seconds_opt")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "utc_seconds_opt")]
    pub last_renewed_at: Option<DateTime<Utc>>,
}

// ---- reports ----

#[derive(Debug, FromRow)]
pub struct ReportAdminRow {
    pub id: Uuid,
    pub kind: String,
    pub line: String,
    pub stop_name: String,
    pub stop_lat: Option<f64>,
    pub stop_lon: Option<f64>,
    pub bus_id: Option<i32>,
    pub confirmations: i64,
    pub reporter_user_id: Option<Uuid>,
    pub confirmer_user_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ReportAdminItem {
    pub id: Uuid,
    pub kind: String,
    pub line: String,
    #[serde(rename = "stopName")]
    pub stop_name: String,
    pub stop_lat: Option<f64>,
    pub stop_lon: Option<f64>,
    pub bus_id: Option<i32>,
    pub confirmations: i64,
    pub reporter_user_id: Option<Uuid>,
    pub confirmer_user_ids: Vec<Uuid>,
    #[serde(rename = "createdAt", serialize_with = "utc_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "expiresAt", serialize_with = "utc_seconds")]
    pub expires_at: DateTime<Utc>,
}

// ---- ticket checks ----

#[derive(Debug, Serialize, FromRow)]
pub struct VerificationInfo {
    pub id: i64,
    #[serde(serialize_with = "utc_seconds")]
    pub at: DateTime<Utc>,
    pub inspector_id: Uuid,
    pub charge_id: Option<Uuid>,
    pub valid: bool,
    pub paid: bool,
    pub line: Option<String>,
    pub passengers: Option<i32>,
    #[serde(serialize_with = "utc_seconds_opt")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// A bus with the stops of its route (only the named points: the display needs the last one for the destination).
#[derive(FromRow)]
pub struct BusRoute {
    pub bus_id: i32,
    pub route_id: i32,
    pub name: String,
    pub points: Json<Vec<Point>>,
}
