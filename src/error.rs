use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Every error the API can return (codes from server-api.md).
/// The `#[error("...")]` text is the message sent to the app.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Missing, invalid or expired token.")]
    AuthInvalid,
    /// A proof of identity that is not accepted: a wrong or expired phone code, an ID token that
    /// does not check out. Same status and code as `AuthInvalid` (a critical failure), with its own message.
    #[error("{0}")]
    AuthRejected(String),
    #[error("An account is required to pay.")]
    AccountRequired,
    #[error("You are not allowed to do this.")]
    Forbidden,
    #[error("Resource not found.")]
    NotFound,
    #[error("Unknown vehicle.")]
    VehicleUnknown,
    #[error("{0}")]
    Validation(String),
    /// A well-formed call that cannot be done because of the state of things, for example "I am on
    /// the bus" without a recent sighting (server-api.md gives the status 422, no body and no code).
    #[error("{0}")]
    Unprocessable(String),
    /// The payment failed. The text is the `reason` sent with the error: `card_declined` or
    /// `authentication_required` (server-api.md: "PAYMENT_DECLINED (cu reason)").
    #[error("The payment was declined.")]
    PaymentDeclined(&'static str),
    /// No card and no valid pass to pay with. The message says which case it is.
    #[error("{0}")]
    NoPaymentMethod(String),
    #[error("The ride session has expired.")]
    SessionExpired,
    #[error("This Idempotency-Key was already used with a different body.")]
    IdempotencyConflict,
    #[error("This identity already has an account.")]
    IdentityInUse,
    #[error("Too many requests.")]
    RateLimited,
    #[error("Internal server error.")]
    Server,
}

impl AppError {
    /// The one place that maps each error to its HTTP status and code.
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::AuthInvalid | Self::AuthRejected(_) => (StatusCode::UNAUTHORIZED, "AUTH_INVALID"),
            Self::AccountRequired => (StatusCode::FORBIDDEN, "ACCOUNT_REQUIRED"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            Self::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            Self::VehicleUnknown => (StatusCode::NOT_FOUND, "VEHICLE_UNKNOWN"),
            Self::Validation(_) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
            Self::Unprocessable(_) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR"),
            Self::PaymentDeclined(_) => (StatusCode::PAYMENT_REQUIRED, "PAYMENT_DECLINED"),
            Self::NoPaymentMethod(_) => (StatusCode::CONFLICT, "NO_PAYMENT_METHOD"),
            Self::SessionExpired => (StatusCode::CONFLICT, "SESSION_EXPIRED"),
            Self::IdempotencyConflict => (StatusCode::CONFLICT, "IDEMPOTENCY_CONFLICT"),
            Self::IdentityInUse => (StatusCode::CONFLICT, "IDENTITY_IN_USE"),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED"),
            Self::Server => (StatusCode::INTERNAL_SERVER_ERROR, "SERVER_ERROR"),
        }
    }
}

/// Lets handlers return `Result<_, AppError>`; axum calls this to build the response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        let mut body = json!({ "error": { "code": code, "message": self.to_string() } });
        if let Self::PaymentDeclined(reason) = &self {
            body["error"]["reason"] = json!(reason);
        }
        (status, Json(body)).into_response()
    }
}

/// Lets handlers use `?` on database calls. The real error is only logged,
/// the app just sees SERVER_ERROR.
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!("database error: {err}");
        Self::Server
    }
}
