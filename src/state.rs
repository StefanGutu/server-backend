use sqlx::PgPool;

use crate::config::Config;

/// Shared data given to every request handler.
/// Cloning is cheap: PgPool is a handle to a shared pool.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Config,
}
