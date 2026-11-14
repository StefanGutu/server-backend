use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use uuid::Uuid;

use super::{
    models::{DeviceInput, Notification},
    service,
};
use crate::{
    error::AppError,
    extract::{ApiJson, ApiPath, ApiQuery, ListQuery},
    modules::auth::extractor::CurrentUser,
    state::AppState,
};

/// GET /notifications?limit=&before=: the notifications of the logged-in user, newest first.
async fn get_notifications(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<Vec<Notification>>, AppError> {
    Ok(Json(
        service::list_notifications(&state.db, user.id, &query).await?,
    ))
}

/// POST /notifications/{id}/read: 204.
async fn post_read(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(notification_id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    service::mark_read(&state.db, user.id, notification_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /devices: `{ "pushToken", "platform" }`, 204.
async fn post_device(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<DeviceInput>, // must be last: it reads the body
) -> Result<StatusCode, AppError> {
    service::register_device(&state.db, user.id, &input).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(get_notifications))
        .route("/notifications/{id}/read", post(post_read))
        .route("/devices", post(post_device))
}
