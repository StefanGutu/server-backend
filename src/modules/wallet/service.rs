//! Rules of the wallet. `validate_new_card` is pure.

use chrono::{Days, NaiveDate, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    models::{
        NewCardInput, NewPassInput, PassCheck, PassLines, PassProduct, PassRow, PaymentMethod,
        RenewAnswer, RenewInput, SetupResponse, Source, WalletItem,
    },
    provider::{ChargeResult, PaymentProvider},
    repo,
};
use crate::{
    error::AppError,
    modules::{
        auth::{self, models::User},
        charges::{
            self,
            models::{ChargeStatus, NewCharge},
        },
        idempotency::{
            self,
            models::{Begin, IdempotentRequest},
        },
    },
};

fn invalid(message: &str) -> AppError {
    AppError::Validation(message.to_string())
}

/// "MM/YY": month 01 to 12, then two digits.
fn expiry_ok(expiry: &str) -> bool {
    let bytes = expiry.as_bytes();
    bytes.len() == 5
        && bytes[2] == b'/'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 2 || b.is_ascii_digit())
        && expiry[..2]
            .parse::<u8>()
            .is_ok_and(|month| (1..=12).contains(&month))
}

/// Shape of the card data the app sends. A wrong shape is a 400.
pub fn validate_new_card(card: &NewCardInput) -> Result<(), AppError> {
    if card.payment_method_id.trim().is_empty() {
        return Err(invalid("paymentMethodId must not be empty."));
    }
    if card.last4.len() != 4 || !card.last4.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("last4 must be 4 digits."));
    }
    if card.holder.trim().is_empty() {
        return Err(invalid("holder must not be empty."));
    }
    if !expiry_ok(&card.expiry) {
        return Err(invalid("expiry must look like MM/YY."));
    }
    Ok(())
}

/// POST /wallet/payment-methods/setup: a client secret for the app to add a card with.
pub async fn start_card_setup(provider: &dyn PaymentProvider) -> Result<SetupResponse, AppError> {
    Ok(SetupResponse {
        client_secret: provider.create_setup().await?,
    })
}

/// POST /wallet/payment-methods: remember a card the provider already holds.
/// The first item of a user becomes the default way to pay.
/// Adding the same `paymentMethodId` again returns the card that is already there.
pub async fn add_card(
    db: &PgPool,
    user_id: Uuid,
    card: &NewCardInput,
) -> Result<WalletItem, AppError> {
    validate_new_card(card)?;
    if let Some(added) = repo::insert_card(db, user_id, card).await? {
        auth::service::set_default_source_if_none(db, user_id, added.id).await?;
        return Ok(added.into());
    }
    // Same card sent twice: answer with the stored one.
    repo::find_card_by_provider_id(db, user_id, &card.payment_method_id)
        .await?
        .map(WalletItem::from)
        .ok_or(AppError::Server)
}

/// Shape of the pass data the app sends. `today` is the day the pass is added: a pass that
/// is already over is refused, because it would start after it ends. A wrong shape is a 400.
pub fn validate_new_pass(pass: &NewPassInput, today: NaiveDate) -> Result<(), AppError> {
    if pass.card_number.trim().is_empty() {
        return Err(invalid("cardNumber must not be empty."));
    }
    if pass.name.trim().is_empty() {
        return Err(invalid("name must not be empty."));
    }
    if pass.holder.trim().is_empty() {
        return Err(invalid("holder must not be empty."));
    }
    if let PassLines::Only(lines) = &pass.lines
        && (lines.is_empty() || lines.iter().any(|line| line.trim().is_empty()))
    {
        return Err(invalid("lines must be \"all\" or a list of line names."));
    }
    if pass.valid_until < today {
        return Err(invalid("validUntil must not be in the past."));
    }
    Ok(())
}

/// POST /wallet/passes: add a pass. It is valid from today. The first item of a user becomes the
/// default way to pay. Adding the same pass card number again returns the pass that is already there.
pub async fn add_pass(
    db: &PgPool,
    user_id: Uuid,
    pass: &NewPassInput,
) -> Result<WalletItem, AppError> {
    let today = chrono::Utc::now().date_naive();
    validate_new_pass(pass, today)?;
    if let Some(added) = repo::insert_pass(db, user_id, pass, today).await? {
        auth::service::set_default_source_if_none(db, user_id, added.id).await?;
        return Ok(added.into());
    }
    repo::find_pass_by_card_number(db, user_id, &pass.card_number)
        .await?
        .map(WalletItem::from)
        .ok_or(AppError::Server)
}

/// Cards and passes in one list, in the order they were added (the ids are time-ordered).
pub fn merge_items(cards: Vec<PaymentMethod>, passes: Vec<PassRow>) -> Vec<WalletItem> {
    let mut items: Vec<WalletItem> = cards
        .into_iter()
        .map(WalletItem::from)
        .chain(passes.into_iter().map(WalletItem::from))
        .collect();
    items.sort_by_key(WalletItem::id);
    items
}

/// GET /wallet/items: the user's cards and passes.
pub async fn list_items(db: &PgPool, user_id: Uuid) -> Result<Vec<WalletItem>, AppError> {
    let cards = repo::list_cards(db, user_id).await?;
    let passes = repo::list_passes(db, user_id).await?;
    Ok(merge_items(cards, passes))
}

/// DELETE /wallet/items/{id}: delete a card or a pass of the user. If it was the default way
/// to pay, there is no default any more.
///
/// The order matters: check, clear the default, then delete. If a step fails, the worst case is
/// "no default". The other order could leave a default that points to an item that is gone.
pub async fn delete_item(db: &PgPool, user_id: Uuid, item_id: Uuid) -> Result<(), AppError> {
    if !repo::item_exists(db, user_id, item_id).await? {
        return Err(AppError::NotFound);
    }
    auth::service::clear_default_source_if(db, user_id, item_id).await?;
    let deleted = repo::delete_card(db, user_id, item_id).await?
        || repo::delete_pass(db, user_id, item_id).await?;
    if deleted {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// PUT /wallet/default-source: the card or pass to pay with when the app does not say.
pub async fn set_default_source(db: &PgPool, user_id: Uuid, item_id: Uuid) -> Result<(), AppError> {
    if !repo::item_exists(db, user_id, item_id).await? {
        return Err(AppError::NotFound);
    }
    auth::service::set_default_source(db, user_id, item_id).await
}

/// GET /passes/catalog: the passes a user can buy.
pub async fn list_catalog(db: &PgPool) -> Result<Vec<PassProduct>, AppError> {
    Ok(repo::list_products(db).await?)
}

/// One product of the catalog, or NOT_FOUND.
pub async fn get_product(db: &PgPool, id: Uuid) -> Result<PassProduct, AppError> {
    repo::find_product(db, id).await?.ok_or(AppError::NotFound)
}

/// A card or a pass of the user, by id. None if the user has no such item.
pub async fn find_source(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<Option<Source>, AppError> {
    if let Some(card) = repo::find_card(db, user_id, id).await? {
        return Ok(Some(Source::Card(card)));
    }
    Ok(repo::find_pass(db, user_id, id).await?.map(Source::Pass))
}

/// The card the user added first (used for the passengers a pass does not cover).
pub async fn first_card(db: &PgPool, user_id: Uuid) -> Result<Option<PaymentMethod>, AppError> {
    Ok(repo::first_card(db, user_id).await?)
}

/// Can this pass cover a ride on `line` today? Pure. A pass nobody verified does not count,
/// except with `allow_unverified` (test mode: no verification service exists yet, R21).
pub fn check_pass(
    pass: &PassRow,
    line: &str,
    today: NaiveDate,
    allow_unverified: bool,
) -> PassCheck {
    if !pass.verified && !allow_unverified {
        return PassCheck::NotVerified;
    }
    if today < pass.valid_from || today > pass.valid_until {
        return PassCheck::NotValidToday;
    }
    match &pass.lines.0 {
        PassLines::All => PassCheck::Valid,
        PassLines::Only(lines) if lines.iter().any(|l| l == line) => PassCheck::Valid,
        PassLines::Only(_) => PassCheck::LineNotCovered,
    }
}

/// The new dates of a pass that is renewed for `days` days. Pure.
/// - A pass that is still valid (it ends today or later) keeps its start and is extended from its end.
/// - A pass that has already ended starts again today and runs `days` days from today.
///
/// `None` only if the date would be out of range.
pub fn renewed_dates(
    valid_from: NaiveDate,
    valid_until: NaiveDate,
    today: NaiveDate,
    days: i32,
) -> Option<(NaiveDate, NaiveDate)> {
    let days = Days::new(u64::try_from(days).ok()?);
    if valid_until < today {
        Some((today, today.checked_add_days(days)?))
    } else {
        Some((valid_from, valid_until.checked_add_days(days)?))
    }
}

/// POST /wallet/passes/{id}/renew: buy a pass product for a pass, paid with a card. Answers
/// `{ item, charge }` as JSON. Needs an `Idempotency-Key`: the same call sent again gets the stored
/// answer (the same JSON) and is not charged again.
/// One renew of a pass at a time (a lock), so two calls with the same key at the same moment cannot
/// both be new. The payment, the charge and the new dates are three steps, not one transaction.
pub async fn renew_pass(
    db: &PgPool,
    provider: &dyn PaymentProvider,
    user: &User,
    pass_id: Uuid,
    input: &RenewInput,
    request: &IdempotentRequest,
) -> Result<Value, AppError> {
    auth::service::require_account(user)?;
    let name = format!("pass:{pass_id}");
    let lock = idempotency::service::lock(db, &name).await?;
    let result = renew_with_key(db, provider, user, pass_id, input, request).await;
    idempotency::service::unlock(lock, &name).await;
    result
}

async fn renew_with_key(
    db: &PgPool,
    provider: &dyn PaymentProvider,
    user: &User,
    pass_id: Uuid,
    input: &RenewInput,
    request: &IdempotentRequest,
) -> Result<Value, AppError> {
    if let Begin::Replay(stored) = idempotency::service::begin(db, user.id, request).await? {
        tracing::info!(key = %request.key, pass_id = %pass_id, "same Idempotency-Key: stored answer sent, nothing charged");
        return Ok(stored);
    }
    let answer = renew_locked(db, provider, user, pass_id, input).await?;
    let stored = serde_json::to_value(&answer).map_err(|_| AppError::Server)?;
    // If keeping the answer fails the pass is renewed anyway; the log says so.
    if let Err(e) = idempotency::service::finish(db, user.id, request, &stored).await {
        tracing::error!(key = %request.key, "cannot store the answer of an Idempotency-Key: {e}");
    }
    Ok(stored)
}

async fn renew_locked(
    db: &PgPool,
    provider: &dyn PaymentProvider,
    user: &User,
    pass_id: Uuid,
    input: &RenewInput,
) -> Result<RenewAnswer, AppError> {
    let pass = repo::find_pass(db, user.id, pass_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let product = repo::find_product(db, input.product_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let Source::Card(card) = find_source(db, user.id, input.source_id)
        .await?
        .ok_or(AppError::NotFound)?
    else {
        return Err(AppError::Validation(
            "A pass is paid with a card, not with another pass.".into(),
        ));
    };

    if product.price_bani > 0 {
        match provider
            .charge(&card.provider_method_id, product.price_bani)
            .await?
        {
            ChargeResult::Paid => {}
            ChargeResult::Declined => {
                tracing::warn!(pass_id = %pass.id, reason = "card_declined", "pass renewal: payment failed");
                return Err(AppError::PaymentDeclined("card_declined"));
            }
            ChargeResult::AuthenticationRequired { .. } => {
                tracing::warn!(pass_id = %pass.id, reason = "authentication_required", "pass renewal: payment failed");
                return Err(AppError::PaymentDeclined("authentication_required"));
            }
        }
    }

    let now = Utc::now();
    let (valid_from, valid_until) = renewed_dates(
        pass.valid_from,
        pass.valid_until,
        now.date_naive(),
        product.days,
    )
    .ok_or(AppError::Server)?;
    // The charge is over at once: there is no ride. Its line is the name of the product.
    let charge = charges::service::create_charge(
        db,
        &NewCharge {
            user_id: user.id,
            session_id: None,
            line: product.name.clone(),
            destination: None,
            amount_bani: product.price_bani,
            status: ChargeStatus::Completed,
            paid_with: card.id,
            passengers: 1,
            started_at: now,
            ended_at: Some(now),
        },
    )
    .await?;
    let renewed = repo::renew_pass(
        db,
        user.id,
        pass.id,
        valid_from,
        valid_until,
        &product.lines.0,
    )
    .await?
    .ok_or(AppError::Server)?;
    tracing::info!(pass_id = %pass.id, product = %product.name, %valid_until, charge_id = %charge.id, "pass renewed");
    Ok(RenewAnswer {
        item: renewed.into(),
        charge,
    })
}
