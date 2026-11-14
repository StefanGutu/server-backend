use axum::{
    Json, Router,
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    models::{ChargeStatus, PublicKeysResponse, VerifyAnswer, VerifyInput},
    service,
};
use crate::{
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery, ListQuery},
    modules::{
        auth::extractor::CurrentUser,
        display_info::{self, admin_models::ChargesQuery, extractor::Caller},
    },
    state::AppState,
};

/// The query of `GET /charges`. The app uses `limit` and `before`; the display uses `limit`, `offset`, `user_id` and
/// `status` (they are plain fields in one struct: a `flatten` would not read numbers from a URL).
#[derive(Debug, Deserialize)]
struct ChargesCallQuery {
    limit: Option<i64>,
    before: Option<Uuid>,
    offset: Option<i64>,
    user_id: Option<Uuid>,
    status: Option<ChargeStatus>,
}

/// GET /charges?limit=&before=: the charges of the logged-in user, newest first. With the display token it is the
/// display's list instead: the charges of everyone, with more about each (see `api-display.md`).
async fn get_charges(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(query): ApiQuery<ChargesCallQuery>,
) -> Result<Response, AppError> {
    Ok(match caller {
        Caller::User(user) => {
            let list = ListQuery {
                limit: query.limit,
                before: query.before,
            };
            Json(service::list_charges(&state.db, user.id, &list).await?).into_response()
        }
        Caller::Display => {
            let display = ChargesQuery {
                user_id: query.user_id,
                status: query.status,
                limit: query.limit,
                offset: query.offset,
            };
            Json(display_info::admin_service::list_charges(&state.db, &display).await?)
                .into_response()
        }
    })
}

/// GET /tickets/public-key: `{ "keys": [{ "kid", "alg": "EdDSA", "publicKey" }] }`.
async fn get_public_keys(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
) -> Result<Json<PublicKeysResponse>, AppError> {
    Ok(Json(service::public_keys(&state.db).await?))
}

/// POST /tickets/verify: `{ "qrToken" }` -> `{ valid, line, passengers, expiresAt, paid }`. Inspectors only.
async fn post_verify(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<VerifyInput>, // must be last: it reads the body
) -> Result<Json<VerifyAnswer>, AppError> {
    Ok(Json(
        service::verify_ticket(&state.db, &user, &input.qr_token).await?,
    ))
}

/// GET /statements/{YYYY-MM}: the PDF of the month. 404 when the user has no trip in it.
async fn get_statement(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(month): ApiPath<String>,
) -> Result<Response, AppError> {
    let pdf = service::statement_pdf(&state.db, user.id, &month).await?;
    let disposition = format!("inline; filename=\"statement-{month}.pdf\"");
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        pdf,
    )
        .into_response())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/statements/{month}", get(get_statement))
        .route("/tickets/verify", post(post_verify))
        .route("/charges", get(get_charges))
        .route("/tickets/public-key", get(get_public_keys))
}
