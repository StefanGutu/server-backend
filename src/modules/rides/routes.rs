use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::ETAG},
    response::{IntoResponse, Response},
    routing::get,
};

use super::{models::RideReply, service};
use crate::{
    error::AppError, extract::header_str, modules::auth::extractor::CurrentUser, state::AppState,
};

/// GET /rides/current: the ride of the logged-in user, with an `ETag`.
/// If `If-None-Match` names the current version: 304 with no body.
async fn get_current_ride(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // The header is optional: a missing one just means "send the ride".
    let if_none_match = header_str(&headers, "If-None-Match").ok();

    let response = match service::current_ride(&state.db, user.id, if_none_match).await? {
        RideReply::NotModified { etag } => (StatusCode::NOT_MODIFIED, [(ETAG, etag)]).into_response(),
        RideReply::Modified { ride, etag } => (StatusCode::OK, [(ETAG, etag)], Json(ride)).into_response(),
    };
    Ok(response)
}

pub fn router() -> Router<AppState> {
    Router::new().route("/rides/current", get(get_current_ride))
}
