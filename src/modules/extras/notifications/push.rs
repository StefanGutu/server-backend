//! Pushes to the phones of a user, through Expo (https://docs.expo.dev/push-notifications/sending-notifications/).
//! `expo_messages` and `tokens_to_forget` are pure. `PushSender::Fake` only writes the push in the log.
//!
//! **The real sender (`PushSender::Expo`) has not been tried against Expo**: nothing here can reach a phone. Its request
//! and its reading of the answer are checked against a local server that answers like Expo.

use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::repo;
use crate::config::Config;

/// The three things the app is told about when it is closed (server-api.md: "Push primit când aplicația e închisă").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PushType {
    /// The server thinks the user is on the bus and waits for them to confirm the payment.
    AwaitingConfirmation,
    /// The payment of the ride failed.
    PaymentFailed,
    /// A charge was cancelled: nothing is taken.
    ChargeCancelled,
}

/// A push: its type and the ride it is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Push {
    pub kind: PushType,
    pub session_id: Uuid,
}

impl Push {
    pub fn new(kind: PushType, session_id: Uuid) -> Self {
        Self { kind, session_id }
    }

    /// The title and the text the phone shows (English: the language of the messages is open, R12).
    pub fn text(&self) -> (&'static str, &'static str) {
        match self.kind {
            PushType::AwaitingConfirmation => (
                "Are you on the bus?",
                "Open EchoPass to confirm the payment for your ride.",
            ),
            PushType::PaymentFailed => (
                "Payment failed",
                "Your payment could not be completed. Open EchoPass to try again.",
            ),
            PushType::ChargeCancelled => (
                "Charge cancelled",
                "A charge for your ride was cancelled. Open EchoPass to see your rides.",
            ),
        }
    }

    /// What the app gets with the message: `{ "type", "sessionId" }` (server-api.md).
    pub fn data(&self) -> Value {
        json!({ "type": self.kind, "sessionId": self.session_id })
    }
}

/// The body of the call to Expo: one message for each phone. Pure.
pub fn expo_messages(tokens: &[String], push: &Push) -> Value {
    let (title, body) = push.text();
    let messages: Vec<Value> = tokens
        .iter()
        .map(|token| json!({ "to": token, "title": title, "body": body, "data": push.data(), "sound": "default", "priority": "high" }))
        .collect();
    Value::Array(messages)
}

/// The tokens Expo says no phone uses any more (`DeviceNotRegistered`: the app was removed, the token is old).
/// The answer of Expo is `{ "data": [ ticket, ... ] }` with one ticket for each message, in the same order. Pure.
pub fn tokens_to_forget(answer: &Value, tokens: &[String]) -> Vec<String> {
    let Some(tickets) = answer.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    tokens
        .iter()
        .zip(tickets)
        .filter(|(_, ticket)| {
            ticket["status"] == "error" && ticket["details"]["error"] == "DeviceNotRegistered"
        })
        .map(|(token, _)| token.clone())
        .collect()
}

/// How a push is sent: really, through Expo, or only written in the log.
#[derive(Clone)]
pub enum PushSender {
    /// Writes the push in the log and sends nothing. For tests and for the demo without a phone.
    Fake,
    /// Posts to Expo.
    Expo {
        client: reqwest::Client,
        url: String,
        access_token: Option<String>,
    },
}

impl PushSender {
    /// The sender the server starts with, from the settings.
    pub fn from_config(config: &Config) -> Self {
        if !config.expo_push {
            return Self::Fake;
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("cannot make the HTTP client for Expo");
        Self::Expo {
            client,
            url: config.expo_push_url.clone(),
            access_token: config.expo_access_token.clone(),
        }
    }

    /// Sends the push to these phones. It never makes the caller wait or fail: the fake sender only logs, the Expo
    /// sender works in the background and logs what went wrong.
    pub fn send(&self, db: &PgPool, tokens: Vec<String>, push: Push) {
        match self {
            Self::Fake => {
                tracing::info!(kind = ?push.kind, session_id = %push.session_id, phones = tokens.len(), "push (fake, nothing is sent)");
            }
            Self::Expo {
                client,
                url,
                access_token,
            } => {
                let (client, url, access_token, db) = (
                    client.clone(),
                    url.clone(),
                    access_token.clone(),
                    db.clone(),
                );
                tokio::spawn(async move {
                    post_to_expo(&client, &url, access_token.as_deref(), &db, &tokens, &push).await
                });
            }
        }
    }
}

/// The call to Expo, and what comes of the answer: a token Expo does not know is forgotten.
async fn post_to_expo(
    client: &reqwest::Client,
    url: &str,
    access_token: Option<&str>,
    db: &PgPool,
    tokens: &[String],
    push: &Push,
) {
    let mut request = client
        .post(url)
        .header("Accept", "application/json")
        .json(&expo_messages(tokens, push));
    if let Some(access_token) = access_token {
        request = request.bearer_auth(access_token);
    }
    let answer: Value = match request.send().await {
        Ok(response) if response.status().is_success() => match response.json().await {
            Ok(answer) => answer,
            Err(e) => return tracing::error!("Expo answered with something that is not JSON: {e}"),
        },
        Ok(response) => {
            return tracing::error!(status = %response.status(), "Expo refused the push");
        }
        Err(e) => return tracing::error!("cannot reach Expo: {e}"),
    };
    tracing::info!(kind = ?push.kind, session_id = %push.session_id, phones = tokens.len(), "push sent to Expo");
    for token in tokens_to_forget(&answer, tokens) {
        match repo::delete_device(db, &token).await {
            Ok(_) => {
                tracing::info!(%token, "Expo does not know this phone any more: its token was forgotten")
            }
            Err(e) => tracing::error!("cannot forget the token of a phone Expo does not know: {e}"),
        }
    }
}
