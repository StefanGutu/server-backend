//! Counting the people in the photos: what is kept and what is asked of the database.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::{
    client::{CountError, HeadcountClient},
    models::{BusHeadcount, LatestCount, Rider, Summary, Uncounted},
    repo,
};
use crate::{
    error::AppError,
    modules::{
        charges::ticket::TICKET_VALID_SECS, display_info::service::frame_id_of,
        rides::service::SILENCE_SLOTS, sightings::service::SLOT_SECS,
    },
};

/// For each bus, its newest photo when it has no count yet (the work of the next pass of the loop).
pub async fn newest_uncounted(db: &PgPool) -> Result<Vec<Uncounted>, AppError> {
    Ok(repo::newest_uncounted(db).await?)
}

/// Keeps the count of a photo. False when the photo is gone.
pub async fn save_count(db: &PgPool, file_path: &str, count: i32) -> Result<bool, AppError> {
    Ok(repo::save_count(db, file_path, count).await?)
}

/// The newest counted photo of a bus, or None.
pub async fn latest_count(db: &PgPool, bus_id: i32) -> Result<Option<LatestCount>, AppError> {
    Ok(repo::latest_count(db, bus_id).await?)
}

/// For each bus, the id (file path) of the last photo the loop tried. A photo in it is not sent to the model again, so a
/// photo that was refused is not asked about over and over. One entry per bus.
pub type Tried = HashMap<i32, String>;

/// What one pass of the loop did, for the log and the tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PassReport {
    /// Photos the model counted and that were kept.
    pub counted: usize,
    /// Photos not sent: the loop had already tried that very photo (its id has not changed).
    pub same_photo: usize,
    /// Photos that failed (a file that is gone, a refusal of the model, no answer).
    pub failed: usize,
}

/// One pass of the loop. The model is called only for a photo that is **new**: the newest photo of a bus that has no
/// count (`newest_uncounted`) and that the loop has not tried yet (`tried`). A photo that already has a count keeps its
/// data and is not sent again; the same photo is never sent twice after a refusal.
///
/// A photo that cannot be read, or that the model refuses, is marked as tried (the next new photo of the bus is
/// tried). When the model does not answer (down, too slow) the pass stops at once, so a dead model costs one call and
/// not one for each bus, and the photo is tried again on the next pass.
pub async fn count_pass(
    db: &PgPool,
    client: &HeadcountClient,
    tried: &mut Tried,
) -> Result<PassReport, AppError> {
    let mut report = PassReport::default();
    for photo in newest_uncounted(db).await? {
        if tried.get(&photo.bus_id) == Some(&photo.file_path) {
            report.same_photo += 1;
            continue;
        }
        let jpeg = match tokio::fs::read(&photo.file_path).await {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!(bus_id = photo.bus_id, file = %photo.file_path, "photo not counted: cannot read the file: {e}");
                tried.insert(photo.bus_id, photo.file_path);
                report.failed += 1;
                continue;
            }
        };
        match client.count_people(jpeg).await {
            Ok(count) => {
                if save_count(db, &photo.file_path, count).await? {
                    tracing::debug!(bus_id = photo.bus_id, file = %photo.file_path, count, "people counted");
                    report.counted += 1;
                }
            }
            Err(error @ (CountError::Timeout | CountError::Unreachable(_))) => {
                tracing::warn!(
                    bus_id = photo.bus_id,
                    "people not counted, the model does not answer: {error}"
                );
                report.failed += 1;
                break; // do not ask again for the other buses; this photo is tried again on the next pass
            }
            Err(error) => {
                tracing::warn!(bus_id = photo.bus_id, file = %photo.file_path, "photo not counted, not tried again: {error}");
                tried.insert(photo.bus_id, photo.file_path);
                report.failed += 1;
            }
        }
    }
    Ok(report)
}

/// The oldest slot in which a user still counts as heard on a bus at `now` (unix seconds): the rule the rides already
/// use for "still on board" (`went_silent`): a user is silent after `SILENCE_SLOTS` (2) slots without a sighting, so the
/// current slot and the one before it count (30 to 60 seconds). Pure.
pub fn heard_since_slot(now_secs: i64) -> i64 {
    now_secs / SLOT_SECS - (SILENCE_SLOTS - 1)
}

/// The users of the app heard on this bus now, with what they paid and whether their ticket is good.
pub async fn riders(db: &PgPool, bus_id: i32, now: DateTime<Utc>) -> Result<Vec<Rider>, AppError> {
    Ok(repo::riders(
        db,
        bus_id,
        heard_since_slot(now.timestamp()),
        now,
        TICKET_VALID_SECS,
    )
    .await?)
}

/// How the people the model counted compare with the users of the app who paid. Pure.
///
/// - `devices_heard` are all the riders, `devices_paid` those with a valid ticket, `devices_unpaid` the rest.
/// - `paid_passengers` is the sum of `passengers` of the valid tickets only: a ticket that is over covers nobody.
/// - `without_ticket` is `people_counted - paid_passengers`, never below 0, and `None` when nothing was counted.
///   It is an estimate: the model can miss a person or count one twice, and if the camera sees the driver it is 1 too high.
pub fn summarize(people_counted: Option<i32>, riders: &[Rider]) -> Summary {
    let devices_heard = riders.len() as i64;
    let devices_paid = riders.iter().filter(|r| r.ticket_valid).count() as i64;
    let paid_passengers: i64 = riders
        .iter()
        .filter(|r| r.ticket_valid)
        .map(|r| i64::from(r.passengers))
        .sum();
    Summary {
        people_counted,
        devices_heard,
        devices_paid,
        devices_unpaid: devices_heard - devices_paid,
        paid_passengers,
        without_ticket: people_counted.map(|counted| (i64::from(counted) - paid_passengers).max(0)),
    }
}

/// A count older than this (60 seconds) is not shown as the people in the bus now: a number from a camera that
/// stopped would look current.
pub const COUNT_FRESH_SECS: i64 = 60;

/// The answer of `GET /buses/{bus_id}/headcount` from what the database has. `people_counted` is the count of the newest
/// counted photo only while it is at most `COUNT_FRESH_SECS` old; `counted_at` and `frame_id` say which photo it was
/// either way. Pure.
pub fn build_headcount(
    bus_id: i32,
    latest: Option<&LatestCount>,
    riders: &[Rider],
    now: DateTime<Utc>,
) -> BusHeadcount {
    let fresh = latest.filter(|l| (now - l.counted_at).num_seconds() <= COUNT_FRESH_SECS);
    BusHeadcount {
        bus_id,
        counted_at: latest.map(|l| l.counted_at),
        frame_id: latest.and_then(|l| frame_id_of(&l.file_path)),
        summary: summarize(fresh.map(|l| l.people_count), riders),
    }
}

/// How many people are in the bus, how many paid, how many have no ticket (the caller checks that the bus exists).
pub async fn bus_headcount(
    db: &PgPool,
    bus_id: i32,
    now: DateTime<Utc>,
) -> Result<BusHeadcount, AppError> {
    let latest = latest_count(db, bus_id).await?;
    let riders = riders(db, bus_id, now).await?;
    Ok(build_headcount(bus_id, latest.as_ref(), &riders, now))
}
