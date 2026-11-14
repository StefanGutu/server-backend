use crate::modules::auth::token::MIN_SECRET_LEN;

/// Settings read once at start from environment variables (.env file).
#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    /// When true, the app may send `X-User-Id` instead of a token (server-api.md).
    pub test_mode: bool,
    /// The secret that signs the access tokens (HS256). Whoever knows it can log in as anyone.
    pub jwt_secret: String,
    /// True: pushes are really sent through Expo. False: a push is only written in the log.
    /// `EXPO_PUSH=true` or `false`; without it pushes are real except in test mode.
    pub expo_push: bool,
    /// Where pushes are posted (`EXPO_PUSH_URL`). Expo's address by default; another one is for tests.
    pub expo_push_url: String,
    /// Expo's access token (`EXPO_ACCESS_TOKEN`), needed only if the Expo project asks for one.
    pub expo_access_token: Option<String>,
    /// Login calls one IP address may make in the window (`RATE_LIMIT_AUTH_MAX`, default 20; 0 = no limit).
    pub rate_limit_max: usize,
    /// The window of that limit in seconds (`RATE_LIMIT_AUTH_WINDOW_SECS`, default 60).
    pub rate_limit_window_secs: u64,
    /// The secret of the display calls (`DISPLAY_TOKEN`, at least 32 characters): who sends it as
    /// `Authorization: Bearer ...` may read the buses, positions, photos and so on. None = those calls are off.
    pub display_token: Option<String>,
    /// Where the model that counts the people in a photo runs (`HEADCOUNT_URL`, for example
    /// `http://127.0.0.1:8000`, no trailing slash). None = counting is off and the server works as before.
    pub headcount_url: Option<String>,
}

/// The shortest `DISPLAY_TOKEN` the server starts with.
pub const MIN_DISPLAY_TOKEN_LEN: usize = 32;

/// Where Expo takes push messages.
const EXPO_PUSH_URL: &str = "https://exp.host/--/api/v2/push/send";

impl Config {
    /// Panics with a clear message if a setting is missing or wrong: better at start than later.
    pub fn from_env() -> Self {
        let test_mode = std::env::var("TEST_MODE").is_ok_and(|v| v == "true");
        Self {
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .expect("PORT must be a number"),
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            test_mode,
            jwt_secret: jwt_secret(std::env::var("JWT_SECRET").expect("JWT_SECRET must be set")),
            expo_push: std::env::var("EXPO_PUSH").map_or(!test_mode, |v| v == "true"),
            expo_push_url: std::env::var("EXPO_PUSH_URL")
                .unwrap_or_else(|_| EXPO_PUSH_URL.to_string()),
            expo_access_token: std::env::var("EXPO_ACCESS_TOKEN")
                .ok()
                .filter(|t| !t.is_empty()),
            rate_limit_max: number_setting("RATE_LIMIT_AUTH_MAX", 20),
            rate_limit_window_secs: number_setting("RATE_LIMIT_AUTH_WINDOW_SECS", 60),
            display_token: display_token(std::env::var("DISPLAY_TOKEN").ok()),
            headcount_url: parse_headcount_url(std::env::var("HEADCOUNT_URL").ok())
                .unwrap_or_else(|message| panic!("{message}")),
        }
    }
}

/// `HEADCOUNT_URL`: empty or missing means counting is off. Otherwise it must be an `http://` or `https://` address;
/// a trailing `/` is dropped so `{url}/count` is always right. Anything else is an error message (the server
/// refuses to start with it, better at start than at the first photo). Pure.
pub fn parse_headcount_url(raw: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = raw.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()) else {
        return Ok(None);
    };
    if !(raw.starts_with("http://") || raw.starts_with("https://")) {
        return Err(format!(
            "HEADCOUNT_URL must start with http:// or https:// (for example http://127.0.0.1:8000), not {raw}"
        ));
    }
    Ok(Some(raw.trim_end_matches('/').to_string()))
}

/// `DISPLAY_TOKEN`: empty or missing means the display calls are off. A short one is refused at start: it can be guessed.
fn display_token(token: Option<String>) -> Option<String> {
    let token = token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())?;
    assert!(
        token.len() >= MIN_DISPLAY_TOKEN_LEN,
        "DISPLAY_TOKEN must have at least {MIN_DISPLAY_TOKEN_LEN} characters (make one with: openssl rand -hex 32)"
    );
    Some(token)
}

/// A whole-number setting with a default. Panics when it is set to something else than a number.
fn number_setting<T: std::str::FromStr>(name: &str, default: T) -> T {
    match std::env::var(name) {
        Ok(text) => text
            .parse()
            .unwrap_or_else(|_| panic!("{name} must be a whole number")),
        Err(_) => default,
    }
}

/// A short secret can be guessed, and then anyone can make tokens: refuse to start with one.
fn jwt_secret(secret: String) -> String {
    assert!(
        !secret.starts_with("change-me"),
        "JWT_SECRET is still the example value from .env.example: everyone can read it. Make one with: openssl rand -hex 32"
    );
    assert!(
        secret.len() >= MIN_SECRET_LEN,
        "JWT_SECRET must have at least {MIN_SECRET_LEN} characters (make one with: openssl rand -hex 32)"
    );
    secret
}
