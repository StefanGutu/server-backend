use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use sha2::{Digest, Sha256};

use crate::{
    error::AppError,
    modules::auth::{extractor::CurrentUser, token::bearer_token},
    state::AppState,
};

/// True when `authorization` is `Bearer <the configured display token>`. The comparison takes the same time
/// wherever the two texts differ. Pure.
pub fn display_token_matches(configured: &str, authorization: &str) -> bool {
    let Some(sent) = bearer_token(authorization) else {
        return false;
    };
    // Compare the hashes: they have the same length for any input, so the length of the token is not leaked either.
    let a = Sha256::digest(configured.as_bytes());
    let b = Sha256::digest(sent.as_bytes());
    a.iter()
        .zip(b.iter())
        .fold(0u8, |diff, (x, y)| diff | (x ^ y))
        == 0
}

/// The caller sent the display token. Put it as a handler argument and the handler only runs for it.
/// `DISPLAY_TOKEN` not set: every call is 401 (the display calls are off).
pub struct DisplayAccess;

fn has_display_token(parts: &Parts, state: &AppState) -> bool {
    let Some(configured) = state.config.display_token.as_deref() else {
        return false;
    };
    parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|header| display_token_matches(configured, header))
}

impl FromRequestParts<AppState> for DisplayAccess {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        if has_display_token(parts, state) {
            return Ok(Self);
        }
        tracing::warn!(path = %parts.uri.path(), "display call refused: no display token, or a wrong one");
        Err(AppError::AuthInvalid)
    }
}

/// For a call that the app (a user login) and the display (the display token) both make, such as `GET /reports`.
pub struct UserOrDisplay;

impl FromRequestParts<AppState> for UserOrDisplay {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        if has_display_token(parts, state) {
            return Ok(Self);
        }
        CurrentUser::from_request_parts(parts, state)
            .await
            .map(|_| Self)
    }
}
