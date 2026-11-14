//! Rules for who the caller is.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[cfg(feature = "phone-login")]
use super::otp;
use super::{
    id_token::IdTokenVerifier,
    models::{
        Identity, LinkInput, Linked, LoginReply, Me, NewRefreshToken, PatchMe, Provider,
        RefreshTokenRow, RefreshTokenState, SigninInput, Tier, TokenReply, User, UserRole,
    },
    repo, token,
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

/// The user behind an `Authorization: Bearer <accessToken>` header. `AUTH_INVALID` when the header
/// has another shape, the token is not ours or has expired, or the user was deleted.
pub async fn user_from_authorization(
    db: &PgPool,
    jwt_secret: &str,
    header: &str,
) -> Result<User, AppError> {
    let access_token = token::bearer_token(header).ok_or(AppError::AuthInvalid)?;
    let user_id = token::verify_access(jwt_secret, access_token, Utc::now().timestamp())
        .ok_or(AppError::AuthInvalid)?;
    repo::find_user(db, user_id)
        .await?
        .ok_or(AppError::AuthInvalid)
}

/// A new pair of tokens for this user: the reply for the app, and the hash and the end of the
/// refresh token for the database. Pure apart from the random bytes.
fn new_tokens(
    jwt_secret: &str,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(TokenReply, String, DateTime<Utc>), AppError> {
    let refresh_token = token::new_refresh_token()?;
    let token_hash = token::hash_refresh_token(&refresh_token);
    let access_token = token::sign_access(
        jwt_secret,
        &token::AccessClaims::new(user_id, now.timestamp()),
    )?;
    let reply = TokenReply {
        user_id,
        access_token,
        refresh_token,
        expires_in: token::ACCESS_TOKEN_SECS,
    };
    Ok((
        reply,
        token_hash,
        now + Duration::seconds(token::REFRESH_TOKEN_SECS),
    ))
}

/// POST /auth/anonymous: a new anonymous user (tier `anonymous`) with their first tokens. Every
/// call makes a new user: `deviceId` is not a secret, so it never gives access to an existing
/// user. It only says which phone the login belongs to.
pub async fn login_anonymous(
    db: &PgPool,
    jwt_secret: &str,
    device_id: &str,
) -> Result<TokenReply, AppError> {
    let device_id = token::clean_device_id(device_id)?;
    let user_id = Uuid::new_v4();
    let (reply, token_hash, expires_at) = new_tokens(jwt_secret, user_id, Utc::now())?;

    let first_token = NewRefreshToken {
        id: Uuid::now_v7(),
        user_id,
        device_id,
        token_hash: &token_hash,
        expires_at,
    };
    repo::insert_anonymous_user(db, &first_token).await?;
    tracing::info!(%user_id, "anonymous user created");
    Ok(reply)
}

/// What a stored refresh token is good for at `now`. Pure. A token that was used is `Revoked`
/// even when its time has also passed.
pub fn refresh_token_state(row: &RefreshTokenRow, now: DateTime<Utc>) -> RefreshTokenState {
    if row.revoked_at.is_some() {
        RefreshTokenState::Revoked
    } else if row.expires_at <= now {
        RefreshTokenState::Expired
    } else {
        RefreshTokenState::Usable
    }
}

/// A refresh token that was already used came back: the app never does that, so the token was
/// probably copied. Every login of this user on this device stops working.
async fn revoke_device(db: &PgPool, row: &RefreshTokenRow) -> Result<(), AppError> {
    let revoked = repo::revoke_device_tokens(db, row.user_id, &row.device_id).await?;
    tracing::warn!(user_id = %row.user_id, revoked, "a used refresh token was sent again: the logins of the device are revoked");
    Ok(())
}

/// POST /auth/refresh: exchanges a refresh token for a new access token and a new refresh token.
/// The old refresh token stops working (it is used once). `AUTH_INVALID` for a token that is
/// unknown, expired or already used; an already used one also revokes the logins of its device.
pub async fn refresh(
    db: &PgPool,
    jwt_secret: &str,
    refresh_token: &str,
) -> Result<TokenReply, AppError> {
    let old_hash = token::hash_refresh_token(refresh_token.trim());
    let row = repo::find_refresh_token(db, &old_hash)
        .await?
        .ok_or(AppError::AuthInvalid)?;

    let now = Utc::now();
    match refresh_token_state(&row, now) {
        RefreshTokenState::Usable => {}
        RefreshTokenState::Expired => return Err(AppError::AuthInvalid),
        RefreshTokenState::Revoked => {
            revoke_device(db, &row).await?;
            return Err(AppError::AuthInvalid);
        }
    }

    let (reply, token_hash, expires_at) = new_tokens(jwt_secret, row.user_id, now)?;
    let next_token = NewRefreshToken {
        id: Uuid::now_v7(),
        user_id: row.user_id,
        device_id: &row.device_id,
        token_hash: &token_hash,
        expires_at,
    };
    if !repo::rotate_refresh_token(db, row.id, &next_token).await? {
        // Another request used the same token between our read and our write.
        revoke_device(db, &row).await?;
        return Err(AppError::AuthInvalid);
    }
    Ok(reply)
}

/// Background clean-up: forgets the refresh tokens whose time has passed. Returns how many.
pub async fn delete_expired_refresh_tokens(db: &PgPool) -> Result<u64, AppError> {
    Ok(repo::delete_expired_refresh_tokens(db).await?)
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
pub async fn set_default_source(
    db: &PgPool,
    user_id: Uuid,
    source_id: Uuid,
) -> Result<(), AppError> {
    Ok(repo::set_default_source(db, user_id, Some(source_id)).await?)
}

/// A wallet item was deleted: if it was the default way to pay, there is no default any more.
pub async fn clear_default_source_if(
    db: &PgPool,
    user_id: Uuid,
    source_id: Uuid,
) -> Result<(), AppError> {
    Ok(repo::clear_default_source_if(db, user_id, source_id).await?)
}

/// A new wallet item becomes the default way to pay if the user has no default yet.
pub async fn set_default_source_if_none(
    db: &PgPool,
    user_id: Uuid,
    source_id: Uuid,
) -> Result<(), AppError> {
    Ok(repo::set_default_source_if_none(db, user_id, source_id).await?)
}

/// Only an inspector may check tickets: anyone else is 403 `FORBIDDEN`. The role is set by hand
/// (`users.role`); nothing in the app gives it.
pub fn require_inspector(user: &User) -> Result<(), AppError> {
    if user.role == UserRole::Inspector {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// POST /auth/logout: the refresh tokens of the user stop working, so the app cannot get new access
/// tokens. An **anonymous** user is deleted instead (see `abandon_anonymous_user`): without a login there is
/// no way back to them. The call has no body and the access token names no device, so it ends every login of the
/// user (all phones). Calling it again is fine. The access token in use keeps working until its 15
/// minutes pass (doc/reminders.md R36).
pub async fn logout(db: &PgPool, user: &User) -> Result<(), AppError> {
    if user.tier == Tier::Anonymous {
        // Their login was their only way in: nobody can reach this user again. Delete them.
        abandon_anonymous_user(db, user.id, "logged out").await;
        return Ok(());
    }
    let revoked = repo::revoke_user_tokens(db, user.id).await?;
    tracing::info!(user_id = %user.id, revoked, "logout: the refresh tokens of the user are revoked");
    Ok(())
}

/// Paying needs an account: an `anonymous` user (device login only) gets 403 `ACCOUNT_REQUIRED`
/// (server-api.md). Pure.
pub fn require_account(user: &User) -> Result<(), AppError> {
    match user.tier {
        Tier::Account => Ok(()),
        Tier::Anonymous => Err(AppError::AccountRequired),
    }
}

/// What the app gets for `GET /me`: the user and the identities linked to them.
pub async fn me(db: &PgPool, user: User) -> Result<Me, AppError> {
    let identities: Vec<Identity> = repo::list_identities(db, user.id).await?;
    Ok(Me::new(user, identities))
}

/// Used when a login has no device to name (the user has no earlier login: a test-mode user).
const UNKNOWN_DEVICE: &str = "unknown-device";

// ---- the phone code login: behind the cargo feature `phone-login` (off: no SMS provider) ----

/// What a wrong, expired or used-up code looks like to the app: one answer for all three, so it does
/// not tell which. A 401 `AUTH_INVALID` (Stefan, 2026-11-14: a wrong code is a critical failure).
#[cfg(feature = "phone-login")]
fn code_error() -> AppError {
    AppError::AuthRejected("The code is wrong or has expired.".to_string())
}

/// POST /auth/otp/start: makes a code for the phone and keeps its hash. There is no SMS provider
/// yet, so the code is written in the log. Asking again replaces the code.
#[cfg(feature = "phone-login")]
pub async fn start_otp(db: &PgPool, phone: &str) -> Result<(), AppError> {
    let phone = otp::normalize_phone(phone)?;
    let code = otp::new_code()?;
    let expires_at = Utc::now() + Duration::seconds(otp::CODE_LIFE_SECS);
    repo::upsert_otp(db, &phone, &otp::hash_code(&phone, &code), expires_at).await?;
    tracing::info!(%phone, %code, "phone code made (no SMS is sent: this line is the message)");
    Ok(())
}

/// Checks the code of a phone and uses it up. Every try counts, right or wrong; after
/// `otp::MAX_ATTEMPTS` tries, or after the time, the code is dead. A right code works once.
#[cfg(feature = "phone-login")]
async fn verify_code(db: &PgPool, phone: &str, code: &str) -> Result<(), AppError> {
    let code = otp::clean_code(code)?;
    let stored = repo::use_otp_attempt(db, phone, otp::MAX_ATTEMPTS)
        .await?
        .ok_or_else(code_error)?;
    if stored != otp::hash_code(phone, code) {
        tracing::info!(%phone, "wrong phone code");
        return Err(code_error());
    }
    // Used once: a second request with the same code finds it gone.
    if !repo::consume_otp(db, phone, &stored).await? {
        return Err(code_error());
    }
    Ok(())
}

/// The phone number the caller proved to own (the subject of a `phone` identity).
#[cfg(feature = "phone-login")]
async fn phone_subject(
    db: &PgPool,
    phone: Option<&str>,
    code: Option<&str>,
) -> Result<String, AppError> {
    let phone = phone.ok_or_else(|| AppError::Validation("phone is required.".to_string()))?;
    let code = code.ok_or_else(|| AppError::Validation("code is required.".to_string()))?;
    let phone = otp::normalize_phone(phone)?;
    verify_code(db, &phone, code).await?;
    Ok(phone)
}

/// Login with a phone is switched off (no SMS provider): `provider` `phone` is refused.
#[cfg(not(feature = "phone-login"))]
async fn phone_subject(
    _db: &PgPool,
    _phone: Option<&str>,
    _code: Option<&str>,
) -> Result<String, AppError> {
    Err(AppError::Validation(
        "Login with a phone number is switched off.".to_string(),
    ))
}

// ---- link and sign in, for every provider ----

/// Who the caller proves to be, as the provider says: the `subject` of the identity. Google and
/// Apple: the `idToken` is checked by the verifier. Phone: the code (only with the feature `phone-login`).
async fn proven_subject(
    db: &PgPool,
    verifier: &IdTokenVerifier,
    provider: Provider,
    id_token: Option<&str>,
    phone: Option<&str>,
    code: Option<&str>,
) -> Result<String, AppError> {
    match provider {
        Provider::Google | Provider::Apple => {
            let id_token =
                id_token.ok_or_else(|| AppError::Validation("idToken is required.".to_string()))?;
            verifier.verify(provider, id_token).await
        }
        Provider::Phone => phone_subject(db, phone, code).await,
    }
}

/// New tokens for a user on a device, stored, in the shape of link and sign-in: `userId`, `tier`,
/// `accessToken`, `refreshToken`.
async fn issue_login(
    db: &PgPool,
    jwt_secret: &str,
    user_id: Uuid,
    tier: Tier,
    device_id: &str,
) -> Result<LoginReply, AppError> {
    let (reply, token_hash, expires_at) = new_tokens(jwt_secret, user_id, Utc::now())?;
    let new = NewRefreshToken {
        id: Uuid::now_v7(),
        user_id,
        device_id,
        token_hash: &token_hash,
        expires_at,
    };
    repo::insert_refresh_token(db, &new).await?;
    Ok(LoginReply {
        user_id,
        tier,
        access_token: reply.access_token,
        refresh_token: reply.refresh_token,
    })
}

/// POST /auth/link: the logged-in user proves who they are at a provider (an `idToken` from Google or
/// Apple), and that identity becomes theirs: they are an `account` from now on (and can pay). The `userId`
/// stays the same, with new tokens. An identity that belongs to another user is 409 `IDENTITY_IN_USE`.
/// The same call again is fine. The new refresh token is for the device of the user's newest login.
pub async fn link(
    db: &PgPool,
    jwt_secret: &str,
    verifier: &IdTokenVerifier,
    user: &User,
    input: &LinkInput,
) -> Result<LoginReply, AppError> {
    let subject = proven_subject(
        db,
        verifier,
        input.provider,
        input.id_token.as_deref(),
        input.phone.as_deref(),
        input.code.as_deref(),
    )
    .await?;

    match repo::link_identity(db, user.id, input.provider, &subject).await? {
        Linked::InUse => return Err(AppError::IdentityInUse),
        Linked::New => {
            tracing::info!(user_id = %user.id, provider = ?input.provider, "identity linked: the user is an account now")
        }
        Linked::AlreadyTheirs => {}
    }
    let device = repo::latest_device_of(db, user.id).await?;
    issue_login(
        db,
        jwt_secret,
        user.id,
        Tier::Account,
        device.as_deref().unwrap_or(UNKNOWN_DEVICE),
    )
    .await
}

/// Logs in to the account that owns this identity, on this device. `None` when nobody owns it.
async fn login_to_owner(
    db: &PgPool,
    jwt_secret: &str,
    provider: Provider,
    subject: &str,
    device_id: &str,
) -> Result<Option<LoginReply>, AppError> {
    let Some(owner) = repo::find_identity_owner(db, provider, subject).await? else {
        return Ok(None);
    };
    let user = repo::find_user(db, owner)
        .await?
        .ok_or(AppError::NotFound)?;
    tracing::info!(user_id = %user.id, ?provider, "signed in with an identity");
    Ok(Some(
        issue_login(db, jwt_secret, user.id, user.tier, device_id).await?,
    ))
}

/// The anonymous user who is left behind by a sign-in, if there is one: the caller, when they proved who
/// they are (a good `Bearer` token), are `anonymous`, and are not the user they signed in as. An account is
/// never abandoned. Pure.
pub fn abandoned_anonymous(caller: Option<&User>, signed_in_as: Uuid) -> Option<Uuid> {
    caller
        .filter(|user| user.tier == Tier::Anonymous && user.id != signed_in_as)
        .map(|user| user.id)
}

/// An anonymous user nobody can reach any more: the user and everything they had (cards, passes, rides,
/// sightings, charges, logins) are deleted, and a warning is written in the log. A failure is logged and
/// does not fail the call that led here: the person is already signed in or out.
async fn abandon_anonymous_user(db: &PgPool, user_id: Uuid, why: &str) {
    match repo::delete_user(db, user_id).await {
        Ok(true) => {
            tracing::warn!(%user_id, why, "anonymous user abandoned: the user and everything they had were deleted")
        }
        Ok(false) => {}
        Err(e) => tracing::error!(%user_id, why, "cannot delete an abandoned anonymous user: {e}"),
    }
}

/// POST /auth/signin: logs in with a Google or Apple identity, on this device. No login needed.
/// - The identity already has an account: the app gets that account's tokens. **Nothing is moved**:
///   what the anonymous user of this phone had (cards, passes, rides) is abandoned and **deleted**
///   (with a warning in the log), but only when the call carries that anonymous user's own `Bearer`
///   token (`caller`): a `deviceId` is not a secret and must not be enough to delete someone's data.
///   Data is only carried when an account is made with `link`.
/// - The identity has no account: **one is made** (tier `account`, empty), so a person who never used
///   the app can sign in straight away. Two calls at the same moment make one account.
pub async fn signin(
    db: &PgPool,
    jwt_secret: &str,
    verifier: &IdTokenVerifier,
    input: &SigninInput,
    caller: Option<&User>,
) -> Result<LoginReply, AppError> {
    let reply = signin_identity(db, jwt_secret, verifier, input).await?;
    match abandoned_anonymous(caller, reply.user_id) {
        Some(anonymous_id) => {
            abandon_anonymous_user(db, anonymous_id, "signed in to an account").await
        }
        None if caller.is_none() => tracing::info!(
            user_id = %reply.user_id,
            "signed in without a login: an anonymous user of this phone, if there is one, is not deleted (nothing proves it is theirs)"
        ),
        None => {}
    }
    Ok(reply)
}

/// The sign-in itself: the tokens of the account that owns the identity, or of a new one.
async fn signin_identity(
    db: &PgPool,
    jwt_secret: &str,
    verifier: &IdTokenVerifier,
    input: &SigninInput,
) -> Result<LoginReply, AppError> {
    let device_id = token::clean_device_id(&input.device_id)?;
    let subject = proven_subject(
        db,
        verifier,
        input.provider,
        input.id_token.as_deref(),
        input.phone.as_deref(),
        input.code.as_deref(),
    )
    .await?;

    if let Some(reply) = login_to_owner(db, jwt_secret, input.provider, &subject, device_id).await?
    {
        return Ok(reply);
    }

    // Nobody owns this identity yet: make the account, with its first tokens.
    let user_id = Uuid::new_v4();
    let (reply, token_hash, expires_at) = new_tokens(jwt_secret, user_id, Utc::now())?;
    let first_token = NewRefreshToken {
        id: Uuid::now_v7(),
        user_id,
        device_id,
        token_hash: &token_hash,
        expires_at,
    };
    if repo::create_account_with_identity(db, input.provider, &subject, &first_token).await? {
        tracing::info!(%user_id, provider = ?input.provider, "account made by a first sign-in");
        return Ok(LoginReply {
            user_id,
            tier: Tier::Account,
            access_token: reply.access_token,
            refresh_token: reply.refresh_token,
        });
    }
    // Another request made it between our look and our write: log in to that one.
    login_to_owner(db, jwt_secret, input.provider, &subject, device_id)
        .await?
        .ok_or(AppError::Server)
}

/// Background clean-up: forgets the phone codes whose time has passed. Returns how many.
#[cfg(feature = "phone-login")]
pub async fn delete_expired_otp_codes(db: &PgPool) -> Result<u64, AppError> {
    Ok(repo::delete_expired_otp_codes(db).await?)
}
