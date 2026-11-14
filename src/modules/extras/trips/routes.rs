use axum::{Json, Router, extract::State, routing::get};

use super::{
    models::{RecentTrip, SavedRoute},
    service,
};
use crate::{
    error::AppError,
    extract::{ApiQuery, ListQuery},
    modules::auth::extractor::CurrentUser,
    state::AppState,
};

/// GET /trips/recent?limit=&before=: the trips of the logged-in user, newest first.
async fn get_recent_trips(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<Vec<RecentTrip>>, AppError> {
    Ok(Json(service::list_trips(&state.db, user.id, &query).await?))
}

/// GET /trips/saved: the routes the logged-in user keeps.
async fn get_saved_routes(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<SavedRoute>>, AppError> {
    Ok(Json(service::list_saved_routes(&state.db, user.id).await?))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/trips/recent", get(get_recent_trips))
        .route("/trips/saved", get(get_saved_routes))
}
