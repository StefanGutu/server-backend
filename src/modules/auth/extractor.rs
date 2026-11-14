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
