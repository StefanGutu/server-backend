use serde_json::Value;
use uuid::Uuid;

/// The `Idempotency-Key` of a call, and what identifies the call itself.
#[derive(Debug, Clone)]
pub struct IdempotentRequest {
    /// The uuid from the header.
    pub key: Uuid,
    /// For example "POST /rides/<id>/confirm".
    pub endpoint: String,
    /// `request_hash` of the endpoint and the body.
    pub request_hash: String,
}

/// What to do with a call that has a key.
#[derive(Debug, PartialEq)]
pub enum Begin {
    /// The key is new: do the operation, then `finish` it.
    Proceed,
    /// The same call was done before: answer with the stored answer, do nothing.
    Replay(Value),
}
