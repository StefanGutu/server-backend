use std::sync::Arc;

use sqlx::PgPool;

use crate::{config::Config, modules::wallet::provider::PaymentProvider};

/// Shared data given to every request handler.
/// Cloning is cheap: PgPool is a handle to a shared pool.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Config,
    /// The payment provider (a fake one for now).
    pub payments: Arc<dyn PaymentProvider>,
}
