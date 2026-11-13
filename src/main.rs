mod config;
mod error;
mod modules;
mod state;

use axum::{Json, Router, routing::get};
use config::Config;
use serde_json::{Value, json};
use sqlx::PgPool;
use state::AppState;
use tower_http::trace::TraceLayer;

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// All routes, nested under /api/v1, with one log line per request.
fn app(state: AppState) -> Router {
    let api = Router::new().route("/health", get(health));
    Router::new()
        .nest("/api/v1", api)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    // Log level comes from RUST_LOG, default is "info".
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let config = Config::from_env();

    let db = PgPool::connect(&config.database_url)
        .await
        .expect("cannot connect to the database");
    tracing::info!("connected to the database");

    // Run every .sql file in migrations/ that was not applied yet.
    sqlx::migrate!().run(&db).await.expect("migrations failed");

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .expect("cannot bind the port");
    tracing::info!("server running on port {}", config.port);
    if config.test_mode {
        tracing::warn!("TEST_MODE is on: X-User-Id is accepted instead of tokens");
    }

    axum::serve(listener, app(AppState { db, config }))
        .await
        .unwrap();
}
