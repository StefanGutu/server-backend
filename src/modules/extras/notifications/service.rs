//! Notifications and the phones that get pushes. `validate_push_token` is pure.

use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{DeviceInput, Notification},
    push::{Push, PushSender},
    repo,
};
use crate::{error::AppError, extract::ListQuery};

/// The longest push token accepted (characters). Real Expo tokens are about 40.
const MAX_PUSH_TOKEN_LEN: usize = 200;

/// An Expo push token: `ExponentPushToken[...]` or `ExpoPushToken[...]` with something inside the brackets. Trimmed.
/// Anything else is a 400: the server would send pushes to a token that Expo cannot use. Pure.
pub fn validate_push_token(token: &str) -> Result<&str, AppError> {
    let token = token.trim();
    let inner = token
        .strip_prefix("ExponentPushToken[")
        .or_else(|| token.strip_prefix("ExpoPushToken["))
        .and_then(|rest| rest.strip_suffix(']'));
    let well_formed = token.chars().count() <= MAX_PUSH_TOKEN_LEN
        && inner.is_some_and(|inner| {
            !inner.is_empty()
                && !inner.contains(['[', ']'])
                && !inner.chars().any(char::is_whitespace)
        });
    if well_formed {
        Ok(token)
    } else {
        Err(AppError::Validation("pushToken must be an Expo push token such as ExponentPushToken[xxxxxxxxxxxxxxxxxxxxxx].".to_string()))
    }
}

/// GET /notifications: the user's notifications, newest first, `limit` at a time. `before` is the id of the last
/// one the app has; an id that is not one of the user's notifications is a 404 (the same rule as `GET /charges`).
pub async fn list_notifications(
    db: &PgPool,
    user_id: Uuid,
    query: &ListQuery,
) -> Result<Vec<Notification>, AppError> {
    let limit = query.page_size()?;
    let after = match query.before {
        Some(notification_id) => Some(
            repo::find_cursor(db, user_id, notification_id)
                .await?
                .ok_or(AppError::NotFound)?,
        ),
        None => None,
    };
    Ok(repo::list_notifications(db, user_id, after, limit).await?)
}

/// POST /notifications/{id}/read: the user has seen it. Calling it again is fine. Not the user's, or unknown: 404.
pub async fn mark_read(db: &PgPool, user_id: Uuid, notification_id: Uuid) -> Result<(), AppError> {
    if repo::mark_read(db, user_id, notification_id).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// POST /devices: remembers the phone (its Expo push token) of the user, so pushes can be sent to it. The same token
/// again is fine. A token that was another user's moves to this user: it is the same phone.
pub async fn register_device(
    db: &PgPool,
    user_id: Uuid,
    input: &DeviceInput,
) -> Result<(), AppError> {
    let token = validate_push_token(&input.push_token)?;
    repo::upsert_device(db, user_id, token, input.platform).await?;
    Ok(())
}

/// Sends a push to every phone of the user. Best effort: a user with no phone gets nothing, and a failure is logged and
/// never fails the call that led here (a ride is not undone because a push could not be sent).
pub async fn send_push(db: &PgPool, sender: &PushSender, user_id: Uuid, push: Push) {
    match repo::tokens_of_user(db, user_id).await {
        Ok(tokens) if tokens.is_empty() => {
            tracing::debug!(%user_id, kind = ?push.kind, "no push: the user has no phone")
        }
        Ok(tokens) => sender.send(db, tokens, push),
        Err(e) => {
            tracing::error!(%user_id, "cannot read the phones of the user, no push sent: {e}")
        }
    }
}
