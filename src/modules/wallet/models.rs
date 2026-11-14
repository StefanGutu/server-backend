use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

use crate::{extract::ron_from_bani, modules::charges::models::Charge};

/// Card network. Stored as text in `payment_methods.network`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum CardNetwork {
    Visa,
    Mastercard,
    Other,
}

/// How the app says the card was added: `walletPay` in `NewCardInput`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalletPay {
    GooglePay,
    ApplePay,
}

impl WalletPay {
    /// The label shown on the card.
    pub fn tag(self) -> CardTag {
        match self {
            Self::GooglePay => CardTag::GooglePay,
            Self::ApplePay => CardTag::ApplePay,
        }
    }
}

/// The label on a card (`tag`). Stored as text in `payment_methods.tag`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum CardTag {
    #[serde(rename = "Google Pay")]
    #[sqlx(rename = "Google Pay")]
    GooglePay,
    #[serde(rename = "Apple Pay")]
    #[sqlx(rename = "Apple Pay")]
    ApplePay,
}

/// Body of POST /wallet/payment-methods (`NewCardInput` in server-api.md).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCardInput {
    pub payment_method_id: String,
    pub wallet_pay: Option<WalletPay>,
    pub network: CardNetwork,
    pub last4: String,
    pub holder: String,
    /// "MM/YY"
    pub expiry: String,
}

/// A row of the `payment_methods` table.
#[derive(FromRow)]
pub struct PaymentMethod {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider_method_id: String,
    pub network: CardNetwork,
    pub last4: String,
    pub holder: String,
    pub expiry: String,
    pub bank: Option<String>,
    pub tag: Option<CardTag>,
}

/// A card as the app sees it (`WalletItem` with `kind: "card"`).
#[derive(Debug, Serialize)]
pub struct Card {
    pub id: Uuid,
    pub network: CardNetwork,
    pub last4: String,
    pub holder: String,
    pub expiry: String,
    /// The provider's id of the card.
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<CardTag>,
}

/// The lines a pass is valid on: `"all"` or a list such as `["15", "33"]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawLines", into = "RawLines")]
pub enum PassLines {
    All,
    Only(Vec<String>),
}

/// How `PassLines` looks in JSON.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawLines {
    Word(String),
    List(Vec<String>),
}

impl TryFrom<RawLines> for PassLines {
    type Error = String;

    fn try_from(raw: RawLines) -> Result<Self, String> {
        match raw {
            RawLines::Word(word) if word == "all" => Ok(Self::All),
            RawLines::Word(_) => Err("lines must be \"all\" or a list of line names".to_string()),
            RawLines::List(lines) => Ok(Self::Only(lines)),
        }
    }
}

impl From<PassLines> for RawLines {
    fn from(lines: PassLines) -> Self {
        match lines {
            PassLines::All => Self::Word("all".to_string()),
            PassLines::Only(lines) => Self::List(lines),
        }
    }
}

/// Colour theme of a pass. Stored as text in `passes.theme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum PassTheme {
    Ocean,
    Sunset,
    Forest,
}

/// Body of POST /wallet/passes (`NewPassInput` in server-api.md).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPassInput {
    pub card_number: String,
    pub name: String,
    pub holder: String,
    /// "YYYY-MM-DD"
    pub valid_until: NaiveDate,
    pub lines: PassLines,
    pub theme: PassTheme,
}

/// A row of the `passes` table.
#[derive(FromRow)]
pub struct PassRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub holder: String,
    pub valid_from: NaiveDate,
    pub valid_until: NaiveDate,
    pub lines: Json<PassLines>,
    pub theme: PassTheme,
    pub card_number: String,
    pub verified: bool,
}

/// A pass as the app sees it (`WalletItem` with `kind: "pass"`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pass {
    pub id: Uuid,
    pub name: String,
    pub holder: String,
    pub valid_from: NaiveDate,
    pub valid_until: NaiveDate,
    pub lines: PassLines,
    pub theme: PassTheme,
    pub card_number: String,
    pub verified: bool,
}

impl From<PassRow> for WalletItem {
    fn from(p: PassRow) -> Self {
        Self::Pass(Pass {
            id: p.id,
            name: p.name,
            holder: p.holder,
            valid_from: p.valid_from,
            valid_until: p.valid_until,
            lines: p.lines.0,
            theme: p.theme,
            card_number: p.card_number,
            verified: p.verified,
        })
    }
}

/// An item of the wallet: a card or a pass.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum WalletItem {
    Card(Card),
    Pass(Pass),
}

impl WalletItem {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Card(card) => card.id,
            Self::Pass(pass) => pass.id,
        }
    }
}

/// Body of PUT /wallet/default-source.
#[derive(Debug, Deserialize)]
pub struct DefaultSourceInput {
    pub id: Uuid,
}

impl From<PaymentMethod> for WalletItem {
    fn from(m: PaymentMethod) -> Self {
        Self::Card(Card {
            id: m.id,
            network: m.network,
            last4: m.last4,
            holder: m.holder,
            expiry: m.expiry,
            token: m.provider_method_id,
            bank: m.bank,
            tag: m.tag,
        })
    }
}

/// Answer of POST /wallet/payment-methods/setup.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResponse {
    pub client_secret: String,
}

/// A pass that can be bought (`GET /passes/catalog`). The row is also the JSON the app gets.
#[derive(Debug, Serialize, FromRow)]
pub struct PassProduct {
    pub id: Uuid,
    pub name: String,
    pub days: i32,
    /// Kept in bani, sent as lei in `priceRon`.
    #[serde(rename = "priceRon", serialize_with = "ron_from_bani")]
    pub price_bani: i32,
    pub lines: Json<PassLines>,
}

/// The thing a ride is paid with: one of the user's cards or passes.
pub enum Source {
    Card(PaymentMethod),
    Pass(PassRow),
}

/// Whether a pass may cover a ride. Anything but `Valid` means the user must pay another way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassCheck {
    Valid,
    /// Nobody checked the pass card (`verified` is false).
    NotVerified,
    /// Today is before `validFrom` or after `validUntil`.
    NotValidToday,
    /// The pass is for other lines than the one of the ride.
    LineNotCovered,
}

impl PassCheck {
    /// What the user is told when the pass cannot be used. `None` for `Valid`.
    pub fn message(self) -> Option<&'static str> {
        match self {
            Self::Valid => None,
            Self::NotVerified => Some("This pass has not been verified. Pay with a card instead."),
            Self::NotValidToday => Some("This pass is not valid today. Pay with a card instead."),
            Self::LineNotCovered => Some("This pass is not valid on this line. Pay with a card instead."),
        }
    }
}

/// Body of POST /wallet/passes/{id}/renew (server-api.md): `{ productId, sourceId }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenewInput {
    /// The pass to buy, from `GET /passes/catalog`.
    pub product_id: Uuid,
    /// The card that pays.
    pub source_id: Uuid,
}

/// Answer of the renew call: the pass as it is now, and the charge of the purchase.
#[derive(Debug, Serialize)]
pub struct RenewAnswer {
    pub item: WalletItem,
    pub charge: Charge,
}
