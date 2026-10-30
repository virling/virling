mod dodo;
pub mod lemonsqueezy;
mod polar;
pub mod revenuecat;
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

/// A subscription's current state as the provider reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub subscription_id: String,
    pub customer_id: String,
    pub status: String,
    /// The provider still bills it: active, trialing or retrying a failed renewal.
    pub live: bool,
    /// When a cancelled but still live subscription stops; None when it renews.
    pub ends_at: Option<OffsetDateTime>,
    /// Price per billing period in minor units of `currency`, before discounts and tax; None when unknown.
    pub amount: Option<i64>,
    pub currency: Option<String>,
    /// Billing period unit (day, week, month or year) and how many of them.
    pub interval: Option<String>,
    pub interval_count: i64,
    /// `amount` per month in USD, set by sync.
    pub mrr_usd: Option<f64>,
}

impl Subscription {
    /// `amount` per month in minor units; None without a price or a known interval.
    pub fn monthly(&self) -> Option<f64> {
        Some(self.amount? as f64 / months(self.interval.as_deref()?, self.interval_count)?)
    }
}

/// Months in `count` billing periods of `unit`: day, week, month or year.
pub fn months(unit: &str, count: i64) -> Option<f64> {
    let one = match unit {
        "day" => 12.0 / 365.0,
        "week" => 12.0 / 52.0,
        "month" => 1.0,
        "year" => 12.0,
        _ => return None,
    };
    Some(one * count.max(1) as f64)
}

/// Most subscriptions read per round, newest first; older ones fall back to payment recency.
pub const MAX_SUBSCRIPTION_PAGES: u32 = 50;

/// Payment metadata keys that hold the visitor ID, in order; the second is a compatibility alias.
pub const VID_KEYS: [&str; 2] = ["virling_vid", "datafast_visitor_id"];

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
    RevenueCat,
}

impl Provider {
    pub const ALL: [Provider; 5] = [
        Provider::Stripe,
        Provider::Dodo,
        Provider::Polar,
        Provider::LemonSqueezy,
        Provider::RevenueCat,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Stripe => "stripe",
            Provider::Dodo => "dodo",
            Provider::Polar => "polar",
            Provider::LemonSqueezy => "lemonsqueezy",
            Provider::RevenueCat => "revenuecat",
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
            Provider::RevenueCat => revenuecat::check_key(client, key).await,
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
            Provider::RevenueCat => revenuecat::fetch(client, key, cursor).await,
        }
    }

    /// Every subscription's current state; None when the key may not read subscriptions.
    pub async fn subscriptions(
        self,
        client: &reqwest::Client,
        key: &str,
    ) -> anyhow::Result<Option<Vec<Subscription>>> {
        match self {
            Provider::Stripe => stripe::subscriptions(client, key).await,
            Provider::Dodo => dodo::subscriptions(client, key).await,
            Provider::Polar => polar::subscriptions(client, key).await,
            Provider::LemonSqueezy => lemonsqueezy::subscriptions(client, key).await,
            Provider::RevenueCat => revenuecat::subscriptions(client, key).await,
        }
    }
}

#[cfg(test)]
pub(crate) mod e2e {
    use time::Duration;

    use super::{Fetched, Provider};
    use crate::events::{DuckStore, Event, EventStore, Payment};
    use crate::stats::{PaymentFilter, Period, Touch};

    pub struct Site {
        _dir: tempfile::TempDir,
        pub store: DuckStore,
        pub period: Period,
    }

    /// Stores `payments` after a Reddit visit by `visitor` a day before the earliest, then rebuilds attribution.
    pub fn site(provider: Provider, payments: Vec<Fetched>, visitor: &str) -> Site {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        let first = payments.iter().map(|p| p.ts).min().unwrap();
        let last = payments.iter().map(|p| p.ts).max().unwrap();
        store
            .insert_events(&[Event {
                site_id: "s".into(),
                ts: first - Duration::days(1),
                visitor_id: visitor.into(),
                session_id: "reddit".into(),
                kind: "pageview".into(),
                name: None,
                path: "/pricing".into(),
                referrer_host: Some("reddit.com".into()),
                utm_source: Some("reddit".into()),
                utm_medium: None,
                utm_campaign: Some("launch".into()),
                utm_content: None,
                utm_term: None,
                channel: "Social".into(),
                country: Some("CA".into()),
                device: "Mobile".into(),
                browser: "Safari".into(),
                os: "iOS".into(),
                region: None,
                city: None,
                host: None,
                language: None,
                direct: None,
            }])
            .unwrap();
        let rows: Vec<Payment> = payments
            .into_iter()
            .map(|f| Payment {
                site_id: "s".into(),
                provider: provider.as_str().into(),
                provider_payment_id: f.provider_payment_id,
                ts: f.ts,
                amount: f.amount,
                currency: f.currency.to_ascii_lowercase(),
                amount_usd: Some(f.amount as f64 / 100.0),
                refunded: f.refunded,
                status: f.status,
                visitor_id: f.visitor_id,
                customer_id: f.customer_id,
                is_renewal: f.is_renewal,
            })
            .collect();
        store.upsert_payments(&rows).unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 30)
            .unwrap();
        let period = Period {
            from: (first - Duration::days(1)).date(),
            to: last.date(),
            tz: "UTC".into(),
        };
        Site {
            _dir: dir,
            store,
            period,
        }
    }

    impl Site {
        pub fn payments(&self) -> usize {
            self.store
                .payment_list("s", &self.period, PaymentFilter::All, 100, 0)
                .unwrap()
                .rows
                .len()
        }

        pub fn touch(&self, id: &str) -> Option<Touch> {
            self.store
                .payment_list("s", &self.period, PaymentFilter::All, 100, 0)
                .unwrap()
                .rows
                .into_iter()
                .find(|p| p.provider_payment_id == id)
                .unwrap()
                .touch
        }

        /// Asserts every payment is credited to the Reddit visit and the report counts all of them under it.
        pub fn assert_all_reddit(&self) {
            let rows = self
                .store
                .payment_list("s", &self.period, PaymentFilter::All, 100, 0)
                .unwrap()
                .rows;
            for p in &rows {
                let t = p.touch.as_ref().unwrap_or_else(|| panic!("{p:?}"));
                assert_eq!(
                    (
                        t.session_id.as_str(),
                        t.channel.as_str(),
                        t.referrer_host.as_deref(),
                        t.campaign.as_deref(),
                        t.landing_page.as_deref(),
                        t.country.as_deref(),
                        t.device.as_deref(),
                    ),
                    (
                        "reddit",
                        "Social",
                        Some("reddit.com"),
                        Some("launch"),
                        Some("/pricing"),
                        Some("CA"),
                        Some("Mobile"),
                    ),
                    "{}",
                    p.provider_payment_id
                );
            }
            let net: f64 = rows.iter().filter_map(|p| p.net_usd).sum();
            let r = self.store.report("s", &self.period).unwrap();
            for (breakdown, label) in [
                (&r.channels, "Social"),
                (&r.referrers, "reddit.com"),
                (&r.landing_pages, "/pricing"),
                (&r.countries, "CA"),
            ] {
                assert_eq!(breakdown.len(), 1, "{breakdown:?}");
                assert_eq!(breakdown[0].label, label);
                assert_eq!(breakdown[0].payments, rows.len() as u64, "{label}");
                assert!((breakdown[0].revenue_usd - net).abs() < 1e-9, "{label}");
            }
            for p in &rows {
                let day = r.days.iter().find(|d| d.date == p.ts.date()).unwrap();
                assert_eq!(day.by_channel.len(), 1);
                assert_eq!(day.by_channel[0].0.as_deref(), Some("Social"));
            }
        }
    }
}
