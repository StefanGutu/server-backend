use sqlx::{PgExecutor, PgPool, types::Json};
use uuid::Uuid;

#[cfg(feature = "phone-login")]
use chrono::{DateTime, Utc};

use super::models::{
    Identity, Linked, NewRefreshToken, Provider, RefreshTokenRow, Settings, SettingsPatch, Tier,
    User,
};

/// The column list, written once. A macro (not a const) because sqlx only accepts
/// literal SQL, and `concat!` builds a literal.
macro_rules! user_columns {
    () => {
        "id, tier, role, name, email, settings, default_source_id, created_at"
    };
}

/// The user with this id, or None.
pub async fn find_user(db: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        user_columns!(),
        " FROM users WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(db)
    .await
}

/// Creates a user with default settings. Returns None when the id already exists
/// (another request created it first). `db` is the pool, or a transaction.
pub async fn insert_user(
    db: impl PgExecutor<'_>,
    id: Uuid,
    tier: Tier,
) -> Result<Option<User>, sqlx::Error> {
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

/// Sets (or, with None, clears) the default payment source of a user.
pub async fn set_default_source(
    db: &PgPool,
    user_id: Uuid,
    source_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET default_source_id = $2 WHERE id = $1")
        .bind(user_id)
        .bind(source_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Clears the default source, but only if it is this one (the item was deleted).
pub async fn clear_default_source_if(
    db: &PgPool,
    user_id: Uuid,
    source_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET default_source_id = NULL WHERE id = $1 AND default_source_id = $2",
    )
    .bind(user_id)
    .bind(source_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Makes this item the default source, but only if the user has none yet. One statement, so two
/// items added at the same moment cannot both become the default.
pub async fn set_default_source_if_none(
    db: &PgPool,
    user_id: Uuid,
    source_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET default_source_id = $2 WHERE id = $1 AND default_source_id IS NULL",
    )
    .bind(user_id)
    .bind(source_id)
    .execute(db)
    .await?;
    Ok(())
}

// ---- refresh tokens (step 24) ----

/// Stores a refresh token. `db` is the pool, or a transaction.
pub async fn insert_refresh_token(
    db: impl PgExecutor<'_>,
    new: &NewRefreshToken<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO refresh_tokens (id, user_id, device_id, token_hash, expires_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(new.id)
    .bind(new.user_id)
    .bind(new.device_id)
    .bind(new.token_hash)
    .bind(new.expires_at)
    .execute(db)
    .await?;
    Ok(())
}

/// Creates an anonymous user together with their first refresh token. One transaction: if the
/// token cannot be stored there is no user left behind that nobody can log in to.
pub async fn insert_anonymous_user(
    db: &PgPool,
    first_token: &NewRefreshToken<'_>,
) -> Result<User, sqlx::Error> {
    let mut tx = db.begin().await?;
    // The id is new and random, so the insert always returns the row.
    let user = insert_user(&mut *tx, first_token.user_id, Tier::Anonymous)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    insert_refresh_token(&mut *tx, first_token).await?;
    tx.commit().await?;
    Ok(user)
}

/// Creates an `account` user that owns this identity, together with their first refresh token: what
/// a first sign-in makes. One transaction. Returns false, and stores nothing, when the identity
/// already belongs to someone (another request created it a moment ago): the caller then logs in to
/// that account.
pub async fn create_account_with_identity(
    db: &PgPool,
    provider: Provider,
    subject: &str,
    first_token: &NewRefreshToken<'_>,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    insert_user(&mut *tx, first_token.user_id, Tier::Account)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let linked = sqlx::query(
        "INSERT INTO auth_identities (user_id, provider, subject) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(first_token.user_id)
    .bind(provider)
    .bind(subject)
    .execute(&mut *tx)
    .await?;
    if linked.rows_affected() != 1 {
        return Ok(false); // dropping the transaction rolls the new user back
    }
    insert_refresh_token(&mut *tx, first_token).await?;
    tx.commit().await?;
    Ok(true)
}

/// The refresh token with this hash, or None.
pub async fn find_refresh_token(
    db: &PgPool,
    token_hash: &str,
) -> Result<Option<RefreshTokenRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, user_id, device_id, expires_at, revoked_at FROM refresh_tokens WHERE token_hash = $1")
        .bind(token_hash)
        .fetch_optional(db)
        .await
}

/// Exchanges a refresh token: revokes the old one and stores the new one, in one transaction.
/// Returns false, and stores nothing, when the old one is revoked already (another request used
/// it a moment ago).
pub async fn rotate_refresh_token(
    db: &PgPool,
    old_id: Uuid,
    new: &NewRefreshToken<'_>,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    let revoked = sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = now(), revoked_reason = 'rotated' WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(old_id)
    .execute(&mut *tx)
    .await?;
    if revoked.rows_affected() != 1 {
        return Ok(false); // dropping the transaction rolls it back
    }
    insert_refresh_token(&mut *tx, new).await?;
    tx.commit().await?;
    Ok(true)
}

/// True when the device still has a refresh token that works (not used or revoked, not expired). After a
/// logout or after a revoked device it has none.
pub async fn device_has_working_token(
    db: &PgPool,
    user_id: Uuid,
    device_id: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM refresh_tokens
         WHERE user_id = $1 AND device_id = $2 AND revoked_at IS NULL AND expires_at > now())",
    )
    .bind(user_id)
    .bind(device_id)
    .fetch_one(db)
    .await
}

/// Notes that the user was seen now. At most once a minute, so a busy user is not one write per call.
pub async fn touch_user(db: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET last_seen_at = now()
         WHERE id = $1 AND (last_seen_at IS NULL OR last_seen_at < now() - interval '1 minute')",
    )
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Revokes every refresh token of this user on this device that still works. Returns how many.
pub async fn revoke_device_tokens(
    db: &PgPool,
    user_id: Uuid,
    device_id: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = now(), revoked_reason = 'token_reuse' WHERE user_id = $1 AND device_id = $2 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .bind(device_id)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

/// Deletes the refresh tokens whose time has passed (used or not). Returns how many.
pub async fn delete_expired_refresh_tokens(db: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM refresh_tokens WHERE expires_at <= now()")
        .execute(db)
        .await?;
    Ok(result.rows_affected())
}

/// Revokes every refresh token of this user that still works, on every device. Returns how many.
pub async fn revoke_user_tokens(db: &PgPool, user_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = now(), revoked_reason = 'logout' WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

// ---- phone codes and identities (step 26) ----

#[cfg(feature = "phone-login")]
/// Stores the code for a phone, replacing the one that was there, with no attempts used.
pub async fn upsert_otp(
    db: &PgPool,
    phone: &str,
    code_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO otp_codes (phone, code_hash, expires_at, attempts) VALUES ($1, $2, $3, 0)
         ON CONFLICT (phone) DO UPDATE SET code_hash = EXCLUDED.code_hash, expires_at = EXCLUDED.expires_at, attempts = 0",
    )
    .bind(phone)
    .bind(code_hash)
    .bind(expires_at)
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(feature = "phone-login")]
/// Counts one try of the code of this phone and gives its hash, if the code is still alive: not
/// expired and tried fewer than `max_attempts` times. One statement, so two tries at the same
/// moment cannot both get a try the code does not have. None: no code, expired, or used up.
pub async fn use_otp_attempt(
    db: &PgPool,
    phone: &str,
    max_attempts: i32,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE otp_codes SET attempts = attempts + 1
         WHERE phone = $1 AND expires_at > now() AND attempts < $2 RETURNING code_hash",
    )
    .bind(phone)
    .bind(max_attempts)
    .fetch_optional(db)
    .await
}

#[cfg(feature = "phone-login")]
/// Deletes the code after it was used right. False if it was already gone (another request used it).
pub async fn consume_otp(db: &PgPool, phone: &str, code_hash: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM otp_codes WHERE phone = $1 AND code_hash = $2")
        .bind(phone)
        .bind(code_hash)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}

#[cfg(feature = "phone-login")]
/// Deletes the codes whose time has passed (they hold phone numbers). Returns how many.
pub async fn delete_expired_otp_codes(db: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM otp_codes WHERE expires_at <= now()")
        .execute(db)
        .await?;
    Ok(result.rows_affected())
}

/// The user an identity belongs to, or None.
pub async fn find_identity_owner(
    db: &PgPool,
    provider: Provider,
    subject: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT user_id FROM auth_identities WHERE provider = $1 AND subject = $2")
        .bind(provider)
        .bind(subject)
        .fetch_optional(db)
        .await
}

/// The identities linked to a user, ordered by provider and then subject.
pub async fn list_identities(db: &PgPool, user_id: Uuid) -> Result<Vec<Identity>, sqlx::Error> {
    sqlx::query_as("SELECT provider, subject FROM auth_identities WHERE user_id = $1 ORDER BY provider, subject")
        .bind(user_id)
        .fetch_all(db)
        .await
}

/// Links an identity to a user. A new identity makes the user an `account`, in the same transaction.
/// An identity that is somebody else's changes nothing.
pub async fn link_identity(
    db: &PgPool,
    user_id: Uuid,
    provider: Provider,
    subject: &str,
) -> Result<Linked, sqlx::Error> {
    let mut tx = db.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO auth_identities (user_id, provider, subject) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(provider)
    .bind(subject)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 1 {
        sqlx::query("UPDATE users SET tier = $2 WHERE id = $1")
            .bind(user_id)
            .bind(Tier::Account)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(Linked::New);
    }
    let owner: Uuid = sqlx::query_scalar(
        "SELECT user_id FROM auth_identities WHERE provider = $1 AND subject = $2",
    )
    .bind(provider)
    .bind(subject)
    .fetch_one(&mut *tx)
    .await?;
    Ok(if owner == user_id {
        Linked::AlreadyTheirs
    } else {
        Linked::InUse
    })
}

/// The device of the newest login of this user, or None if the user has none (a test-mode user).
pub async fn latest_device_of(db: &PgPool, user_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT device_id FROM refresh_tokens WHERE user_id = $1 ORDER BY id DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
}
