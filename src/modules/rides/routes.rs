use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header::ETAG},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use uuid::Uuid;

use super::{
    models::{ConfirmInput, RideReply},
    service::{self, PayContext},
};
use crate::{
    error::AppError,
    extract::{ApiPath, header_str, parse_json_body},
    modules::{auth::extractor::CurrentUser, idempotency},
    state::AppState,
};

/// The answer of every call that returns the ride: 200 with an `ETag`, or 304 with no body.
pub fn ride_response(reply: RideReply) -> Response {
    match reply {
        RideReply::NotModified { etag } => {
            (StatusCode::NOT_MODIFIED, [(ETAG, etag)]).into_response()
        }
        RideReply::Modified { ride, etag } => {
            (StatusCode::OK, [(ETAG, etag)], Json(ride)).into_response()
        }
        RideReply::Stored(answer) => (StatusCode::OK, Json(answer)).into_response(),
    }
}

/// GET /rides/current: the ride of the logged-in user, with an `ETag`.
/// If `If-None-Match` names the current version: 304 with no body.
async fn get_current_ride(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // The header is optional: a missing one just means "send the ride".
    let if_none_match = header_str(&headers, "If-None-Match").ok();
    Ok(ride_response(
        service::current_ride(&state.db, user.id, if_none_match).await?,
    ))
}

/// POST /rides/{sessionId}/confirm: pay for the ride. The answer is the ride, like `GET /rides/current`.
/// The `Idempotency-Key` header is required: the same call sent again is not charged a second time.
async fn post_confirm(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(session_id): ApiPath<Uuid>,
    headers: HeaderMap,
    body: Bytes, // must be last: it reads the body
) -> Result<Response, AppError> {
    let input: ConfirmInput = parse_json_body(&body)?;
    let endpoint = format!("POST /rides/{session_id}/confirm");
    let request = idempotency::service::request_from_headers(&headers, &endpoint, &body)?;
    let pay = PayContext {
        provider: state.payments.as_ref(),
        push: &state.push,
        test_mode: state.config.test_mode,
    };
    let reply = service::confirm(&state.db, &pay, &user, session_id, &input, &request).await?;
    Ok(ride_response(reply))
}

/// POST /rides/{sessionId}/decline: "this is not my ride". The answer is the ride, now `rejected`.
async fn post_decline(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(session_id): ApiPath<Uuid>,
) -> Result<Response, AppError> {
    Ok(ride_response(
        service::decline(&state.db, &user, session_id).await?,
    ))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rides/current", get(get_current_ride))
        .route("/rides/{session_id}/decline", post(post_decline))
        .route("/rides/{session_id}/confirm", post(post_confirm))
}
