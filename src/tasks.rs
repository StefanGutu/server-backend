//! Background work that runs while the server is up (step 13). It lives outside the modules
//! because it needs rides, sightings and telemetry together, and the modules must not call
//! each other in a circle.

use std::time::Duration;

use sqlx::PgPool;

use crate::{
    error::AppError,
    modules::{
        charges,
        rides::{self, models::RideStatus},
        sightings, telemetry,
    },
};

/// Seconds between two passes.
const TICK_SECS: u64 = 10;

/// Runs forever: one pass every `TICK_SECS`. A failed pass is logged and the next one tries again.
/// `allow_sim`: simulator positions may count as bus movement (test mode only).
pub async fn run(db: PgPool, allow_sim: bool) {
    let mut ticker = tokio::time::interval(Duration::from_secs(TICK_SECS));
    loop {
        ticker.tick().await;
        if let Err(e) = run_once(&db, allow_sim).await {
            tracing::error!("background pass failed: {e}");
        }
    }
}

/// One pass: reject rides nobody answered, reject `pending` rides whose user is gone, end rides whose
/// user is gone while the bus drives on.
pub async fn run_once(db: &PgPool, allow_sim: bool) -> Result<(), AppError> {
    let rejected = rides::service::reject_unanswered(db).await?;
    if rejected > 0 {
        tracing::info!(rejected, "rides rejected: no answer in time");
    }

    let now = chrono::Utc::now().timestamp();
    let now_slot = now / sightings::service::SLOT_SECS;
    // A `pending` ride whose user is not heard near any bus any more: the user never boarded.
    for ride in rides::service::rides_with_status(db, RideStatus::Pending).await? {
        let Some(last_slot) = sightings::service::last_heard_any_slot(db, ride.user_id).await? else {
            continue;
        };
        if rides::service::went_silent(last_slot, now_slot) && rides::service::reject_pending_ride(db, ride.id).await? {
            tracing::info!(session_id = %ride.id, "ride rejected: user was never on board, no longer heard");
        }
    }

    for ride in rides::service::rides_with_status(db, RideStatus::Confirmed).await? {
        let Some(last_slot) = sightings::service::last_heard_slot(db, ride.user_id, ride.bus_id).await? else {
            continue;
        };
        if !rides::service::went_silent(last_slot, now_slot) {
            continue;
        }
        // The user is no longer heard. Did the bus keep going, or did the signal just drop?
        let heard_until = (last_slot + 1) * sightings::service::SLOT_SECS;
        let moved_m = telemetry::service::bus_movement_m(db, ride.bus_id, heard_until, now, allow_sim).await?;
        if rides::service::bus_kept_moving(moved_m) && rides::service::end_ride(db, ride.id).await? {
            // The ride is over, so its charge is too: it gets an end and is completed.
            let completed = charges::service::complete_ride_charge(db, ride.id).await?;
            tracing::info!(
                session_id = %ride.id, bus_id = ride.bus_id, charges_completed = completed,
                "ride ended: user no longer heard, bus kept moving"
            );
        }
    }
    Ok(())
}
