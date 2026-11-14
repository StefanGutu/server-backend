use axum::{
    Json, Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use uuid::Uuid;

use super::{
    models::{Report, ReportFilter, ReportInput, ReportKind},
    service,
};
use crate::{
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery},
    modules::auth::extractor::CurrentUser,
    modules::display_info::{self, admin_models::Paging, extractor::Caller},
    state::AppState,
};

/// GET /reports?line=&kind=: the reports that have not expired, newest first, only of the line and kind given.
/// The app calls it with a user login and gets the app's `Report`s. The display calls it with the display token
/// (`limit` and `offset` also work) and gets more about each report: the stop's place, the bus, who made it and who
/// confirmed it.
async fn get_reports(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(filter): ApiQuery<ReportFilter>,
) -> Result<Response, AppError> {
    Ok(match caller {
        Caller::User(_) => Json(service::list_reports(&state.db, &filter).await?).into_response(),
        Caller::Display => {
            let paging = Paging {
                limit: filter.limit,
                offset: filter.offset,
            };
            let kind = filter.kind.map(ReportKind::as_str);
            let line = filter
                .line
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty());
            Json(display_info::admin_service::list_reports(&state.db, line, kind, &paging).await?)
                .into_response()
        }
    })
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
