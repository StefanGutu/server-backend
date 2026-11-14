use sqlx::PgPool;

use super::models::FrameMeta;

/// Stores what is known about a photo that was saved at `file_path`.
pub async fn insert_frame(
    db: &PgPool,
    bus_id: i32,
    meta: &FrameMeta,
    file_path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO frames (file_path, bus_id, frame_seq, capture_ts, route_id, width, height)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(file_path)
    .bind(bus_id)
    .bind(meta.seq)
    .bind(meta.capture_ts)
    .bind(meta.route_id)
    .bind(i32::try_from(meta.width).unwrap_or(i32::MAX))
    .bind(i32::try_from(meta.height).unwrap_or(i32::MAX))
    .execute(db)
    .await?;
    Ok(())
}

/// Forgets the photos of this bus beyond the newest `keep` and gives their file paths, so the caller
/// can remove the files. One statement: two photos arriving at the same moment cannot delete the same row twice.
pub async fn delete_older_than_newest(
    db: &PgPool,
    bus_id: i32,
    keep: i64,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "DELETE FROM frames WHERE file_path IN (
             SELECT file_path FROM frames WHERE bus_id = $1
             ORDER BY received_at DESC, file_path DESC OFFSET $2
         ) RETURNING file_path",
    )
    .bind(bus_id)
    .bind(keep)
    .fetch_all(db)
    .await
}
