use sqlx::PgPool;
use uuid::Uuid;

use super::admin_models::{
    BusRoute, ChargeAdminRow, ChargeKind, ChargesQuery, Identity, PassAdminRow, PushDeviceInfo,
    ReportAdminRow, RideRow, RideSighting, SightingInfo, StatusChange, TokenRow, TripAdminItem,
    UserRow, VerificationInfo,
};
use crate::modules::rides::models::RideStatus;

/// The columns of a `UserRow`, with the counts and the open ride.
macro_rules! user_select {
    () => {
        "SELECT u.id, u.tier, u.name, u.email, u.role, u.created_at, u.last_seen_at, u.default_source_id, u.settings,
                (SELECT count(*) FROM payment_methods m WHERE m.user_id = u.id) AS cards_count,
                (SELECT count(*) FROM passes p WHERE p.user_id = u.id) AS passes_count,
                (SELECT s.status FROM ride_sessions s
                  WHERE s.user_id = u.id AND s.status IN ('pending', 'awaiting_confirmation', 'payment_failed', 'confirmed')
                  ORDER BY s.updated_at DESC LIMIT 1) AS open_ride_status
         FROM users u "
    };
}

/// The columns of a `RideRow`.
macro_rules! ride_select {
    () => {
        "SELECT s.id AS session_id, s.user_id, s.status, s.bus_id, s.candidates, s.passengers, s.created_at, s.updated_at,
                s.confirmed_at, s.ended_at, s.end_reason, s.payment_reason, s.manual, s.qualification,
                (SELECT c.id FROM charges c WHERE c.session_id = s.id ORDER BY c.id DESC LIMIT 1) AS charge_id
         FROM ride_sessions s "
    };
}

pub async fn list_users(
    db: &PgPool,
    tier: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<UserRow>, sqlx::Error> {
    sqlx::query_as(concat!(
        user_select!(),
        "WHERE ($1::text IS NULL OR u.tier = $1) ORDER BY u.created_at DESC, u.id LIMIT $2 OFFSET $3"
    ))
    .bind(tier)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

pub async fn find_user(db: &PgPool, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    sqlx::query_as(concat!(user_select!(), "WHERE u.id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await
}

pub async fn identities_of(db: &PgPool, user_id: Uuid) -> Result<Vec<Identity>, sqlx::Error> {
    sqlx::query_as("SELECT provider, subject FROM auth_identities WHERE user_id = $1 ORDER BY provider, subject")
        .bind(user_id)
        .fetch_all(db)
        .await
}

pub async fn push_devices_of(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<PushDeviceInfo>, sqlx::Error> {
    sqlx::query_as(
        "SELECT platform, push_token FROM push_devices WHERE user_id = $1 ORDER BY push_token",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn tokens_of(db: &PgPool, user_id: Uuid) -> Result<Vec<TokenRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, device_id, expires_at, revoked_at, revoked_reason FROM refresh_tokens WHERE user_id = $1 ORDER BY id",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn list_sightings(
    db: &PgPool,
    user_id: Option<Uuid>,
    bus_id: Option<i32>,
    accepted: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<Vec<SightingInfo>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, received_at, user_id, bus_id, slot, slot_offset, rssi, clock_synced, accepted, reject_reason
         FROM sighting_log
         WHERE ($1::uuid IS NULL OR user_id = $1) AND ($2::int IS NULL OR bus_id = $2)
           AND ($3::boolean IS NULL OR accepted = $3)
         ORDER BY id DESC LIMIT $4 OFFSET $5",
    )
    .bind(user_id)
    .bind(bus_id)
    .bind(accepted)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

pub async fn list_rides(
    db: &PgPool,
    user_id: Option<Uuid>,
    bus_id: Option<i32>,
    status: Option<RideStatus>,
    limit: i64,
    offset: i64,
) -> Result<Vec<RideRow>, sqlx::Error> {
    sqlx::query_as(concat!(
        ride_select!(),
        "WHERE ($1::uuid IS NULL OR s.user_id = $1) AND ($2::int IS NULL OR s.bus_id = $2)
           AND ($3::text IS NULL OR s.status = $3)
         ORDER BY s.created_at DESC, s.id LIMIT $4 OFFSET $5"
    ))
    .bind(user_id)
    .bind(bus_id)
    .bind(status)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

pub async fn find_ride(db: &PgPool, session_id: Uuid) -> Result<Option<RideRow>, sqlx::Error> {
    sqlx::query_as(concat!(ride_select!(), "WHERE s.id = $1"))
        .bind(session_id)
        .fetch_optional(db)
        .await
}

pub async fn ride_history(db: &PgPool, session_id: Uuid) -> Result<Vec<StatusChange>, sqlx::Error> {
    sqlx::query_as("SELECT status, at FROM ride_events WHERE session_id = $1 ORDER BY id")
        .bind(session_id)
        .fetch_all(db)
        .await
}

/// What the user heard of the ride's bus from a little before the ride began until a little after it ended.
pub async fn ride_sightings(db: &PgPool, row: &RideRow) -> Result<Vec<RideSighting>, sqlx::Error> {
    sqlx::query_as(
        "SELECT slot, bus_id, rssi, received_at FROM sightings
         WHERE user_id = $1 AND bus_id = $2
           AND received_at >= $3 - interval '10 minutes'
           AND ($4::timestamptz IS NULL OR received_at <= $4 + interval '2 minutes')
         ORDER BY slot",
    )
    .bind(row.user_id)
    .bind(row.bus_id)
    .bind(row.created_at)
    .bind(row.ended_at)
    .fetch_all(db)
    .await
}

pub async fn list_charges(
    db: &PgPool,
    filter: &ChargesQuery,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChargeAdminRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT c.id, c.user_id, c.session_id, c.line, c.destination, c.amount_bani, c.status, c.passengers, c.paid_with,
                CASE WHEN EXISTS (SELECT 1 FROM payment_methods m WHERE m.id = c.paid_with) THEN 'card'
                     WHEN EXISTS (SELECT 1 FROM passes p WHERE p.id = c.paid_with) THEN 'pass' END AS source_kind,
                c.pass_product_id, c.started_at, c.ended_at, c.qr_token
         FROM charges c
         WHERE ($1::uuid IS NULL OR c.user_id = $1) AND ($2::text IS NULL OR c.status = $2)
           AND ($3::timestamptz IS NULL OR c.started_at >= $3) AND ($4::timestamptz IS NULL OR c.started_at <= $4)
           AND ($5::text IS NULL OR (c.session_id IS NOT NULL) = ($5 = 'ride'))
         ORDER BY c.started_at DESC, c.id LIMIT $6 OFFSET $7",
    )
    .bind(filter.user_id)
    .bind(filter.status)
    .bind(filter.from)
    .bind(filter.to)
    .bind(filter.kind.map(ChargeKind::as_str))
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

pub async fn list_trips(
    db: &PgPool,
    user_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<TripAdminItem>, sqlx::Error> {
    sqlx::query_as(
        "SELECT t.id, t.user_id, s.bus_id, c.session_id, t.charge_id, t.from_stop, t.to_stop, t.date, t.time, t.line,
                t.fare_bani, t.distance_km, t.duration_min
         FROM trips t
         JOIN charges c ON c.id = t.charge_id
         LEFT JOIN ride_sessions s ON s.id = c.session_id
         WHERE ($1::uuid IS NULL OR t.user_id = $1)
         ORDER BY t.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

pub async fn list_passes(
    db: &PgPool,
    user_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<PassAdminRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, user_id, name, holder, card_number, valid_from, valid_until, lines, theme, verified, last_renewed_at
         FROM passes WHERE ($1::uuid IS NULL OR user_id = $1) ORDER BY id DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

/// The reports that have not expired, with who made and who confirmed them.
pub async fn list_reports(
    db: &PgPool,
    line: Option<&str>,
    kind: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ReportAdminRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT r.id, r.kind, r.line, r.stop_name, r.stop_lat, r.stop_lon, r.bus_id,
                (SELECT count(*) FROM report_confirmations c WHERE c.report_id = r.id) AS confirmations,
                r.reporter_user_id,
                COALESCE((SELECT array_agg(c.user_id ORDER BY c.user_id) FROM report_confirmations c WHERE c.report_id = r.id),
                         '{}') AS confirmer_user_ids,
                r.created_at, r.expires_at
         FROM reports r
         WHERE r.expires_at > now() AND ($1::text IS NULL OR lower(r.line) = lower($1)) AND ($2::text IS NULL OR r.kind = $2)
         ORDER BY r.id DESC LIMIT $3 OFFSET $4",
    )
    .bind(line)
    .bind(kind)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

pub async fn list_verifications(
    db: &PgPool,
    inspector_id: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<VerificationInfo>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, at, inspector_id, charge_id, valid, paid, line, passengers, expires_at
         FROM ticket_verifications WHERE ($1::uuid IS NULL OR inspector_id = $1) ORDER BY id DESC LIMIT $2 OFFSET $3",
    )
    .bind(inspector_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
}

/// Every bus with the named points (the stops) of its route, to name the line and the destination of a ride.
pub async fn bus_routes(db: &PgPool) -> Result<Vec<BusRoute>, sqlx::Error> {
    sqlx::query_as(
        "SELECT b.bus_id, r.route_id, r.name,
                COALESCE((SELECT jsonb_agg(p ORDER BY o) FROM jsonb_array_elements(r.points) WITH ORDINALITY t(p, o)
                           WHERE p ? 'name'), '[]'::jsonb) AS points
         FROM buses b JOIN routes r USING (route_id)",
    )
    .fetch_all(db)
    .await
}
