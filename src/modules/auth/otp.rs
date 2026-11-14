//! The phone code login (server-api.md section 3). Pure: no database, no HTTP.
//!
//! The app asks for a code for a phone number (`POST /auth/otp/start`), the user types it, and the
//! app sends it with `POST /auth/link` or `POST /auth/signin`. There is no SMS provider yet: the
//! server writes the code in its log.

use sha2::{Digest, Sha256};

use crate::error::AppError;

/// How long a code works: 5 minutes.
pub const CODE_LIFE_SECS: i64 = 5 * 60;

/// How many times a code may be tried (right or wrong); then it is dead and a new one is needed.
pub const MAX_ATTEMPTS: i32 = 5;

/// Digits in a code.
const CODE_DIGITS: usize = 6;

/// A phone number has 8 to 15 digits (E.164 allows 15 at most).
const MIN_PHONE_DIGITS: usize = 8;
const MAX_PHONE_DIGITS: usize = 15;

/// What the app sent as a phone number, written the one way the server keeps it: `+` and the
/// digits, no spaces. Spaces, dashes, dots and brackets are dropped. The number must be international
/// (start with `+`, not `0`) and have 8 to 15 digits. Anything else is a 400.
pub fn normalize_phone(phone: &str) -> Result<String, AppError> {
    let invalid = || AppError::Validation("phone must be an international number such as +40712345678.".to_string());

    let trimmed = phone.trim();
    let digits = trimmed.strip_prefix('+').ok_or_else(invalid)?;
    let digits: String = digits.chars().filter(|c| !matches!(c, ' ' | '-' | '.' | '(' | ')')).collect();
    let well_formed = digits.chars().all(|c| c.is_ascii_digit())
        && !digits.starts_with('0')
        && (MIN_PHONE_DIGITS..=MAX_PHONE_DIGITS).contains(&digits.len());
    if well_formed { Ok(format!("+{digits}")) } else { Err(invalid()) }
}

/// A code typed by the user: trimmed, exactly 6 digits.
pub fn clean_code(code: &str) -> Result<&str, AppError> {
    let code = code.trim();
    if code.len() == CODE_DIGITS && code.bytes().all(|b| b.is_ascii_digit()) {
        Ok(code)
    } else {
        Err(AppError::Validation("code must be 6 digits.".to_string()))
    }
}

/// A new code: 6 digits from secure random bytes. Random numbers above the largest multiple of
/// 1,000,000 are thrown away, so every code is equally likely.
pub fn new_code() -> Result<String, AppError> {
    const RANGE: u32 = 1_000_000;
    const LIMIT: u32 = u32::MAX - (u32::MAX % RANGE);
    loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(|e| {
            tracing::error!("no secure random bytes for a code: {e}");
            AppError::Server
        })?;
        let number = u32::from_be_bytes(bytes);
        if number < LIMIT {
            return Ok(format!("{:0width$}", number % RANGE, width = CODE_DIGITS));
        }
    }
}

/// What the database keeps of a code: sha256 (hex) of the phone, a colon and the code. The phone is
/// part of it, so one code cannot be matched against another phone's row.
pub fn hash_code(phone: &str, code: &str) -> String {
    hex::encode(Sha256::digest(format!("{phone}:{code}").as_bytes()))
}
