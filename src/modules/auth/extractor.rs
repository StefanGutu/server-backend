use axum::{extract::FromRequestParts, http::{header::AUTHORIZATION, request::Parts}};

use super::{models::User, service};
use crate::{error::AppError, extract::header_str, state::AppState};

/// The logged-in user. Put it as a handler argument and the handler only runs for a known user.
///
/// - `Authorization: Bearer <accessToken>` (server-api.md): always accepted.
/// - `X-User-Id: <any text>`: only in test mode, and only when there is no `Authorization` header.
///
/// Anything else is 401 `AUTH_INVALID`. A request with a bad token is refused even in test mode:
/// it does not fall back to `X-User-Id`.
pub struct CurrentUser(pub User);

/// The user of a good `Authorization: Bearer` token, if the request has one. Unlike `CurrentUser` it never
/// refuses a request: no header, a bad or expired token, a deleted user all give `None`. For the calls that
/// need no login (sign in) but may use the caller's token as proof of who they are.
pub struct MaybeBearerUser(pub Option<User>);

impl FromRequestParts<AppState> for MaybeBearerUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let Some(header) = parts.headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()) else {
            return Ok(Self(None));
        };
        match service::user_from_authorization(&state.db, &state.config.jwt_secret, header).await {
            Ok(user) => Ok(Self(Some(user))),
            Err(AppError::AuthInvalid) => Ok(Self(None)),
            Err(other) => Err(other), // a database failure is still a failure
        }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        if parts.headers.contains_key(AUTHORIZATION) {
            let header = header_str(&parts.headers, AUTHORIZATION.as_str())?;
            let user = service::user_from_authorization(&state.db, &state.config.jwt_secret, header).await?;
            return Ok(Self(user));
        }
        if !state.config.test_mode {
            return Err(AppError::AuthInvalid);
        }
        let header = header_str(&parts.headers, "X-User-Id")?;
        let user = service::user_from_test_header(&state.db, header).await?;
        Ok(Self(user))
    }
}
