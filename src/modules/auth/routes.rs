use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    middleware,
    routing::{get, post},
};

use super::{
    extractor::{CurrentUser, MaybeBearerUser},
    models::{
        AnonymousInput, LinkInput, LoginReply, Me, PatchMe, RefreshInput, SigninInput, TokenReply,
    },
    service,
};
use crate::{error::AppError, extract::ApiJson, rate_limit::limit_auth, state::AppState};

/// POST /auth/anonymous: a new anonymous user and their tokens. No login needed.
async fn post_anonymous(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<AnonymousInput>,
) -> Result<Json<TokenReply>, AppError> {
    let reply =
        service::login_anonymous(&state.db, &state.config.jwt_secret, &input.device_id).await?;
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
    Ok(Json(
        service::link(
            &state.db,
            &state.config.jwt_secret,
            &state.id_tokens,
            &user,
            &input,
        )
        .await?,
    ))
}

/// POST /auth/signin: logs in with a Google or Apple identity. No login needed. If the call carries the
/// `Bearer` token of an anonymous user, that user is the one left behind: it is deleted (see the service).
async fn post_signin(
    State(state): State<AppState>,
    MaybeBearerUser(caller): MaybeBearerUser,
    ApiJson(input): ApiJson<SigninInput>, // must be last: it reads the body
) -> Result<Json<LoginReply>, AppError> {
    let reply = service::signin(
        &state.db,
        &state.config.jwt_secret,
        &state.id_tokens,
        &input,
        caller.as_ref(),
    )
    .await?;
    Ok(Json(reply))
}

/// POST /auth/logout: the refresh tokens of the user are revoked. 204, no body.
async fn post_logout(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, AppError> {
    service::logout(&state.db, &user).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /me: the logged-in user.
async fn get_me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Me>, AppError> {
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

/// The calls that make or prove a login. They are the ones to hammer for tokens or codes, so each IP
/// address may make only a few of them in a minute (`rate_limit::limit_auth`, step 34).
fn login_router(state: AppState) -> Router<AppState> {
    let router = Router::new()
        .route("/auth/anonymous", post(post_anonymous))
        .route("/auth/refresh", post(post_refresh))
        .route("/auth/link", post(post_link))
        .route("/auth/signin", post(post_signin));
    #[cfg(feature = "phone-login")]
    let router = router.route("/auth/otp/start", post(post_otp_start));
    router.route_layer(middleware::from_fn_with_state(state, limit_auth))
}

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/auth/logout", post(post_logout))
        .route("/me", get(get_me).patch(patch_me).delete(delete_me))
        .merge(login_router(state))
}
