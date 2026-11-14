use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::models::{Report, ReportKind, StopFound};

/// The columns of a `Report`, with the number of confirmations counted.
macro_rules! select_report {
    ($tail:literal) => {
        concat!(
            "SELECT r.id, r.kind, r.line, r.stop_name, r.created_at, r.expires_at,
                    (SELECT count(*) FROM report_confirmations c WHERE c.report_id = r.id) AS confirmations
             FROM reports r ",
            $tail
        )
    };
}

/// The reports that have not expired, newest first (at most `limit`), only of this line and kind when given.
pub async fn list_active(
    db: &PgPool,
    limit: i64,
    line: Option<&str>,
    kind: Option<ReportKind>,
) -> Result<Vec<Report>, sqlx::Error> {
    sqlx::query_as(select_report!(
        "WHERE r.expires_at > now()
           AND ($2::text IS NULL OR lower(r.line) = lower($2))
           AND ($3::text IS NULL OR r.kind = $3)
         ORDER BY r.id DESC LIMIT $1"
    ))
    .bind(limit)
    .bind(line)
    .bind(kind)
    .fetch_all(db)
    .await
}

/// One report that has not expired.
pub async fn find_active(db: &PgPool, id: Uuid) -> Result<Option<Report>, sqlx::Error> {
    sqlx::query_as(select_report!("WHERE r.id = $1 AND r.expires_at > now()"))
        .bind(id)
        .fetch_optional(db)
        .await
}

/// The id of a report that has not expired and says the same thing (kind, line, stop).
pub async fn find_same(
    db: &PgPool,
    kind: ReportKind,
    line: &str,
    stop_name: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM reports
         WHERE kind = $1 AND line = $2 AND stop_name = $3 AND expires_at > now()
         ORDER BY id DESC LIMIT 1",
    )
    .bind(kind)
    .bind(line)
    .bind(stop_name)
    .fetch_optional(db)
    .await
}

/// Stores a new report.
pub async fn insert_report(
    db: &PgPool,
    id: Uuid,
    kind: ReportKind,
    line: &str,
    stop: &StopFound,
    reporter_user_id: Uuid,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO reports (id, kind, line, stop_name, bus_id, stop_lat, stop_lon, reporter_user_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(kind)
    .bind(line)
    .bind(&stop.stop_name)
    .bind(stop.bus_id)
    .bind(stop.lat)
    .bind(stop.lon)
    .bind(reporter_user_id)
    .bind(expires_at)
    .execute(db)
    .await?;
    Ok(())
}

/// The user says the report is true. Saying it again changes nothing. Returns whether it was new.
pub async fn add_confirmation(
    db: &PgPool,
    report_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO report_confirmations (report_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(report_id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Deletes the reports that have expired. Returns how many.
pub async fn delete_expired(db: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM reports WHERE expires_at <= now()")
        .execute(db)
        .await?;
    Ok(result.rows_affected())
}
