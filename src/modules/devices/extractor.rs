use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
    http::StatusCode,
};

use super::{models::Device, service};
use crate::{error::AppError, extract::header_str, state::AppState};

/// A request whose signature was checked. Put it as a handler argument and the
/// handler only runs for a real device. `body` is the raw body (empty for GET).
pub struct SignedRequest {
    pub device: Device,
    pub body: Bytes,
}

impl FromRequest<AppState> for SignedRequest {
    type Rejection = AppError;

    async fn from_request(req: Request, state: &AppState) -> Result<Self, AppError> {
        // Copy the headers first: reading the body below consumes the request.
        let device_id = header_str(req.headers(), "X-Device-Id")?.to_string();
        let timestamp = header_str(req.headers(), "X-Timestamp")?.to_string();
        let signature = header_str(req.headers(), "X-Signature")?.to_string();

        let body = Bytes::from_request(req, state).await.map_err(|rejection| {
            let message = if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
                "The body is too big."
            } else {
                "Cannot read the body."
            };
            AppError::Validation(message.to_string())
        })?;

        let device = service::authenticate(&state.db, &device_id, &timestamp, &signature, &body).await?;
        Ok(Self { device, body })
    }
}
