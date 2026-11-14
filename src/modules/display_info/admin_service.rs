//! The views for a screen of what users, rides, charges and so on are doing. `resolve_paging`, `uuid_time`,
//! `build_sessions`, `ride_item`, `charge_item`, `pass_item` and `report_item` are pure.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    admin_models::{
        ChargeAdminItem, ChargeAdminRow, ChargesQuery, OwnerQuery, Paging, PassAdminItem,
        PassAdminRow, ReportAdminItem, ReportAdminRow, RideDetail, RideListItem, RideRow,
        RidesQuery, Session, SightingInfo, SightingsQuery, TokenRow, TripAdminItem, UserDetail,
        UserListItem, UserRow, UsersQuery, VerificationInfo, VerificationsQuery,
    },
    admin_repo as repo,
    service::clean_limit,
};
use crate::{
    error::AppError,
    modules::{charges, devices::models::Route},
};

/// `limit` (default 100, at most 1000) and `offset` (default 0). A `limit` below 1 or an `offset` below 0 is a 400. Pure.
pub fn resolve_paging(paging: &Paging) -> Result<(i64, i64), AppError> {
    let limit = clean_limit(paging.limit)?;
    let offset = paging.offset.unwrap_or(0);
    if offset < 0 {
        return Err(AppError::Validation(
            "offset must be 0 or more.".to_string(),
        ));
    }
    Ok((limit, offset))
}

/// The time inside a time-ordered (v7) id: when the row was made. None for any other kind of id. Pure.
pub fn uuid_time(id: Uuid) -> Option<DateTime<Utc>> {
    let (secs, nanos) = id.get_timestamp()?.to_unix();
    DateTime::from_timestamp(i64::try_from(secs).ok()?, nanos)
}

/// The logins of a user, one for each phone: the first token began it, the newest was the last refresh. It is
/// `revoked` when every token of the phone is, and then `revoked_reason` says why (`logout` or `token_reuse`;
/// a token that was only used up for a refresh is not a reason). Phones in the order they first logged in. Pure.
pub fn build_sessions(tokens: &[TokenRow]) -> Vec<Session> {
    let mut devices: Vec<&str> = Vec::new();
    for token in tokens {
        if !devices.contains(&token.device_id.as_str()) {
            devices.push(&token.device_id);
        }
    }
    let mut sessions: Vec<Session> = devices
        .into_iter()
        .map(|device| {
            let own: Vec<&TokenRow> = tokens.iter().filter(|t| t.device_id == device).collect();
            let times: Vec<DateTime<Utc>> = own.iter().filter_map(|t| uuid_time(t.id)).collect();
            let revoked = own.iter().all(|t| t.revoked_at.is_some());
            let revoked_reason = if revoked {
                own.iter()
                    .filter(|t| t.revoked_reason.as_deref().is_some_and(|r| r != "rotated"))
                    .max_by_key(|t| t.revoked_at)
                    .and_then(|t| t.revoked_reason.clone())
            } else {
                None
            };
            Session {
                device_id: device.to_string(),
                created_at: times.iter().min().copied(),
                last_refresh_at: times.iter().max().copied(),
                expires_at: own
                    .iter()
                    .map(|t| t.expires_at)
                    .max()
                    .unwrap_or_else(Utc::now),
                revoked,
                revoked_reason,
            }
        })
        .collect();
    sessions.sort_by_key(|s| s.created_at);
    sessions
}

fn user_item(row: UserRow) -> UserListItem {
    UserListItem {
        id: row.id,
        tier: row.tier,
        name: row.name,
        email: row.email,
        roles: vec![row.role],
        created_at: row.created_at,
        last_seen_at: row.last_seen_at,
        cards_count: row.cards_count,
        passes_count: row.passes_count,
        open_ride_status: row.open_ride_status,
    }
}

pub async fn list_users(db: &PgPool, query: &UsersQuery) -> Result<Vec<UserListItem>, AppError> {
    let (limit, offset) = resolve_paging(&query.paging())?;
    let rows = repo::list_users(db, query.tier.as_deref(), limit, offset).await?;
    Ok(rows.into_iter().map(user_item).collect())
}

pub async fn user_detail(db: &PgPool, id: Uuid) -> Result<UserDetail, AppError> {
    let row = repo::find_user(db, id).await?.ok_or(AppError::NotFound)?;
    let identities = repo::identities_of(db, id).await?;
    let push_devices = repo::push_devices_of(db, id).await?;
    let sessions = build_sessions(&repo::tokens_of(db, id).await?);
    let settings = row.settings.0.clone();
    let default_source_id = row.default_source_id;
    let item = user_item(row);
    Ok(UserDetail {
        id: item.id,
        tier: item.tier,
        name: item.name,
        email: item.email,
        roles: item.roles,
        created_at: item.created_at,
        last_seen_at: item.last_seen_at,
        identities,
        settings,
        default_source_id,
        sessions,
        push_devices,
        cards_count: item.cards_count,
        passes_count: item.passes_count,
        open_ride_status: item.open_ride_status,
    })
}

pub async fn list_sightings(
    db: &PgPool,
    query: &SightingsQuery,
) -> Result<Vec<SightingInfo>, AppError> {
    let (limit, offset) = resolve_paging(&query.paging())?;
    Ok(repo::list_sightings(
        db,
        query.user_id,
        query.bus_id,
        query.accepted,
        limit,
        offset,
    )
    .await?)
}

/// The line and destination of every bus, to put in the rides.
async fn bus_lines(db: &PgPool) -> Result<HashMap<i32, (String, Option<String>)>, AppError> {
    let mut lines = HashMap::new();
    for bus in repo::bus_routes(db).await? {
        let route = Route {
            route_id: bus.route_id,
            name: bus.name,
            points: bus.points,
        };
        let destination = route.destination().map(str::to_string);
        lines.insert(bus.bus_id, (route.name, destination));
    }
    Ok(lines)
}

/// A ride row as the list shows it. The line and the destination come from the bus's route. Pure.
pub fn ride_item(row: &RideRow, lines: &HashMap<i32, (String, Option<String>)>) -> RideListItem {
    let (line, destination) = match lines.get(&row.bus_id) {
        Some((line, destination)) => (Some(line.clone()), destination.clone()),
        None => (None, None),
    };
    RideListItem {
        session_id: row.session_id,
        user_id: row.user_id,
        status: row.status,
        bus_id: row.bus_id,
        line,
        destination,
        candidates: row.candidates.0.clone(),
        passengers: row.passengers,
        created_at: row.created_at,
        updated_at: row.updated_at,
        confirmed_at: row.confirmed_at,
        ended_at: row.ended_at,
        end_reason: row.end_reason.clone(),
        payment_reason: row.payment_reason,
        charge_id: row.charge_id,
        manual: row.manual,
    }
}

pub async fn list_rides(db: &PgPool, query: &RidesQuery) -> Result<Vec<RideListItem>, AppError> {
    let (limit, offset) = resolve_paging(&query.paging())?;
    let rows =
        repo::list_rides(db, query.user_id, query.bus_id, query.status, limit, offset).await?;
    let lines = bus_lines(db).await?;
    Ok(rows.iter().map(|row| ride_item(row, &lines)).collect())
}

pub async fn ride_detail(db: &PgPool, session_id: Uuid) -> Result<RideDetail, AppError> {
    let row = repo::find_ride(db, session_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let lines = bus_lines(db).await?;
    let status_history = repo::ride_history(db, session_id).await?;
    let sightings = repo::ride_sightings(db, &row).await?;
    let charge = charges::service::charge_for_session(db, row.user_id, session_id).await?;
    Ok(RideDetail {
        ride: ride_item(&row, &lines),
        status_history,
        sightings,
        qualification: row.qualification.map(|q| q.0),
        charge,
    })
}

/// A charge row as the list shows it: `kind` is `pass_renewal` when it has no ride, `ride` otherwise. Pure.
pub fn charge_item(row: ChargeAdminRow) -> ChargeAdminItem {
    ChargeAdminItem {
        id: row.id,
        user_id: row.user_id,
        session_id: row.session_id,
        kind: if row.session_id.is_none() {
            "pass_renewal"
        } else {
            "ride"
        },
        line: row.line,
        destination: row.destination,
        amount: row.amount_bani,
        currency: "RON",
        status: row.status,
        passengers: row.passengers,
        paid_with: row.paid_with,
        source_kind: row.source_kind,
        pass_product_id: row.pass_product_id,
        started_at: row.started_at,
        ended_at: row.ended_at,
        qr_token: row.qr_token,
    }
}

pub async fn list_charges(
    db: &PgPool,
    query: &ChargesQuery,
) -> Result<Vec<ChargeAdminItem>, AppError> {
    let (limit, offset) = resolve_paging(&query.paging())?;
    let rows = repo::list_charges(db, query.user_id, query.status, limit, offset).await?;
    Ok(rows.into_iter().map(charge_item).collect())
}

pub async fn list_trips(db: &PgPool, query: &OwnerQuery) -> Result<Vec<TripAdminItem>, AppError> {
    let (limit, offset) = resolve_paging(&query.paging())?;
    Ok(repo::list_trips(db, query.user_id, limit, offset).await?)
}

/// A pass row as the list shows it; `created_at` is the time inside its id. Pure.
pub fn pass_item(row: PassAdminRow) -> PassAdminItem {
    PassAdminItem {
        created_at: uuid_time(row.id),
        id: row.id,
        user_id: row.user_id,
        name: row.name,
        holder: row.holder,
        card_number: row.card_number,
        valid_from: row.valid_from,
        valid_until: row.valid_until,
        lines: row.lines.0,
        theme: row.theme,
        verified: row.verified,
        last_renewed_at: row.last_renewed_at,
    }
}

pub async fn list_passes(db: &PgPool, query: &OwnerQuery) -> Result<Vec<PassAdminItem>, AppError> {
    let (limit, offset) = resolve_paging(&query.paging())?;
    let rows = repo::list_passes(db, query.user_id, limit, offset).await?;
    Ok(rows.into_iter().map(pass_item).collect())
}

/// A report row as the display sees it. Pure.
pub fn report_item(row: ReportAdminRow) -> ReportAdminItem {
    ReportAdminItem {
        id: row.id,
        kind: row.kind,
        line: row.line,
        stop_name: row.stop_name,
        stop_lat: row.stop_lat,
        stop_lon: row.stop_lon,
        bus_id: row.bus_id,
        confirmations: row.confirmations,
        reporter_user_id: row.reporter_user_id,
        confirmer_user_ids: row.confirmer_user_ids,
        created_at: row.created_at,
        expires_at: row.expires_at,
    }
}

/// `GET /reports` for the display: the reports that have not expired, filtered by line and kind and paged.
pub async fn list_reports(
    db: &PgPool,
    line: Option<&str>,
    kind: Option<&str>,
    paging: &Paging,
) -> Result<Vec<ReportAdminItem>, AppError> {
    let (limit, offset) = resolve_paging(paging)?;
    let rows = repo::list_reports(db, line, kind, limit, offset).await?;
    Ok(rows.into_iter().map(report_item).collect())
}

pub async fn list_verifications(
    db: &PgPool,
    query: &VerificationsQuery,
) -> Result<Vec<VerificationInfo>, AppError> {
    let (limit, offset) = resolve_paging(&query.paging())?;
    Ok(repo::list_verifications(db, query.inspector_id, limit, offset).await?)
}
