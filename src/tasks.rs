//! Background work that runs while the server is up (step 13). It lives outside the modules
//! because it needs rides, sightings and telemetry together, and the modules must not call
//! each other in a circle.

use std::time::Duration;

use sqlx::PgPool;

use crate::{
    error::AppError,
    modules::{
        auth, charges, devices,
        extras::{
            reports,
            trips::{self, service::TripFacts},
        },
        rides::{
            self,
            models::{RideRef, RideStatus},
        },
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
/// user is gone while the bus drives on, forget the refresh tokens and the phone codes whose time has passed.
pub async fn run_once(db: &PgPool, allow_sim: bool) -> Result<(), AppError> {
    let rejected = rides::service::reject_unanswered(db).await?;
    if rejected > 0 {
        tracing::info!(rejected, "rides rejected: no answer in time");
    }

    let now = chrono::Utc::now().timestamp();
    let now_slot = now / sightings::service::SLOT_SECS;
    // A `pending` ride whose user is not heard near any bus any more: the user never boarded.
    for ride in rides::service::rides_with_status(db, RideStatus::Pending).await? {
        let Some(last_slot) = sightings::service::last_heard_any_slot(db, ride.user_id).await?
        else {
            continue;
        };
        if rides::service::went_silent(last_slot, now_slot)
            && rides::service::reject_pending_ride(db, ride.id).await?
        {
            tracing::info!(session_id = %ride.id, "ride rejected: user was never on board, no longer heard");
        }
    }

    for ride in rides::service::rides_with_status(db, RideStatus::Confirmed).await? {
        let Some(last_slot) =
            sightings::service::last_heard_slot(db, ride.user_id, ride.bus_id).await?
        else {
            continue;
        };
        if !rides::service::went_silent(last_slot, now_slot) {
            continue;
        }
        // The user is no longer heard. Did the bus keep going, or did the signal just drop?
        let heard_until = (last_slot + 1) * sightings::service::SLOT_SECS;
        let moved_m =
            telemetry::service::bus_movement_m(db, ride.bus_id, heard_until, now, allow_sim)
                .await?;
        if rides::service::bus_kept_moving(moved_m) && rides::service::end_ride(db, ride.id).await?
        {
            // The ride is over, so its charge is too: it gets an end and is completed.
            let completed = charges::service::complete_ride_charge(db, ride.id).await?;
            tracing::info!(
                session_id = %ride.id, bus_id = ride.bus_id, charges_completed = completed,
                "ride ended: user no longer heard, bus kept moving"
            );
            // One failed trip must not stop the other rides: the error is logged.
            if let Err(e) = write_trip(db, &ride, allow_sim).await {
                tracing::error!(session_id = %ride.id, "cannot write the trip of the ride: {e}");
            }
        }
    }

    let forgotten = auth::service::delete_expired_refresh_tokens(db).await?;
    if forgotten > 0 {
        tracing::info!(forgotten, "expired refresh tokens deleted");
    }
    let reports = reports::service::delete_expired(db).await?;
    if reports > 0 {
        tracing::info!(reports, "expired reports deleted");
    }
    #[cfg(feature = "phone-login")]
    {
        let codes = auth::service::delete_expired_otp_codes(db).await?;
        if codes > 0 {
            tracing::info!(codes, "expired phone codes deleted");
        }
    }
    Ok(())
}

/// The trip of a ride that has just ended: the stops of the route nearest to where the bus was when the user got on
/// and off, the distance the bus drove and the time the user was heard near it. Needs the charge (fare and line),
/// the sightings (when), the telemetry (where) and the route (the stops), so it lives here, with the other work that
/// needs several modules together.
async fn write_trip(db: &PgPool, ride: &RideRef, allow_sim: bool) -> Result<(), AppError> {
    let Some(charge) = charges::service::charge_for_session(db, ride.user_id, ride.id).await?
    else {
        return Ok(()); // a ride with no charge has no trip
    };
    // The time on the bus: the run of slots in which the user was heard. Without it, the time of the charge.
    let (started_at, ended_at) =
        match sightings::service::ride_window(db, ride.user_id, ride.bus_id).await? {
            Some(window) => window,
            None => (
                charge.started_at.timestamp(),
                charge.ended_at.unwrap_or(charge.started_at).timestamp(),
            ),
        };
    let positions =
        telemetry::service::bus_positions(db, ride.bus_id, started_at, ended_at, allow_sim).await?;
    let Some(route) = devices::service::route_of_bus(db, ride.bus_id).await? else {
        return Ok(());
    };
    let facts = TripFacts {
        user_id: ride.user_id,
        charge_id: charge.id,
        line: &charge.line,
        fare_bani: charge.amount,
        started_at,
        ended_at,
        route: &route.points.0,
        positions: &positions,
    };
    trips::service::record_trip(db, &facts).await?;
    Ok(())
}
