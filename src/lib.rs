//! The server as a library: main.rs only starts it, tests/ call into it.

pub mod config;
pub mod error;
pub mod extract;
pub mod logging;
pub mod modules;
pub mod state;
pub mod tasks;

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};
use error::AppError;
use state::AppState;
use tower_http::trace::TraceLayer;

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Unknown path or wrong method: the same JSON error as everything else.
async fn not_found() -> AppError {
    AppError::NotFound
}

/// All routes, nested under /api/v1, with one log line per request.
pub fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .merge(modules::auth::routes::router())
        .merge(modules::charges::routes::router())
        .merge(modules::devices::routes::router())
        .merge(modules::rides::routes::router())
        .merge(modules::sightings::routes::router())
        .merge(modules::telemetry::routes::router())
        .merge(modules::wallet::routes::router())
        .method_not_allowed_fallback(not_found);
    Router::new()
        .nest("/api/v1", api)
        .fallback(not_found)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
