use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::models::{LatestCount, Rider, Uncounted};

/// For each bus, its newest photo, only if it has no count yet. An older photo that was never counted is skipped on
/// purpose: only the newest photo of a bus says how many people are in it now.
pub async fn newest_uncounted(db: &PgPool) -> Result<Vec<Uncounted>, sqlx::Error> {
    sqlx::query_as(
        "SELECT bus_id, file_path FROM (
             SELECT DISTINCT ON (bus_id) bus_id, file_path, people_count
             FROM frames ORDER BY bus_id, received_at DESC, file_path DESC
         ) newest WHERE people_count IS NULL ORDER BY bus_id",
    )
    .fetch_all(db)
    .await
}

/// Keeps what the model saw in a photo. Returns false when there is no such photo any more (the clean-up that keeps
/// the newest 1000 photos of a bus may have deleted it while the model was counting).
pub async fn save_count(db: &PgPool, file_path: &str, count: i32) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("UPDATE frames SET people_count = $2, counted_at = now() WHERE file_path = $1")
            .bind(file_path)
            .bind(count)
            .execute(db)
            .await?;
    Ok(result.rows_affected() == 1)
}

/// The newest photo of the bus that has a count, or None when none was counted.
pub async fn latest_count(db: &PgPool, bus_id: i32) -> Result<Option<LatestCount>, sqlx::Error> {
    sqlx::query_as(
        "SELECT people_count, counted_at, file_path FROM frames
         WHERE bus_id = $1 AND people_count IS NOT NULL
         ORDER BY received_at DESC, file_path DESC LIMIT 1",
    )
    .bind(bus_id)
    .fetch_optional(db)
    .await
}

/// The users heard on this bus from `since_slot` on (one row for each user), with what they paid and whether the
/// ticket is good at `now`. A user has at most one open ride (a unique index), so one row each. A charge that was
/// cancelled is no ticket. `ticket_secs` is how long a ticket paid by card is good (a pass lasts as long as the ride).
pub async fn riders(
    db: &PgPool,
    bus_id: i32,
    since_slot: i64,
    now: DateTime<Utc>,
    ticket_secs: i64,
) -> Result<Vec<Rider>, sqlx::Error> {
    sqlx::query_as(
        "SELECT h.user_id,
                COALESCE(r.passengers, 1) AS passengers,
                COALESCE(c.status = 'covered_by_pass'
                         OR (c.status = 'in_progress' AND c.started_at > $3 - make_interval(secs => $4)), false) AS ticket_valid
         FROM (SELECT DISTINCT user_id FROM sightings WHERE bus_id = $1 AND slot >= $2) h
         LEFT JOIN ride_sessions r ON r.user_id = h.user_id AND r.bus_id = $1 AND r.status = 'confirmed'
         LEFT JOIN LATERAL (
             SELECT status, started_at FROM charges
             WHERE session_id = r.id AND status <> 'cancelled' ORDER BY id DESC LIMIT 1
         ) c ON true
         ORDER BY h.user_id",
    )
    .bind(bus_id)
    .bind(since_slot)
    .bind(now)
    .bind(ticket_secs as f64)
    .fetch_all(db)
    .await
}
