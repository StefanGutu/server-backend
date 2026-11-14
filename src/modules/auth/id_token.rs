//! Checking the `idToken` of a Google or Apple sign-in (server-api.md: `POST /auth/link` and
//! `POST /auth/signin`). It answers one thing: who is this, as the provider says (the `subject`).
//!
//! A real check needs the provider's public keys fetched over the internet, the issuer and the
//! audience (our app id) of the token, and its end. That is not built: nothing here can be tried against
//! a real Google or Apple token. What exists is the place for it, and a verifier for tests:
//! - `Test` (test mode): accepts `test:<subject>` as the token of either provider.
//! - `Unconfigured` (any other mode): accepts nothing and says so in the log.
//!
//! A real verifier is one more variant of `IdTokenVerifier`.

use super::models::Provider;
use crate::error::AppError;

/// The longest subject a test token may name (characters).
const MAX_TEST_SUBJECT_LEN: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdTokenVerifier {
    /// Test mode: `test:<subject>` is a valid token. Anyone can make one, so never outside test mode.
    Test,
    /// No real verifier is set up: every token is refused.
    Unconfigured,
}

impl IdTokenVerifier {
    /// The verifier the server starts with: `Test` in test mode, otherwise `Unconfigured`.
    pub fn for_mode(test_mode: bool) -> Self {
        if test_mode {
            Self::Test
        } else {
            Self::Unconfigured
        }
    }

    /// The subject of a token: who the provider says it is. A token that does not check out is 401
    /// `AUTH_INVALID`. `Unconfigured` is a 500: the server is not set up for this login.
    pub async fn verify(&self, provider: Provider, id_token: &str) -> Result<String, AppError> {
        match self {
            Self::Test => test_subject(id_token)
                .ok_or_else(|| AppError::AuthRejected("The ID token is not valid.".to_string())),
            Self::Unconfigured => {
                tracing::error!(
                    ?provider,
                    "sign-in with an ID token, but no verifier is set up: the token was refused"
                );
                Err(AppError::Server)
            }
        }
    }
}

/// The subject of a test token `test:<subject>`: 1 to 100 letters, digits, `.`, `_`, `@` or `-`.
/// `None` for any other text.
pub fn test_subject(id_token: &str) -> Option<String> {
    let subject = id_token.trim().strip_prefix("test:")?;
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | '-');
    let well_formed = !subject.is_empty()
        && subject.chars().count() <= MAX_TEST_SUBJECT_LEN
        && subject.chars().all(allowed);
    well_formed.then(|| subject.to_string())
}
