//! Display info: read-only calls for a screen that shows the buses (a dashboard). They are not for the phone
//! app: they need the `DISPLAY_TOKEN` of `.env` instead of a user login. Nothing here changes anything.
pub mod admin_models;
mod admin_repo;
pub mod admin_routes;
pub mod admin_service;
pub mod extractor;
pub mod models;
mod repo;
pub mod routes;
pub mod service;
