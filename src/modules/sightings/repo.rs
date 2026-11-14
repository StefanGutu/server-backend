use sqlx::PgPool;
use uuid::Uuid;

use super::models::Sighting;

/// (bus_id, slot) of the user's sightings from `since_slot` on.
pub async fn recent_sightings(
    db: &PgPool,
    user_id: Uuid,
    since_slot: i64,
) -> Result<Vec<(i32, i64)>, sqlx::Error> {
    sqlx::query_as("SELECT bus_id, slot FROM sightings WHERE user_id = $1 AND slot >= $2 ORDER BY slot")
        .bind(user_id)
        .bind(since_slot)
        .fetch_all(db)
        .await
}

/// The newest slot in which the user heard this bus, or None if never.
pub async fn last_slot(db: &PgPool, user_id: Uuid, bus_id: i32) -> Result<Option<i64>, sqlx::Error> {
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
pub async fn recent_bus_ids(db: &PgPool, user_id: Uuid, since_slot: i64) -> Result<Vec<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT DISTINCT bus_id FROM sightings WHERE user_id = $1 AND slot >= $2 ORDER BY bus_id")
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
