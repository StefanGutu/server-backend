use axum::{
    Json, Router,
    extract::State,
    http::{HeaderName, HeaderValue, header},
    response::{IntoResponse, Response},
    routing::get,
};
use uuid::Uuid;

use super::{
    extractor::DisplayAccess,
    models::{
        BusInfo, CaptureInterval, DeviceInfo, FrameInfo, FramesQuery, Occupancy, PositionsQuery,
        TelemetryInfo, TelemetryQuery,
    },
    service,
};
use crate::{
    error::AppError,
    extract::{ApiPath, ApiQuery},
    modules::{
        headcount::models::BusHeadcount,
        wallet::{self, models::PassProduct},
    },
    state::AppState,
};

/// GET /buses: every bus and its planned route.
async fn get_buses(
    State(state): State<AppState>,
    _access: DisplayAccess,
) -> Result<Json<Vec<BusInfo>>, AppError> {
    Ok(Json(service::list_buses(&state.db).await?))
}

/// GET /buses/{bus_id}/devices
async fn get_bus_devices(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(bus_id): ApiPath<i32>,
) -> Result<Json<Vec<DeviceInfo>>, AppError> {
    Ok(Json(service::devices_of_bus(&state.db, bus_id).await?))
}

/// GET /bus-devices/{device_id}
async fn get_device(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(device_id): ApiPath<String>,
) -> Result<Json<DeviceInfo>, AppError> {
    Ok(Json(service::get_device(&state.db, &device_id).await?))
}

/// GET /buses/{bus_id}/telemetry?from=&to=&limit=
async fn get_telemetry(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(bus_id): ApiPath<i32>,
    ApiQuery(query): ApiQuery<TelemetryQuery>,
) -> Result<Json<Vec<TelemetryInfo>>, AppError> {
    Ok(Json(
        service::telemetry_history(&state.db, bus_id, &query).await?,
    ))
}

/// GET /buses/{bus_id}/position
async fn get_position(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(bus_id): ApiPath<i32>,
) -> Result<Json<TelemetryInfo>, AppError> {
    Ok(Json(service::latest_position(&state.db, bus_id).await?))
}

/// GET /buses/positions?route_id=
async fn get_positions(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiQuery(query): ApiQuery<PositionsQuery>,
) -> Result<Json<Vec<TelemetryInfo>>, AppError> {
    Ok(Json(service::latest_positions(&state.db, &query).await?))
}

/// GET /buses/{bus_id}/frames?limit=&before=
async fn get_frames(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(bus_id): ApiPath<i32>,
    ApiQuery(query): ApiQuery<FramesQuery>,
) -> Result<Json<Vec<FrameInfo>>, AppError> {
    Ok(Json(service::list_frames(&state.db, bus_id, &query).await?))
}

/// GET /buses/{bus_id}/frames/latest: the JPEG itself.
async fn get_latest_frame(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(bus_id): ApiPath<i32>,
) -> Result<Response, AppError> {
    let (bytes, details) = service::latest_frame(&state.db, bus_id).await?;
    // `no-store`: the photo changes every few seconds and must never be kept by a cache between the server and the display
    let mut response = (
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response();
    for (name, value) in details {
        if let Ok(value) = HeaderValue::from_str(&value) {
            response
                .headers_mut()
                .insert(HeaderName::from_static(name), value);
        }
    }
    Ok(response)
}

/// GET /buses/{bus_id}/capture-interval
async fn get_capture_interval(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(bus_id): ApiPath<i32>,
) -> Result<Json<CaptureInterval>, AppError> {
    Ok(Json(service::capture_interval(&state.db, bus_id).await?))
}

/// GET /buses/{bus_id}/headcount
async fn get_headcount(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(bus_id): ApiPath<i32>,
) -> Result<Json<BusHeadcount>, AppError> {
    Ok(Json(service::bus_headcount(&state.db, bus_id).await?))
}

/// GET /buses/{bus_id}/occupancy
async fn get_occupancy(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(bus_id): ApiPath<i32>,
) -> Result<Json<Occupancy>, AppError> {
    Ok(Json(service::occupancy(&state.db, bus_id).await?))
}

/// GET /passes/catalog/{product_id}: one pass product.
async fn get_product(
    State(state): State<AppState>,
    _access: DisplayAccess,
    ApiPath(product_id): ApiPath<Uuid>,
) -> Result<Json<PassProduct>, AppError> {
    Ok(Json(
        wallet::service::get_product(&state.db, product_id).await?,
    ))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/buses", get(get_buses))
        .route("/buses/positions", get(get_positions))
        .route("/buses/{bus_id}/devices", get(get_bus_devices))
        .route("/bus-devices/{device_id}", get(get_device))
        .route("/buses/{bus_id}/telemetry", get(get_telemetry))
        .route("/buses/{bus_id}/position", get(get_position))
        .route("/buses/{bus_id}/frames", get(get_frames))
        .route("/buses/{bus_id}/frames/latest", get(get_latest_frame))
        .route(
            "/buses/{bus_id}/capture-interval",
            get(get_capture_interval),
        )
        .route("/buses/{bus_id}/occupancy", get(get_occupancy))
        .route("/buses/{bus_id}/headcount", get(get_headcount))
        .route("/passes/catalog/{product_id}", get(get_product))
        .merge(super::admin_routes::router())
}
