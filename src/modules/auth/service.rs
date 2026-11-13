//! Rules for who the caller is.

use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{Tier, User},
    repo,
};
use crate::error::AppError;

/// Fixed "namespace" for turning test ids into uuids. Never change it: the same
/// `X-User-Id` must always give the same user.
const TEST_NAMESPACE: Uuid = Uuid::from_u128(0x6e2f_8a31_0c4d_4b7a_9a55_3d1e_7c20_91ab);

/// Test mode: `X-User-Id` may be any text ("u1"). Our ids are uuids, so a value that
/// already is a uuid is used as is, anything else is hashed into a stable uuid.
pub fn test_user_id(header: &str) -> Uuid {
    Uuid::parse_str(header).unwrap_or_else(|_| Uuid::new_v5(&TEST_NAMESPACE, header.as_bytes()))
}

/// The user with this id. Created on first use, so the usual call (user exists)
/// only reads.
async fn get_or_create_user(db: &PgPool, id: Uuid, tier: Tier) -> Result<User, AppError> {
    if let Some(user) = repo::find_user(db, id).await? {
        return Ok(user);
    }
    if let Some(user) = repo::insert_user(db, id, tier).await? {
        return Ok(user);
    }
    // Another request created the same user a moment ago: read it.
    repo::find_user(db, id).await?.ok_or(AppError::Server)
}

/// The user behind an `X-User-Id` header, created on first use.
/// Test users are `account` so they can pay (open question 6 in PLAN.md).
pub async fn user_from_test_header(db: &PgPool, header: &str) -> Result<User, AppError> {
    if header.trim().is_empty() {
        return Err(AppError::AuthInvalid);
    }
    get_or_create_user(db, test_user_id(header), Tier::Account).await
}
