use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::HeaderMap,
    routing::post,
};

use super::{
    models::NextCapture,
    service::{self, MAX_FRAME_BYTES},
};
use crate::{
    error::AppError, extract::ApiPath, modules::devices::extractor::SignedRequest, state::AppState,
};

/// POST /buses/{bus_id}/frames: a photo from Node B. The body is the JPEG, signed like every device call.
/// Answer: `{ "next_capture_ms": 3000 }`.
async fn post_frame(
    State(state): State<AppState>,
    ApiPath(bus_id): ApiPath<i32>,
    headers: HeaderMap,
    signed: SignedRequest, // must be last: it reads the body
) -> Result<Json<NextCapture>, AppError> {
    let answer = service::store_frame(&state.db, &signed.device, bus_id, &headers, &signed.body).await?;
    Ok(Json(answer))
}

pub fn router() -> Router<AppState> {
    // A photo is bigger than a telemetry message: this route takes up to MAX_FRAME_BYTES.
    Router::new().route(
        "/buses/{bus_id}/frames",
        post(post_frame).layer(DefaultBodyLimit::max(MAX_FRAME_BYTES)),
    )
}
