use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use serde_json::{Value, json};

use super::{
    models::{Outcome, REJECTED_REASON, Sighting},
    service,
};
use crate::{
    error::AppError, extract::ApiJson, modules::auth::extractor::CurrentUser, state::AppState,
};

/// POST /sightings: "this user heard this bus".
/// 200 `{ ok: true, ride_status }`, 422 `{ ok: false, reason }` (bodies from server-api.md).
async fn post_sighting(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(sighting): ApiJson<Sighting>, // must be last: it reads the body
) -> Result<(StatusCode, Json<Value>), AppError> {
    let reply = match service::handle(&state.db, user.id, &sighting).await? {
        Outcome::Accepted(status) => (StatusCode::OK, json!({ "ok": true, "ride_status": status })),
        Outcome::Rejected => (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({ "ok": false, "reason": REJECTED_REASON }),
        ),
    };
    Ok((reply.0, Json(reply.1)))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/sightings", post(post_sighting))
}
