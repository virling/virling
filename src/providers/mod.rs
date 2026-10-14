mod dodo;
mod lemonsqueezy;
mod polar;
mod stripe;

use time::OffsetDateTime;

/// One payment as a provider reports it, before currency conversion.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub provider_payment_id: String,
    pub ts: OffsetDateTime,
    pub amount: i64,
    pub currency: String,
    pub refunded: i64,
    pub status: String,
    pub visitor_id: Option<String>,
    pub customer_id: Option<String>,
    pub is_renewal: bool,
}

/// A batch of payments plus the opaque cursor to resume from next time.
#[derive(Debug, Default)]
pub struct Page {
    pub payments: Vec<Fetched>,
    pub cursor: Option<String>,
    /// More pages are available right now.
    pub more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Stripe,
    Dodo,
    Polar,
    LemonSqueezy,
}

impl Provider {
    pub const ALL: [Provider; 4] = [
        Provider::Stripe,
        Provider::Dodo,
        Provider::Polar,
        Provider::LemonSqueezy,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Stripe => "stripe",
            Provider::Dodo => "dodo",
            Provider::Polar => "polar",
            Provider::LemonSqueezy => "lemonsqueezy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }

    /// Checks the key works and is read-only. Err carries a message fit to show the user.
    pub async fn check_key(self, client: &reqwest::Client, key: &str) -> anyhow::Result<()> {
        match self {
            Provider::Stripe => stripe::check_key(client, key).await,
            Provider::Dodo => dodo::check_key(client, key).await,
            Provider::Polar => polar::check_key(client, key).await,
            Provider::LemonSqueezy => lemonsqueezy::check_key(client, key).await,
        }
    }

    /// Fetches the next page after `cursor`. With no cursor, starts 90 days back.
    pub async fn fetch(
        self,
        client: &reqwest::Client,
        key: &str,
        cursor: Option<&str>,
    ) -> anyhow::Result<Page> {
        match self {
            Provider::Stripe => stripe::fetch(client, key, cursor).await,
            Provider::Dodo => dodo::fetch(client, key, cursor).await,
            Provider::Polar => polar::fetch(client, key, cursor).await,
            Provider::LemonSqueezy => lemonsqueezy::fetch(client, key, cursor).await,
        }
    }
}
