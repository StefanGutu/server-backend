use serde::Serialize;

/// What the camera says about a photo, from the headers of the call (server.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameMeta {
    pub route_id: i32,
    /// Photo counter of the camera; it starts again from 0 when the camera restarts.
    pub seq: i64,
    /// Unix seconds when the photo was taken, 0 if the camera has no clock yet.
    pub capture_ts: i64,
    pub width: u32,
    pub height: u32,
}

/// The answer: how long the camera waits before the next photo (server.md: 1000 to 60000).
#[derive(Debug, Serialize)]
pub struct NextCapture {
    pub next_capture_ms: u32,
}
