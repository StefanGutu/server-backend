use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use uuid::Uuid;

use super::{
    models::{
        DefaultSourceInput, NewCardInput, NewPassInput, PassProduct, RenewInput, SetupResponse,
        WalletItem,
    },
    service,
};
use crate::{
    error::AppError,
    extract::{ApiJson, ApiPath, parse_json_body, run_to_the_end},
    modules::display_info::extractor::UserOrDisplay,
    modules::{auth::extractor::CurrentUser, idempotency},
    state::AppState,
};

/// GET /wallet/items: the cards and passes of the logged-in user.
async fn get_items(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<WalletItem>>, AppError> {
    Ok(Json(service::list_items(&state.db, user.id).await?))
}

/// GET /passes/catalog: the passes that can be bought. The app calls it with a user login, the display with the
/// display token.
async fn get_catalog(
    State(state): State<AppState>,
    _access: UserOrDisplay,
) -> Result<Json<Vec<PassProduct>>, AppError> {
    Ok(Json(service::list_catalog(&state.db).await?))
}

/// POST /wallet/payment-methods/setup: `{ "clientSecret" }`.
async fn post_setup(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
) -> Result<Json<SetupResponse>, AppError> {
    Ok(Json(
        service::start_card_setup(state.payments.as_ref()).await?,
    ))
}

/// POST /wallet/payment-methods: remember a card.
async fn post_card(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(card): ApiJson<NewCardInput>, // must be last: it reads the body
) -> Result<Json<WalletItem>, AppError> {
    Ok(Json(service::add_card(&state.db, user.id, &card).await?))
}

/// POST /wallet/passes: add a pass.
async fn post_pass(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(pass): ApiJson<NewPassInput>, // must be last: it reads the body
) -> Result<Json<WalletItem>, AppError> {
    Ok(Json(service::add_pass(&state.db, user.id, &pass).await?))
}

/// DELETE /wallet/items/{id}: delete a card or a pass. 204.
async fn delete_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(item_id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    service::delete_item(&state.db, user.id, item_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// PUT /wallet/default-source: `{ "id" }`. 204.
async fn put_default_source(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<DefaultSourceInput>, // must be last: it reads the body
) -> Result<StatusCode, AppError> {
    service::set_default_source(&state.db, user.id, input.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /wallet/passes/{id}/renew: buy a pass product for a pass, paid with a card. The
/// `Idempotency-Key` header is required. The answer is `{ item, charge }`.
async fn post_renew(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(pass_id): ApiPath<Uuid>,
    headers: HeaderMap,
    body: Bytes, // must be last: it reads the body
) -> Result<Response, AppError> {
    let input: RenewInput = parse_json_body(&body)?;
    let endpoint = format!("POST /wallet/passes/{pass_id}/renew");
    let request = idempotency::service::request_from_headers(&headers, &endpoint, &body)?;
    // In its own task: if the phone gives up, the payment still ends cleanly.
    let answer = run_to_the_end(async move {
        let provider = state.payments.as_ref();
        service::renew_pass(&state.db, provider, &user, pass_id, &input, &request).await
    })
    .await?;
    Ok(Json(answer).into_response())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/passes/catalog", get(get_catalog))
        .route("/wallet/items", get(get_items))
        .route("/wallet/items/{id}", delete(delete_item))
        .route("/wallet/payment-methods/setup", post(post_setup))
        .route("/wallet/payment-methods", post(post_card))
        .route("/wallet/passes", post(post_pass))
        .route("/wallet/passes/{id}/renew", post(post_renew))
        .route("/wallet/default-source", put(put_default_source))
}
