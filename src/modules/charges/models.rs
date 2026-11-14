use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::extract::{ron_from_bani, utc_seconds, utc_seconds_opt};

/// State of a charge. Stored as text in `charges.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ChargeStatus {
    /// The ride goes on.
    InProgress,
    Completed,
    /// A pass paid for it, nothing was taken from a card.
    CoveredByPass,
    Cancelled,
}

/// The currency of every amount (server-api.md: `currency` is always `"RON"`).
const CURRENCY: &str = "RON";

/// A row of the `charges` table.
#[derive(FromRow)]
pub struct ChargeRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub session_id: Option<Uuid>,
    pub line: String,
    pub destination: Option<String>,
    pub amount_bani: i32,
    pub status: ChargeStatus,
    pub paid_with: Uuid,
    pub passengers: i32,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub qr_token: String,
}

/// A charge as the app sees it (`Charge` in server-api.md).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Charge {
    pub id: Uuid,
    pub line: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    /// In lei.
    #[serde(serialize_with = "ron_from_bani")]
    pub amount: i32,
    pub currency: &'static str,
    pub status: ChargeStatus,
    /// The id of the card or the pass that paid.
    pub paid_with: Uuid,
    #[serde(serialize_with = "utc_seconds")]
    pub started_at: DateTime<Utc>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "utc_seconds_opt"
    )]
    pub ended_at: Option<DateTime<Utc>>,
    pub qr_token: String,
}

impl From<ChargeRow> for Charge {
    fn from(row: ChargeRow) -> Self {
        Self {
            id: row.id,
            line: row.line,
            destination: row.destination,
            amount: row.amount_bani,
            currency: CURRENCY,
            status: row.status,
            paid_with: row.paid_with,
            started_at: row.started_at,
            ended_at: row.ended_at,
            qr_token: row.qr_token,
        }
    }
}

/// A signing key of the QR tickets: a row of the `ticket_keys` table.
#[derive(FromRow)]
pub struct TicketKey {
    pub kid: String,
    /// The public key, base64url without padding.
    pub public_key: String,
    /// The private key as PKCS#8 DER bytes.
    pub private_key: Vec<u8>,
}

/// A public key as the app gets it in `GET /tickets/public-key`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyInfo {
    pub kid: String,
    /// Always `"EdDSA"`.
    pub alg: &'static str,
    /// base64url
    pub public_key: String,
}

/// Answer of `GET /tickets/public-key`.
#[derive(Debug, Serialize)]
pub struct PublicKeysResponse {
    pub keys: Vec<PublicKeyInfo>,
}

impl From<TicketKey> for PublicKeyInfo {
    fn from(key: TicketKey) -> Self {
        Self {
            kid: key.kid,
            alg: "EdDSA",
            public_key: key.public_key,
        }
    }
}

/// What a new charge needs. The id and the ticket are made when it is stored.
pub struct NewCharge {
    pub user_id: Uuid,
    /// The ride this charge is for. None for the purchase of a pass.
    pub session_id: Option<Uuid>,
    pub line: String,
    pub destination: Option<String>,
    pub amount_bani: i32,
    pub status: ChargeStatus,
    /// The id of the card or pass that pays.
    pub paid_with: Uuid,
    pub passengers: i32,
    pub started_at: DateTime<Utc>,
    /// Set for a charge that is over at once (the purchase of a pass).
    pub ended_at: Option<DateTime<Utc>>,
}

/// Body of POST /tickets/verify.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyInput {
    pub qr_token: String,
}

/// What the inspector gets (server-api.md: `{ valid, line, passengers, expiresAt, paid }`).
/// `line`, `passengers` and `expiresAt` are `null` when the token is not one of ours.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyAnswer {
    /// The ticket is genuine (signed by us) and has not expired.
    pub valid: bool,
    pub line: Option<String>,
    pub passengers: Option<i32>,
    #[serde(serialize_with = "utc_seconds_opt")]
    pub expires_at: Option<DateTime<Utc>>,
    /// The charge of the ticket was paid: it exists and was not cancelled (a pass counts as paid).
    pub paid: bool,
}
