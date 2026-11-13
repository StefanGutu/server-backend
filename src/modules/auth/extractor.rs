use axum::{extract::FromRequestParts, http::request::Parts};

use super::{models::User, service};
use crate::{error::AppError, extract::header_str, state::AppState};

/// The logged-in user. Put it as a handler argument and the handler only runs
/// for a known user. Today only test mode works; real tokens come in step 24.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        if !state.config.test_mode {
            return Err(AppError::AuthInvalid);
        }
        let header = header_str(&parts.headers, "X-User-Id")?;

        let user = service::user_from_test_header(&state.db, header).await?;
        Ok(Self(user))
    }
}
