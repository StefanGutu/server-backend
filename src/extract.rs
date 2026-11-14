//! Helpers shared by every module's extractors and routes.

use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Request},
    http::{HeaderMap, request::Parts},
};
use serde::de::DeserializeOwned;

use crate::error::AppError;

/// Same as axum's `Path`, but a bad value (`/routes/abc`) gives our JSON error
/// (400 `VALIDATION_ERROR`) instead of plain text.
pub struct ApiPath<T>(pub T);

impl<T, S> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(|_| AppError::Validation("Invalid value in the path.".to_string()))
    }
}

/// Same as axum's `Json`, but a bad body gives our JSON error (400 `VALIDATION_ERROR`).
pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, AppError> {
        Json::<T>::from_request(req, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|e| AppError::Validation(format!("Invalid body: {e}")))
    }
}

/// Reads a header as text, or `AUTH_INVALID` if it is missing or not text.
pub fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, AppError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or(AppError::AuthInvalid)
}
