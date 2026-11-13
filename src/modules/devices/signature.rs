//! Pure functions for the device signature (server.md). No database, no HTTP.

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

/// Requests with a real timestamp must be this close to our clock (seconds).
pub const MAX_SKEW_SECS: i64 = 300;

/// HMAC-SHA256 over "<timestamp>.<device_id>.<sha256 hex of body>".
/// The key is the secret's ASCII bytes, not hex-decoded.
fn make_mac(secret: &str, timestamp: &str, device_id: &str, body: &[u8]) -> Hmac<Sha256> {
    let message = format!("{timestamp}.{device_id}.{}", hex::encode(Sha256::digest(body)));
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(message.as_bytes());
    mac
}

/// True when `signature_hex` is the right signature. The comparison is constant-time.
pub fn verify_signature(
    secret: &str,
    timestamp: &str,
    device_id: &str,
    body: &[u8],
    signature_hex: &str,
) -> bool {
    let Ok(signature) = hex::decode(signature_hex) else {
        return false;
    };
    make_mac(secret, timestamp, device_id, body)
        .verify_slice(&signature)
        .is_ok()
}

/// Timestamp 0 means "device clock not set yet" and is always accepted
/// (hackathon rule in server.md). Any other value must be near `now`.
pub fn timestamp_ok(timestamp: i64, now: i64) -> bool {
    timestamp == 0 || (now - timestamp).abs() <= MAX_SKEW_SECS
}
