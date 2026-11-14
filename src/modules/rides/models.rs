use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

use crate::{extract::utc_seconds, modules::charges::models::Charge};

/// The `status` of a ride (server-api.md section 2). Stored as text in `ride_sessions.status`
/// (`None` is never stored: it means "no row").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum RideStatus {
    /// No ride.
    None,
    /// Not sure yet that the user is on the bus.
    Pending,
    /// On the bus, waiting for the payment confirmation.
    AwaitingConfirmation,
    PaymentFailed,
    /// Paid, ticket issued.
    Confirmed,
    /// Did not board, or refused.
    Rejected,
    /// Got off.
    Ended,
}

/// Why a payment failed. Stored as text in `ride_sessions.payment_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum PaymentReason {
    CardDeclined,
    AuthenticationRequired,
}

impl PaymentReason {
    /// The text sent as `reason` (the same as in the ride's `payment.reason`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CardDeclined => "card_declined",
            Self::AuthenticationRequired => "authentication_required",
        }
    }
}

/// A bus the user may be on, when two are heard and the server cannot choose.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub bus_id: i32,
    pub line: String,
}

/// A row of the `ride_sessions` table.
#[derive(FromRow)]
pub struct RideSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub bus_id: i32,
    pub status: RideStatus,
    pub version: i64,
    pub candidates: Json<Vec<Candidate>>,
    pub passengers: i32,
    pub payment_reason: Option<PaymentReason>,
    pub client_secret: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The bus of a ride, as the app shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vehicle {
    pub bus_id: i32,
    pub line: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
}

/// Present only when status is `payment_failed`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Payment {
    pub reason: PaymentReason,
    /// Only for `authentication_required`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

/// The ride object of GET /rides/current (server-api.md section 2). Every field is always
/// present; the ones that do not apply are `null`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ride {
    pub status: RideStatus,
    pub session_id: Option<Uuid>,
    #[serde(serialize_with = "utc_seconds")]
    pub updated_at: DateTime<Utc>,
    /// Milliseconds before the app asks again. `null` when it should stop asking.
    pub poll_after_ms: Option<u32>,
    pub vehicle: Option<Vehicle>,
    pub candidates: Vec<Candidate>,
    pub payment: Option<Payment>,
    /// The charge of the ride: shown when the ride is `confirmed` or `ended`, otherwise `null`.
    pub charge: Option<Charge>,
}

/// What the service answers for GET /rides/current.
pub enum RideReply {
    /// The caller already has this version (its `If-None-Match` matched): 304, no body.
    NotModified { etag: String },
    /// The ride object and its ETag: 200.
    /// The ride is boxed: it is much bigger than the other answer.
    Modified { ride: Box<Ride>, etag: String },
    /// The same call (same `Idempotency-Key`) was done before: its stored answer, as it was sent.
    Stored(serde_json::Value),
}

/// What the server knows about one bus the user heard recently (built by `sightings`).
/// The input of the boarding decision.
#[derive(Debug, Clone, PartialEq)]
pub struct BusEvidence {
    pub bus_id: i32,
    /// Slots in a row in which the user heard this bus, ending at `last_slot`.
    pub run_len: usize,
    /// Unix time at which the first slot of the run began.
    pub started_at: i64,
    pub last_slot: i64,
    /// Metres the bus moved during the run. Left at 0 for a run too short to matter.
    pub moved_m: f64,
}

/// A ride, as the background task needs it to decide if it is over.
#[derive(FromRow)]
pub struct RideRef {
    pub id: Uuid,
    pub user_id: Uuid,
    pub bus_id: i32,
}

/// Body of POST /rides/{sessionId}/confirm (server-api.md).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmInput {
    /// 1 to 10.
    pub passengers: i32,
    /// The card or pass to pay with. Without it: the user's default source.
    pub source_id: Option<Uuid>,
    /// Required only when the ride has `candidates`: the bus the user is on.
    pub bus_id: Option<i32>,
}
