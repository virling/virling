use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use time::{Duration, OffsetDateTime};

use super::{Fetched, MAX_SUBSCRIPTION_PAGES, Page, Subscription};
use crate::events::TRIAL;

const PAGE_SIZE: u32 = 50;
const BACKFILL: Duration = Duration::days(90);
/// How far back a refund rescan reaches, since refunds do not change `created_at`.
const REFUND_WINDOW: Duration = Duration::days(60);
const REFUND_SCAN_EVERY: Duration = Duration::hours(6);
const OVERLAP: Duration = Duration::minutes(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Env {
    Production,
    Test,
}

impl Env {
    fn base(self) -> &'static str {
        match self {
            Env::Production => "https://api.creem.io",
            Env::Test => "https://test-api.creem.io",
        }
    }
}

/// Scans transactions newest first down to `floor`, then idles until the next round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cursor {
    env: Env,
    page: u32,
    idle: bool,
    /// Unix seconds; transactions created before this end the scan.
    floor: i64,
    /// When the current (or, while idle, the last) scan started.
    started: i64,
    /// When the last refund rescan started.
    refund_scan: i64,
}

impl Cursor {
    fn backfill(env: Env, now: i64) -> Self {
        Cursor {
            env,
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
            ..self
        }
    }

    fn step(mut self, list: List<Transaction>) -> anyhow::Result<Page> {
        let reached_floor = list
            .items
            .last()
            .is_none_or(|t| millis(t.created_at).unix_timestamp() < self.floor);
        let payments = list
            .items
            .into_iter()
            .filter(|t| millis(t.created_at).unix_timestamp() >= self.floor)
            .filter_map(Transaction::into_fetched)
            .collect();
        if list.pagination.next_page.is_some() && !reached_floor {
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
struct List<T> {
    items: Vec<T>,
    pagination: Pagination,
}

#[derive(Debug, Deserialize)]
struct Pagination {
    next_page: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Transaction {
    id: String,
    amount: i64,
    amount_paid: Option<i64>,
    currency: String,
    status: String,
    refunded_amount: Option<i64>,
    customer: Option<String>,
    created_at: i64,
}

impl Transaction {
    fn into_fetched(self) -> Option<Fetched> {
        let status = match self.status.as_str() {
            "paid" => "paid",
            "refunded" | "chargedBack" => "refunded",
            "partialRefund" => "partially_refunded",
            "declined" | "void" | "canceled" | "uncollectible" => "failed",
            _ => return None,
        };
        let amount = self.amount_paid.unwrap_or(self.amount);
        if amount <= 0 {
            return None;
        }
        let refunded = match status {
            "refunded" => amount,
            _ => self.refunded_amount.unwrap_or(0).min(amount),
        };
        Some(Fetched {
            provider_payment_id: self.id,
            ts: millis(self.created_at),
            amount,
            currency: self.currency.to_ascii_lowercase(),
            refunded,
            status: status.into(),
            visitor_id: None,
            customer_id: self.customer,
            is_renewal: false,
            email: None,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CreemSubscription {
    id: String,
    status: String,
    #[serde(default)]
    product: Value,
    customer: Value,
    #[serde(default)]
    items: Vec<Item>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    current_period_end_date: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize)]
struct Product {
    price: Option<i64>,
    currency: Option<String>,
    billing_period: Option<String>,
    recurring_interval: Option<String>,
    recurring_interval_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct Item {
    units: Option<i64>,
}

impl CreemSubscription {
    /// Active, trialing and past_due are live, and so is scheduled_cancel until its period ends.
    fn into_subscription(self) -> Option<Subscription> {
        let (live, ends_at) = match self.status.as_str() {
            "active" | "trialing" | "past_due" => (true, None),
            "scheduled_cancel" => (true, self.current_period_end_date),
            _ => (false, None),
        };
        let product: Option<Product> = serde_json::from_value(self.product).ok();
        let p = product.as_ref();
        let (interval, count) = match p.and_then(|p| p.recurring_interval.clone()) {
            Some(unit) => (Some(unit), p.and_then(|p| p.recurring_interval_count)),
            None => match p.and_then(|p| p.billing_period.as_deref()) {
                Some("every-day") => (Some("day".into()), Some(1)),
                Some("every-month") => (Some("month".into()), Some(1)),
                Some("every-three-months") => (Some("month".into()), Some(3)),
                Some("every-six-months") => (Some("month".into()), Some(6)),
                Some("every-year") => (Some("year".into()), Some(1)),
                _ => (None, None),
            },
        };
        let units = self.items.first().and_then(|i| i.units).unwrap_or(1);
        Some(Subscription {
            subscription_id: self.id,
            customer_id: id_of(&self.customer)?,
            status: self.status,
            live,
            ends_at,
            amount: p.and_then(|p| p.price).map(|price| price * units),
            currency: p.and_then(|p| p.currency.as_ref().map(|c| c.to_ascii_lowercase())),
            interval,
            interval_count: count.unwrap_or(1),
            mrr_usd: None,
        })
    }
}

#[derive(Debug, Deserialize)]
struct Webhook {
    #[serde(rename = "eventType")]
    event_type: String,
    object: Value,
}

#[derive(Debug, Deserialize)]
struct Checkout {
    order: Option<Order>,
    customer: Option<Value>,
    subscription: Option<Value>,
    #[serde(default)]
    metadata: Option<Map<String, Value>>,
}

#[derive(Debug, Deserialize)]
struct Order {
    id: String,
    amount: i64,
    amount_paid: Option<i64>,
    currency: String,
    status: String,
    transaction: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

pub async fn check_key(client: &reqwest::Client, key: &str) -> anyhow::Result<()> {
    detect_env(client, key).await.map(|_| ())
}

pub async fn fetch(
    client: &reqwest::Client,
    key: &str,
    cursor: Option<&str>,
) -> anyhow::Result<Page> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let cur = match cursor {
        None => Cursor::backfill(detect_env(client, key).await?, now),
        Some(s) => serde_json::from_str::<Cursor>(s)?.resume(now),
    };
    let list = get(
        client,
        cur.env,
        key,
        "/v1/transactions/search",
        &[("page_number", cur.page), ("page_size", PAGE_SIZE)],
    )
    .await?;
    cur.step(list)
}

/// Lists every subscription; None when the key may not read them.
pub async fn subscriptions(
    client: &reqwest::Client,
    key: &str,
) -> anyhow::Result<Option<Vec<Subscription>>> {
    let env = detect_env(client, key).await?;
    let mut out = Vec::new();
    for page in 1..=MAX_SUBSCRIPTION_PAGES {
        let resp = client
            .get(format!("{}/v1/subscriptions/search", env.base()))
            .header("x-api-key", key)
            .query(&[("page_number", page), ("page_size", PAGE_SIZE)])
            .send()
            .await?;
        match resp.status() {
            StatusCode::FORBIDDEN => return Ok(None),
            s if !s.is_success() => return Err(api_error(s)),
            _ => {}
        }
        let list: List<CreemSubscription> = resp.json().await?;
        let last = list.pagination.next_page.is_none();
        out.extend(
            list.items
                .into_iter()
                .filter_map(CreemSubscription::into_subscription),
        );
        if last {
            break;
        }
    }
    Ok(Some(out))
}

/// Adds a webhook for completed checkouts pointing at `url` and returns its signing secret.
pub async fn register_webhook(
    client: &reqwest::Client,
    key: &str,
    url: &str,
) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct Created {
        secret: String,
    }
    let env = detect_env(client, key).await?;
    let resp = client
        .post(format!("{}/v1/webhooks", env.base()))
        .header("x-api-key", key)
        .json(&json!({ "name": "Virling", "url": url, "events": ["checkout.completed"] }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "Creem did not accept the webhook (HTTP {}). Give the key the webhooks:write scope; Virling also needs a public https address.",
            resp.status()
        );
    }
    Ok(resp.json::<Created>().await?.secret)
}

/// The payment a completed checkout reports, with the visitor ID from its metadata.
pub fn webhook_payment(body: &[u8]) -> anyhow::Result<Option<Fetched>> {
    let hook: Webhook = serde_json::from_slice(body)?;
    if hook.event_type != "checkout.completed" {
        return Ok(None);
    }
    let checkout: Checkout = serde_json::from_value(hook.object)?;
    let Some(order) = checkout.order else {
        return Ok(None);
    };
    if order.status != "paid" {
        return Ok(None);
    }
    let visitor_id = checkout
        .metadata
        .as_ref()
        .and_then(vid)
        .or_else(|| meta_vid(checkout.subscription.as_ref()))
        .or_else(|| meta_vid(checkout.customer.as_ref()));
    let amount = order.amount_paid.unwrap_or(order.amount);
    let (id, status) = match order.transaction {
        Some(t) if amount > 0 => (t, "paid"),
        _ if amount == 0 && checkout.subscription.is_some() => (order.id, TRIAL),
        _ => return Ok(None),
    };
    Ok(Some(Fetched {
        provider_payment_id: id,
        ts: order.created_at,
        amount,
        currency: order.currency.to_ascii_lowercase(),
        refunded: 0,
        status: status.into(),
        visitor_id,
        customer_id: checkout.customer.as_ref().and_then(id_of),
        is_renewal: false,
        email: None,
    }))
}

/// An expanded object's `id`, or the ID string itself.
fn id_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("id")?.as_str().map(str::to_owned),
        _ => None,
    }
}

fn meta_vid(v: Option<&Value>) -> Option<String> {
    vid(v?.get("metadata")?.as_object()?)
}

fn vid(map: &Map<String, Value>) -> Option<String> {
    super::VID_KEYS.iter().find_map(|k| match map.get(*k) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    })
}

/// Creem timestamps are Unix milliseconds; small values are read as seconds.
fn millis(n: i64) -> OffsetDateTime {
    let ms = if n < 100_000_000_000 { n * 1000 } else { n };
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

async fn detect_env(client: &reqwest::Client, key: &str) -> anyhow::Result<Env> {
    let envs = if key.starts_with("creem_test_") {
        [Env::Test, Env::Production]
    } else {
        [Env::Production, Env::Test]
    };
    let mut status = StatusCode::UNAUTHORIZED;
    for env in envs {
        status = client
            .get(format!("{}/v1/transactions/search", env.base()))
            .header("x-api-key", key)
            .query(&[("page_size", "1")])
            .send()
            .await?
            .status();
        if status.is_success() {
            return Ok(env);
        }
        if status != StatusCode::UNAUTHORIZED {
            break;
        }
    }
    Err(api_error(status))
}

async fn get<T: DeserializeOwned>(
    client: &reqwest::Client,
    env: Env,
    key: &str,
    path: &str,
    query: &[(&str, u32)],
) -> anyhow::Result<T> {
    let resp = client
        .get(format!("{}{path}", env.base()))
        .header("x-api-key", key)
        .query(query)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(api_error(resp.status()));
    }
    Ok(resp.json().await?)
}

fn api_error(status: StatusCode) -> anyhow::Error {
    match status {
        StatusCode::UNAUTHORIZED => anyhow::anyhow!(
            "Creem did not accept this API key. Copy it again from Developers > API keys in Creem."
        ),
        StatusCode::FORBIDDEN => anyhow::anyhow!(
            "This Creem API key cannot read transactions. Create a key with the transactions:read scope, plus subscriptions:read for retention."
        ),
        StatusCode::TOO_MANY_REQUESTS => {
            anyhow::anyhow!("Creem rate limit reached, sync will retry shortly.")
        }
        s => anyhow::anyhow!("Creem returned HTTP {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, e2e};
    use time::macros::datetime;

    const TRANSACTIONS: &str = include_str!("fixtures/creem_transactions.json");
    const CHECKOUT: &str = include_str!("fixtures/creem_webhook_checkout.json");

    fn page(now: i64) -> (Page, Cursor) {
        let cur = Cursor::backfill(Env::Production, now);
        let page = cur
            .step(serde_json::from_str(TRANSACTIONS).unwrap())
            .unwrap();
        let next = serde_json::from_str(page.cursor.as_deref().unwrap()).unwrap();
        (page, next)
    }

    #[test]
    fn maps_transactions() {
        let now = datetime!(2026-10-01 00:00 UTC).unix_timestamp();
        let (page, cur) = page(now);
        let got: Vec<(&str, &str, i64, i64)> = page
            .payments
            .iter()
            .map(|p| {
                (
                    p.provider_payment_id.as_str(),
                    p.status.as_str(),
                    p.amount,
                    p.refunded,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("tran_paid", "paid", 2900, 0),
                ("tran_partial", "partially_refunded", 4900, 1000),
                ("tran_refunded", "refunded", 1900, 1900),
                ("tran_declined", "failed", 900, 0),
            ]
        );
        let paid = &page.payments[0];
        assert_eq!(paid.ts, datetime!(2026-09-30 12:00 UTC));
        assert_eq!(paid.currency, "usd");
        assert_eq!(paid.customer_id.as_deref(), Some("cust_1"));
        assert!(!page.more, "last page");
        assert!(cur.idle);
    }

    #[test]
    fn stops_at_floor_and_rescans_for_refunds() {
        let now = datetime!(2026-10-01 00:00 UTC).unix_timestamp();
        let mut cur = Cursor::backfill(Env::Production, now);
        cur.floor = datetime!(2026-09-29 00:00 UTC).unix_timestamp();
        let mut list: List<Transaction> = serde_json::from_str(TRANSACTIONS).unwrap();
        list.pagination.next_page = Some(2);
        let page = cur.step(list).unwrap();
        assert_eq!(page.payments.len(), 1);
        assert!(!page.more, "older than the floor ends the scan");
        let idle: Cursor = serde_json::from_str(page.cursor.as_deref().unwrap()).unwrap();
        let next = idle.clone().resume(now + 60);
        assert_eq!(next.floor, now - OVERLAP.whole_seconds());
        let later = idle.resume(now + REFUND_SCAN_EVERY.whole_seconds());
        assert_eq!(
            later.floor,
            now + REFUND_SCAN_EVERY.whole_seconds() - REFUND_WINDOW.whole_seconds()
        );
    }

    #[test]
    fn webhook_checkout_carries_the_visitor() {
        let p = webhook_payment(CHECKOUT.as_bytes()).unwrap().unwrap();
        assert_eq!(p.provider_payment_id, "tran_paid");
        assert_eq!(p.visitor_id.as_deref(), Some("vid_creem"));
        assert_eq!(p.customer_id.as_deref(), Some("cust_1"));
        assert_eq!((p.amount, p.status.as_str()), (2900, "paid"));
        let mut v: Value = serde_json::from_str(CHECKOUT).unwrap();
        v["object"]["metadata"] = json!({ "datafast_visitor_id": "vid_compat" });
        let p = webhook_payment(v.to_string().as_bytes()).unwrap().unwrap();
        assert_eq!(p.visitor_id.as_deref(), Some("vid_compat"));
        v["eventType"] = "refund.created".into();
        assert!(webhook_payment(v.to_string().as_bytes()).unwrap().is_none());
    }

    #[test]
    fn webhook_then_sync_credits_the_visit() {
        let hook = webhook_payment(CHECKOUT.as_bytes()).unwrap().unwrap();
        let (page, _) = page(datetime!(2026-10-01 00:00 UTC).unix_timestamp());
        let mut synced = page.payments[0].clone();
        synced.visitor_id = Some(hook.visitor_id.clone().unwrap());
        e2e::site(Provider::Creem, vec![synced], "vid_creem").assert_all_reddit();
    }

    #[test]
    fn free_trial_checkout_is_a_trial_start() {
        let mut v: Value = serde_json::from_str(CHECKOUT).unwrap();
        v["object"]["order"]["amount"] = 0.into();
        v["object"]["order"]["amount_paid"] = 0.into();
        v["object"]["order"]["transaction"] = Value::Null;
        v["object"]["subscription"] = json!({ "id": "sub_1", "metadata": {} });
        let p = webhook_payment(v.to_string().as_bytes()).unwrap().unwrap();
        assert_eq!(
            (p.provider_payment_id.as_str(), p.status.as_str()),
            ("ord_1", TRIAL)
        );
    }

    #[test]
    fn subscription_status_decides_live() {
        let list: List<CreemSubscription> =
            serde_json::from_str(include_str!("fixtures/creem_subscriptions.json")).unwrap();
        let subs: Vec<Subscription> = list
            .items
            .into_iter()
            .filter_map(CreemSubscription::into_subscription)
            .collect();
        let got: Vec<(&str, bool, Option<f64>)> = subs
            .iter()
            .map(|s| (s.status.as_str(), s.live, s.monthly()))
            .collect();
        assert_eq!(
            got,
            [
                ("active", true, Some(1500.0)),
                ("scheduled_cancel", true, Some(1000.0)),
                ("canceled", false, Some(2900.0)),
            ]
        );
        assert_eq!(subs[0].customer_id, "cust_1");
        assert_eq!(subs[1].ends_at, Some(datetime!(2026-11-01 00:00 UTC)));
        assert_eq!(subs[0].currency.as_deref(), Some("usd"));
    }

    #[test]
    fn cursor_round_trips() {
        let cur = Cursor::backfill(Env::Test, 1_790_000_000);
        let json = serde_json::to_string(&cur).unwrap();
        assert!(json.contains("\"env\":\"test\""));
        assert_eq!(serde_json::from_str::<Cursor>(&json).unwrap(), cur);
    }
}
