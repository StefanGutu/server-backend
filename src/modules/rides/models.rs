use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

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
    pub updated_at: DateTime<Utc>,
    /// Milliseconds before the app asks again. `null` when it should stop asking.
    pub poll_after_ms: Option<u32>,
    pub vehicle: Option<Vehicle>,
    pub candidates: Vec<Candidate>,
    pub payment: Option<Payment>,
    /// Always `null` until the `Charge` type exists (step 17).
    pub charge: Option<serde_json::Value>,
}

/// What the service answers for GET /rides/current.
pub enum RideReply {
    /// The caller already has this version (its `If-None-Match` matched): 304, no body.
    NotModified { etag: String },
    /// The ride object and its ETag: 200.
    Modified { ride: Ride, etag: String },
}
