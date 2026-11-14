//! The rules for a sighting. `validate` and `check_token` are pure (no database).

use std::collections::{BTreeMap, BTreeSet};

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{Outcome, Sighting},
    repo,
};
use crate::{
    error::AppError,
    modules::{
        devices,
        rides::{
            self,
            models::{BusEvidence, RideReply},
        },
        telemetry,
    },
};

/// Seconds in one token slot.
pub const SLOT_SECS: i64 = 30;
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

/// How far back (in slots, 120 = 1 hour) we look at what the user heard. Long enough to see
/// where a run of slots in a row started.
const LOOKBACK_SLOTS: i64 = 120;

/// The newest slot in which the user heard this bus, or None if never.
pub async fn last_heard_slot(db: &PgPool, user_id: Uuid, bus_id: i32) -> Result<Option<i64>, AppError> {
    Ok(repo::last_slot(db, user_id, bus_id).await?)
}

/// The newest slot in which the user heard any bus, or None if never.
pub async fn last_heard_any_slot(db: &PgPool, user_id: Uuid) -> Result<Option<i64>, AppError> {
    Ok(repo::last_slot_any_bus(db, user_id).await?)
}

/// POST /rides/manual: the user says "I am on the bus". The buses heard in the last 2 minutes (the
/// slots a token is still accepted for) decide which bus it is; with none it is a 422. The ride
/// itself is changed by `rides`.
pub async fn manual_ride(db: &PgPool, user_id: Uuid) -> Result<RideReply, AppError> {
    let since_slot = chrono::Utc::now().timestamp() / SLOT_SECS - MAX_SLOTS_OLD;
    let heard = repo::recent_bus_ids(db, user_id, since_slot).await?;
    rides::service::start_manual_ride(db, user_id, &heard).await
}

/// Groups (bus_id, slot) pairs by bus and finds, for each bus, the run of slots in a row that
/// ends at the newest slot of that bus. `moved_m` is left at 0. Pure.
pub fn runs_by_bus(heard: &[(i32, i64)]) -> Vec<BusEvidence> {
    let mut slots_by_bus: BTreeMap<i32, BTreeSet<i64>> = BTreeMap::new();
    for &(bus_id, slot) in heard {
        slots_by_bus.entry(bus_id).or_default().insert(slot);
    }
    slots_by_bus
        .into_iter()
        .filter_map(|(bus_id, slots)| {
            let last_slot = *slots.last()?;
            let mut start_slot = last_slot;
            while slots.contains(&(start_slot - 1)) {
                start_slot -= 1;
            }
            Some(BusEvidence {
                bus_id,
                run_len: (last_slot - start_slot + 1) as usize,
                started_at: start_slot * SLOT_SECS,
                last_slot,
                moved_m: 0.0,
            })
        })
        .collect()
}

/// What the user heard lately, with how far each bus moved while they heard it.
/// Only runs long enough to count get the (more expensive) movement lookup.
async fn gather_evidence(db: &PgPool, user_id: Uuid, allow_sim: bool) -> Result<Vec<BusEvidence>, AppError> {
    let since_slot = chrono::Utc::now().timestamp() / SLOT_SECS - LOOKBACK_SLOTS;
    let heard = repo::recent_sightings(db, user_id, since_slot).await?;

    let mut evidence = runs_by_bus(&heard);
    for e in evidence.iter_mut().filter(|e| e.run_len >= rides::service::REQUIRED_SLOTS) {
        let (from, to) = (e.started_at, (e.last_slot + 1) * SLOT_SECS);
        e.moved_m = telemetry::service::bus_movement_m(db, e.bus_id, from, to, allow_sim).await?;
    }
    Ok(evidence)
}

/// Whole flow: validate, check the phone clock, check the token with the bus's beacon
/// secret, store, then update the ride. A sighting that was already stored is still accepted.
/// `allow_sim`: simulator telemetry may count as bus movement (test mode only).
pub async fn handle(db: &PgPool, user_id: Uuid, s: &Sighting, allow_sim: bool) -> Result<Outcome, AppError> {
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

    let evidence = gather_evidence(db, user_id, allow_sim).await?;
    let status = rides::service::update_from_sightings(db, user_id, &evidence).await?;
    Ok(Outcome::Accepted(status))
}
