// One folder per feature (devices, telemetry, auth, ...).
// Each is added in its own step of PLAN.md.
pub mod auth;
pub mod charges;
pub mod devices;
pub mod display_info;
pub mod extras;
pub mod frames;
pub mod headcount;
pub mod idempotency;
pub mod rides;
pub mod sightings;
pub mod telemetry;
pub mod wallet;
