use axum::{Json, Router, extract::State, http::StatusCode, routing::get};

use super::{extractor::CurrentUser, models::{Me, PatchMe}, service};
use crate::{error::AppError, extract::ApiJson, state::AppState};

/// GET /me: the logged-in user.
async fn get_me(CurrentUser(user): CurrentUser) -> Json<Me> {
    Json(Me::from(user))
}

/// PATCH /me: change name and/or settings, answer with the updated user.
async fn patch_me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(patch): ApiJson<PatchMe>, // must be last: it reads the body
) -> Result<Json<Me>, AppError> {
    let updated = service::update_me(&state.db, user.id, patch).await?;
    Ok(Json(Me::from(updated)))
}

/// DELETE /me: remove the account. 204, no body.
async fn delete_me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, AppError> {
    service::delete_me(&state.db, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new().route("/me", get(get_me).patch(patch_me).delete(delete_me))
}
