use sqlx::PgPool;
use uuid::Uuid;

use super::models::RideSession;

/// The latest ride of a user, or None when the user never had one.
pub async fn find_current_session(db: &PgPool, user_id: Uuid) -> Result<Option<RideSession>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, user_id, bus_id, status, version, candidates, passengers,
                payment_reason, client_secret, expires_at, updated_at
         FROM ride_sessions WHERE user_id = $1 ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
}
