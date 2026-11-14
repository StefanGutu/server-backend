use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::extract::utc_seconds;

/// What a notification is about (server-api.md). Stored as text in `notifications.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum NotificationKind {
    Delay,
    Charge,
    Crowding,
    Renewal,
}

/// A notification as the app sees it (`Notification` in server-api.md). The row is also the JSON.
#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: Uuid,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    #[serde(serialize_with = "utc_seconds")]
    pub created_at: DateTime<Utc>,
    pub read: bool,
}

/// The system of a phone. Stored as text in `push_devices.platform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum Platform {
    Ios,
    Android,
}

/// Body of POST /devices (server-api.md): `{ "pushToken", "platform" }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInput {
    /// The Expo push token, such as `ExponentPushToken[xxxxxxxxxxxxxxxxxxxxxx]`.
    pub push_token: String,
    pub platform: Platform,
}
