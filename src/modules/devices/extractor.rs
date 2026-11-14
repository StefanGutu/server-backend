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
        let headers = req.headers().clone();
        let header = |name: &str| {
            header_str(&headers, name)
                .map(str::to_string)
                .inspect_err(|_| {
                    tracing::warn!("device refused: the header {name} is missing or not text");
                })
        };
        let device_id = header("X-Device-Id")?;
        let timestamp = header("X-Timestamp")?;
        let signature = header("X-Signature")?;

        let body = Bytes::from_request(req, state).await.map_err(|rejection| {
            let message = if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
                "The body is too big."
            } else {
                "Cannot read the body."
            };
            AppError::Validation(message.to_string())
        })?;

        let device =
            service::authenticate(&state.db, &device_id, &timestamp, &signature, &body).await?;
        Ok(Self { device, body })
    }
}
