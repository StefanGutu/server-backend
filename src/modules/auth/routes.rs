use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};

use super::{
    extractor::CurrentUser,
    models::{AnonymousInput, LinkInput, LoginReply, Me, PatchMe, RefreshInput, SigninInput, TokenReply},
    service,
};
use crate::{error::AppError, extract::ApiJson, state::AppState};

/// POST /auth/anonymous: a new anonymous user and their tokens. No login needed.
async fn post_anonymous(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<AnonymousInput>,
) -> Result<Json<TokenReply>, AppError> {
    let reply = service::login_anonymous(&state.db, &state.config.jwt_secret, &input.device_id).await?;
    Ok(Json(reply))
}

/// POST /auth/refresh: new tokens for a refresh token. No login needed (the access token may have expired).
async fn post_refresh(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<RefreshInput>,
) -> Result<Json<TokenReply>, AppError> {
    let reply = service::refresh(&state.db, &state.config.jwt_secret, &input.refresh_token).await?;
    Ok(Json(reply))
}

/// POST /auth/otp/start: a code for a phone number. 204. No login needed. Only with the cargo feature `phone-login`.
#[cfg(feature = "phone-login")]
async fn post_otp_start(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<super::models::OtpStartInput>,
) -> Result<StatusCode, AppError> {
    service::start_otp(&state.db, &input.phone).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /auth/link: the logged-in user links a phone. The same user, now an account, with new tokens.
async fn post_link(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(input): ApiJson<LinkInput>, // must be last: it reads the body
) -> Result<Json<LoginReply>, AppError> {
    Ok(Json(service::link(&state.db, &state.config.jwt_secret, &state.id_tokens, &user, &input).await?))
}

/// POST /auth/signin: logs in to the account that owns the phone. No login needed.
async fn post_signin(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<SigninInput>,
) -> Result<Json<LoginReply>, AppError> {
    Ok(Json(service::signin(&state.db, &state.config.jwt_secret, &state.id_tokens, &input).await?))
}

/// POST /auth/logout: the refresh tokens of the user are revoked. 204, no body.
async fn post_logout(State(state): State<AppState>, CurrentUser(user): CurrentUser) -> Result<StatusCode, AppError> {
    service::logout(&state.db, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /me: the logged-in user.
async fn get_me(State(state): State<AppState>, CurrentUser(user): CurrentUser) -> Result<Json<Me>, AppError> {
    Ok(Json(service::me(&state.db, user).await?))
}

/// PATCH /me: change name and/or settings, answer with the updated user.
async fn patch_me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(patch): ApiJson<PatchMe>, // must be last: it reads the body
) -> Result<Json<Me>, AppError> {
    let updated = service::update_me(&state.db, user.id, patch).await?;
    Ok(Json(service::me(&state.db, updated).await?))
}

/// DELETE /me: remove the account. 204, no body.
async fn delete_me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, AppError> {
    service::delete_me(&state.db, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    let router = Router::new()
        .route("/auth/anonymous", post(post_anonymous))
        .route("/auth/refresh", post(post_refresh))
        .route("/auth/link", post(post_link))
        .route("/auth/signin", post(post_signin))
        .route("/auth/logout", post(post_logout))
        .route("/me", get(get_me).patch(patch_me).delete(delete_me));
    #[cfg(feature = "phone-login")]
    let router = router.route("/auth/otp/start", post(post_otp_start));
    router
}
