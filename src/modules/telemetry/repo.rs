use sqlx::PgPool;

use super::models::Telemetry;

/// GPS positions (lat, lon) of a bus received in [from_epoch, to_epoch), oldest first.
/// Only messages with a GPS fix; simulator messages (`sim = 1`) only when `allow_sim`.
pub async fn fix_positions(
    db: &PgPool,
    bus_id: i32,
    from_epoch: i64,
    to_epoch: i64,
    allow_sim: bool,
) -> Result<Vec<(f64, f64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT lat, lon FROM telemetry
         WHERE bus_id = $1 AND fix_ok AND lat IS NOT NULL AND lon IS NOT NULL
           AND received_at >= to_timestamp($2) AND received_at < to_timestamp($3)
           AND (sim = 0 OR $4)
         ORDER BY received_at, id",
    )
    .bind(bus_id)
    .bind(from_epoch as f64)
    .bind(to_epoch as f64)
    .bind(allow_sim)
    .fetch_all(db)
    .await
}

/// Deletes the oldest messages of every bus that has more than `keep`: the newest `keep` of each bus stay
/// (the ids count up, so the newest have the biggest ids). Returns how many were deleted.
pub async fn delete_beyond_newest(db: &PgPool, keep: i64) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM telemetry WHERE id IN (
             SELECT id FROM (
                 SELECT id, row_number() OVER (PARTITION BY bus_id ORDER BY id DESC) AS place FROM telemetry
             ) numbered WHERE place > $1
         )",
    )
    .bind(keep)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

/// Stores one message. A message already stored (same device, boot, seq) is skipped.
pub async fn insert_if_new(db: &PgPool, t: &Telemetry) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO telemetry
            (bus_id, device_id, boot_id, seq, ts, uptime_ms, lat, lon, speed_kmh, fix_ok, sim)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         ON CONFLICT (device_id, boot_id, seq) DO NOTHING",
    )
    .bind(t.bus_id)
    .bind(&t.device_id)
    .bind(&t.boot_id)
    .bind(t.seq)
    .bind(t.ts)
    .bind(t.uptime_ms)
    .bind(t.lat)
    .bind(t.lon)
    .bind(t.speed_kmh)
    .bind(t.fix_ok)
    .bind(t.sim)
    .execute(db)
    .await?;
    Ok(())
}
