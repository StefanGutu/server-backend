use server_echopass::{app, config::Config, logging, state::AppState};
use sqlx::PgPool;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    logging::init();

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
