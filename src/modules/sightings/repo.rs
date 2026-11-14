use sqlx::PgPool;
use uuid::Uuid;

use super::models::{RejectReason, Sighting};

/// (bus_id, slot) of the user's sightings from `since_slot` on.
pub async fn recent_sightings(
    db: &PgPool,
    user_id: Uuid,
    since_slot: i64,
) -> Result<Vec<(i32, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT bus_id, slot FROM sightings WHERE user_id = $1 AND slot >= $2 ORDER BY slot",
    )
    .bind(user_id)
    .bind(since_slot)
    .fetch_all(db)
    .await
}

/// The newest slot in which the user heard this bus, or None if never.
pub async fn last_slot(
    db: &PgPool,
    user_id: Uuid,
    bus_id: i32,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT max(slot) FROM sightings WHERE user_id = $1 AND bus_id = $2")
        .bind(user_id)
        .bind(bus_id)
        .fetch_one(db)
        .await
}

/// The newest slot in which the user heard any bus, or None if never.
pub async fn last_slot_any_bus(db: &PgPool, user_id: Uuid) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT max(slot) FROM sightings WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(db)
        .await
}

/// The buses the user heard from `since_slot` on (each bus once).
pub async fn recent_bus_ids(
    db: &PgPool,
    user_id: Uuid,
    since_slot: i64,
) -> Result<Vec<i32>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT bus_id FROM sightings WHERE user_id = $1 AND slot >= $2 ORDER BY bus_id",
    )
    .bind(user_id)
    .bind(since_slot)
    .fetch_all(db)
    .await
}

/// Stores a sighting. The same user, bus and slot again is skipped (a retry).
pub async fn insert_if_new(db: &PgPool, user_id: Uuid, s: &Sighting) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO sightings (user_id, bus_id, slot, rssi) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, bus_id, slot) DO NOTHING",
    )
    .bind(user_id)
    .bind(s.bus_id)
    .bind(s.slot)
    .bind(s.rssi)
    .execute(db)
    .await?;
    Ok(())
}

/// Notes a sighting in the log, accepted (`reject` is None) or refused with its reason.
pub async fn log_attempt(
    db: &PgPool,
    user_id: Uuid,
    s: &Sighting,
    slot_offset: i64,
    reject: Option<RejectReason>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO sighting_log (user_id, bus_id, slot, slot_offset, rssi, clock_synced, accepted, reject_reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(user_id)
    .bind(s.bus_id)
    .bind(s.slot)
    .bind(slot_offset)
    .bind(s.rssi)
    .bind(s.clock_synced)
    .bind(reject.is_none())
    .bind(reject.map(RejectReason::as_str))
    .execute(db)
    .await?;
    Ok(())
}

/// Deletes the oldest entries of the log, keeping the newest `keep`. Returns how many.
pub async fn delete_log_beyond_newest(db: &PgPool, keep: i64) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM sighting_log WHERE id < (SELECT id FROM sighting_log ORDER BY id DESC OFFSET $1 LIMIT 1)",
    )
    .bind(keep)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}
