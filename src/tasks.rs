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
            notifications::push::PushSender,
            reports,
            trips::{self, service::TripFacts},
        },
        headcount::{self, client::HeadcountClient},
        idempotency,
        rides::{
            self,
            models::{RideRef, RideStatus},
        },
        sightings, telemetry,
    },
};

/// Seconds between two passes.
const TICK_SECS: u64 = 5;

/// Runs forever: one pass every `TICK_SECS`. A failed pass is logged and the next one tries again.
/// `allow_sim`: simulator positions may count as bus movement (test mode only).
pub async fn run(db: PgPool, push: PushSender, allow_sim: bool) {
    let mut ticker = tokio::time::interval(Duration::from_secs(TICK_SECS));
    loop {
        ticker.tick().await;
        if let Err(e) = run_once(&db, &push, allow_sim).await {
            tracing::error!("background pass failed: {e}");
        }
    }
}

/// One pass: reject rides nobody answered, reject `pending` rides whose user is gone, end rides whose
/// user is gone while the bus drives on, forget the refresh tokens and the phone codes whose time has passed.
pub async fn run_once(db: &PgPool, push: &PushSender, allow_sim: bool) -> Result<(), AppError> {
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

    // A `pending` ride that is 10 s old: decide again with what we know now. The next sighting can be a
    // whole slot away, and the user should not wait for it to be asked to pay.
    let to_look_at = rides::service::old_pending_rides(db)
        .await?
        .into_iter()
        // A rejected ride whose cooldown is over: if the user is still heard, ask again.
        .chain(rides::service::rejected_rides_to_reoffer(db).await?);
    for ride in to_look_at {
        let offered = async {
            let evidence = sightings::service::gather_evidence(db, ride.user_id, allow_sim).await?;
            rides::service::update_from_sightings(db, push, ride.user_id, &evidence).await
        };
        // One user's failure must not stop the other rides: the error is logged.
        if let Err(e) = offered.await {
            tracing::error!(session_id = %ride.id, "cannot look at the pending ride again: {e}");
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
        // Silent for a whole ticket (60 minutes): over, even if the bus sent no positions.
        let silent_too_long = rides::service::silent_past_ticket(heard_until, now);
        let kept_moving = !silent_too_long
            && rides::service::bus_kept_moving(
                telemetry::service::bus_movement_m(db, ride.bus_id, heard_until, now, allow_sim)
                    .await?,
            );
        let reason = if silent_too_long {
            "silence_60min"
        } else {
            "silence_90s"
        };
        if (silent_too_long || kept_moving) && rides::service::end_ride(db, ride.id, reason).await?
        {
            // The ride is over, so its charge is too: it gets an end and is completed.
            let completed = charges::service::complete_ride_charge(db, ride.id).await?;
            tracing::info!(
                session_id = %ride.id, bus_id = ride.bus_id, charges_completed = completed,
                silent_too_long, "ride ended: user no longer heard, and the bus kept moving or the ticket time is over"
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
    let old_sightings = sightings::service::delete_old_log(db).await?;
    if old_sightings > 0 {
        tracing::info!(old_sightings, "old entries of the sighting log deleted");
    }
    let old_positions = telemetry::service::delete_old_positions(db).await?;
    if old_positions > 0 {
        tracing::info!(
            old_positions,
            "old bus positions deleted: only the newest are kept for each bus"
        );
    }
    let keys = idempotency::service::delete_old_keys(db).await?;
    if keys > 0 {
        tracing::info!(keys, "idempotency keys older than a day deleted");
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

/// How often the loop that counts the people in the photos looks for a new photo.
const HEADCOUNT_TICK_SECS: u64 = 5;

/// The loop that counts the people in the newest photo of each bus (doc/PLAN.md, step 39), started only when
/// `HEADCOUNT_URL` is set. It is its own loop and not part of `run_once`: a slow model must never delay the rides.
pub async fn run_headcount(db: PgPool, url: String) {
    let client = HeadcountClient::new(&url);
    let mut tried = headcount::service::Tried::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(HEADCOUNT_TICK_SECS));
    // a pass that takes longer than a tick (a slow model) is not followed by a burst of passes
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        if let Err(e) = headcount::service::count_pass(&db, &client, &mut tried).await {
            tracing::error!("headcount pass failed: {e}");
        }
    }
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
