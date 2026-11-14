use sqlx::PgPool;

use super::models::FrameMeta;

/// Stores what is known about a photo that was saved at `file_path`.
pub async fn insert_frame(db: &PgPool, bus_id: i32, meta: &FrameMeta, file_path: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO frames (file_path, bus_id, frame_seq, capture_ts) VALUES ($1, $2, $3, $4)")
        .bind(file_path)
        .bind(bus_id)
        .bind(meta.seq)
        .bind(meta.capture_ts)
        .execute(db)
        .await?;
    Ok(())
}
