use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

use super::models::IdempotentRequest;

/// The stored (request_hash, response) of this user's key, or None if the key is new.
pub async fn find(
    db: &PgPool,
    user_id: Uuid,
    key: Uuid,
) -> Result<Option<(String, Value)>, sqlx::Error> {
    let row: Option<(String, Json<Value>)> = sqlx::query_as(
        "SELECT request_hash, response FROM idempotency_keys WHERE user_id = $1 AND key = $2",
    )
    .bind(user_id)
    .bind(key)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(hash, response)| (hash, response.0)))
}

/// Stores the answer of the first call. If the key is already there, the first answer stays.
pub async fn insert(
    db: &PgPool,
    user_id: Uuid,
    request: &IdempotentRequest,
    response: &Value,
) -> Result<(), sqlx::Error> {
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

/// Takes a lock with this name and waits if another request holds it. The lock lives as long as the
/// returned transaction (`pg_advisory_xact_lock`): it is released when the transaction ends, and also
/// when the transaction is dropped (the database rolls it back and the connection goes back to the pool
/// clean), so a request that is abandoned half way can never leave a lock behind.
pub async fn lock(db: &PgPool, name: &str) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(name)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// Releases the lock of `lock` by ending its transaction. If that fails the transaction is dropped,
/// which rolls it back: the lock is gone either way.
pub async fn unlock(lock: Transaction<'static, Postgres>, name: &str) {
    if let Err(e) = lock.commit().await {
        tracing::error!("cannot release the lock {name}: {e}");
    }
}
