use serde_json::Value;
use sqlx::{PgPool, Postgres, pool::PoolConnection, types::Json};
use uuid::Uuid;

use super::models::IdempotentRequest;

/// The stored (request_hash, response) of this user's key, or None if the key is new.
pub async fn find(db: &PgPool, user_id: Uuid, key: Uuid) -> Result<Option<(String, Value)>, sqlx::Error> {
    let row: Option<(String, Json<Value>)> =
        sqlx::query_as("SELECT request_hash, response FROM idempotency_keys WHERE user_id = $1 AND key = $2")
            .bind(user_id)
            .bind(key)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(hash, response)| (hash, response.0)))
}

/// Stores the answer of the first call. If the key is already there, the first answer stays.
pub async fn insert(db: &PgPool, user_id: Uuid, request: &IdempotentRequest, response: &Value) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO idempotency_keys (key, user_id, endpoint, request_hash, response)
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (user_id, key) DO NOTHING",
    )
    .bind(request.key)
    .bind(user_id)
    .bind(&request.endpoint)
    .bind(&request.request_hash)
    .bind(Json(response))
    .execute(db)
    .await?;
    Ok(())
}

/// Takes a lock with this name and waits if another request holds it. The lock belongs to the
/// connection that is returned, so keep it until `unlock`. It is also released if the connection is
/// closed (for example when the server stops).
pub async fn lock(db: &PgPool, name: &str) -> Result<PoolConnection<Postgres>, sqlx::Error> {
    let mut conn = db.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1, 0))")
        .bind(name)
        .execute(&mut *conn)
        .await?;
    Ok(conn)
}

/// Releases the lock of `lock`. If that fails, the connection is closed instead of going back to the
/// pool, so a lock can never stay on a pooled connection.
pub async fn unlock(mut conn: PoolConnection<Postgres>, name: &str) {
    let unlocked = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
        .bind(name)
        .execute(&mut *conn)
        .await;
    if let Err(e) = unlocked {
        tracing::error!("cannot release the lock {name}: {e}");
        drop(conn.detach());
    }
}
