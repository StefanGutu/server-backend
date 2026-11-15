//! The call to the model that counts people (`site/headcount/headcount.py`).
//!
//! The request is the one found by hand in step 35: `POST {url}/count`, `Content-Type: image/jpeg`, the bytes of the
//! photo exactly as they are as the body. No multipart (the model answers 500), no base64, no JSON around the bytes.
//! The answer `200 {"count": n}` is a result; anything else is a failure. `parse_answer` is pure.

use std::time::Duration;

use base64::Engine;
use serde_json::Value;

/// How long the model has to answer. A photo takes about 0.15 s; a stopped or stuck model must not hold the server.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a photo was not counted. Only logged (the camera and the app never see it), so it is not an `AppError`.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CountError {
    /// The model answered with a status that is not 200. `body` holds its `detail` message.
    #[error("the model answered {status}: {body}")]
    Status { status: u16, body: String },
    /// A 200 whose body is not `{ "count": <whole number, 0 or more> }`.
    #[error("the model answered 200 but not with a count: {0}")]
    BadAnswer(String),
    /// No answer in time.
    #[error("the model did not answer in time")]
    Timeout,
    /// The model could not be reached (stopped, wrong address, network).
    #[error("the model cannot be reached: {0}")]
    Unreachable(String),
}

/// The count of an answer. `200` with a whole number `count` of 0 or more is a result (`0` is an empty cabin, not an
/// error). Any other status carries the status and the body. Pure.
pub fn parse_answer(status: u16, body: &str) -> Result<i32, CountError> {
    if status != 200 {
        return Err(CountError::Status {
            status,
            body: body.trim().to_string(),
        });
    }
    let bad = || CountError::BadAnswer(body.trim().chars().take(200).collect());
    let value: Value = serde_json::from_str(body).map_err(|_| bad())?;
    let count = value.get("count").and_then(Value::as_i64).ok_or_else(bad)?;
    i32::try_from(count)
        .ok()
        .filter(|n| *n >= 0)
        .ok_or_else(bad)
}

/// Talks to the model. Cheap to clone: it holds the HTTP client and the address.
#[derive(Clone)]
pub struct HeadcountClient {
    http: reqwest::Client,
    url: String,
    timeout: Duration,
}

impl HeadcountClient {
    /// `url` is `HEADCOUNT_URL` (no trailing slash, see `config::parse_headcount_url`).
    pub fn new(url: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            url: url.to_string(),
            timeout: REQUEST_TIMEOUT,
        }
    }

    /// The same client with another time limit (the tests use a short one).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sends the bytes of a photo and returns how many people the model sees.
    pub async fn count_people(&self, jpeg: Vec<u8>) -> Result<i32, CountError> {
        let response = self
            .http
            .post(format!("{}/count?image=0", self.url))
            .header("Content-Type", "image/jpeg")
            .timeout(self.timeout)
            .body(jpeg)
            .send()
            .await
            .map_err(failure)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(failure)?;
        parse_answer(status, &body)
    }
}

impl HeadcountClient {
    /// Sends the bytes of a photo to `POST {url}/count` (the picture comes along by default) and returns the picture the
    /// model draws (a JPEG). A `200` whose `image` is the base64 of a JPEG is a result; anything else is a failure.
    pub async fn annotate(&self, jpeg: Vec<u8>) -> Result<Vec<u8>, CountError> {
        let response = self
            .http
            .post(format!("{}/count", self.url))
            .header("Content-Type", "image/jpeg")
            .timeout(self.timeout)
            .body(jpeg)
            .send()
            .await
            .map_err(failure)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(failure)?;
        parse_image_answer(status, &body)
    }
}

/// The picture of a `/count` answer: `200` and `{ "image": "<base64 of a JPEG>" }`. Pure.
pub fn parse_image_answer(status: u16, body: &str) -> Result<Vec<u8>, CountError> {
    if status != 200 {
        return Err(CountError::Status {
            status,
            body: body.trim().chars().take(200).collect(),
        });
    }
    let bad = |why: &str| CountError::BadAnswer(why.to_string());
    let value: Value = serde_json::from_str(body).map_err(|_| bad("the body is not JSON"))?;
    let text = value
        .get("image")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("there is no image"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|_| bad("the image is not base64"))?;
    if !bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err(bad("the image is not a JPEG"));
    }
    Ok(bytes)
}

/// A failed send, as the error to log.
fn failure(error: reqwest::Error) -> CountError {
    if error.is_timeout() {
        CountError::Timeout
    } else {
        CountError::Unreachable(error.to_string())
    }
}
