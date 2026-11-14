use sqlx::PgPool;
use uuid::Uuid;

use super::models::Sighting;

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
