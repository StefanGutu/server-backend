//! Reports of problems on a line. `clean_line` and `pick_bus` are pure.

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{Report, ReportInput},
    repo,
};
use crate::{
    error::AppError,
    modules::{devices, extras::trips::service::nearest_stop, sightings, telemetry},
};

/// How long a report is shown: 30 minutes.
pub const REPORT_LIFETIME_SECS: i64 = 30 * 60;

/// The most reports `GET /reports` sends.
const MAX_REPORTS: i64 = 100;

/// A report can only be made for a bus the user heard in the last 10 minutes (20 slots of 30 seconds).
const HEARD_WITHIN_SECS: i64 = 10 * 60;

/// The position of the bus must be from the last 5 minutes.
const POSITION_WITHIN_SECS: i64 = 5 * 60;

/// The longest line name accepted (characters). Real ones are 2 to 4.
const MAX_LINE_LEN: usize = 20;

/// A `line` is trimmed, must not be blank and has at most `MAX_LINE_LEN` characters. Pure.
pub fn clean_line(line: &str) -> Result<&str, AppError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(AppError::Validation("line must not be empty.".to_string()));
    }
    if line.chars().count() > MAX_LINE_LEN {
        return Err(AppError::Validation(format!(
            "line must have at most {MAX_LINE_LEN} characters."
        )));
    }
    Ok(line)
}

/// Of the buses the user heard (bus id and its line), the first one that runs this line (the line in
/// any letter case). Pure.
pub fn pick_bus(heard: &[(i32, String)], line: &str) -> Option<i32> {
    heard
        .iter()
        .find(|(_, bus_line)| bus_line.eq_ignore_ascii_case(line))
        .map(|(bus_id, _)| *bus_id)
}

/// GET /reports: what has not expired.
pub async fn list_reports(db: &PgPool) -> Result<Vec<Report>, AppError> {
    Ok(repo::list_active(db, MAX_REPORTS).await?)
}

/// POST /reports: the user says something is wrong on a line. The stop is the one nearest to the bus of that
/// line the user was on, so the user has to have heard it a moment ago (422 if not). The same thing already
/// reported for that line and stop is not made twice: the user confirms it. The user counts as a confirmation.
pub async fn create_report(
    db: &PgPool,
    user_id: Uuid,
    input: &ReportInput,
    allow_sim: bool,
) -> Result<Report, AppError> {
    let line = clean_line(&input.line)?;
    let (line, stop_name) = find_stop(db, user_id, line, allow_sim).await?;

    let id = match repo::find_same(db, input.kind, &line, &stop_name).await? {
        Some(existing) => existing,
        None => {
            let id = Uuid::now_v7();
            let expires_at = Utc::now() + chrono::Duration::seconds(REPORT_LIFETIME_SECS);
            repo::insert_report(db, id, input.kind, &line, &stop_name, expires_at).await?;
            tracing::info!(report_id = %id, kind = ?input.kind, line = %line, stop = %stop_name, "report made");
            id
        }
    };
    repo::add_confirmation(db, id, user_id).await?;
    repo::find_active(db, id).await?.ok_or(AppError::NotFound)
}

/// POST /reports/{id}/confirm: the user says the report is true. 404 for a report that does not exist or has
/// expired. Confirming again changes nothing.
pub async fn confirm_report(
    db: &PgPool,
    user_id: Uuid,
    report_id: Uuid,
) -> Result<Report, AppError> {
    repo::find_active(db, report_id)
        .await?
        .ok_or(AppError::NotFound)?;
    repo::add_confirmation(db, report_id, user_id).await?;
    repo::find_active(db, report_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// Deletes the reports that have expired (called by the background task).
pub async fn delete_expired(db: &PgPool) -> Result<u64, AppError> {
    Ok(repo::delete_expired(db).await?)
}

/// The name of the line (as the route calls it) and the stop nearest to the user's bus of that line.
async fn find_stop(
    db: &PgPool,
    user_id: Uuid,
    line: &str,
    allow_sim: bool,
) -> Result<(String, String), AppError> {
    let no_bus = || {
        AppError::Unprocessable(format!(
            "You have not been on a bus of line {line} in the last 10 minutes."
        ))
    };
    let now = Utc::now().timestamp();
    let heard = sightings::service::recent_bus_ids(db, user_id, now - HEARD_WITHIN_SECS).await?;

    let mut heard_with_line = Vec::new();
    for bus_id in heard {
        if let Some(bus) = devices::service::bus_line(db, bus_id).await? {
            heard_with_line.push((bus_id, bus.line));
        }
    }
    let bus_id = pick_bus(&heard_with_line, line).ok_or_else(no_bus)?;
    let line_name = heard_with_line
        .iter()
        .find(|(id, _)| *id == bus_id)
        .map(|(_, name)| name.clone())
        .ok_or_else(no_bus)?;

    let positions = telemetry::service::bus_positions(
        db,
        bus_id,
        now - POSITION_WITHIN_SECS,
        now + 1,
        allow_sim,
    )
    .await?;
    let position = positions.last().copied().ok_or_else(|| {
        AppError::Unprocessable(
            "The bus has not sent its position in the last 5 minutes.".to_string(),
        )
    })?;
    let route = devices::service::route_of_bus(db, bus_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let stop = nearest_stop(&route.points.0, position)
        .ok_or_else(|| AppError::Unprocessable("The line has no stop.".to_string()))?;
    Ok((line_name, stop.to_string()))
}
