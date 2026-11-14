use serde::Deserialize;

use crate::modules::rides::models::RideStatus;

/// Body of POST /sightings (server-api.md). Optional fields a later app version may send
/// (heard, rssi_min, activity, lat, ...) are ignored by serde.
#[derive(Debug, Deserialize)]
pub struct Sighting {
    pub bus_id: i32,
    /// unix time / 30
    pub slot: i64,
    /// 32 lowercase hex characters
    pub token: String,
    pub rssi: i32,
    pub clock_synced: bool,
}

/// The `reason` of the 422 body (the one text server-api.md gives).
pub const REJECTED_REASON: &str = "token not issued by this bus recently";

/// What the service decided about a sighting.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Stored (or already stored). 200 with the current ride status.
    Accepted(RideStatus),
    /// 422: wrong or old token, unsynced phone clock, or a bus with no beacon.
    Rejected,
}
