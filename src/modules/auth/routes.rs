use axum::{Json, Router, routing::get};

use super::{extractor::CurrentUser, models::Me};
use crate::state::AppState;

/// GET /me: the logged-in user.
async fn get_me(CurrentUser(user): CurrentUser) -> Json<Me> {
    Json(Me::from(user))
}

pub fn router() -> Router<AppState> {
    Router::new().route("/me", get(get_me))
}
