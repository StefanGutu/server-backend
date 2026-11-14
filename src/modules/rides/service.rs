//! Rules for the ride object. `poll_after_ms`, `needs_vehicle` and `build_ride` are pure.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{
        BusEvidence, Candidate, ConfirmInput, OfferedBy, Payment, PaymentReason, Ride, RideRef,
        RideReply, RideSession, RideStatus, Vehicle,
    },
    progress::{self, Progress, RouteLine, build_progress},
    repo,
};
use crate::{
    error::AppError,
    modules::{
        auth::{self, models::User},
        charges::{
            self,
            models::{Charge, ChargeStatus, NewCharge},
        },
        devices::{self, models::BusLine},
        extras::{
            notifications::{
                self,
                push::{Push, PushSender, PushType},
            },
            trips::service::nearest_stop,
        },
        idempotency::{
            self,
            models::{Begin, IdempotentRequest},
        },
        telemetry,
        wallet::{
            self,
            models::{PaymentMethod, Source},
            provider::{ChargeResult, PaymentProvider},
        },
    },
};

/// Slots in a row the user must hear a bus before we think they are on it
/// (open question 3 in PLAN.md).
pub const REQUIRED_SLOTS: usize = 1;
/// Slots without hearing the user before a ride may be ended (or a `pending` one given up). The phone's
/// sighting for a slot can arrive up to ~30 s into that slot, so 1 would end rides that are fine.
pub const SILENCE_SLOTS: i64 = 2;
/// Metres the bus must move for the user to be asked to pay. 0: the user is asked as soon as they are heard
/// (a bus cannot drive far in the first seconds). Raise it to ask only when the bus is really moving.
pub const MIN_BOARDING_MOVEMENT_M: f64 = 0.0;
/// Metres the bus must keep moving after the user went silent for the ride to be over (not a lost signal).
pub const MIN_MOVEMENT_M: f64 = 300.0;
/// A `pending` ride this old (seconds since the user was first heard) is looked at again by the background
/// task, so the question to pay does not wait for the next sighting (up to a whole slot away).
pub const FIRST_OFFER_AFTER_SECS: i64 = 10;
/// Seconds the user has to answer before the ride expires (open question 4).
const CONFIRM_WINDOW_SECS: i64 = 120;
/// Seconds after a ride was rejected (declined, or nobody answered) before a user who is still on the
/// bus is asked again. The background task checks every few seconds, so the question comes 10 to 15 s later.
pub const REOFFER_AFTER_SECS: i64 = 10;

/// How long the app waits before asking again (server-api.md). `None`: stop asking.
pub fn poll_after_ms(status: RideStatus) -> Option<u32> {
    match status {
        RideStatus::Pending | RideStatus::AwaitingConfirmation | RideStatus::PaymentFailed => {
            Some(3000)
        }
        RideStatus::Confirmed => Some(5000), // the next stop must change in time
        RideStatus::None | RideStatus::Rejected | RideStatus::Ended => None,
    }
}

/// `vehicle` is shown for these statuses, and only when the server could choose one bus
/// (with `candidates` it is `null`).
pub fn needs_vehicle(session: &RideSession) -> bool {
    let status_shows_vehicle = matches!(
        session.status,
        RideStatus::AwaitingConfirmation
            | RideStatus::PaymentFailed
            | RideStatus::Confirmed
            | RideStatus::Ended
    );
    status_shows_vehicle && session.candidates.is_empty()
}

/// The ride object when the user has no ride.
pub fn no_ride(now: DateTime<Utc>) -> Ride {
    Ride {
        status: RideStatus::None,
        session_id: None,
        updated_at: now,
        poll_after_ms: poll_after_ms(RideStatus::None),
        vehicle: None,
        candidates: Vec::new(),
        payment: None,
        charge: None,
        progress: None,
    }
}

/// The charge is shown for these statuses (server-api.md: "Charge at `confirmed` and `ended`").
pub fn needs_charge(status: RideStatus) -> bool {
    matches!(status, RideStatus::Confirmed | RideStatus::Ended)
}

/// The ride object of a stored session. `bus` is the line of the session's bus, needed
/// only when `needs_vehicle` is true; `charge` is the charge of the ride, needed when `needs_charge`.
pub fn build_ride(session: RideSession, bus: Option<BusLine>, charge: Option<Charge>) -> Ride {
    let vehicle = if needs_vehicle(&session) {
        bus.map(|b| Vehicle {
            bus_id: session.bus_id,
            line: b.line,
            destination: b.destination,
        })
    } else {
        None
    };
    let payment = match (session.status, session.payment_reason) {
        (RideStatus::PaymentFailed, Some(reason)) => Some(Payment {
            reason,
            // the client secret is only for the "authentication required" case
            client_secret: match reason {
                PaymentReason::AuthenticationRequired => session.client_secret,
                PaymentReason::CardDeclined => None,
            },
        }),
        _ => None,
    };
    Ride {
        status: session.status,
        session_id: Some(session.id),
        updated_at: session.updated_at,
        poll_after_ms: poll_after_ms(session.status),
        vehicle,
        candidates: session.candidates.0,
        payment,
        charge: if needs_charge(session.status) {
            charge
        } else {
            None
        },
        // filled in by `current_ride`, which reads the position of the bus (this function does no reading)
        progress: None,
    }
}

/// The ETag of the ride. It changes whenever the ride changes: the session id tells two rides
/// apart (each starts at version 1), the version counts the changes of one ride.
/// A user with no ride has the fixed ETag "none".
pub fn etag_of(session: Option<&RideSession>) -> String {
    match session {
        Some(s) => format!("\"{}-v{}\"", s.id, s.version),
        None => "\"none\"".to_string(),
    }
}

/// The ETag of the ride with the position of its bus. A `confirmed` ride shows where the bus is, so its ETag also changes
/// when the newest position changes (`<id>-v<version>-<time of the position in ms>`), or when there is no position newer than
/// 30 seconds (`-x`): the app gets a 200 while the bus moves and a 304 when it does not. Any other ride has the ETag of
/// `etag_of`. Pure.
pub fn etag_of_ride(session: Option<&RideSession>, position_at_ms: Option<i64>) -> String {
    let base = etag_of(session);
    match session {
        Some(s) if s.status == RideStatus::Confirmed => {
            let part = position_at_ms.map_or_else(|| "x".to_string(), |ms| ms.to_string());
            format!("{}-{part}\"", base.trim_end_matches('"'))
        }
        _ => base,
    }
}

/// True when the `If-None-Match` header names our ETag. The header may hold several ETags
/// separated by commas, a weak mark (`W/`), or `*` (any version).
pub fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    if_none_match.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

/// GET /rides/current: the user's latest ride, or the `none` ride. When `if_none_match`
/// names the current version nothing else is loaded: the app polls every 3 seconds (every 5 while the ride is `confirmed`).
///
/// A `confirmed` ride shows where the bus is (`progress`), so its ETag includes the time of the newest position: that
/// one cheap read decides the 304, and only a changed ride loads the route and the rest. `allow_sim`: simulator positions
/// count (test mode only).
pub async fn current_ride(
    db: &PgPool,
    user_id: Uuid,
    if_none_match: Option<&str>,
    allow_sim: bool,
) -> Result<RideReply, AppError> {
    let session = repo::find_current_session(db, user_id).await?;
    let now = Utc::now();
    let position_at_ms = match &session {
        Some(s) if s.status == RideStatus::Confirmed => telemetry::service::recent_fixes(
            db,
            s.bus_id,
            now.timestamp(),
            progress::POSITION_FRESH_SECS,
            allow_sim,
            1,
        )
        .await?
        .first()
        .map(|fix| fix.at.timestamp_millis()),
        _ => None,
    };
    let etag = etag_of_ride(session.as_ref(), position_at_ms);
    if if_none_match.is_some_and(|header| etag_matches(header, &etag)) {
        return Ok(RideReply::NotModified { etag });
    }

    let ride = match session {
        None => no_ride(now),
        Some(session) => {
            let bus = if needs_vehicle(&session) {
                devices::service::bus_line(db, session.bus_id).await?
            } else {
                None
            };
            let charge = if needs_charge(session.status) {
                charges::service::charge_for_session(db, session.user_id, session.id).await?
            } else {
                None
            };
            let progress = progress_of(db, &session, allow_sim, now).await?;
            let mut ride = build_ride(session, bus, charge);
            ride.progress = progress;
            ride
        }
    };
    Ok(RideReply::Modified {
        ride: Box::new(ride),
        etag,
    })
}

/// How far back the positions of the bus are read to see which way it goes (seconds), and how many at most.
const TRAIL_SECS: i64 = 60;
const TRAIL_FIXES: i64 = 40;

/// The `progress` of a ride: where the bus is, where the user got on, the next stop and the stops of the route
/// (the geometry is `progress::build_progress`). Only a `confirmed` ride has one: any other is `None`. `allow_sim`:
/// simulator positions count (test mode only). `now` is the time of the call.
///
/// "Where the user got on" is the stop nearest to the first position the bus sent after the ride began (`created_at`).
pub async fn progress_of(
    db: &PgPool,
    session: &RideSession,
    allow_sim: bool,
    now: DateTime<Utc>,
) -> Result<Option<Progress>, AppError> {
    if session.status != RideStatus::Confirmed {
        return Ok(None);
    }
    let route = devices::service::route_of_bus(db, session.bus_id).await?;
    let line = route.as_ref().and_then(|r| RouteLine::new(&r.points.0));
    let recent = telemetry::service::recent_fixes(
        db,
        session.bus_id,
        now.timestamp(),
        TRAIL_SECS,
        allow_sim,
        TRAIL_FIXES,
    )
    .await?;
    let boarded_stop = match &route {
        Some(route) => {
            let first = telemetry::service::first_position(
                db,
                session.bus_id,
                session.created_at.timestamp(),
                allow_sim,
            )
            .await?;
            first
                .and_then(|position| nearest_stop(&route.points.0, position))
                .map(str::to_string)
        }
        None => None,
    };
    Ok(Some(build_progress(
        line.as_ref(),
        &recent,
        boarded_stop,
        now,
    )))
}

/// What a new sighting changes about the ride.
#[derive(Debug, PartialEq, Eq)]
pub enum RideAction {
    Nothing,
    /// Start a ride in `pending` on this bus.
    Start {
        bus_id: i32,
    },
    /// The current `pending` ride becomes `awaiting_confirmation`.
    /// `candidate_bus_ids` is empty when one bus qualifies, and lists every bus when
    /// several do (then the server cannot choose, the app asks the user).
    Offer {
        bus_id: i32,
        candidate_bus_ids: Vec<i32>,
    },
}

/// A bus we think the user is on: heard in enough slots in a row, the bus moved, and it is
/// still being heard (not an old run of a bus the user left behind).
fn qualifies(e: &BusEvidence, newest_slot: i64) -> bool {
    e.run_len >= REQUIRED_SLOTS
        && e.moved_m >= MIN_BOARDING_MOVEMENT_M
        && e.last_slot >= newest_slot - 1
}

/// The boarding decision. Pure: the current ride and what we know about the heard buses
/// go in, the action comes out.
pub fn decide(current: Option<&RideSession>, evidence: &[BusEvidence]) -> RideAction {
    let Some(newest_slot) = evidence.iter().map(|e| e.last_slot).max() else {
        return RideAction::Nothing;
    };
    // Buses heard in the newest slot, lowest id first.
    let mut heard_now: Vec<&BusEvidence> = evidence
        .iter()
        .filter(|e| e.last_slot == newest_slot)
        .collect();
    heard_now.sort_by_key(|e| e.bus_id);

    match current {
        None => match heard_now.first() {
            Some(e) => RideAction::Start { bus_id: e.bus_id },
            None => RideAction::Nothing,
        },
        // A ride that is over: a new one starts only when the user's hearing started again
        // after it ended. A user who keeps riding after "rejected" is not asked again.
        Some(s) if matches!(s.status, RideStatus::Rejected | RideStatus::Ended) => {
            let ended_at = s.updated_at.timestamp();
            match heard_now.iter().find(|e| e.started_at > ended_at) {
                Some(e) => RideAction::Start { bus_id: e.bus_id },
                None => RideAction::Nothing,
            }
        }
        Some(s) if s.status == RideStatus::Pending => {
            // Another bus one slot short of qualifying: its sighting for this slot may arrive
            // seconds after this one. Wait one slot, so that two buses heard together become
            // candidates instead of the first one to qualify being offered alone.
            let rival_almost_there = evidence
                .iter()
                .any(|e| e.run_len == REQUIRED_SLOTS - 1 && e.last_slot >= newest_slot - 1);
            if rival_almost_there {
                return RideAction::Nothing;
            }
            let mut bus_ids: Vec<i32> = evidence
                .iter()
                .filter(|e| qualifies(e, newest_slot))
                .map(|e| e.bus_id)
                .collect();
            bus_ids.sort_unstable();
            match bus_ids.as_slice() {
                [] => RideAction::Nothing,
                [only] => RideAction::Offer {
                    bus_id: *only,
                    candidate_bus_ids: Vec::new(),
                },
                [first, ..] => RideAction::Offer {
                    bus_id: *first,
                    candidate_bus_ids: bus_ids.clone(),
                },
            }
        }
        // awaiting_confirmation, payment_failed, confirmed: already in a ride.
        Some(_) => RideAction::Nothing,
    }
}

/// A rejected ride whose cooldown is over, while the user is still heard on a bus: the bus to ask about again
/// (the lowest id of the buses heard in the newest slot). `now` is in Unix seconds. Pure.
/// This is on top of `decide`, which asks again only when the user's hearing starts anew.
pub fn reoffer_bus(current: &RideSession, evidence: &[BusEvidence], now: i64) -> Option<i32> {
    if current.status != RideStatus::Rejected
        || now - current.updated_at.timestamp() < REOFFER_AFTER_SECS
    {
        return None;
    }
    let newest_slot = evidence.iter().map(|e| e.last_slot).max()?;
    // The user must be heard right now, not only some time ago.
    if newest_slot < now / crate::modules::sightings::service::SLOT_SECS - 1 {
        return None;
    }
    evidence
        .iter()
        .filter(|e| e.last_slot == newest_slot)
        .map(|e| e.bus_id)
        .min()
}

/// The candidates of a ride: each bus with its line.
async fn candidates_for(db: &PgPool, bus_ids: &[i32]) -> Result<Vec<Candidate>, AppError> {
    let mut candidates = Vec::new();
    for &bus_id in bus_ids {
        if let Some(bus) = devices::service::bus_line(db, bus_id).await? {
            candidates.push(Candidate {
                bus_id,
                line: bus.line,
            });
        }
    }
    Ok(candidates)
}

/// Called after every stored sighting with what the user heard recently. Changes the ride
/// if the decision says so and returns the status the ride has afterwards.
pub async fn update_from_sightings(
    db: &PgPool,
    push: &PushSender,
    user_id: Uuid,
    evidence: &[BusEvidence],
) -> Result<RideStatus, AppError> {
    let current = repo::find_current_session(db, user_id).await?;
    let mut action = decide(current.as_ref(), evidence);
    // A rejected ride whose cooldown is over: ask again, even though the user never stopped being heard.
    if action == RideAction::Nothing
        && let Some(bus_id) = current
            .as_ref()
            .and_then(|s| reoffer_bus(s, evidence, Utc::now().timestamp()))
    {
        action = RideAction::Start { bus_id };
    }
    let started = matches!(action, RideAction::Start { .. });
    apply_action(db, push, user_id, current.as_ref(), action, evidence).await?;

    if started {
        // A ride was just started for a user who is heard: ask to pay at once, instead of waiting
        // for one more sighting (the phone sends one per 30 s slot).
        let current = repo::find_current_session(db, user_id).await?;
        let action = decide(current.as_ref(), evidence);
        apply_action(db, push, user_id, current.as_ref(), action, evidence).await?;
    }

    // Read the ride again: it is the truth even if another request changed it meanwhile.
    let after = repo::find_current_session(db, user_id).await?;
    Ok(after.map_or(RideStatus::None, |s| s.status))
}

/// Does what the decision says: starts a ride, or offers it to the user and pushes a message.
async fn apply_action(
    db: &PgPool,
    push: &PushSender,
    user_id: Uuid,
    current: Option<&RideSession>,
    action: RideAction,
    evidence: &[BusEvidence],
) -> Result<(), AppError> {
    let expires_at = Utc::now() + chrono::Duration::seconds(CONFIRM_WINDOW_SECS);
    match (action, current) {
        (RideAction::Start { bus_id }, _) => {
            repo::insert_pending(db, user_id, bus_id, expires_at).await?
        }
        (
            RideAction::Offer {
                bus_id,
                candidate_bus_ids,
            },
            Some(session),
        ) => {
            let candidates = candidates_for(db, &candidate_bus_ids).await?;
            // What qualified the ride: how long the user was heard and how far the bus moved (of the chosen bus).
            let chosen = evidence.iter().find(|e| e.bus_id == bus_id);
            let how = OfferedBy::Sightings {
                slots_in_row: chosen.map_or(0, |e| e.run_len),
                distance_m: chosen.map_or(0, |e| e.moved_m.round() as i64),
            };
            if repo::offer_ride(db, session.id, bus_id, &candidates, expires_at, &how).await? {
                // The app may be closed: tell the phone. Only the call that changed the ride sends it.
                let awaiting = Push::new(PushType::AwaitingConfirmation, session.id);
                notifications::service::send_push(db, push, user_id, awaiting).await;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Rides that waited too long for an answer become `rejected`. Returns how many.
pub async fn reject_unanswered(db: &PgPool) -> Result<u64, AppError> {
    Ok(repo::reject_expired(db).await?)
}

/// The users whose newest ride was rejected at least `REOFFER_AFTER_SECS` ago (and not over an hour ago): the
/// background task asks them again if they are still heard.
pub async fn rejected_rides_to_reoffer(db: &PgPool) -> Result<Vec<RideRef>, AppError> {
    Ok(repo::find_rejected_older_than(db, REOFFER_AFTER_SECS).await?)
}

/// The `pending` rides that are at least `FIRST_OFFER_AFTER_SECS` old.
pub async fn old_pending_rides(db: &PgPool) -> Result<Vec<RideRef>, AppError> {
    Ok(repo::find_pending_older_than(db, FIRST_OFFER_AFTER_SECS).await?)
}

/// The rides in this status (the background task looks at `pending` and `confirmed` ones).
pub async fn rides_with_status(db: &PgPool, status: RideStatus) -> Result<Vec<RideRef>, AppError> {
    Ok(repo::find_by_status(db, status).await?)
}

/// The user has not been heard near the bus for `SILENCE_SLOTS` slots in a row.
/// `last_heard_slot` is the newest slot of the user's sightings of that bus.
pub fn went_silent(last_heard_slot: i64, now_slot: i64) -> bool {
    now_slot - last_heard_slot >= SILENCE_SLOTS
}

/// The bus kept moving after the user was last heard (`moved_m` metres since then).
/// A bus that stands still means the user may just have lost the signal.
pub fn bus_kept_moving(moved_m: f64) -> bool {
    moved_m >= MIN_MOVEMENT_M
}

/// The user has been silent for as long as a ticket is valid (60 minutes). The ride ends even if the bus
/// sent no positions, otherwise a ride whose bus has no GPS would stay `confirmed` for ever and the user
/// could never start another one. `heard_until` is the end of the last slot the user was heard in, `now`
/// is unix seconds. Pure.
pub fn silent_past_ticket(heard_until: i64, now: i64) -> bool {
    now - heard_until >= crate::modules::charges::ticket::TICKET_VALID_SECS
}

/// confirmed -> ended. False when the ride was no longer `confirmed`.
/// `reason` is `silence_90s` or `silence_60min` (kept for the display info).
pub async fn end_ride(db: &PgPool, session_id: Uuid, reason: &str) -> Result<bool, AppError> {
    Ok(repo::change_status(
        db,
        session_id,
        RideStatus::Confirmed,
        RideStatus::Ended,
        reason,
    )
    .await?)
}

/// pending -> rejected: the user never boarded. False when the ride was no longer `pending`.
pub async fn reject_pending_ride(db: &PgPool, session_id: Uuid) -> Result<bool, AppError> {
    Ok(repo::change_status(
        db,
        session_id,
        RideStatus::Pending,
        RideStatus::Rejected,
        "not_boarded",
    )
    .await?)
}

/// Price of one person for one ride: 5 lei (PLAN.md step 19).
pub const FARE_PER_PERSON_BANI: i32 = 500;

/// A pass covers the holder: one passenger of the ride is free.
const PASSENGERS_COVERED_BY_PASS: i32 = 1;

/// What the card must pay, in bani. `pass_covers_holder`: the ride is paid with a pass, which takes
/// one passenger. Never below 0. Pure.
pub fn amount_to_pay_bani(passengers: i32, pass_covers_holder: bool) -> i32 {
    let covered = if pass_covers_holder {
        PASSENGERS_COVERED_BY_PASS
    } else {
        0
    };
    FARE_PER_PERSON_BANI * (passengers - covered).max(0)
}

/// Passengers allowed in one ride (server-api.md: `passengers` 1 to 10).
const MAX_PASSENGERS: i32 = 10;

/// Can this ride be confirmed now?
#[derive(Debug, PartialEq, Eq)]
pub enum Confirmable {
    /// Already `confirmed`: nothing to do, the app gets the ride as it is.
    AlreadyConfirmed,
    /// Yes, on this bus.
    Ready { bus_id: i32 },
}

/// The rules that decide if a confirm is accepted. Pure.
/// - `confirmed`: nothing changes. `pending`: not waiting for an answer yet (400).
///   `rejected`, `ended`, `none`: the session is over (409 SESSION_EXPIRED).
/// - `awaiting_confirmation` and `payment_failed` (a retry) are open until `expires_at`.
/// - `passengers` 1 to 10. With `candidates` the app must say which bus (`busId`) and it must be one
///   of them; without, `busId` may be left out or must be the ride's bus.
pub fn check_confirmable(
    session: &RideSession,
    input: &ConfirmInput,
    now: DateTime<Utc>,
) -> Result<Confirmable, AppError> {
    match session.status {
        RideStatus::Confirmed => return Ok(Confirmable::AlreadyConfirmed),
        RideStatus::Pending => {
            return Err(AppError::Validation(
                "The ride is not waiting for a confirmation yet.".into(),
            ));
        }
        RideStatus::Rejected | RideStatus::Ended | RideStatus::None => {
            return Err(AppError::SessionExpired);
        }
        RideStatus::AwaitingConfirmation | RideStatus::PaymentFailed => {}
    }
    if session.expires_at <= now {
        return Err(AppError::SessionExpired);
    }
    if !(1..=MAX_PASSENGERS).contains(&input.passengers) {
        return Err(AppError::Validation(
            "passengers must be between 1 and 10.".into(),
        ));
    }
    let bus_id = if session.candidates.0.is_empty() {
        match input.bus_id {
            Some(bus_id) if bus_id != session.bus_id => {
                return Err(AppError::Validation(
                    "busId is not the bus of this ride.".into(),
                ));
            }
            _ => session.bus_id,
        }
    } else {
        let chosen = input.bus_id.ok_or_else(|| {
            AppError::Validation("busId is required: say which bus you are on.".into())
        })?;
        if !session.candidates.0.iter().any(|c| c.bus_id == chosen) {
            return Err(AppError::Validation(
                "busId is not one of the candidates.".into(),
            ));
        }
        chosen
    };
    Ok(Confirmable::Ready { bus_id })
}

/// The ride could not be paid: it becomes `payment_failed` (the user can try again for 2 minutes)
/// and the app gets 402 `PAYMENT_DECLINED` with the reason.
async fn payment_failed(
    db: &PgPool,
    push: &PushSender,
    user_id: Uuid,
    session_id: Uuid,
    reason: PaymentReason,
    client_secret: Option<&str>,
) -> AppError {
    let expires_at = Utc::now() + chrono::Duration::seconds(CONFIRM_WINDOW_SECS);
    if let Err(e) =
        repo::mark_payment_failed(db, session_id, reason, client_secret, expires_at).await
    {
        return e.into();
    }
    tracing::warn!(session_id = %session_id, reason = reason.as_str(), "payment failed");
    notifications::service::send_push(
        db,
        push,
        user_id,
        Push::new(PushType::PaymentFailed, session_id),
    )
    .await;
    AppError::PaymentDeclined(reason.as_str())
}

/// POST /rides/{sessionId}/confirm: the user says "I am on this bus, charge me". Checks the ride,
/// works out what is paid with what, takes the money (the fake provider), makes the charge with its
/// ticket and sets the ride to `confirmed`. The answer is the ride, like `GET /rides/current`.
///
/// What a confirm needs besides the database: the payment provider, the push sender and whether the
/// server runs in test mode.
pub struct PayContext<'a> {
    pub provider: &'a dyn PaymentProvider,
    pub push: &'a PushSender,
    pub test_mode: bool,
}

/// One confirm at a time per ride: a second one waits for the first to finish, then finds the ride
/// `confirmed` and answers with it, so a card is never charged twice (R26). The payment, the charge
/// and the ride are still three steps, so a stop of the server between them can leave a charge
/// without a confirmed ride (see doc/reminders.md R26).
///
/// Every call has an `Idempotency-Key` (`idempotency`): the same call sent again gets the stored
/// answer and nothing is charged; the same key with another body is `IDEMPOTENCY_CONFLICT`. Only a
/// call that worked is stored, so a refused payment can be tried again with the same key.
pub async fn confirm(
    db: &PgPool,
    pay: &PayContext<'_>,
    user: &User,
    session_id: Uuid,
    input: &ConfirmInput,
    idempotency: &IdempotentRequest,
) -> Result<RideReply, AppError> {
    auth::service::require_account(user)?;
    let name = format!("ride:{session_id}");
    let lock = idempotency::service::lock(db, &name).await?;
    let result = confirm_with_key(db, pay, user, session_id, input, idempotency).await;
    idempotency::service::unlock(lock, &name).await;
    result
}

/// `confirm_locked`, wrapped in the `Idempotency-Key` check. Runs while the ride is locked, so two
/// calls with the same key cannot both be new.
async fn confirm_with_key(
    db: &PgPool,
    pay: &PayContext<'_>,
    user: &User,
    session_id: Uuid,
    input: &ConfirmInput,
    request: &IdempotentRequest,
) -> Result<RideReply, AppError> {
    if let Begin::Replay(stored) = idempotency::service::begin(db, user.id, request).await? {
        tracing::info!(key = %request.key, session_id = %session_id, "same Idempotency-Key: stored answer sent, nothing charged");
        return Ok(RideReply::Stored(stored));
    }
    let reply = confirm_locked(db, pay, user, session_id, input).await?;
    if let RideReply::Modified { ride, .. } = &reply {
        let stored = serde_json::to_value(ride).map_err(|_| AppError::Server)?;
        // If keeping the answer fails the ride is confirmed anyway, and a repeat finds it `confirmed`.
        if let Err(e) = idempotency::service::finish(db, user.id, request, &stored).await {
            tracing::error!(key = %request.key, "cannot store the answer of an Idempotency-Key: {e}");
        }
    }
    Ok(reply)
}

/// The work of `confirm`, done while this request holds the lock of the ride.
async fn confirm_locked(
    db: &PgPool,
    pay: &PayContext<'_>,
    user: &User,
    session_id: Uuid,
    input: &ConfirmInput,
) -> Result<RideReply, AppError> {
    let session = repo::find_session(db, user.id, session_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let bus_id = match check_confirmable(&session, input, Utc::now())? {
        Confirmable::AlreadyConfirmed => {
            return current_ride(db, user.id, None, pay.test_mode).await;
        }
        Confirmable::Ready { bus_id } => bus_id,
    };
    let bus = devices::service::bus_line(db, bus_id)
        .await?
        .ok_or(AppError::Server)?;

    // What pays: the source of the request, or the user's default.
    let source_id = input.source_id.or(user.default_source_id).ok_or_else(|| {
        AppError::NoPaymentMethod("No card or valid pass to pay with. Add a card first.".into())
    })?;
    let source = wallet::service::find_source(db, user.id, source_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // The amount, and the card that takes it (none when a pass covers everything).
    let (amount_bani, card): (i32, Option<PaymentMethod>) = match source {
        Source::Card(card) => (amount_to_pay_bani(input.passengers, false), Some(card)),
        Source::Pass(pass) => {
            let today = Utc::now().date_naive();
            let check = wallet::service::check_pass(&pass, &bus.line, today, pay.test_mode);
            if let Some(message) = check.message() {
                tracing::info!(session_id = %session.id, pass_id = %pass.id, ?check, "pass cannot be used for this ride");
                return Err(AppError::NoPaymentMethod(message.into()));
            }
            let amount = amount_to_pay_bani(input.passengers, true);
            let card = if amount > 0 {
                let first = wallet::service::first_card(db, user.id).await?;
                Some(first.ok_or_else(|| {
                    AppError::NoPaymentMethod(
                        "The pass covers one passenger; the others must be paid by card, and you have no card.".into(),
                    )
                })?)
            } else {
                None
            };
            (amount, card)
        }
    };

    if let Some(card) = &card {
        match pay
            .provider
            .charge(&card.provider_method_id, amount_bani)
            .await?
        {
            ChargeResult::Paid => {}
            ChargeResult::Declined => {
                return Err(payment_failed(
                    db,
                    pay.push,
                    user.id,
                    session.id,
                    PaymentReason::CardDeclined,
                    None,
                )
                .await);
            }
            ChargeResult::AuthenticationRequired { client_secret } => {
                let reason = PaymentReason::AuthenticationRequired;
                return Err(payment_failed(
                    db,
                    pay.push,
                    user.id,
                    session.id,
                    reason,
                    Some(&client_secret),
                )
                .await);
            }
        }
    }

    let charge = charges::service::create_charge(
        db,
        &NewCharge {
            user_id: user.id,
            session_id: Some(session.id),
            line: bus.line,
            destination: bus.destination,
            amount_bani,
            status: if amount_bani == 0 {
                ChargeStatus::CoveredByPass
            } else {
                ChargeStatus::InProgress
            },
            paid_with: source_id,
            passengers: input.passengers,
            started_at: Utc::now(),
            ended_at: None,
            pass_product_id: None,
        },
    )
    .await?;

    if !repo::confirm_session(db, session.id, bus_id, input.passengers).await? {
        // Another request changed the ride first (for example it expired): do not keep this charge. There is no
        // refund in this server (a bus ride is not worth one: R48), so the card was charged for nothing: say so loudly.
        charges::service::cancel_charge(db, charge.id).await?;
        tracing::warn!(session_id = %session.id, charge_id = %charge.id, "ride changed while paying: charge cancelled, and NOT refunded (no refund is built)");
        notifications::service::send_push(
            db,
            pay.push,
            user.id,
            Push::new(PushType::ChargeCancelled, session.id),
        )
        .await;
    } else {
        tracing::info!(
            session_id = %session.id, charge_id = %charge.id, amount_bani, passengers = input.passengers,
            "ride confirmed"
        );
    }
    current_ride(db, user.id, None, pay.test_mode).await
}

/// Can this ride be declined now?
#[derive(Debug, PartialEq, Eq)]
pub enum Declinable {
    /// Already `rejected` (a repeat of the same call): nothing to do, the app gets the ride.
    AlreadyRejected,
    /// Yes: it becomes `rejected`.
    Ready,
}

/// The rules that decide if a decline is accepted. Pure.
/// - `pending`, `awaiting_confirmation` and `payment_failed` can be declined.
/// - `awaiting_confirmation`, `payment_failed` and `rejected` are open until `expires_at`; after it the
///   session is over: 409 `SESSION_EXPIRED` (server-api.md).
/// - `ended`, and a ride that does not exist (`none`), are over: 409 `SESSION_EXPIRED`.
/// - `confirmed` is paid: it cannot be declined (400).
pub fn check_declinable(session: &RideSession, now: DateTime<Utc>) -> Result<Declinable, AppError> {
    let expired = session.expires_at <= now;
    match session.status {
        RideStatus::Confirmed => Err(AppError::Validation(
            "A paid ride cannot be declined.".into(),
        )),
        RideStatus::Ended | RideStatus::None => Err(AppError::SessionExpired),
        RideStatus::Rejected if expired => Err(AppError::SessionExpired),
        RideStatus::Rejected => Ok(Declinable::AlreadyRejected),
        RideStatus::AwaitingConfirmation | RideStatus::PaymentFailed if expired => {
            Err(AppError::SessionExpired)
        }
        RideStatus::AwaitingConfirmation | RideStatus::PaymentFailed | RideStatus::Pending => {
            Ok(Declinable::Ready)
        }
    }
}

/// POST /rides/{sessionId}/decline: the user says "this is not my ride". The ride becomes `rejected`
/// and the app gets it. Takes the same lock as confirm, so a decline and a confirm of one ride run
/// one after the other.
pub async fn decline(
    db: &PgPool,
    user: &User,
    session_id: Uuid,
    allow_sim: bool,
) -> Result<RideReply, AppError> {
    let name = format!("ride:{session_id}");
    let lock = idempotency::service::lock(db, &name).await?;
    let result = decline_locked(db, user, session_id, allow_sim).await;
    idempotency::service::unlock(lock, &name).await;
    result
}

async fn decline_locked(
    db: &PgPool,
    user: &User,
    session_id: Uuid,
    allow_sim: bool,
) -> Result<RideReply, AppError> {
    let session = repo::find_session(db, user.id, session_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if check_declinable(&session, Utc::now())? == Declinable::Ready
        && repo::reject_open_session(db, session.id).await?
    {
        tracing::info!(session_id = %session.id, user_id = %user.id, "ride declined by the user");
    }
    current_ride(db, user.id, None, allow_sim).await
}

/// What "I am on the bus" does to the user's ride.
#[derive(Debug, PartialEq, Eq)]
pub enum ManualAction {
    /// No ride, or the last one is over: start one in `awaiting_confirmation`.
    StartAwaiting,
    /// A `pending` ride: it becomes `awaiting_confirmation`.
    OfferPending,
    /// A ride is already waiting, failed or paid: the app just gets it.
    Nothing,
}

/// Pure. A user who says "I am on the bus" after declining (`rejected`) or after a ride (`ended`)
/// starts a new ride: it is a new, explicit action.
pub fn decide_manual(current: Option<&RideSession>) -> ManualAction {
    match current.map(|s| s.status) {
        None | Some(RideStatus::Rejected | RideStatus::Ended | RideStatus::None) => {
            ManualAction::StartAwaiting
        }
        Some(RideStatus::Pending) => ManualAction::OfferPending,
        Some(
            RideStatus::AwaitingConfirmation | RideStatus::PaymentFailed | RideStatus::Confirmed,
        ) => ManualAction::Nothing,
    }
}

/// From the buses the user heard in the last 2 minutes: the bus of the ride and the candidates.
/// One bus: no candidates. Several: the lowest id is the ride's bus and all are candidates (the app
/// chooses with `busId` when it confirms, as for a ride found by the server). None: `None`. Pure.
pub fn manual_offer(heard_bus_ids: &[i32]) -> Option<(i32, Vec<i32>)> {
    let mut bus_ids = heard_bus_ids.to_vec();
    bus_ids.sort_unstable();
    bus_ids.dedup();
    match bus_ids.as_slice() {
        [] => None,
        [only] => Some((*only, Vec::new())),
        [first, ..] => Some((*first, bus_ids.clone())),
    }
}

/// POST /rides/manual: "I am on the bus", without waiting for the server to find out. `heard_bus_ids`
/// are the buses the user heard in the last 2 minutes (the caller asks `sightings`); with none it is
/// a 422. The ride goes to `awaiting_confirmation`, without the 3 slots and the 300 m the server asks
/// for on its own. The app gets the ride.
pub async fn start_manual_ride(
    db: &PgPool,
    push: &PushSender,
    user_id: Uuid,
    heard_bus_ids: &[i32],
    allow_sim: bool,
) -> Result<RideReply, AppError> {
    let Some((bus_id, candidate_bus_ids)) = manual_offer(heard_bus_ids) else {
        return Err(AppError::Unprocessable(
            "No valid sighting in the last 2 minutes.".into(),
        ));
    };
    let candidates = candidates_for(db, &candidate_bus_ids).await?;
    let expires_at = Utc::now() + chrono::Duration::seconds(CONFIRM_WINDOW_SECS);

    let current = repo::find_current_session(db, user_id).await?;
    match decide_manual(current.as_ref()) {
        ManualAction::Nothing => {}
        ManualAction::OfferPending => {
            if let Some(session) = &current
                && repo::offer_ride(
                    db,
                    session.id,
                    bus_id,
                    &candidates,
                    expires_at,
                    &OfferedBy::User,
                )
                .await?
            {
                let awaiting = Push::new(PushType::AwaitingConfirmation, session.id);
                notifications::service::send_push(db, push, user_id, awaiting).await;
            }
        }
        ManualAction::StartAwaiting => {
            let inserted =
                repo::insert_awaiting(db, user_id, bus_id, &candidates, expires_at).await?;
            if let Some(session_id) = inserted {
                let awaiting = Push::new(PushType::AwaitingConfirmation, session_id);
                notifications::service::send_push(db, push, user_id, awaiting).await;
            } else if let Some(session) = repo::find_current_session(db, user_id).await?
                && session.status == RideStatus::Pending
                // The sightings of this user may have started a `pending` ride a moment ago: use it.
                && repo::offer_ride(db, session.id, bus_id, &candidates, expires_at, &OfferedBy::User).await?
            {
                let awaiting = Push::new(PushType::AwaitingConfirmation, session.id);
                notifications::service::send_push(db, push, user_id, awaiting).await;
            }
        }
    }
    tracing::info!(user_id = %user_id, bus_id, "manual ride: the user says they are on the bus");
    current_ride(db, user_id, None, allow_sim).await
}
