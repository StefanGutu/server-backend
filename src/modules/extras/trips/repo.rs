use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use super::models::{NewTrip, RecentTrip, SavedRoute};

/// Stores a trip. A charge has one trip: a second one for the same charge is skipped. Returns whether one was stored.
pub async fn insert_trip(db: &PgPool, id: Uuid, trip: &NewTrip) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO trips (id, user_id, charge_id, from_stop, to_stop, date, time, line, fare_bani, distance_km, duration_min)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) ON CONFLICT (charge_id) DO NOTHING",
    )
    .bind(id)
    .bind(trip.user_id)
    .bind(trip.charge_id)
    .bind(&trip.from_stop)
    .bind(&trip.to_stop)
    .bind(trip.date)
    .bind(trip.time)
    .bind(&trip.line)
    .bind(trip.fare_bani)
    .bind(trip.distance_km)
    .bind(trip.duration_min)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// True when the user has a trip with this id.
pub async fn trip_exists(db: &PgPool, user_id: Uuid, trip_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM trips WHERE id = $1 AND user_id = $2)")
        .bind(trip_id)
        .bind(user_id)
        .fetch_one(db)
        .await
}

/// The user's trips, newest first (the ids are time-ordered). With `before`: only the older ones.
pub async fn list_trips(
    db: &PgPool,
    user_id: Uuid,
    before: Option<Uuid>,
    limit: i64,
) -> Result<Vec<RecentTrip>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, from_stop, to_stop, date, time, line, fare_bani, distance_km, duration_min
         FROM trips WHERE user_id = $1 AND ($2::uuid IS NULL OR id < $2)
         ORDER BY id DESC LIMIT $3",
    )
    .bind(user_id)
    .bind(before)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// The routes the user keeps.
pub async fn list_saved_routes(db: &PgPool, user_id: Uuid) -> Result<Vec<SavedRoute>, sqlx::Error> {
    sqlx::query_as("SELECT id, from_stop, to_stop, line FROM saved_routes WHERE user_id = $1 ORDER BY from_stop, to_stop, id")
        .bind(user_id)
        .fetch_all(db)
        .await
}

/// The user's trips whose day is from `first_day` up to (not including) `end_day`, oldest first.
pub async fn list_trips_between(
    db: &PgPool,
    user_id: Uuid,
    first_day: NaiveDate,
    end_day: NaiveDate,
) -> Result<Vec<RecentTrip>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, from_stop, to_stop, date, time, line, fare_bani, distance_km, duration_min
         FROM trips WHERE user_id = $1 AND date >= $2 AND date < $3
         ORDER BY date, time, id",
    )
    .bind(user_id)
    .bind(first_day)
    .bind(end_day)
    .fetch_all(db)
    .await
}
