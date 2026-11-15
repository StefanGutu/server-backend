//! Photos from Node B (server.md). `parse_meta`, `check_jpeg` and `frame_path` are pure.

use axum::http::HeaderMap;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{FrameMeta, NextCapture},
    repo,
};
use crate::{
    error::AppError,
    extract::required_header,
    modules::devices::{
        self,
        models::{Device, DeviceRole},
    },
};

/// The biggest photo accepted: 2 MB. A photo of 800x600 is about 100 KB.
pub const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;

/// What the camera is told after each photo: wait 3 seconds. Must be 1000 to 60000 (server.md).
pub const NEXT_CAPTURE_MS: u32 = 3000;

/// The folder the photos are saved in, next to the code (`photos/` is not in git). One folder per bus.
pub const PHOTOS_DIR: &str = "photos";

/// How many photos are kept for each bus: the newest 100, a buffer. Nothing is deleted while a bus has 100 or fewer; each
/// photo after that makes the oldest one go (the file and the row, together), so the newest photo is always there and a camera
/// cannot fill the disk. At one photo every 20 seconds (`NEXT_CAPTURE_MS`) 100 photos are about 33 minutes.
pub const MAX_PHOTOS_PER_BUS: i64 = 100;

fn invalid(message: String) -> AppError {
    AppError::Validation(message)
}

/// A header that holds a whole number, read with `parse`.
fn number<T: std::str::FromStr>(headers: &HeaderMap, name: &str) -> Result<T, AppError> {
    required_header(headers, name)?
        .trim()
        .parse()
        .map_err(|_| invalid(format!("The {name} header must be a whole number.")))
}

/// `X-Frame-Size`: `WIDTHxHEIGHT`, both whole numbers from 1 to 10000.
fn frame_size(text: &str) -> Option<(u32, u32)> {
    let (width, height) = text.trim().split_once('x')?;
    let (width, height): (u32, u32) = (width.parse().ok()?, height.parse().ok()?);
    ((1..=10_000).contains(&width) && (1..=10_000).contains(&height)).then_some((width, height))
}

/// What the camera says in the headers of the call. The call is a JPEG (`Content-Type: image/jpeg`),
/// `X-Bus-Id` must be the bus of the URL, `X-Frame-Seq` and `X-Capture-Ts` are whole numbers from 0,
/// `X-Route-Id` a whole number, `X-Frame-Size` is `800x600`. Anything else is a 400, and the camera
/// does not send the photo again. Pure.
pub fn parse_meta(headers: &HeaderMap, url_bus_id: i32) -> Result<FrameMeta, AppError> {
    let content_type = required_header(headers, "Content-Type")?;
    if !content_type
        .split(';')
        .next()
        .is_some_and(|t| t.trim().eq_ignore_ascii_case("image/jpeg"))
    {
        return Err(invalid("Content-Type must be image/jpeg.".to_string()));
    }
    let bus_id: i32 = number(headers, "X-Bus-Id")?;
    if bus_id != url_bus_id {
        return Err(invalid(
            "X-Bus-Id is not the bus of the request.".to_string(),
        ));
    }
    let seq: i64 = number(headers, "X-Frame-Seq")?;
    let capture_ts: i64 = number(headers, "X-Capture-Ts")?;
    if seq < 0 || capture_ts < 0 {
        return Err(invalid(
            "X-Frame-Seq and X-Capture-Ts must not be negative.".to_string(),
        ));
    }
    let size = required_header(headers, "X-Frame-Size")?;
    let (width, height) = frame_size(size)
        .ok_or_else(|| invalid("X-Frame-Size must look like 800x600.".to_string()))?;
    Ok(FrameMeta {
        route_id: number(headers, "X-Route-Id")?,
        seq,
        capture_ts,
        width,
        height,
    })
}

/// The body must be a JPEG: not empty, not over `MAX_FRAME_BYTES`, and it starts with the JPEG marker
/// (FF D8 FF). The end of the file is not checked: some cameras pad it. Pure.
pub fn check_jpeg(body: &[u8]) -> Result<(), AppError> {
    if body.is_empty() {
        return Err(invalid("The photo is empty.".to_string()));
    }
    if body.len() > MAX_FRAME_BYTES {
        return Err(invalid(format!(
            "The photo is bigger than {MAX_FRAME_BYTES} bytes."
        )));
    }
    if !body.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err(invalid("The body is not a JPEG.".to_string()));
    }
    Ok(())
}

/// Where the photo `id` of this bus is saved. One folder per bus, one file per photo. Pure.
pub fn frame_path(bus_id: i32, id: Uuid) -> String {
    format!("{PHOTOS_DIR}/{bus_id}/{id}.jpg")
}

/// Where the picture the model made from the photo `file_path` is saved: `photos/model_image/<bus_id>/<same name>.jpg`.
/// The two files are linked by their name, so no column is needed. Pure.
pub fn model_image_path(file_path: &str) -> Option<String> {
    let path = std::path::Path::new(file_path);
    let name = path.file_name()?.to_str()?;
    let bus = path.parent()?.file_name()?.to_str()?;
    Some(format!("{PHOTOS_DIR}/{MODEL_IMAGE_DIR}/{bus}/{name}"))
}

/// The folder (inside `photos/`) of the pictures made by the model.
pub const MODEL_IMAGE_DIR: &str = "model_image";

/// Saves the picture of the model next to its photo's name. Written to a temporary file and renamed, so a display never
/// reads half a file. A failure is logged and returned as false (the display then gets the camera photo).
pub async fn save_model_image(file_path: &str, bytes: &[u8]) -> bool {
    let Some(path) = model_image_path(file_path) else {
        return false;
    };
    let temp = format!("{path}.tmp");
    if save_file(&temp, bytes).await.is_err() {
        return false;
    }
    match tokio::fs::rename(&temp, &path).await {
        Ok(()) => true,
        Err(e) => {
            tracing::error!("cannot save the model image {path}: {e}");
            false
        }
    }
}

/// POST /buses/{bus_id}/frames: checks that the camera of this bus sent it, saves the JPEG to disk,
/// keeps the facts in the table, and tells the camera when to take the next photo. The file is written
/// first and the row second; if the row cannot be stored the file is removed again.
pub async fn store_frame(
    db: &PgPool,
    device: &Device,
    url_bus_id: i32,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<NextCapture, AppError> {
    devices::service::require_device(device, DeviceRole::Camera, url_bus_id)?;
    let meta = parse_meta(headers, url_bus_id)?;
    check_jpeg(body)?;
    devices::service::warn_if_route_differs(db, url_bus_id, meta.route_id).await?;

    let path = frame_path(url_bus_id, Uuid::now_v7());
    save_file(&path, body).await?;
    if let Err(e) = repo::insert_frame(db, url_bus_id, &meta, &path).await {
        if let Err(remove) = tokio::fs::remove_file(&path).await {
            tracing::error!("cannot remove {path} after a failed insert: {remove}");
        }
        return Err(e.into());
    }
    tracing::debug!(bus_id = url_bus_id, seq = meta.seq, bytes = body.len(), size = %format!("{}x{}", meta.width, meta.height), "photo stored");
    delete_old_photos(db, url_bus_id).await;
    Ok(NextCapture {
        next_capture_ms: NEXT_CAPTURE_MS,
    })
}

/// Keeps only the newest `MAX_PHOTOS_PER_BUS` photos of the bus: the older rows are forgotten and their
/// files removed. This is clean-up: a failure is logged and never fails the photo that was just stored.
async fn delete_old_photos(db: &PgPool, bus_id: i32) {
    let paths = match repo::delete_older_than_newest(db, bus_id, MAX_PHOTOS_PER_BUS).await {
        Ok(paths) => paths,
        Err(e) => {
            tracing::error!(bus_id, "cannot delete the old photos: {e}");
            return;
        }
    };
    if paths.is_empty() {
        return;
    }
    for path in &paths {
        if let Err(e) = tokio::fs::remove_file(path).await {
            tracing::warn!("cannot remove the old photo {path}: {e}");
        }
        // its model image goes with it (there may be none: the file is then simply not there)
        if let Some(image) = model_image_path(path) {
            let _ = tokio::fs::remove_file(image).await;
        }
    }
    tracing::info!(
        bus_id,
        deleted = paths.len(),
        kept = MAX_PHOTOS_PER_BUS,
        "old photos deleted: only the newest are kept"
    );
}

/// Makes the `photos` folder, so it is there from the start. Called once when the server starts.
pub fn prepare_photos_folder() {
    std::fs::create_dir_all(PHOTOS_DIR).expect("cannot create the photos folder");
}

/// Writes the file, making its folder first. A failure is a 500 and is logged.
async fn save_file(path: &str, body: &[u8]) -> Result<(), AppError> {
    let write = async {
        if let Some(folder) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(folder).await?;
        }
        tokio::fs::write(path, body).await
    };
    write.await.map_err(|e| {
        tracing::error!("cannot save the photo {path}: {e}");
        AppError::Server
    })
}
