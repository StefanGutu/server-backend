//! `RATE_LIMITED` (step 34): at most `max` calls from one IP address in `window` on the login calls
//! (`/auth/anonymous`, `/auth/refresh`, `/auth/link`, `/auth/signin`, and `/auth/otp/start` with the phone
//! login). The counts are in memory: they start again when the server starts, and each server has its own.

use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, header::RETRY_AFTER},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{error::AppError, state::AppState};

/// When the table has more addresses than this, the ones with no call in the window are forgotten.
const PURGE_ABOVE: usize = 10_000;

/// A sliding window: the times of the calls of each address in the last `window`.
pub struct RateLimiter {
    max: usize,
    window: Duration,
    hits: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl RateLimiter {
    /// `max` calls in `window` for each address. `max` 0 switches the limit off.
    pub fn new(max: usize, window: Duration) -> Self {
        Self {
            max,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Counts a call of this address at `now`. `Ok` when it may go on; `Err(wait)` when the address has
    /// already made `max` calls in the window (the call is not counted): `wait` is how long until the oldest
    /// of those calls leaves the window.
    pub fn check(&self, ip: IpAddr, now: Instant) -> Result<(), Duration> {
        if self.max == 0 {
            return Ok(());
        }
        // A poisoned lock only means another request panicked: the counts are still fine.
        let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        if hits.len() > PURGE_ABOVE {
            let window = self.window;
            hits.retain(|_, times| {
                times
                    .back()
                    .is_some_and(|t| now.duration_since(*t) < window)
            });
        }
        let times = hits.entry(ip).or_default();
        while times
            .front()
            .is_some_and(|t| now.duration_since(*t) >= self.window)
        {
            times.pop_front();
        }
        if times.len() >= self.max {
            let oldest = times.front().copied().unwrap_or(now);
            return Err(self.window.saturating_sub(now.duration_since(oldest)));
        }
        times.push_back(now);
        Ok(())
    }
}

/// The middleware of the login calls: 429 `RATE_LIMITED` with `Retry-After` (whole seconds, at least 1)
/// when the address of the caller made too many calls.
pub async fn limit_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    // Set by `into_make_service_with_connect_info` in main.rs; a call without it (a test) shares one address.
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or(IpAddr::from([0, 0, 0, 0]), |ConnectInfo(addr)| addr.ip());
    match state.rate_limiter.check(ip, Instant::now()) {
        Ok(()) => next.run(request).await,
        Err(wait) => {
            tracing::warn!(ip = %ip, "too many login calls: 429");
            let seconds = wait.as_secs_f64().ceil().max(1.0) as u64;
            let mut response = AppError::RateLimited.into_response();
            if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
                response.headers_mut().insert(RETRY_AFTER, value);
            }
            response
        }
    }
}
