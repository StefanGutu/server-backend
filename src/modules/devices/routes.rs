use axum::{Json, Router, extract::State, routing::get};

use super::{extractor::SignedRequest, models::Route, service};
use crate::{error::AppError, extract::ApiPath, state::AppState};

/// GET /routes/{route_id}: the line a bus is configured to run. Signed by the device.
async fn get_route(
    State(state): State<AppState>,
    ApiPath(route_id): ApiPath<i32>,
    _signed: SignedRequest, // must be last: it reads the body
) -> Result<Json<Route>, AppError> {
    Ok(Json(service::get_route(&state.db, route_id).await?))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/routes/{route_id}", get(get_route))
}
