//! The login tokens of the phone app (server-api.md section 3). Pure: no database, no HTTP.
//!
//! - Access token: a JWT signed with HS256 and the `JWT_SECRET` setting. Claims: `sub` (the user
//!   id), `iat` and `exp` (unix seconds). Sent as `Authorization: Bearer <accessToken>`.
//! - Refresh token: 32 random bytes as hex. The server keeps only its sha256.

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppError;

/// How long an access token works: 15 minutes. This is `expiresIn` (seconds) in the answer.
pub const ACCESS_TOKEN_SECS: i64 = 15 * 60;

/// How long a refresh token works: 30 days.
pub const REFRESH_TOKEN_SECS: i64 = 30 * 24 * 60 * 60;

/// The shortest `JWT_SECRET` the server starts with. HS256 is only as strong as its secret.
pub const MIN_SECRET_LEN: usize = 32;

/// Random bytes in a refresh token (64 hex characters).
const REFRESH_TOKEN_BYTES: usize = 32;

/// The longest `deviceId` that is accepted (characters).
const MAX_DEVICE_ID_LEN: usize = 200;

/// What the access token says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessClaims {
    /// The user.
    pub sub: Uuid,
    pub iat: i64,
    pub exp: i64,
}

impl AccessClaims {
    /// Claims for a token made at `now` (unix seconds): valid for `ACCESS_TOKEN_SECS`.
    pub fn new(user_id: Uuid, now: i64) -> Self {
        Self { sub: user_id, iat: now, exp: now + ACCESS_TOKEN_SECS }
    }
}

/// Signs an access token for this user.
pub fn sign_access(secret: &str, claims: &AccessClaims) -> Result<String, AppError> {
    let sign = || {
        let key = EncodingKey::from_secret(secret.as_bytes())?;
        encode(&Header::new(Algorithm::HS256), claims, &key)
    };
    sign().map_err(|e: jsonwebtoken::errors::Error| {
        tracing::error!("cannot sign an access token: {e}");
        AppError::Server
    })
}

/// The user id of an access token that was signed with `secret` and has not expired (`now` is
/// unix seconds). `None` for any other text: wrong signature, another algorithm, expired, a QR
/// ticket, not a token.
pub fn verify_access(secret: &str, token: &str, now: i64) -> Option<Uuid> {
    let key = DecodingKey::from_secret(secret.as_bytes()).ok()?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.validate_exp = false; // checked below against the time we are given
    validation.set_required_spec_claims(&["sub", "exp", "iat"]);
    let claims = decode::<AccessClaims>(token, &key, &validation).ok()?.claims;
    (claims.exp > now).then_some(claims.sub)
}

/// The token of an `Authorization` header: `Bearer <token>` (the word in any letter case).
/// `None` when the header has another shape.
pub fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.trim().split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("Bearer") && !token.is_empty()).then_some(token)
}

/// A new refresh token: 32 secure random bytes as 64 hex characters.
pub fn new_refresh_token() -> Result<String, AppError> {
    let mut bytes = [0u8; REFRESH_TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| {
        tracing::error!("no secure random bytes for a refresh token: {e}");
        AppError::Server
    })?;
    Ok(hex::encode(bytes))
}

/// What the database keeps of a refresh token: its sha256 as hex. Someone who reads the table
/// cannot log in with it.
pub fn hash_refresh_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// A `deviceId` is trimmed, must not be blank and has at most `MAX_DEVICE_ID_LEN` characters.
pub fn clean_device_id(device_id: &str) -> Result<&str, AppError> {
    let device_id = device_id.trim();
    if device_id.is_empty() {
        return Err(AppError::Validation("deviceId must not be empty.".to_string()));
    }
    if device_id.chars().count() > MAX_DEVICE_ID_LEN {
        return Err(AppError::Validation(format!("deviceId must have at most {MAX_DEVICE_ID_LEN} characters.")));
    }
    Ok(device_id)
}
