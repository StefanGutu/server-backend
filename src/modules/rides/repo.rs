use chrono::{DateTime, Utc};
use sqlx::{PgPool, types::Json};
use uuid::Uuid;

use super::models::{Candidate, OfferedBy, PaymentReason, RideRef, RideSession, RideStatus};

/// The latest ride of a user, or None when the user never had one.
pub async fn find_current_session(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Option<RideSession>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, user_id, bus_id, status, version, candidates, passengers,
                payment_reason, client_secret, expires_at, updated_at, created_at
         FROM ride_sessions WHERE user_id = $1 ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// Starts a ride in `pending`. A user has at most one open ride (a unique index): if another
/// request started one a moment ago, nothing is inserted and the ride that is there stays.
pub async fn insert_pending(
    db: &PgPool,
    user_id: Uuid,
    bus_id: i32,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO ride_sessions (id, user_id, bus_id, status, expires_at) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(bus_id)
    .bind(RideStatus::Pending)
    .bind(expires_at)
    .execute(db)
    .await?;
    Ok(())
}

/// pending -> awaiting_confirmation. Only changes a ride that is still `pending`, and raises
/// `version` and `updated_at` so the app's ETag changes. Returns whether the ride changed (false: it was not
/// `pending` any more), so only the call that really changed it sends the push.
pub async fn offer_ride(
    db: &PgPool,
    session_id: Uuid,
    bus_id: i32,
    candidates: &[Candidate],
    expires_at: DateTime<Utc>,
    how: &OfferedBy,
) -> Result<bool, sqlx::Error> {
    let (manual, qualification) = match how {
        OfferedBy::User => (true, None),
        OfferedBy::Sightings {
            slots_in_row,
            distance_m,
        } => (
            false,
            Some(serde_json::json!({ "slots_in_row": slots_in_row, "distance_m": distance_m })),
        ),
    };
    let result = sqlx::query(
        "UPDATE ride_sessions
         SET status = $2, bus_id = $3, candidates = $4, expires_at = $5,
             manual = $7, qualification = $8, version = version + 1, updated_at = now()
         WHERE id = $1 AND status = $6",
    )
    .bind(session_id)
    .bind(RideStatus::AwaitingConfirmation)
    .bind(bus_id)
    .bind(Json(candidates))
    .bind(expires_at)
    .bind(RideStatus::Pending)
    .bind(manual)
    .bind(qualification)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// `awaiting_confirmation` and `payment_failed` rides whose time has run out become `rejected`. (A failed
/// payment keeps the `expires_at` it got: if it were never rejected the ride would stay open for ever, and
/// the user could neither confirm nor start another.) Returns how many. Raises `version` and `updated_at`
/// so the app's ETag changes.
pub async fn reject_expired(db: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE ride_sessions SET status = $1, end_reason = 'timeout', version = version + 1, updated_at = now()
         WHERE status IN ($2, $3) AND expires_at < now()",
    )
    .bind(RideStatus::Rejected)
    .bind(RideStatus::AwaitingConfirmation)
    .bind(RideStatus::PaymentFailed)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

/// All rides in this status.
pub async fn find_by_status(db: &PgPool, status: RideStatus) -> Result<Vec<RideRef>, sqlx::Error> {
    sqlx::query_as("SELECT id, user_id, bus_id FROM ride_sessions WHERE status = $1")
        .bind(status)
        .fetch_all(db)
        .await
}

/// The newest ride of a user, when it was rejected more than `secs` seconds ago (and less than an hour ago).
pub async fn find_rejected_older_than(db: &PgPool, secs: i64) -> Result<Vec<RideRef>, sqlx::Error> {
    sqlx::query_as(
        "SELECT r.id, r.user_id, r.bus_id FROM ride_sessions r
         WHERE r.status = $1
           AND r.updated_at < now() - make_interval(secs => $2)
           AND r.updated_at > now() - interval '1 hour'
           AND NOT EXISTS (SELECT 1 FROM ride_sessions n WHERE n.user_id = r.user_id AND n.created_at > r.created_at)",
    )
    .bind(RideStatus::Rejected)
    .bind(secs as f64)
    .fetch_all(db)
    .await
}

/// The `pending` rides created more than `secs` seconds ago.
pub async fn find_pending_older_than(db: &PgPool, secs: i64) -> Result<Vec<RideRef>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, user_id, bus_id FROM ride_sessions
         WHERE status = $1 AND created_at < now() - make_interval(secs => $2)",
    )
    .bind(RideStatus::Pending)
    .bind(secs as f64)
    .fetch_all(db)
    .await
}

/// Moves a ride from one status to another. Only changes a ride that is still in `from` (another
/// request may have changed it meanwhile). Raises `version` and `updated_at` so the ETag changes.
/// Returns false if the ride was not in `from`.
pub async fn change_status(
    db: &PgPool,
    session_id: Uuid,
    from: RideStatus,
    to: RideStatus,
    end_reason: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE ride_sessions SET status = $3, end_reason = $4, version = version + 1, updated_at = now()
         WHERE id = $1 AND status = $2",
    )
    .bind(session_id)
    .bind(from)
    .bind(to)
    .bind(end_reason)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// One ride of this user, or None.
pub async fn find_session(
    db: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<Option<RideSession>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, user_id, bus_id, status, version, candidates, passengers,
                payment_reason, client_secret, expires_at, updated_at, created_at
         FROM ride_sessions WHERE id = $1 AND user_id = $2",
    )
    .bind(session_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// A payment failed: the ride becomes `payment_failed`, with the reason (and the client secret when
/// the bank wants authentication). The user has until `expires_at` to try again. Only changes a ride
/// that is still waiting for the payment. Returns false if it was not.
pub async fn mark_payment_failed(
    db: &PgPool,
    session_id: Uuid,
    reason: PaymentReason,
    client_secret: Option<&str>,
    expires_at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE ride_sessions
         SET status = $2, payment_reason = $3, client_secret = $4, expires_at = $5,
             version = version + 1, updated_at = now()
         WHERE id = $1 AND (status = $6 OR status = $2)",
    )
    .bind(session_id)
    .bind(RideStatus::PaymentFailed)
    .bind(reason)
    .bind(client_secret)
    .bind(expires_at)
    .bind(RideStatus::AwaitingConfirmation)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// The ride is paid: `confirmed`, on the chosen bus, with its passengers, no more candidates and no
/// payment problem. Only changes a ride that is still waiting for the payment (so two confirms at
/// the same moment cannot both win). Returns false if it was not.
pub async fn confirm_session(
    db: &PgPool,
    session_id: Uuid,
    bus_id: i32,
    passengers: i32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE ride_sessions
         SET status = $2, bus_id = $3, passengers = $4, candidates = '[]',
             payment_reason = NULL, client_secret = NULL, version = version + 1, updated_at = now()
         WHERE id = $1 AND (status = $5 OR status = $6)",
    )
    .bind(session_id)
    .bind(RideStatus::Confirmed)
    .bind(bus_id)
    .bind(passengers)
    .bind(RideStatus::AwaitingConfirmation)
    .bind(RideStatus::PaymentFailed)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// The user declines (or the ride is given up): a ride that is still open becomes `rejected`. Only
/// changes `pending`, `awaiting_confirmation` and `payment_failed` rides. Returns false if the
/// ride was none of these.
pub async fn reject_open_session(db: &PgPool, session_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE ride_sessions SET status = $2, end_reason = 'declined', version = version + 1, updated_at = now()
         WHERE id = $1 AND (status = $3 OR status = $4 OR status = $5)",
    )
    .bind(session_id)
    .bind(RideStatus::Rejected)
    .bind(RideStatus::Pending)
    .bind(RideStatus::AwaitingConfirmation)
    .bind(RideStatus::PaymentFailed)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// "I am on the bus": starts a ride straight in `awaiting_confirmation`. A user has at most one open
/// ride (unique index), so if another request started one a moment ago nothing is inserted.
/// Returns the id of the ride, or None when none was inserted.
pub async fn insert_awaiting(
    db: &PgPool,
    user_id: Uuid,
    bus_id: i32,
    candidates: &[Candidate],
    expires_at: DateTime<Utc>,
) -> Result<Option<Uuid>, sqlx::Error> {
    let id = Uuid::new_v4();
    let result = sqlx::query(
        "INSERT INTO ride_sessions (id, user_id, bus_id, status, candidates, expires_at, manual)
         VALUES ($1, $2, $3, $4, $5, $6, true) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(user_id)
    .bind(bus_id)
    .bind(RideStatus::AwaitingConfirmation)
    .bind(Json(candidates))
    .bind(expires_at)
    .execute(db)
    .await?;
    Ok((result.rows_affected() == 1).then_some(id))
}
