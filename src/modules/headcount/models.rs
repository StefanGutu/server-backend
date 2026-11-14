use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// The newest photo of a bus, when the model has not counted it yet.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Uncounted {
    pub bus_id: i32,
    pub file_path: String,
}

/// What the model saw in the newest photo of a bus that was counted.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct LatestCount {
    pub people_count: i32,
    pub counted_at: DateTime<Utc>,
    pub file_path: String,
}

/// A user of the app who is heard on a bus now.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Rider {
    pub user_id: Uuid,
    /// The passengers the user paid for: those of their confirmed ride on this bus, or 1 (themselves) when they have none.
    pub passengers: i32,
    /// The user has a ticket that is good now: a confirmed ride on this bus whose charge is covered by a pass, or paid and
    /// started less than `TICKET_VALID_SECS` (60 minutes) ago.
    pub ticket_valid: bool,
}

/// How the people in a bus compare with those who paid (the numbers `summarize` works out; step 42 adds the bus and
/// the photo around them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// What the model saw in the newest counted photo. `None` when there is none, or it is too old to be true now.
    pub people_counted: Option<i32>,
    /// Users of the app heard on the bus now.
    pub devices_heard: i64,
    /// Of those, the ones with a valid ticket.
    pub devices_paid: i64,
    /// `devices_heard - devices_paid`: they have the app and did not pay (yet).
    pub devices_unpaid: i64,
    /// The passengers of the valid tickets (one ticket can cover several people).
    pub paid_passengers: i64,
    /// `max(0, people_counted - paid_passengers)`: an estimate of the people who ride without a ticket.
    /// `None` when `people_counted` is `None`.
    pub without_ticket: Option<i64>,
}

/// What `GET /buses/{bus_id}/headcount` answers: how many people are in the bus, how many paid and how many have no ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BusHeadcount {
    pub bus_id: i32,
    /// When the newest counted photo was counted (even when it is too old to be shown as `people_counted`, so the
    /// screen can say how old it is), and which photo it was (the id of `GET /buses/{bus_id}/frames`).
    #[serde(serialize_with = "crate::extract::utc_seconds_opt")]
    pub counted_at: Option<DateTime<Utc>>,
    pub frame_id: Option<Uuid>,
    #[serde(flatten)]
    pub summary: Summary,
}
