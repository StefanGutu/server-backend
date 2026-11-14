use axum::{Json, Router, extract::State, routing::get};
use uuid::Uuid;

use super::{
    admin_models::{
        OwnerQuery, PassAdminItem, RideDetail, RideListItem, RidesQuery, SightingInfo,
        SightingsQuery, TripAdminItem, UserDetail, UserListItem, UsersQuery, VerificationInfo,
        VerificationsQuery,
    },
    admin_service as service,
    extractor::DisplayAccess,
};
use crate::{
    error::AppError,
    extract::{ApiPath, ApiQuery},
    state::AppState,
};

/// GET /users?tier=&limit=&offset=
async fn get_users(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiQuery(query): ApiQuery<UsersQuery>,
) -> Result<Json<Vec<UserListItem>>, AppError> {
    Ok(Json(service::list_users(&state.db, &query).await?))
}

/// GET /users/{id}
async fn get_user(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<UserDetail>, AppError> {
    Ok(Json(service::user_detail(&state.db, id).await?))
}

/// GET /sightings?user_id=&bus_id=&accepted=&limit=&offset=
async fn get_sightings(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiQuery(query): ApiQuery<SightingsQuery>,
) -> Result<Json<Vec<SightingInfo>>, AppError> {
    Ok(Json(service::list_sightings(&state.db, &query).await?))
}

/// GET /rides?user_id=&bus_id=&status=&limit=&offset=
async fn get_rides(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiQuery(query): ApiQuery<RidesQuery>,
) -> Result<Json<Vec<RideListItem>>, AppError> {
    Ok(Json(service::list_rides(&state.db, &query).await?))
}

/// GET /rides/{session_id}
async fn get_ride(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(session_id): ApiPath<Uuid>,
) -> Result<Json<RideDetail>, AppError> {
    Ok(Json(service::ride_detail(&state.db, session_id).await?))
}

/// GET /trips?user_id=&limit=&offset=
async fn get_trips(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiQuery(query): ApiQuery<OwnerQuery>,
) -> Result<Json<Vec<TripAdminItem>>, AppError> {
    Ok(Json(service::list_trips(&state.db, &query).await?))
}

/// GET /passes?user_id=&limit=&offset=
async fn get_passes(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiQuery(query): ApiQuery<OwnerQuery>,
) -> Result<Json<Vec<PassAdminItem>>, AppError> {
    Ok(Json(service::list_passes(&state.db, &query).await?))
}

/// GET /ticket-verifications?inspector_id=&limit=&offset=
async fn get_verifications(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiQuery(query): ApiQuery<VerificationsQuery>,
) -> Result<Json<Vec<VerificationInfo>>, AppError> {
    Ok(Json(service::list_verifications(&state.db, &query).await?))
}

/// The views of users, rides and so on. (`GET /charges` and `GET /reports` are in the routes of charges and
/// reports: the app calls the same paths, and the display token makes them answer with the display's view.)
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", get(get_users))
        .route("/users/{id}", get(get_user))
        .route("/sightings", get(get_sightings))
        .route("/rides", get(get_rides))
        .route("/rides/{session_id}", get(get_ride))
        .route("/trips", get(get_trips))
        .route("/passes", get(get_passes))
        .route("/ticket-verifications", get(get_verifications))
}
