use sqlx::{PgPool, types::Json};
use uuid::Uuid;

use super::models::{Settings, Tier, User};

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
