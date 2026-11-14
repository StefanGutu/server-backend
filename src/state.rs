use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    config::Config,
    modules::{
        auth::id_token::IdTokenVerifier, extras::notifications::push::PushSender,
        wallet::provider::PaymentProvider,
    },
    rate_limit::RateLimiter,
};

/// Shared data given to every request handler.
/// Cloning is cheap: PgPool is a handle to a shared pool.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Config,
    /// The payment provider (a fake one for now).
    pub payments: Arc<dyn PaymentProvider>,
    /// Checks the `idToken` of a Google or Apple sign-in (a test verifier in test mode).
    pub id_tokens: IdTokenVerifier,
    /// Sends pushes to the phones of the users (the fake one only writes them in the log).
    pub push: PushSender,
    /// Counts the login calls of each IP address (step 34).
    pub rate_limiter: Arc<RateLimiter>,
}
