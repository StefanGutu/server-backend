//! Rules of the charges.

use sqlx::PgPool;
use uuid::Uuid;

use chrono::DateTime;

use super::{
    models::{
        Charge, ChargeStatus, NewCharge, PublicKeyInfo, PublicKeysResponse, TicketKey, VerifyAnswer,
    },
    repo, statement,
    ticket::{self, TicketClaims},
};
use crate::{
    error::AppError,
    extract::ListQuery,
    modules::{
        auth::{self, models::User},
        extras::trips,
    },
};

/// GET /statements/{YYYY-MM}: the PDF of the month with the user's trips and their total. 400 for a month
/// that is not `YYYY-MM`, 404 when the user has no trip in it.
pub async fn statement_pdf(db: &PgPool, user_id: Uuid, month: &str) -> Result<Vec<u8>, AppError> {
    let (first_day, end_day) = statement::parse_month(month)?;
    let month_trips = trips::service::trips_between(db, user_id, first_day, end_day).await?;
    if month_trips.is_empty() {
        return Err(AppError::NotFound);
    }
    Ok(statement::build_statement(
        &statement::month_label(first_day),
        &month_trips,
    ))
}

/// GET /charges: the user's charges, newest first, `limit` at a time. `before` is the id of
/// the last charge the app has; an id that is not one of the user's charges is a 404.
pub async fn list_charges(
    db: &PgPool,
    user_id: Uuid,
    query: &ListQuery,
) -> Result<Vec<Charge>, AppError> {
    let limit = query.page_size()?;
    let after = match query.before {
        Some(charge_id) => Some(
            repo::find_cursor(db, user_id, charge_id)
                .await?
                .ok_or(AppError::NotFound)?,
        ),
        None => None,
    };
    let rows = repo::list_charges(db, user_id, after, limit).await?;
    Ok(rows.into_iter().map(Charge::from).collect())
}

/// Creates the first signing key if there is none. Called at every start; does nothing when a key exists.
pub async fn ensure_ticket_key(db: &PgPool) -> Result<(), AppError> {
    if !repo::list_keys(db).await?.is_empty() {
        return Ok(());
    }
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| {
        tracing::error!("no secure random bytes for the ticket key: {e}");
        AppError::Server
    })?;
    let key = TicketKey::from_seed(Uuid::now_v7().to_string(), &seed)?;
    repo::insert_key(db, &key).await?;
    tracing::info!(kid = %key.kid, "ticket signing key created");
    Ok(())
}

/// Signs a ticket for a charge with the newest key. Used when a ride is confirmed (step 19).
pub async fn sign_ticket(db: &PgPool, claims: &TicketClaims) -> Result<String, AppError> {
    let keys = repo::list_keys(db).await?;
    let newest = keys.first().ok_or(AppError::Server)?;
    ticket::sign(newest, claims)
}

/// GET /tickets/public-key: every public key, so the app and the inspector can check a ticket.
pub async fn public_keys(db: &PgPool) -> Result<PublicKeysResponse, AppError> {
    let keys = repo::list_keys(db).await?;
    Ok(PublicKeysResponse {
        keys: keys.into_iter().map(PublicKeyInfo::from).collect(),
    })
}

/// Creates a charge and its QR ticket (valid 60 minutes from `new.started_at`). The charge id is
/// in the ticket (`cid`), so the id is made first and the ticket is signed before the row is stored.
pub async fn create_charge(db: &PgPool, new: &NewCharge) -> Result<Charge, AppError> {
    let id = Uuid::now_v7();
    let claims = TicketClaims::new(id, &new.line, new.passengers, new.started_at.timestamp());
    let qr_token = sign_ticket(db, &claims).await?;
    let row = repo::insert_charge(db, id, new, &qr_token).await?;
    Ok(Charge::from(row))
}

/// The charge of a ride, or None if it has none.
pub async fn charge_for_session(
    db: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<Option<Charge>, AppError> {
    Ok(repo::find_by_session(db, user_id, session_id)
        .await?
        .map(Charge::from))
}

/// Cancels a charge (for example when the ride could not be confirmed after it was made).
pub async fn cancel_charge(db: &PgPool, charge_id: Uuid) -> Result<(), AppError> {
    Ok(repo::cancel_charge(db, charge_id).await?)
}

/// The ride is over: its charge gets an end (`endedAt`) and, if it was `in_progress`, becomes
/// `completed`. Returns how many charges changed (0 when the ride had none).
pub async fn complete_ride_charge(db: &PgPool, session_id: Uuid) -> Result<u64, AppError> {
    Ok(repo::complete_for_session(db, session_id).await?)
}

/// What the inspector is told about a ticket. Pure.
/// - `valid`: the token is ours and `now` (unix seconds) is before its end.
/// - `line`, `passengers`, `expiresAt`: from the token, also when it has expired; `null` when the
///   token is not ours.
/// - `paid`: its charge exists and was not cancelled.
pub fn verify_answer(
    claims: Option<&TicketClaims>,
    now: i64,
    charge_status: Option<ChargeStatus>,
) -> VerifyAnswer {
    VerifyAnswer {
        valid: claims.is_some_and(|c| c.exp > now),
        line: claims.map(|c| c.line.clone()),
        passengers: claims.map(|c| c.pax),
        expires_at: claims.and_then(|c| DateTime::from_timestamp(c.exp, 0)),
        paid: charge_status.is_some_and(|status| status != ChargeStatus::Cancelled),
    }
}

/// POST /tickets/verify: an inspector checks a `qrToken`. Only for the inspector role (403 for others).
pub async fn verify_ticket(
    db: &PgPool,
    inspector: &User,
    qr_token: &str,
) -> Result<VerifyAnswer, AppError> {
    auth::service::require_inspector(inspector)?;
    let keys = repo::list_keys(db).await?;
    let claims = ticket::read(qr_token, &keys);
    let charge_status = match &claims {
        Some(claims) => repo::find_charge_status(db, claims.cid).await?,
        None => None,
    };
    let answer = verify_answer(
        claims.as_ref(),
        chrono::Utc::now().timestamp(),
        charge_status,
    );
    tracing::info!(inspector_id = %inspector.id, valid = answer.valid, paid = answer.paid, "ticket checked");
    // The scan log is only for the display info: a failure to write it must not refuse the check.
    let charge_id = claims.as_ref().map(|c| c.cid);
    if let Err(e) = repo::insert_verification(db, inspector.id, charge_id, &answer).await {
        tracing::warn!("cannot note the ticket check: {e}");
    }
    Ok(answer)
}
