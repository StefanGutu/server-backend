use sqlx::PgPool;
use uuid::Uuid;

use chrono::NaiveDate;
use sqlx::types::Json;

use super::models::{NewCardInput, NewPassInput, PassLines, PassProduct, PassRow, PaymentMethod};

/// The column list, written once (sqlx only accepts literal SQL, and `concat!` builds one).
macro_rules! method_columns {
    () => {
        "id, user_id, provider_method_id, network, last4, holder, expiry, bank, tag"
    };
}

/// Stores a card. Returns None when the user already has this provider id (a retry).
pub async fn insert_card(
    db: &PgPool,
    user_id: Uuid,
    card: &NewCardInput,
) -> Result<Option<PaymentMethod>, sqlx::Error> {
    sqlx::query_as(concat!(
        "INSERT INTO payment_methods (id, user_id, provider_method_id, network, last4, holder, expiry, tag) ",
        "VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ",
        "ON CONFLICT (user_id, provider_method_id) DO NOTHING RETURNING ",
        method_columns!()
    ))
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(&card.payment_method_id)
    .bind(card.network)
    .bind(&card.last4)
    .bind(&card.holder)
    .bind(&card.expiry)
    .bind(card.wallet_pay.map(|w| w.tag()))
    .fetch_optional(db)
    .await
}

/// The card of this user with this provider id, or None.
pub async fn find_card_by_provider_id(
    db: &PgPool,
    user_id: Uuid,
    provider_method_id: &str,
) -> Result<Option<PaymentMethod>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        method_columns!(),
        " FROM payment_methods WHERE user_id = $1 AND provider_method_id = $2"
    ))
    .bind(user_id)
    .bind(provider_method_id)
    .fetch_optional(db)
    .await
}

/// All cards of a user, in the order they were added (the ids are time-ordered).
pub async fn list_cards(db: &PgPool, user_id: Uuid) -> Result<Vec<PaymentMethod>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        method_columns!(),
        " FROM payment_methods WHERE user_id = $1 ORDER BY id"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await
}

macro_rules! pass_columns {
    () => {
        "id, user_id, name, holder, valid_from, valid_until, lines, theme, card_number, verified"
    };
}

/// Stores a pass. Returns None when the user already has this pass card (a retry).
pub async fn insert_pass(
    db: &PgPool,
    user_id: Uuid,
    pass: &NewPassInput,
    valid_from: NaiveDate,
) -> Result<Option<PassRow>, sqlx::Error> {
    sqlx::query_as(concat!(
        "INSERT INTO passes (id, user_id, name, holder, valid_from, valid_until, lines, theme, card_number) ",
        "VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ",
        "ON CONFLICT (user_id, card_number) DO NOTHING RETURNING ",
        pass_columns!()
    ))
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(&pass.name)
    .bind(&pass.holder)
    .bind(valid_from)
    .bind(pass.valid_until)
    .bind(Json(&pass.lines))
    .bind(pass.theme)
    .bind(&pass.card_number)
    .fetch_optional(db)
    .await
}

/// The pass of this user with this pass card number, or None.
pub async fn find_pass_by_card_number(
    db: &PgPool,
    user_id: Uuid,
    card_number: &str,
) -> Result<Option<PassRow>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        pass_columns!(),
        " FROM passes WHERE user_id = $1 AND card_number = $2"
    ))
    .bind(user_id)
    .bind(card_number)
    .fetch_optional(db)
    .await
}

/// All passes of a user.
pub async fn list_passes(db: &PgPool, user_id: Uuid) -> Result<Vec<PassRow>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        pass_columns!(),
        " FROM passes WHERE user_id = $1 ORDER BY id"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// Deletes a card of this user. False when there is no such card.
pub async fn delete_card(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM payment_methods WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// Deletes a pass of this user. False when there is no such pass.
pub async fn delete_pass(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM passes WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}

/// True when the user has a card or a pass with this id.
pub async fn item_exists(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM payment_methods WHERE id = $1 AND user_id = $2)
             OR EXISTS (SELECT 1 FROM passes WHERE id = $1 AND user_id = $2)",
    )
    .bind(id)
    .bind(user_id)
    .fetch_one(db)
    .await
}

/// The passes that can be bought: the shortest first, then the cheapest.
pub async fn list_products(db: &PgPool) -> Result<Vec<PassProduct>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, name, days, price_bani, lines FROM pass_products ORDER BY days, price_bani, id",
    )
    .fetch_all(db)
    .await
}

/// One card of this user, or None.
pub async fn find_card(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<PaymentMethod>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        method_columns!(),
        " FROM payment_methods WHERE id = $1 AND user_id = $2"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// One pass of this user, or None.
pub async fn find_pass(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<PassRow>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        pass_columns!(),
        " FROM passes WHERE id = $1 AND user_id = $2"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// The card the user added first, or None if the user has no card.
pub async fn first_card(db: &PgPool, user_id: Uuid) -> Result<Option<PaymentMethod>, sqlx::Error> {
    sqlx::query_as(concat!(
        "SELECT ",
        method_columns!(),
        " FROM payment_methods WHERE user_id = $1 ORDER BY id LIMIT 1"
    ))
    .bind(user_id)
    .fetch_optional(db)
    .await
}

/// One pass product of the catalog, or None.
pub async fn find_product(db: &PgPool, id: Uuid) -> Result<Option<PassProduct>, sqlx::Error> {
    sqlx::query_as("SELECT id, name, days, price_bani, lines FROM pass_products WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await
}

/// A pass is renewed: new dates and the lines of the product. Returns the pass as it is now, or
/// None if the user has no such pass.
pub async fn renew_pass(
    db: &PgPool,
    user_id: Uuid,
    pass_id: Uuid,
    valid_from: NaiveDate,
    valid_until: NaiveDate,
    lines: &PassLines,
) -> Result<Option<PassRow>, sqlx::Error> {
    sqlx::query_as(concat!(
        "UPDATE passes SET valid_from = $3, valid_until = $4, lines = $5, last_renewed_at = now() WHERE id = $1 AND user_id = $2 RETURNING ",
        pass_columns!()
    ))
    .bind(pass_id)
    .bind(user_id)
    .bind(valid_from)
    .bind(valid_until)
    .bind(Json(lines))
    .fetch_optional(db)
    .await
}
