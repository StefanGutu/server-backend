use sqlx::PgPool;

use super::models::Telemetry;

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
