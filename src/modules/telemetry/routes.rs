use axum::{
    Json, Router,
    extract::State,
    routing::post,
};
use serde_json::{Value, json};

use super::service;
use crate::{
    error::AppError, extract::ApiPath, modules::devices::extractor::SignedRequest, state::AppState,
};

/// POST /buses/{bus_id}/telemetry: a position from Node A. Always 200 when accepted.
async fn post_telemetry(
    State(state): State<AppState>,
    ApiPath(bus_id): ApiPath<i32>,
    signed: SignedRequest, // must be last: it reads the body
) -> Result<Json<Value>, AppError> {
    service::handle(&state.db, &signed.device, bus_id, &signed.body).await?;
    Ok(Json(json!({ "ok": true })))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/buses/{bus_id}/telemetry", post(post_telemetry))
}
