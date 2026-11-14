//! Helpers shared by every module's extractors and routes.

use axum::{
    body::Bytes,
    extract::{FromRequest, FromRequestParts, Path, Query, Request},
    http::{HeaderMap, request::Parts},
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serializer, de::DeserializeOwned};
use uuid::Uuid;

use crate::error::AppError;

/// Runs work that must not be cut in half (a payment: the card, the charge and the ride are three
/// steps) in its own task. If the phone gives up and the request is dropped, the task still runs to the
/// end, so a card is never charged without the ride being updated. A task that panics is a 500.
pub async fn run_to_the_end<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: std::future::Future<Output = Result<T, AppError>> + Send + 'static,
{
    match tokio::spawn(work).await {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("a request task did not finish: {e}");
            Err(AppError::Server)
        }
    }
}

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
        let body = Bytes::from_request(req, state)
            .await
            .map_err(|_| AppError::Validation("Cannot read the body.".to_string()))?;
        parse_json_body(&body).map(Self)
    }
}

/// A JSON body as a value, or our JSON error (400 `VALIDATION_ERROR`). For the calls that need the
/// raw bytes too (the `Idempotency-Key` hash).
pub fn parse_json_body<T: DeserializeOwned>(body: &[u8]) -> Result<T, AppError> {
    serde_json::from_slice(body).map_err(|e| AppError::Validation(format!("Invalid body: {e}")))
}

/// A header that the call must have, as text. Missing, or not text: 400 `VALIDATION_ERROR` naming the header.
/// (`header_str` is for the login headers and gives 401.)
pub fn required_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, AppError> {
    let value = headers
        .get(name)
        .ok_or_else(|| AppError::Validation(format!("The {name} header is required.")))?;
    value
        .to_str()
        .map_err(|_| AppError::Validation(format!("The {name} header must be text.")))
}

/// Reads a header as text, or `AUTH_INVALID` if it is missing or not text.
pub fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, AppError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .ok_or(AppError::AuthInvalid)
}

/// How money is written in JSON: lei (`RON`) as a number. The database keeps bani (1000 = 10 lei).
/// Use it on every amount that is sent: `#[serde(serialize_with = "ron_from_bani")]`.
pub fn ron_from_bani<S: Serializer>(bani: &i32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(f64::from(*bani) / 100.0)
}

/// How every date-and-time is written in JSON: whole seconds and a trailing `Z`, like the example
/// of the contract (`2026-11-04T10:15:02Z`). Use it on every `DateTime<Utc>` that is sent:
/// `#[serde(serialize_with = "utc_seconds")]`.
pub fn utc_seconds<S: Serializer>(time: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&time.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// Same as `utc_seconds` for a date that may be missing (`Option`). Use it with
/// `skip_serializing_if = "Option::is_none"` so a missing date is left out.
pub fn utc_seconds_opt<S: Serializer>(
    time: &Option<DateTime<Utc>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match time {
        Some(time) => utc_seconds(time, serializer),
        None => serializer.serialize_none(),
    }
}

/// Same as axum's `Query`, but a bad value (`?limit=abc`) gives our JSON error
/// (400 `VALIDATION_ERROR`) instead of plain text.
pub struct ApiQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|_| AppError::Validation("Invalid value in the query.".to_string()))
    }
}

/// How many items a list returns when `limit` is not given, and the most it may return
/// (server-api.md: `?limit=` default 50, maximum 100).
const DEFAULT_PAGE_SIZE: i64 = 50;
const MAX_PAGE_SIZE: i64 = 100;

/// The query of the lists (`/charges`, `/notifications`, `/trips/recent`): `?limit=` and
/// `?before=<id>`. The newest items come first; `before` is the id of the last item the app
/// already has, and the answer continues after it.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub before: Option<Uuid>,
}

impl ListQuery {
    /// `limit` or 50. More than 100 is cut to 100. Less than 1 is a 400.
    pub fn page_size(&self) -> Result<i64, AppError> {
        match self.limit {
            None => Ok(DEFAULT_PAGE_SIZE),
            Some(limit) if limit < 1 => {
                Err(AppError::Validation("limit must be 1 or more.".to_string()))
            }
            Some(limit) => Ok(limit.min(MAX_PAGE_SIZE)),
        }
    }
}
