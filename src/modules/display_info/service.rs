//! Display info. `clean_limit`, `check_range`, `frame_id_of` and `frame_info` are pure.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{
        BusInfo, CaptureInterval, DeviceInfo, FrameInfo, FrameRow, FramesQuery, Occupancy,
        PositionsQuery, TelemetryInfo, TelemetryQuery,
    },
    repo,
};
use crate::{
    error::AppError,
    modules::{
        frames::service::{NEXT_CAPTURE_MS, frame_path, model_image_path, save_model_image},
        headcount,
    },
};

/// How many items a list sends when `limit` is not given, and the most it sends.
const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 1000;

/// `limit` or 100; more than 1000 is cut to 1000; less than 1 is a 400. Pure.
pub fn clean_limit(limit: Option<i64>) -> Result<i64, AppError> {
    match limit {
        None => Ok(DEFAULT_LIMIT),
        Some(limit) if limit < 1 => {
            Err(AppError::Validation("limit must be 1 or more.".to_string()))
        }
        Some(limit) => Ok(limit.min(MAX_LIMIT)),
    }
}

/// `from` must not be after `to`. Pure.
pub fn check_range(from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> Result<(), AppError> {
    match (from, to) {
        (Some(from), Some(to)) if from > to => Err(AppError::Validation(
            "from must not be after to.".to_string(),
        )),
        _ => Ok(()),
    }
}

/// The id of a photo: the name of its file (`photos/101/<id>.jpg`) is a uuid. None for another name. Pure.
pub fn frame_id_of(file_path: &str) -> Option<Uuid> {
    let name = file_path.rsplit('/').next()?;
    Uuid::parse_str(name.strip_suffix(".jpg")?).ok()
}

/// A photo row as the display sees it. `frame_size` is `800x600`, or `None` when the size was not kept. None when the
/// file name is not an id. Pure.
pub fn frame_info(row: FrameRow) -> Option<FrameInfo> {
    let frame_size = match (row.width, row.height) {
        (Some(width), Some(height)) => Some(format!("{width}x{height}")),
        _ => None,
    };
    Some(FrameInfo {
        id: frame_id_of(&row.file_path)?,
        bus_id: row.bus_id,
        route_id: row.route_id,
        frame_seq: row.frame_seq,
        capture_ts: row.capture_ts,
        frame_size,
        received_at: row.received_at,
    })
}

/// 404 `VEHICLE_UNKNOWN` for a bus that does not exist.
async fn require_bus(db: &PgPool, bus_id: i32) -> Result<(), AppError> {
    if repo::bus_exists(db, bus_id).await? {
        Ok(())
    } else {
        Err(AppError::VehicleUnknown)
    }
}

pub async fn list_buses(db: &PgPool) -> Result<Vec<BusInfo>, AppError> {
    Ok(repo::list_buses(db).await?)
}

pub async fn devices_of_bus(db: &PgPool, bus_id: i32) -> Result<Vec<DeviceInfo>, AppError> {
    require_bus(db, bus_id).await?;
    Ok(repo::devices_of_bus(db, bus_id).await?)
}

pub async fn get_device(db: &PgPool, device_id: &str) -> Result<DeviceInfo, AppError> {
    repo::find_device(db, device_id)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn telemetry_history(
    db: &PgPool,
    bus_id: i32,
    query: &TelemetryQuery,
) -> Result<Vec<TelemetryInfo>, AppError> {
    let limit = clean_limit(query.limit)?;
    check_range(query.from, query.to)?;
    require_bus(db, bus_id).await?;
    Ok(repo::telemetry_history(db, bus_id, query.from, query.to, limit).await?)
}

/// The newest message of the bus that has a GPS position. 404 when the bus never sent one.
pub async fn latest_position(db: &PgPool, bus_id: i32) -> Result<TelemetryInfo, AppError> {
    require_bus(db, bus_id).await?;
    repo::latest_position(db, bus_id)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn latest_positions(
    db: &PgPool,
    query: &PositionsQuery,
) -> Result<Vec<TelemetryInfo>, AppError> {
    Ok(repo::latest_positions(db, query.route_id).await?)
}

/// The photos of a bus, newest first. `before` is the `id` of the last photo the caller has; an id that is not a photo
/// of this bus is a 404.
pub async fn list_frames(
    db: &PgPool,
    bus_id: i32,
    query: &FramesQuery,
) -> Result<Vec<FrameInfo>, AppError> {
    let limit = clean_limit(query.limit)?;
    require_bus(db, bus_id).await?;
    let older_than = match query.before {
        None => None,
        Some(id) => {
            let path = frame_path(bus_id, id);
            let at = repo::frame_received_at(db, bus_id, &path)
                .await?
                .ok_or(AppError::NotFound)?;
            Some((at, path))
        }
    };
    let rows = repo::list_frames(db, bus_id, older_than, limit).await?;
    Ok(rows.into_iter().filter_map(frame_info).collect())
}

/// The details of a photo as headers of the answer that carries it (the names the camera uses when it sends the photo, so the
/// display can show when it was taken and how big it is): `X-Bus-Id`, `X-Frame-Id` (the id of `GET /buses/{bus_id}/frames`),
/// `X-Route-Id`, `X-Frame-Seq`, `X-Capture-Ts` (the clock of the camera, 0 when it had none), `X-Frame-Size` (`800x600`) and
/// `X-Received-At` (when the server got it, `2026-11-14T14:00:05Z`). A detail the server does not have is left out. Pure.
pub fn frame_headers(row: &FrameRow) -> Vec<(&'static str, String)> {
    let mut headers = vec![("x-bus-id", row.bus_id.to_string())];
    if let Some(id) = frame_id_of(&row.file_path) {
        headers.push(("x-frame-id", id.to_string()));
    }
    if let Some(route_id) = row.route_id {
        headers.push(("x-route-id", route_id.to_string()));
    }
    headers.push(("x-frame-seq", row.frame_seq.to_string()));
    headers.push(("x-capture-ts", row.capture_ts.to_string()));
    if let (Some(width), Some(height)) = (row.width, row.height) {
        headers.push(("x-frame-size", format!("{width}x{height}")));
    }
    headers.push((
        "x-received-at",
        row.received_at
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    ));
    headers
}

/// The newest photo of the bus as the display shows it: the picture the model makes from it, and the details of the photo
/// (`frame_headers`). The picture is made once per photo: it is saved in `photos/model_image/<bus_id>/` and the next call
/// reads it from there. When there is none yet, the stored photo is sent to the model (`HEADCOUNT_URL`, `POST /annotate`).
/// If the model is not set, does not answer or answers badly, the display gets the camera photo (logged), never nothing.
/// 404 when the bus has no photo, or its file is gone.
pub async fn latest_frame(
    db: &PgPool,
    model_url: Option<&str>,
    bus_id: i32,
) -> Result<(Vec<u8>, Vec<(&'static str, String)>), AppError> {
    require_bus(db, bus_id).await?;
    let row = repo::latest_frame(db, bus_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let details = frame_headers(&row);
    if let Some(path) = model_image_path(&row.file_path)
        && let Ok(bytes) = tokio::fs::read(&path).await
    {
        return Ok((bytes, details));
    }
    let camera = tokio::fs::read(&row.file_path).await.map_err(|e| {
        tracing::warn!(path = %row.file_path, "the file of the newest photo cannot be read: {e}");
        AppError::NotFound
    })?;
    let Some(url) = model_url else {
        return Ok((camera, details));
    };
    match headcount::client::HeadcountClient::new(url)
        .annotate(camera.clone())
        .await
    {
        Ok(image) => {
            save_model_image(&row.file_path, &image).await;
            Ok((image, details))
        }
        Err(e) => {
            tracing::warn!(bus_id, "no picture from the model, the camera photo is sent: {e}");
            Ok((camera, details))
        }
    }
}

/// How many people are in the bus, how many paid and how many have no ticket. 404 `VEHICLE_UNKNOWN` for a bus that does
/// not exist.
pub async fn bus_headcount(
    db: &PgPool,
    bus_id: i32,
) -> Result<headcount::models::BusHeadcount, AppError> {
    require_bus(db, bus_id).await?;
    headcount::service::bus_headcount(db, bus_id, Utc::now()).await
}

pub async fn capture_interval(db: &PgPool, bus_id: i32) -> Result<CaptureInterval, AppError> {
    require_bus(db, bus_id).await?;
    Ok(CaptureInterval {
        bus_id,
        next_capture_ms: NEXT_CAPTURE_MS,
    })
}

/// The passengers of the paid rides going on in the bus (a ride covers the passengers it was confirmed for).
/// Nothing counts the people in the photos yet.
pub async fn occupancy(db: &PgPool, bus_id: i32) -> Result<Occupancy, AppError> {
    require_bus(db, bus_id).await?;
    Ok(repo::occupancy(db, bus_id).await?)
}
