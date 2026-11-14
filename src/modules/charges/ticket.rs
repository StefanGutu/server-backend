//! The QR ticket token: an EdDSA JWT (server-api.md section 5). Pure: no database, no HTTP.
//!
//! Header: `alg` EdDSA and `kid` (which key signed it). Claims: `cid` (the charge), `line`,
//! `pax` (passengers), `iat` and `exp` (unix seconds).

use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
    jwk::{AlgorithmParameters, Jwk},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::models::TicketKey;
use crate::error::AppError;

/// The start of every Ed25519 private key in PKCS#8 form (RFC 8410): 16 fixed bytes, then the
/// 32 secret bytes. 48 bytes in all, the form `jsonwebtoken` signs with and reads a public key from.
const PKCS8_ED25519_HEADER: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// How long a ticket is valid: 60 minutes (open question 8 in PLAN.md).
pub const TICKET_VALID_SECS: i64 = 60 * 60;

/// What the token says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketClaims {
    /// The charge this ticket is for.
    pub cid: Uuid,
    pub line: String,
    /// Passengers.
    pub pax: i32,
    pub iat: i64,
    pub exp: i64,
}

impl TicketClaims {
    /// Claims for a ticket made at `now` (unix seconds): valid for `TICKET_VALID_SECS`.
    pub fn new(charge_id: Uuid, line: &str, passengers: i32, now: i64) -> Self {
        Self { cid: charge_id, line: line.to_string(), pax: passengers, iat: now, exp: now + TICKET_VALID_SECS }
    }
}

fn server_error(what: &str, err: impl std::fmt::Display) -> AppError {
    tracing::error!("ticket key: {what}: {err}");
    AppError::Server
}

impl TicketKey {
    /// Builds a key from 32 secret bytes (the caller takes them from a secure random source).
    pub fn from_seed(kid: String, seed: &[u8; 32]) -> Result<Self, AppError> {
        let private_key = [PKCS8_ED25519_HEADER.as_slice(), seed.as_slice()].concat();

        // The public key, base64url, comes from the same library that signs.
        let jwk = Jwk::from_encoding_key(&EncodingKey::from_ed_der(&private_key), Algorithm::EdDSA)
            .map_err(|e| server_error("cannot get the public key", e))?;
        let AlgorithmParameters::OctetKeyPair(params) = jwk.algorithm else {
            return Err(server_error("cannot get the public key", "not an Ed25519 key"));
        };
        Ok(Self { kid, public_key: params.x, private_key })
    }
}

/// Signs the claims with this key. The `kid` of the key goes in the header.
pub fn sign(key: &TicketKey, claims: &TicketClaims) -> Result<String, AppError> {
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(key.kid.clone());
    encode(&header, claims, &EncodingKey::from_ed_der(&key.private_key))
        .map_err(|e| server_error("cannot sign a ticket", e))
}

/// The claims of a token that was signed by one of `keys`, whether or not it has expired.
/// `None` for any other token: wrong signature, unknown `kid`, not a token. An expired but genuine
/// token is still read, so the inspector can be told it is only out of date.
pub fn read(token: &str, keys: &[TicketKey]) -> Option<TicketClaims> {
    let kid = decode_header(token).ok()?.kid?;
    let key = keys.iter().find(|k| k.kid == kid)?;
    let decoding_key = DecodingKey::from_ed_components(&key.public_key).ok()?;

    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.leeway = 0;
    validation.validate_exp = false; // `verify` checks it against a time it is given
    validation.set_required_spec_claims(&["exp", "iat"]);
    Some(decode::<TicketClaims>(token, &decoding_key, &validation).ok()?.claims)
}

/// The claims of a token that was signed by one of `keys` and has not expired (`now` is unix
/// seconds). `None` for any other token: wrong signature, unknown `kid`, expired, not a token.
pub fn verify(token: &str, keys: &[TicketKey], now: i64) -> Option<TicketClaims> {
    read(token, keys).filter(|claims| claims.exp > now)
}
