use axum::{
    Json, Router,
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
};

use super::{
    models::{Charge, PublicKeysResponse, VerifyAnswer, VerifyInput},
    service,
};
use crate::{
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery, ListQuery},
    modules::auth::extractor::CurrentUser,
    state::AppState,
};

/// GET /charges?limit=&before=: the charges of the logged-in user, newest first.
async fn get_charges(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<Vec<Charge>>, AppError> {
    Ok(Json(
        service::list_charges(&state.db, user.id, &query).await?,
    ))
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
