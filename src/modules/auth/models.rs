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
    /// Linked phone / Google / Apple identities.
    pub identities: Vec<Identity>,
    pub settings: Settings,
}

impl Me {
    /// The user as the app sees them, with the identities that are linked to them.
    pub fn new(user: User, identities: Vec<Identity>) -> Self {
        Self {
            id: user.id,
            tier: user.tier,
            name: user.name,
            email: user.email,
            identities,
            settings: user.settings.0,
        }
    }
}

impl From<User> for Me {
    /// A user with no identity linked.
    fn from(user: User) -> Self {
        Self::new(user, Vec::new())
    }
}

/// Where an identity comes from. Stored as text in `auth_identities.provider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum Provider {
    Google,
    Apple,
    Phone,
}

/// A linked identity, as the app sees it in `GET /me`: the provider and who it is there
/// (for a phone: the number).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct Identity {
    pub provider: Provider,
    pub subject: String,
}

/// Body of POST /auth/otp/start (server-api.md): `{ "phone" }`.
#[derive(Debug, Deserialize)]
pub struct OtpStartInput {
    pub phone: String,
}

/// Body of POST /auth/link (server-api.md): `{ provider, idToken | phone, code }`.
/// `idToken` is for Google and Apple (step 27).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkInput {
    pub provider: Provider,
    pub id_token: Option<String>,
    pub phone: Option<String>,
    pub code: Option<String>,
}

/// Body of POST /auth/signin (server-api.md): `{ provider, idToken | phone, code, deviceId }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigninInput {
    pub provider: Provider,
    pub id_token: Option<String>,
    pub phone: Option<String>,
    pub code: Option<String>,
    pub device_id: String,
}

/// What POST /auth/link and POST /auth/signin return (server-api.md):
/// `{ "userId", "tier", "accessToken", "refreshToken" }`. Not the same four fields as
/// `TokenReply`: there is a `tier` and no `expiresIn`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginReply {
    pub user_id: Uuid,
    pub tier: Tier,
    pub access_token: String,
    pub refresh_token: String,
}

/// Body of POST /auth/anonymous (server-api.md): `{ "deviceId" }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnonymousInput {
    pub device_id: String,
}

/// Body of POST /auth/refresh (server-api.md): `{ "refreshToken" }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshInput {
    pub refresh_token: String,
}

/// What POST /auth/anonymous and POST /auth/refresh return (server-api.md):
/// `{ "userId", "accessToken", "refreshToken", "expiresIn" }`. `expiresIn` is the life of the
/// access token in seconds.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenReply {
    pub user_id: Uuid,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

/// A row of the `refresh_tokens` table, without the hash (it is only used to find the row).
#[derive(Debug, FromRow)]
pub struct RefreshTokenRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub device_id: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// A refresh token to store.
pub struct NewRefreshToken<'a> {
    pub id: Uuid,
    pub user_id: Uuid,
    pub device_id: &'a str,
    pub token_hash: &'a str,
    pub expires_at: DateTime<Utc>,
}

/// What a stored refresh token is good for at this moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshTokenState {
    /// It works: it can be exchanged for new tokens, once.
    Usable,
    /// It was already used (or revoked). Sending it again is a sign that it was copied.
    Revoked,
    /// Its time has passed.
    Expired,
}

/// What happened when an identity was linked to a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linked {
    /// New: the identity now belongs to the user (who is an `account` from now on).
    New,
    /// It already belonged to this user (the same call again).
    AlreadyTheirs,
    /// It belongs to another user: 409 `IDENTITY_IN_USE`.
    InUse,
}
