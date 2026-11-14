use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use uuid::Uuid;

use super::{
    models::{Report, ReportFilter, ReportInput},
    service,
};
use crate::{
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery},
    modules::auth::extractor::CurrentUser,
    modules::display_info::extractor::UserOrDisplay,
    state::AppState,
};

/// GET /reports?line=&kind=: the reports that have not expired, newest first, only of the line and kind given.
/// The app calls it with a user login, the display with the display token.
async fn get_reports(
    State(state): State<AppState>,
    _access: UserOrDisplay,
    ApiQuery(filter): ApiQuery<ReportFilter>,
) -> Result<Json<Vec<Report>>, AppError> {
    Ok(Json(service::list_reports(&state.db, &filter).await?))
}

/// POST /reports: `{ "kind", "line" }`, answers the report.
async fn post_report(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<ReportInput>, // must be last: it reads the body
) -> Result<Json<Report>, AppError> {
    Ok(Json(
        service::create_report(&state.db, user.id, &input, state.config.test_mode).await?,
    ))
}

/// POST /reports/{id}/confirm: answers the report.
async fn post_confirm(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(report_id): ApiPath<Uuid>,
) -> Result<Json<Report>, AppError> {
    Ok(Json(
        service::confirm_report(&state.db, user.id, report_id).await?,
    ))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reports", get(get_reports).post(post_report))
        .route("/reports/{id}/confirm", post(post_confirm))
}
