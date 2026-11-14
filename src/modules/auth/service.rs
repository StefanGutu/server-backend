//! Rules for who the caller is.

use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{PatchMe, Tier, User, UserRole},
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

/// A name is trimmed and must not be blank. `None` means "do not change it".
pub fn clean_name(name: Option<String>) -> Result<Option<String>, AppError> {
    match name.map(|n| n.trim().to_string()) {
        Some(n) if n.is_empty() => Err(AppError::Validation("name must not be empty.".into())),
        other => Ok(other),
    }
}

/// PATCH /me: change the name and/or some settings, return the updated user.
pub async fn update_me(db: &PgPool, user_id: Uuid, patch: PatchMe) -> Result<User, AppError> {
    let name = clean_name(patch.name)?;
    let settings = patch.settings.unwrap_or_default();
    repo::update_user(db, user_id, name.as_deref(), &settings)
        .await?
        .ok_or(AppError::NotFound)
}

/// DELETE /me: remove the user.
pub async fn delete_me(db: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    match repo::delete_user(db, user_id).await? {
        true => Ok(()),
        false => Err(AppError::NotFound),
    }
}

/// Makes a wallet item (card or pass) the default way to pay. The caller checked it is the user's.
pub async fn set_default_source(db: &PgPool, user_id: Uuid, source_id: Uuid) -> Result<(), AppError> {
    Ok(repo::set_default_source(db, user_id, Some(source_id)).await?)
}

/// A wallet item was deleted: if it was the default way to pay, there is no default any more.
pub async fn clear_default_source_if(db: &PgPool, user_id: Uuid, source_id: Uuid) -> Result<(), AppError> {
    Ok(repo::clear_default_source_if(db, user_id, source_id).await?)
}

/// A new wallet item becomes the default way to pay if the user has no default yet.
pub async fn set_default_source_if_none(db: &PgPool, user_id: Uuid, source_id: Uuid) -> Result<(), AppError> {
    Ok(repo::set_default_source_if_none(db, user_id, source_id).await?)
}

/// Only an inspector may check tickets: anyone else is 403 `FORBIDDEN`. The role is set by hand
/// (`users.role`); nothing in the app gives it.
pub fn require_inspector(user: &User) -> Result<(), AppError> {
    if user.role == UserRole::Inspector { Ok(()) } else { Err(AppError::Forbidden) }
}
