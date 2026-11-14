use sqlx::{PgPool, types::Json};
use uuid::Uuid;

use super::models::{Settings, SettingsPatch, Tier, User};

/// The column list, written once. A macro (not a const) because sqlx only accepts
/// literal SQL, and `concat!` builds a literal.
macro_rules! user_columns {
    () => {
        "id, tier, role, name, email, settings, default_source_id, created_at"
    };
}

/// The user with this id, or None.
pub async fn find_user(db: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(concat!("SELECT ", user_columns!(), " FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await
}

/// Creates a user with default settings. Returns None when the id already exists
/// (another request created it first).
pub async fn insert_user(db: &PgPool, id: Uuid, tier: Tier) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(concat!(
        "INSERT INTO users (id, tier, settings) VALUES ($1, $2, $3) ",
        "ON CONFLICT (id) DO NOTHING RETURNING ",
        user_columns!()
    ))
    .bind(id)
    .bind(tier)
    .bind(Json(Settings::default()))
    .fetch_optional(db)
    .await
}

/// Changes the name (when given) and merges the given settings keys into the stored ones.
/// One statement, so two PATCH calls at the same time cannot overwrite each other's keys.
pub async fn update_user(
    db: &PgPool,
    id: Uuid,
    name: Option<&str>,
    settings: &SettingsPatch,
) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(concat!(
        "UPDATE users SET name = COALESCE($2, name), settings = settings || $3 ",
        "WHERE id = $1 RETURNING ",
        user_columns!()
    ))
    .bind(id)
    .bind(name)
    .bind(Json(settings))
    .fetch_optional(db)
    .await
}

/// Deletes the user. Returns false when there was no such user.
pub async fn delete_user(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}
