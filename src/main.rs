use std::{net::SocketAddr, sync::Arc, time::Duration};

use server_echopass::{
    app,
    config::Config,
    logging,
    modules::{
        auth::id_token::IdTokenVerifier, charges, extras::notifications::push::PushSender, frames,
        wallet::provider::FakeProvider,
    },
    rate_limit::RateLimiter,
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

    // The folder the photos of the buses go to.
    frames::service::prepare_photos_folder();

    // The key that signs the QR tickets is created once, at the first start.
    charges::service::ensure_ticket_key(&db)
        .await
        .expect("cannot create the ticket key");

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .expect("cannot bind the port");
    tracing::info!("server running on port {}", config.port);
    if config.test_mode {
        tracing::warn!(
            "TEST_MODE is on: X-User-Id is accepted when there is no Authorization header"
        );
    }

    let id_tokens = IdTokenVerifier::for_mode(config.test_mode);
    let push = PushSender::from_config(&config);
    // Expires and ends rides in the background, and asks pending rides to pay once they are 10 s old.
    // Counts the people in the photos, only when the model's address is set.
    match &config.headcount_url {
        Some(url) => {
            tracing::info!(url, "counting the people in the photos is on");
            tokio::spawn(tasks::run_headcount(db.clone(), url.clone()));
        }
        None => {
            tracing::info!("HEADCOUNT_URL is not set: the people in the photos are not counted")
        }
    }

    tokio::spawn(tasks::run(db.clone(), push.clone(), config.test_mode));
    let rate_limiter = Arc::new(RateLimiter::new(
        config.rate_limit_max,
        Duration::from_secs(config.rate_limit_window_secs),
    ));
    let state = AppState {
        db,
        config,
        payments: Arc::new(FakeProvider),
        id_tokens,
        push,
        rate_limiter,
    };
    // `ConnectInfo` gives the limit of step 34 the address of the caller.
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
