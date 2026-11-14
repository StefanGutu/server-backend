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

/// Why a call has no display access, for the log. `None` when it has it. Never says the token.
fn refusal(parts: &Parts, state: &AppState) -> Option<&'static str> {
    let Some(configured) = state.config.display_token.as_deref() else {
        return Some(
            "DISPLAY_TOKEN is not set in .env (or the server was started before it was added): the display calls are off",
        );
    };
    let Some(header) = parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Some("there is no Authorization header (a browser does not send one)");
    };
    if bearer_token(header).is_none() {
        return Some("the Authorization header is not `Bearer <token>`");
    }
    if display_token_matches(configured, header) {
        None
    } else {
        Some("the token is not the DISPLAY_TOKEN of this server")
    }
}

fn has_display_token(parts: &Parts, state: &AppState) -> bool {
    refusal(parts, state).is_none()
}

impl FromRequestParts<AppState> for DisplayAccess {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        match refusal(parts, state) {
            None => Ok(Self),
            Some(reason) => {
                tracing::warn!(path = %parts.uri.path(), "display call refused: {reason}");
                Err(AppError::AuthInvalid)
            }
        }
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
