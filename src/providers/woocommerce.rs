use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{Duration, OffsetDateTime, PrimitiveDateTime};

use super::{Fetched, MAX_SUBSCRIPTION_PAGES, Page, Subscription};
use crate::fx::major_to_minor;

const PER_PAGE: u32 = 100;
const BACKFILL: Duration = Duration::days(90);
/// How far back a refund rescan reaches, since refunds do not change an order's creation time.
const REFUND_WINDOW: Duration = Duration::days(60);
const REFUND_SCAN_EVERY: Duration = Duration::hours(6);
const OVERLAP: Duration = Duration::minutes(10);

/// Scans orders newest first down to `floor`, then idles until the next round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cursor {
    page: u32,
    idle: bool,
    /// Unix seconds; orders created before this end the scan.
    floor: i64,
    /// When the current (or, while idle, the last) scan started.
    started: i64,
    /// When the last refund rescan started.
    refund_scan: i64,
}

impl Cursor {
    fn backfill(now: i64) -> Self {
        Cursor {
            page: 1,
            idle: false,
            floor: now - BACKFILL.whole_seconds(),
            started: now,
            refund_scan: now,
        }
    }

    fn resume(self, now: i64) -> Self {
        if !self.idle {
            return self;
        }
        let mut floor = self.started - OVERLAP.whole_seconds();
        let mut refund_scan = self.refund_scan;
        if now - refund_scan >= REFUND_SCAN_EVERY.whole_seconds() {
            floor = floor.min(now - REFUND_WINDOW.whole_seconds());
            refund_scan = now;
        }
        Cursor {
            page: 1,
            idle: false,
            floor,
            started: now,
            refund_scan,
        }
    }

    fn step(mut self, orders: Vec<Order>) -> anyhow::Result<Page> {
        let full = orders.len() as u32 >= PER_PAGE;
        let reached_floor = orders
            .last()
            .is_none_or(|o| o.created().unix_timestamp() < self.floor);
        let payments = orders
            .into_iter()
            .filter(|o| o.created().unix_timestamp() >= self.floor)
            .filter_map(Order::into_fetched)
            .collect();
        if full && !reached_floor {
            self.page += 1;
        } else {
            self.idle = true;
            self.page = 1;
        }
        Ok(Page {
            payments,
            more: !self.idle,
            cursor: Some(serde_json::to_string(&self)?),
        })
    }
}

#[derive(Debug, Deserialize)]
struct Order {
    id: i64,
    status: String,
    currency: String,
    total: String,
    customer_id: i64,
    date_created_gmt: String,
    date_paid_gmt: Option<String>,
    #[serde(default)]
    meta_data: Vec<Meta>,
    #[serde(default)]
    refunds: Vec<Refund>,
    billing: Option<Billing>,
}

#[derive(Debug, Deserialize)]
struct Billing {
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Meta {
    key: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
struct Refund {
    total: String,
}

/// WooCommerce dates in GMT, without an offset.
fn gmt(s: &str) -> Option<OffsetDateTime> {
    let format = time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");
    Some(PrimitiveDateTime::parse(s, format).ok()?.assume_utc())
}

impl Order {
    fn created(&self) -> OffsetDateTime {
        gmt(&self.date_created_gmt).unwrap_or(OffsetDateTime::UNIX_EPOCH)
    }

    fn meta(&self, key: &str) -> Option<&Value> {
        self.meta_data
            .iter()
            .find(|m| m.key == key)
            .map(|m| &m.value)
    }

    fn into_fetched(self) -> Option<Fetched> {
        let status = match self.status.as_str() {
            "processing" | "completed" | "refunded" => "paid",
            "failed" | "cancelled" => "failed",
            _ => return None,
        };
        let amount = major_to_minor(self.total.parse().ok()?, &self.currency);
        if amount <= 0 {
            return None;
        }
        let refunded = self
            .refunds
            .iter()
            .filter_map(|r| r.total.parse::<f64>().ok())
            .map(|t| major_to_minor(t.abs(), &self.currency))
            .sum::<i64>()
            .min(amount);
        let status = match (status, refunded) {
            ("failed", _) => "failed",
            (_, 0) if self.status == "refunded" => "refunded",
            (_, 0) => "paid",
            (_, r) if r >= amount => "refunded",
            _ => "partially_refunded",
        };
        let visitor_id = super::VID_KEYS.iter().find_map(|k| match self.meta(k)? {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        });
        Some(Fetched {
            provider_payment_id: self.id.to_string(),
            ts: self
                .date_paid_gmt
                .as_deref()
                .and_then(gmt)
                .unwrap_or_else(|| self.created()),
            amount,
            currency: self.currency.to_ascii_lowercase(),
            refunded: if status == "refunded" {
                amount
            } else {
                refunded
            },
            status: status.into(),
            visitor_id,
            customer_id: (self.customer_id > 0).then(|| self.customer_id.to_string()),
            // WooCommerce Subscriptions marks renewal orders with this meta key.
            is_renewal: self.meta("_subscription_renewal").is_some(),
            email: self
                .billing
                .and_then(|b| b.email)
                .filter(|e| !e.trim().is_empty()),
        })
    }
}

#[derive(Debug, Deserialize)]
struct WooSubscription {
    id: i64,
    status: String,
    customer_id: i64,
    currency: String,
    total: String,
    billing_period: Option<String>,
    billing_interval: Option<Value>,
    end_date_gmt: Option<String>,
}

impl WooSubscription {
    /// Active is live, and so is pending-cancel until its end date.
    fn into_subscription(self) -> Option<Subscription> {
        let (live, ends_at) = match self.status.as_str() {
            "active" => (true, None),
            "pending-cancel" => (true, self.end_date_gmt.as_deref().and_then(gmt)),
            _ => (false, None),
        };
        let count = match &self.billing_interval {
            Some(Value::String(s)) => s.parse().ok(),
            Some(Value::Number(n)) => n.as_i64(),
            _ => None,
        };
        Some(Subscription {
            subscription_id: self.id.to_string(),
            customer_id: (self.customer_id > 0).then(|| self.customer_id.to_string())?,
            status: self.status,
            live,
            ends_at,
            amount: self
                .total
                .parse::<f64>()
                .ok()
                .map(|t| major_to_minor(t, &self.currency)),
            currency: Some(self.currency.to_ascii_lowercase()),
            interval: self.billing_period,
            interval_count: count.unwrap_or(1),
            mrr_usd: None,
        })
    }
}

pub async fn check_key(client: &reqwest::Client, key: &str, store: &str) -> anyhow::Result<()> {
    let _: Vec<Value> = get(client, key, store, "orders", &[("per_page", "1")]).await?;
    Ok(())
}

pub async fn fetch(
    client: &reqwest::Client,
    key: &str,
    store: &str,
    cursor: Option<&str>,
) -> anyhow::Result<Page> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let cur = match cursor {
        None => Cursor::backfill(now),
        Some(s) => serde_json::from_str::<Cursor>(s)?.resume(now),
    };
    let page = cur.page.to_string();
    let orders = get(
        client,
        key,
        store,
        "orders",
        &[
            ("per_page", &PER_PAGE.to_string()),
            ("page", &page),
            ("orderby", "date"),
            ("order", "desc"),
        ],
    )
    .await?;
    cur.step(orders)
}

/// Lists subscriptions from WooCommerce Subscriptions; None when the store does not have it.
pub async fn subscriptions(
    client: &reqwest::Client,
    key: &str,
    store: &str,
) -> anyhow::Result<Option<Vec<Subscription>>> {
    let mut out = Vec::new();
    for page in 1..=MAX_SUBSCRIPTION_PAGES {
        let resp = request(
            client,
            key,
            store,
            "subscriptions",
            &[("per_page", "100"), ("page", &page.to_string())],
        )
        .await?;
        match resp.status() {
            StatusCode::NOT_FOUND | StatusCode::FORBIDDEN => return Ok(None),
            s if !s.is_success() => return Err(api_error(s)),
            _ => {}
        }
        let list: Vec<WooSubscription> = resp.json().await?;
        let last = (list.len() as u32) < PER_PAGE;
        out.extend(
            list.into_iter()
                .filter_map(WooSubscription::into_subscription),
        );
        if last {
            break;
        }
    }
    Ok(Some(out))
}

/// The key is the consumer key and secret as `ck_...:cs_...`, sent as HTTP Basic auth.
async fn request(
    client: &reqwest::Client,
    key: &str,
    store: &str,
    path: &str,
    query: &[(&str, &str)],
) -> anyhow::Result<reqwest::Response> {
    let (user, secret) = key.split_once(':').ok_or_else(|| {
        anyhow::anyhow!("Paste the consumer key and secret as ck_...:cs_..., joined by a colon.")
    })?;
    Ok(client
        .get(format!("{store}/wp-json/wc/v3/{path}"))
        .basic_auth(user.trim(), Some(secret.trim()))
        .query(query)
        .send()
        .await?)
}

async fn get<T: DeserializeOwned>(
    client: &reqwest::Client,
    key: &str,
    store: &str,
    path: &str,
    query: &[(&str, &str)],
) -> anyhow::Result<T> {
    let resp = request(client, key, store, path, query).await?;
    if !resp.status().is_success() {
        return Err(api_error(resp.status()));
    }
    Ok(resp.json().await?)
}

fn api_error(status: StatusCode) -> anyhow::Error {
    match status {
        StatusCode::UNAUTHORIZED => anyhow::anyhow!(
            "WooCommerce did not accept this key. Create a Read key under WooCommerce > Settings > Advanced > REST API."
        ),
        StatusCode::FORBIDDEN => {
            anyhow::anyhow!("This WooCommerce key cannot read orders. Give it Read access.")
        }
        StatusCode::NOT_FOUND => anyhow::anyhow!(
            "No WooCommerce REST API at this store URL. Check the address and that permalinks are on."
        ),
        s => anyhow::anyhow!("WooCommerce returned HTTP {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, e2e};
    use time::macros::datetime;

    const ORDERS: &str = include_str!("fixtures/woocommerce_orders.json");

    fn orders() -> Vec<Order> {
        serde_json::from_str(ORDERS).unwrap()
    }

    #[test]
    fn maps_orders() {
        let now = datetime!(2026-10-01 00:00 UTC).unix_timestamp();
        let page = Cursor::backfill(now).step(orders()).unwrap();
        let got: Vec<(&str, &str, i64, i64, bool)> = page
            .payments
            .iter()
            .map(|p| {
                (
                    p.provider_payment_id.as_str(),
                    p.status.as_str(),
                    p.amount,
                    p.refunded,
                    p.is_renewal,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("105", "paid", 2000, 0, true),
                ("104", "partially_refunded", 4900, 1000, false),
                ("103", "refunded", 1900, 1900, false),
                ("102", "failed", 900, 0, false),
            ]
        );
        let first = &page.payments[1];
        assert_eq!(first.visitor_id.as_deref(), Some("vid_woo"));
        assert_eq!(first.ts, datetime!(2026-09-28 10:05:00 UTC));
        assert_eq!(first.customer_id.as_deref(), Some("7"));
        assert_eq!(first.email.as_deref(), Some("b@example.com"));
        assert_eq!(page.payments[3].customer_id, None, "guest checkout");
        assert!(!page.more);
    }

    #[test]
    fn stops_at_floor_and_rescans_for_refunds() {
        let now = datetime!(2026-10-01 00:00 UTC).unix_timestamp();
        let mut cur = Cursor::backfill(now);
        cur.floor = datetime!(2026-09-29 00:00 UTC).unix_timestamp();
        let page = cur.step(orders()).unwrap();
        assert_eq!(page.payments.len(), 1);
        let idle: Cursor = serde_json::from_str(page.cursor.as_deref().unwrap()).unwrap();
        assert!(idle.idle);
        assert_eq!(
            idle.clone().resume(now + 60).floor,
            now - OVERLAP.whole_seconds()
        );
        let later = now + REFUND_SCAN_EVERY.whole_seconds();
        assert_eq!(
            idle.resume(later).floor,
            later - REFUND_WINDOW.whole_seconds()
        );
    }

    #[test]
    fn renewals_keep_the_first_channel() {
        let mut v: Vec<Value> = serde_json::from_str(ORDERS).unwrap();
        let mut renewal = v.remove(0);
        let first = v.remove(0);
        renewal["customer_id"] = 7.into();
        let payments: Vec<Fetched> = [first, renewal]
            .into_iter()
            .filter_map(|o| {
                let mut o: Order = serde_json::from_value(o).unwrap();
                o.refunds.clear();
                o.into_fetched()
            })
            .collect();
        e2e::site(Provider::WooCommerce, payments, "vid_woo").assert_all_reddit();
    }

    #[test]
    fn subscription_status_decides_live() {
        let subs: Vec<WooSubscription> =
            serde_json::from_str(include_str!("fixtures/woocommerce_subscriptions.json")).unwrap();
        let subs: Vec<Subscription> = subs
            .into_iter()
            .filter_map(WooSubscription::into_subscription)
            .collect();
        let got: Vec<(&str, bool, Option<f64>)> = subs
            .iter()
            .map(|s| (s.status.as_str(), s.live, s.monthly()))
            .collect();
        assert_eq!(
            got,
            [
                ("active", true, Some(2000.0)),
                ("pending-cancel", true, Some(1000.0)),
                ("cancelled", false, Some(2900.0)),
            ]
        );
        assert_eq!(subs[1].ends_at, Some(datetime!(2026-11-01 00:00 UTC)));
    }

    #[test]
    fn cursor_round_trips() {
        let cur = Cursor::backfill(1_790_000_000);
        let json = serde_json::to_string(&cur).unwrap();
        assert_eq!(serde_json::from_str::<Cursor>(&json).unwrap(), cur);
    }
}
