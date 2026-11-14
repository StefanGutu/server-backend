use std::sync::Arc;

use server_echopass::{
    app,
    config::Config,
    logging,
    modules::{auth::id_token::IdTokenVerifier, charges, wallet::provider::FakeProvider},
    state::AppState,
    tasks,
};
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

    // The key that signs the QR tickets is created once, at the first start.
    charges::service::ensure_ticket_key(&db).await.expect("cannot create the ticket key");

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .expect("cannot bind the port");
    tracing::info!("server running on port {}", config.port);
    if config.test_mode {
        tracing::warn!("TEST_MODE is on: X-User-Id is accepted when there is no Authorization header");
    }

    // Expires and ends rides in the background.
    tokio::spawn(tasks::run(db.clone(), config.test_mode));

    let id_tokens = IdTokenVerifier::for_mode(config.test_mode);
    let state = AppState { db, config, payments: Arc::new(FakeProvider), id_tokens };
    axum::serve(listener, app(state))
        .await
        .unwrap();
}
