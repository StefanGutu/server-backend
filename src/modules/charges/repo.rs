use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::models::{ChargeRow, ChargeStatus, NewCharge, TicketKey};

/// Where a list continues: the (started_at, id) of one charge.
pub type Cursor = (DateTime<Utc>, Uuid);

/// The place of one of the user's charges in the list, or None if the user has no such charge.
pub async fn find_cursor(
    db: &PgPool,
    user_id: Uuid,
    charge_id: Uuid,
) -> Result<Option<Cursor>, sqlx::Error> {
    sqlx::query_as("SELECT started_at, id FROM charges WHERE id = $1 AND user_id = $2")
        .bind(charge_id)
        .bind(user_id)
        .fetch_optional(db)
        .await
}

/// The user's charges, newest first. With a cursor: only those older than it.
pub async fn list_charges(
    db: &PgPool,
    user_id: Uuid,
    after: Option<Cursor>,
    limit: i64,
) -> Result<Vec<ChargeRow>, sqlx::Error> {
    let (started_before, id_before) = match after {
        Some((started_at, id)) => (Some(started_at), Some(id)),
        None => (None, None),
    };
    sqlx::query_as(
        "SELECT id, user_id, session_id, line, destination, amount_bani, status, paid_with,
                passengers, started_at, ended_at, qr_token
         FROM charges
         WHERE user_id = $1
           AND ($2::timestamptz IS NULL OR (started_at, id) < ($2, $3::uuid))
         ORDER BY started_at DESC, id DESC
         LIMIT $4",
    )
    .bind(user_id)
    .bind(started_before)
    .bind(id_before)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// Stores a signing key.
pub async fn insert_key(db: &PgPool, key: &TicketKey) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO ticket_keys (kid, public_key, private_key) VALUES ($1, $2, $3)")
        .bind(&key.kid)
        .bind(&key.public_key)
        .bind(&key.private_key)
        .execute(db)
        .await?;
    Ok(())
}

/// All signing keys, the newest first (the kid is a time-ordered uuid).
pub async fn list_keys(db: &PgPool) -> Result<Vec<TicketKey>, sqlx::Error> {
    sqlx::query_as("SELECT kid, public_key, private_key FROM ticket_keys ORDER BY kid DESC")
        .fetch_all(db)
        .await
}

/// Stores a charge with its ticket token.
pub async fn insert_charge(
    db: &PgPool,
    id: Uuid,
    new: &NewCharge,
    qr_token: &str,
) -> Result<ChargeRow, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO charges (id, user_id, session_id, line, destination, amount_bani, status, paid_with,
                              passengers, started_at, ended_at, qr_token)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id, user_id, session_id, line, destination, amount_bani, status, paid_with,
                   passengers, started_at, ended_at, qr_token",
    )
    .bind(id)
    .bind(new.user_id)
    .bind(new.session_id)
    .bind(&new.line)
    .bind(&new.destination)
    .bind(new.amount_bani)
    .bind(new.status)
    .bind(new.paid_with)
    .bind(new.passengers)
    .bind(new.started_at)
    .bind(new.ended_at)
    .bind(qr_token)
    .fetch_one(db)
    .await
}

/// The newest charge of a ride, or None.
pub async fn find_by_session(
    db: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<Option<ChargeRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, user_id, session_id, line, destination, amount_bani, status, paid_with,
                passengers, started_at, ended_at, qr_token
         FROM charges WHERE user_id = $1 AND session_id = $2 ORDER BY started_at DESC, id DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(session_id)
    .fetch_optional(db)
    .await
}

/// Marks a charge as cancelled.
pub async fn cancel_charge(db: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE charges SET status = $2 WHERE id = $1")
        .bind(id)
        .bind(ChargeStatus::Cancelled)
        .execute(db)
        .await?;
    Ok(())
}

/// The ride of these charges is over: a charge that was `in_progress` becomes `completed`; one that
/// was `covered_by_pass` keeps its status; both get `ended_at`. A cancelled charge, or one that has
/// an end already, is left alone. Returns how many charges changed.
pub async fn complete_for_session(db: &PgPool, session_id: Uuid) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE charges
         SET status = CASE WHEN status = $2 THEN $3 ELSE status END, ended_at = now()
         WHERE session_id = $1 AND ended_at IS NULL AND status <> $4",
    )
    .bind(session_id)
    .bind(ChargeStatus::InProgress)
    .bind(ChargeStatus::Completed)
    .bind(ChargeStatus::Cancelled)
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

/// The status of a charge, or None if there is no such charge. Any charge: the inspector checks
/// tickets of other users.
pub async fn find_charge_status(
    db: &PgPool,
    charge_id: Uuid,
) -> Result<Option<ChargeStatus>, sqlx::Error> {
    sqlx::query_scalar("SELECT status FROM charges WHERE id = $1")
        .bind(charge_id)
        .fetch_optional(db)
        .await
}
