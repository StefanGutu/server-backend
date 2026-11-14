//! The payment provider (Stripe or similar) behind a trait, so a real one can replace the fake one.

use std::{future::Future, pin::Pin};

use uuid::Uuid;

use crate::error::AppError;

/// What an async method of the trait returns. Written out by hand because a trait with
/// `async fn` cannot be used as `dyn PaymentProvider`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What the provider answers to a charge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChargeResult {
    Paid,
    /// The card was refused.
    Declined,
    /// The bank wants the user to confirm in the app, with this client secret.
    AuthenticationRequired {
        client_secret: String,
    },
}

/// The fake provider pays everything except these two test ids (the same names the Stripe test
/// cards have): they make it answer `Declined` and `AuthenticationRequired`.
pub const FAKE_DECLINED_ID: &str = "pm_card_chargeDeclined";
pub const FAKE_AUTHENTICATION_ID: &str = "pm_card_authenticationRequired";

pub trait PaymentProvider: Send + Sync {
    /// Starts adding a card. Returns the client secret the app gives to the provider's own
    /// SDK, which collects the card data (it never comes through our server).
    fn create_setup(&self) -> BoxFuture<'_, Result<String, AppError>>;

    /// Takes `amount_bani` from the card the provider knows as `payment_method_id`.
    fn charge<'a>(
        &'a self,
        payment_method_id: &'a str,
        amount_bani: i32,
    ) -> BoxFuture<'a, Result<ChargeResult, AppError>>;

    /// Gives `amount_bani` back to the card that was charged (a charge that must be undone, for example
    /// when the ride changed while the card was being charged). A real provider needs the id of its own
    /// payment too: see doc/reminders.md R48.
    fn refund<'a>(
        &'a self,
        payment_method_id: &'a str,
        amount_bani: i32,
    ) -> BoxFuture<'a, Result<(), AppError>>;
}

/// A provider that always says yes and keeps nothing. Used until a real one is connected.
pub struct FakeProvider;

fn fake_secret() -> String {
    format!(
        "seti_fake_{}_secret_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

impl PaymentProvider for FakeProvider {
    fn charge<'a>(
        &'a self,
        payment_method_id: &'a str,
        _amount_bani: i32,
    ) -> BoxFuture<'a, Result<ChargeResult, AppError>> {
        Box::pin(async move {
            Ok(match payment_method_id {
                FAKE_DECLINED_ID => ChargeResult::Declined,
                FAKE_AUTHENTICATION_ID => ChargeResult::AuthenticationRequired {
                    client_secret: fake_secret(),
                },
                _ => ChargeResult::Paid,
            })
        })
    }

    fn refund<'a>(
        &'a self,
        payment_method_id: &'a str,
        amount_bani: i32,
    ) -> BoxFuture<'a, Result<(), AppError>> {
        Box::pin(async move {
            tracing::info!(
                payment_method_id,
                amount_bani,
                "refund (fake, nothing is sent)"
            );
            Ok(())
        })
    }

    fn create_setup(&self) -> BoxFuture<'_, Result<String, AppError>> {
        // Same look as a real setup intent secret: "<setup id>_secret_<random>".
        Box::pin(async { Ok(fake_secret()) })
    }
}
