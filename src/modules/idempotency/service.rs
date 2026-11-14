//! The `Idempotency-Key` rules (server-api.md: on `confirm` and `renew`). `parse_key` and
//! `request_hash` are pure.

use axum::http::HeaderMap;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::{
    models::{Begin, IdempotentRequest},
    repo,
};
use crate::{error::AppError, extract::required_header};

/// The name of the header (server-api.md).
const HEADER_NAME: &str = "Idempotency-Key";

/// The header value must be a uuid (server-api.md: `Idempotency-Key: <uuid>`).
pub fn parse_key(header: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(header.trim())
        .map_err(|_| AppError::Validation(format!("{HEADER_NAME} must be a uuid.")))
}

/// Identifies a call: sha256 (hex) of the endpoint, a line break and the body exactly as sent.
/// The same key with another endpoint or another body gives another hash: a conflict.
pub fn request_hash(endpoint: &str, body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(endpoint.as_bytes());
    hasher.update(b"\n");
    hasher.update(body);
    hex::encode(hasher.finalize())
}

/// Builds the request from the call: the `Idempotency-Key` header is required (a missing one,
/// or one that is not a uuid, is a 400), then the endpoint and the body identify the call.
pub fn request_from_headers(
    headers: &HeaderMap,
    endpoint: &str,
    body: &[u8],
) -> Result<IdempotentRequest, AppError> {
    Ok(IdempotentRequest {
        key: parse_key(required_header(headers, HEADER_NAME)?)?,
        endpoint: endpoint.to_string(),
        request_hash: request_hash(endpoint, body),
    })
}

/// Is this call new, or was it done before? The same key with a different call is
/// `IDEMPOTENCY_CONFLICT`; the same call gets the stored answer.
pub async fn begin(
    db: &PgPool,
    user_id: Uuid,
    request: &IdempotentRequest,
) -> Result<Begin, AppError> {
    match repo::find(db, user_id, request.key).await? {
        None => Ok(Begin::Proceed),
        Some((hash, _)) if hash != request.request_hash => Err(AppError::IdempotencyConflict),
        Some((_, response)) => Ok(Begin::Replay(response)),
    }
}

/// Keeps the answer of a call that worked, so a repeat of it gets the same answer.
pub async fn finish(
    db: &PgPool,
    user_id: Uuid,
    request: &IdempotentRequest,
    response: &Value,
) -> Result<(), AppError> {
    Ok(repo::insert(db, user_id, request, response).await?)
}

/// One operation at a time for a thing that is paid for (a ride, a pass): the second request waits
/// until the first has its answer, then sees what the first did. Without it two requests at the same
/// moment could both pay. `name` is for example `ride:<id>`. Give what it returns to `unlock`.
pub async fn lock(db: &PgPool, name: &str) -> Result<Transaction<'static, Postgres>, AppError> {
    Ok(repo::lock(db, name).await?)
}

/// Releases a lock of `lock`.
pub async fn unlock(lock: Transaction<'static, Postgres>, name: &str) {
    repo::unlock(lock, name).await;
}
