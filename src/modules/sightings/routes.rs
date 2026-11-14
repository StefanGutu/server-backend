use axum::{Json, Router, extract::State, http::StatusCode, response::Response, routing::post};
use serde_json::{Value, json};

use super::{
    models::{Outcome, REJECTED_REASON, Sighting},
    service,
};
use crate::{
    error::AppError,
    extract::ApiJson,
    modules::{auth::extractor::CurrentUser, rides::routes::ride_response},
    state::AppState,
};

/// POST /sightings: "this user heard this bus".
/// 200 `{ ok: true, ride_status }`, 422 `{ ok: false, reason }` (bodies from server-api.md).
async fn post_sighting(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(sighting): ApiJson<Sighting>, // must be last: it reads the body
) -> Result<(StatusCode, Json<Value>), AppError> {
    let reply = match service::handle(
        &state.db,
        &state.push,
        user.id,
        &sighting,
        state.config.test_mode,
    )
    .await?
    {
        Outcome::Accepted(status) => (StatusCode::OK, json!({ "ok": true, "ride_status": status })),
        Outcome::Rejected => (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({ "ok": false, "reason": REJECTED_REASON }),
        ),
    };
    Ok((reply.0, Json(reply.1)))
}

/// POST /rides/manual: "I am on the bus". The answer is the ride, like `GET /rides/current`. It lives
/// here because it starts from what the user heard, and `sightings` already calls `rides`.
async fn post_manual_ride(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Response, AppError> {
    Ok(ride_response(
        service::manual_ride(&state.db, &state.push, user.id).await?,
    ))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sightings", post(post_sighting))
        .route("/rides/manual", post(post_manual_ride))
}
