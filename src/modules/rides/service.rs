//! Rules for the ride object. `poll_after_ms`, `needs_vehicle` and `build_ride` are pure.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{Payment, PaymentReason, Ride, RideReply, RideSession, RideStatus, Vehicle},
    repo,
};
use crate::{
    error::AppError,
    modules::devices::{self, models::BusLine},
};

/// How long the app waits before asking again (server-api.md). `None`: stop asking.
pub fn poll_after_ms(status: RideStatus) -> Option<u32> {
    match status {
        RideStatus::Pending | RideStatus::AwaitingConfirmation | RideStatus::PaymentFailed => Some(3000),
        RideStatus::Confirmed => Some(15000),
        RideStatus::None | RideStatus::Rejected | RideStatus::Ended => None,
    }
}

/// `vehicle` is shown for these statuses, and only when the server could choose one bus
/// (with `candidates` it is `null`).
pub fn needs_vehicle(session: &RideSession) -> bool {
    let status_shows_vehicle = matches!(
        session.status,
        RideStatus::AwaitingConfirmation | RideStatus::PaymentFailed | RideStatus::Confirmed | RideStatus::Ended
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
    }
}

/// The ride object of a stored session. `bus` is the line of the session's bus, needed
/// only when `needs_vehicle` is true.
pub fn build_ride(session: RideSession, bus: Option<BusLine>) -> Ride {
    let vehicle = if needs_vehicle(&session) {
        bus.map(|b| Vehicle { bus_id: session.bus_id, line: b.line, destination: b.destination })
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
        charge: None,
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

/// True when the `If-None-Match` header names our ETag. The header may hold several ETags
/// separated by commas, a weak mark (`W/`), or `*` (any version).
pub fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    if_none_match.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

/// GET /rides/current: the user's latest ride, or the `none` ride. When `if_none_match`
/// names the current version nothing else is loaded: the app polls every 3 seconds.
pub async fn current_ride(
    db: &PgPool,
    user_id: Uuid,
    if_none_match: Option<&str>,
) -> Result<RideReply, AppError> {
    let session = repo::find_current_session(db, user_id).await?;
    let etag = etag_of(session.as_ref());
    if if_none_match.is_some_and(|header| etag_matches(header, &etag)) {
        return Ok(RideReply::NotModified { etag });
    }

    let ride = match session {
        None => no_ride(Utc::now()),
        Some(session) => {
            let bus = if needs_vehicle(&session) {
                devices::service::bus_line(db, session.bus_id).await?
            } else {
                None
            };
            build_ride(session, bus)
        }
    };
    Ok(RideReply::Modified { ride, etag })
}
