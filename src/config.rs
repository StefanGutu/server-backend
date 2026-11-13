/// Settings read once at start from environment variables (.env file).
#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    /// When true, the app may send `X-User-Id` instead of a token (server-api.md).
    pub test_mode: bool,
}

impl Config {
    /// Panics with a clear message if a setting is missing or wrong: better at start than later.
    pub fn from_env() -> Self {
        Self {
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .expect("PORT must be a number"),
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            test_mode: std::env::var("TEST_MODE").is_ok_and(|v| v == "true"),
        }
    }
}
