use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

/// User settings (server-api.md). The one place that holds the default values.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_charge: bool,
    pub trip_reminders: bool,
    pub arrival_alerts: bool,
    pub disruptions: bool,
    pub location_sharing: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_charge: false,
            trip_reminders: false,
            arrival_alerts: false,
            disruptions: true,
            location_sharing: false,
        }
    }
}

/// What PATCH /me may change. Only the keys that are sent change, the rest stay.
/// Unknown keys are an error, so a typo is not silently ignored.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_charge: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trip_reminders: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrival_alerts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disruptions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_sharing: Option<bool>,
}

/// Body of PATCH /me (server-api.md): `{ "name?", "settings?" }`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchMe {
    pub name: Option<String>,
    pub settings: Option<SettingsPatch>,
}

/// anonymous: device login only, cannot pay. account: linked to a phone/google/apple identity.
/// Stored as text in `users.tier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Anonymous,
    Account,
}

/// What a user may do. Stored as text in `users.role`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum UserRole {
    User,
    /// Can verify tickets (step 22).
    Inspector,
}

/// A row of the `users` table.
#[derive(FromRow)]
pub struct User {
    pub id: Uuid,
    pub tier: Tier,
    pub role: UserRole,
    pub name: Option<String>,
    pub email: Option<String>,
    pub settings: Json<Settings>,
    pub default_source_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// What GET /me returns (server-api.md). `name` and `email` are left out when empty.
#[derive(Serialize)]
pub struct Me {
    pub id: Uuid,
    pub tier: Tier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Linked phone/google/apple identities. Always empty until step 26.
    pub identities: Vec<String>,
    pub settings: Settings,
}

impl From<User> for Me {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            tier: user.tier,
            name: user.name,
            email: user.email,
            identities: Vec::new(),
            settings: user.settings.0,
        }
    }
}
