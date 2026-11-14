//! The rules for a sighting. `validate` and `check_token` are pure (no database).

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{Outcome, Sighting},
    repo,
};
use crate::{error::AppError, modules::devices, modules::rides::models::RideStatus};

/// Seconds in one token slot.
const SLOT_SECS: i64 = 30;
/// A slot up to 4 slots old (about 2 minutes) is accepted ...
const MAX_SLOTS_OLD: i64 = 4;
/// ... and one slot in the future (small clock differences).
const MAX_SLOTS_AHEAD: i64 = 1;
/// The token is the first 16 bytes of the HMAC.
const TOKEN_BYTES: usize = 16;

/// Shape of the body: `bus_id` 0 to 65535, `token` exactly 32 lowercase hex characters.
/// A wrong shape is a 400 (a wrong token is a 422, see `check_token`).
pub fn validate(s: &Sighting) -> Result<(), AppError> {
    if !(0..=65535).contains(&s.bus_id) {
        return Err(AppError::Validation("bus_id must be between 0 and 65535.".into()));
    }
    let lowercase_hex = s.token.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if s.token.len() != TOKEN_BYTES * 2 || !lowercase_hex {
        return Err(AppError::Validation("token must be 32 lowercase hex characters.".into()));
    }
    Ok(())
}

/// True when `token_hex` was issued by the beacon with this `secret` for this bus and slot,
/// and the slot is recent. `now` is unix seconds. The token comparison is constant-time.
/// The beacon builds the token as the first 16 bytes of HMAC-SHA256(secret, "<bus_id>.<slot>").
pub fn check_token(secret: &str, bus_id: i32, slot: i64, token_hex: &str, now: i64) -> bool {
    let current_slot = now / SLOT_SECS;
    if slot < current_slot - MAX_SLOTS_OLD || slot > current_slot + MAX_SLOTS_AHEAD {
        return false;
    }
    // Exactly 16 bytes: `verify_truncated_left` alone would also accept a shorter prefix.
    let Ok(token) = hex::decode(token_hex) else {
        return false;
    };
    if token.len() != TOKEN_BYTES {
        return false;
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(format!("{bus_id}.{slot}").as_bytes());
    mac.verify_truncated_left(&token).is_ok()
}

/// Whole flow: validate, check the phone clock, check the token with the bus's beacon
/// secret, store. A sighting that was already stored is still accepted.
pub async fn handle(db: &PgPool, user_id: Uuid, s: &Sighting) -> Result<Outcome, AppError> {
    validate(s)?;
    if !s.clock_synced {
        return Ok(Outcome::Rejected);
    }
    // A bus that does not exist has no beacon, so no token can have come from it.
    let Some(secret) = devices::service::beacon_secret(db, s.bus_id).await? else {
        return Ok(Outcome::Rejected);
    };
    if !check_token(&secret, s.bus_id, s.slot, &s.token, chrono::Utc::now().timestamp()) {
        return Ok(Outcome::Rejected);
    }
    repo::insert_if_new(db, user_id, s).await?;
    // Placeholder until step 12 decides the real status from the user's sightings.
    Ok(Outcome::Accepted(RideStatus::Pending))
}
