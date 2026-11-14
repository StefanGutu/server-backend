use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::models::{Notification, Platform};

/// Where a list continues: the (created_at, id) of one notification.
pub type Cursor = (DateTime<Utc>, Uuid);

/// The place of one of the user's notifications in the list, or None if the user has no such notification.
pub async fn find_cursor(
    db: &PgPool,
    user_id: Uuid,
    notification_id: Uuid,
) -> Result<Option<Cursor>, sqlx::Error> {
    sqlx::query_as("SELECT created_at, id FROM notifications WHERE id = $1 AND user_id = $2")
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(db)
        .await
}

/// The user's notifications, newest first. With a cursor: only those older than it.
pub async fn list_notifications(
    db: &PgPool,
    user_id: Uuid,
    after: Option<Cursor>,
    limit: i64,
) -> Result<Vec<Notification>, sqlx::Error> {
    let (created_before, id_before) = match after {
        Some((created_at, id)) => (Some(created_at), Some(id)),
        None => (None, None),
    };
    sqlx::query_as(
        "SELECT id, kind, title, body, created_at, read
         FROM notifications
         WHERE user_id = $1
           AND ($2::timestamptz IS NULL OR (created_at, id) < ($2, $3::uuid))
         ORDER BY created_at DESC, id DESC
         LIMIT $4",
    )
    .bind(user_id)
    .bind(created_before)
    .bind(id_before)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// Marks a notification of this user as read. Returns false when the user has no such notification.
/// A notification that was already read stays read (and the call still finds it).
pub async fn mark_read(
    db: &PgPool,
    user_id: Uuid,
    notification_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("UPDATE notifications SET read = true WHERE id = $1 AND user_id = $2")
        .bind(notification_id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// Remembers the phone of a user. The token belongs to one phone: if it was another user's, it is theirs now.
pub async fn upsert_device(
    db: &PgPool,
    user_id: Uuid,
    push_token: &str,
    platform: Platform,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO push_devices (push_token, user_id, platform) VALUES ($1, $2, $3)
         ON CONFLICT (push_token) DO UPDATE SET user_id = EXCLUDED.user_id, platform = EXCLUDED.platform",
    )
    .bind(push_token)
    .bind(user_id)
    .bind(platform)
    .execute(db)
    .await?;
    Ok(())
}

/// The Expo push tokens of the phones of a user.
pub async fn tokens_of_user(db: &PgPool, user_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT push_token FROM push_devices WHERE user_id = $1 ORDER BY push_token")
        .bind(user_id)
        .fetch_all(db)
        .await
}

/// Forgets a phone by its token. Returns whether there was one.
pub async fn delete_device(db: &PgPool, push_token: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM push_devices WHERE push_token = $1")
        .bind(push_token)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}
