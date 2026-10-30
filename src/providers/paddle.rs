use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::{Duration, OffsetDateTime};

use super::{Fetched, MAX_SUBSCRIPTION_PAGES, Page, Subscription};
use crate::events::TRIAL;

const PER_PAGE: &str = "30";
const BACKFILL: Duration = Duration::days(90);
/// How far back a refund rescan reaches, since refunds do not change `created_at`.
const REFUND_WINDOW: Duration = Duration::days(60);
const REFUND_SCAN_EVERY: Duration = Duration::hours(6);
const OVERLAP: Duration = Duration::minutes(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Env {
    Live,
    Sandbox,
}

impl Env {
    fn base(self) -> &'static str {
        match self {
            Env::Live => "https://api.paddle.com",
            Env::Sandbox => "https://sandbox-api.paddle.com",
        }
    }
}

/// Scans completed transactions newest first down to `floor`, then idles until the next round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cursor {
    env: Env,
    /// The last transaction read in this scan; the next page starts after it.
    after: Option<String>,
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
            after: None,
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
            after: None,
            idle: false,
            floor,
            started: now,
            refund_scan,
            ..self
        }
    }

    fn step(mut self, list: List<Transaction>) -> anyhow::Result<Page> {
        let reached_floor = list
            .data
            .last()
            .is_none_or(|t| t.created_at.unix_timestamp() < self.floor);
        self.after = list.data.last().map(|t| t.id.clone());
        let payments = list
            .data
            .into_iter()
            .filter(|t| t.created_at.unix_timestamp() >= self.floor)
            .filter_map(Transaction::into_fetched)
            .collect();
        if !list.meta.pagination.has_more || reached_floor {
            self.idle = true;
            self.after = None;
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
    data: Vec<T>,
    meta: Meta,
}

#[derive(Debug, Deserialize)]
struct Meta {
    pagination: Pagination,
}

#[derive(Debug, Deserialize)]
struct Pagination {
    has_more: bool,
}

#[derive(Debug, Deserialize)]
struct Transaction {
    id: String,
    status: String,
    customer_id: Option<String>,
    subscription_id: Option<String>,
    origin: String,
    currency_code: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    billed_at: Option<OffsetDateTime>,
    details: Details,
    #[serde(default)]
    custom_data: Option<Map<String, Value>>,
    #[serde(default)]
    adjustments: Vec<Adjustment>,
}

#[derive(Debug, Deserialize)]
struct Details {
    totals: Totals,
}

#[derive(Debug, Deserialize)]
struct Totals {
    total: String,
}

#[derive(Debug, Deserialize)]
struct Adjustment {
    action: String,
    status: String,
    totals: Totals,
}

impl Transaction {
    fn into_fetched(self) -> Option<Fetched> {
        if !matches!(self.status.as_str(), "completed" | "paid") {
            return None;
        }
        let amount: i64 = self.details.totals.total.parse().ok()?;
        let visitor_id = self.custom_data.as_ref().and_then(vid);
        if amount == 0 {
            // A free trial's first transaction starts the trial its later payments are credited to.
            if self.subscription_id.is_none() || self.origin != "web" {
                return None;
            }
            return Some(Fetched {
                provider_payment_id: self.id,
                ts: self.created_at,
                amount: 0,
                currency: self.currency_code.to_ascii_lowercase(),
                refunded: 0,
                status: TRIAL.into(),
                visitor_id,
                customer_id: self.customer_id,
                is_renewal: false,
            });
        }
        let refunded: i64 = self
            .adjustments
            .iter()
            .filter(|a| {
                matches!(a.action.as_str(), "refund" | "chargeback") && a.status == "approved"
            })
            .filter_map(|a| a.totals.total.parse::<i64>().ok())
            .sum::<i64>()
            .min(amount);
        let status = match refunded {
            0 => "paid",
            r if r >= amount => "refunded",
            _ => "partially_refunded",
        };
        Some(Fetched {
            provider_payment_id: self.id,
            ts: self.billed_at.unwrap_or(self.created_at),
            amount,
            currency: self.currency_code.to_ascii_lowercase(),
            refunded,
            status: status.into(),
            visitor_id,
            customer_id: self.customer_id,
            is_renewal: matches!(
                self.origin.as_str(),
                "subscription_recurring" | "subscription_update" | "subscription_charge"
            ),
        })
    }
}

#[derive(Debug, Deserialize)]
struct PaddleSubscription {
    id: String,
    status: String,
    customer_id: String,
    currency_code: String,
    #[serde(default)]
    items: Vec<Item>,
    billing_cycle: Option<Cycle>,
    scheduled_change: Option<Scheduled>,
}

#[derive(Debug, Deserialize)]
struct Item {
    quantity: i64,
    price: Price,
}

#[derive(Debug, Deserialize)]
struct Price {
    unit_price: Money,
}

#[derive(Debug, Deserialize)]
struct Money {
    amount: String,
}

#[derive(Debug, Deserialize)]
struct Cycle {
    interval: String,
    frequency: i64,
}

#[derive(Debug, Deserialize)]
struct Scheduled {
    action: String,
    #[serde(with = "time::serde::rfc3339")]
    effective_at: OffsetDateTime,
}

impl PaddleSubscription {
    /// Active, trialing and past_due are live; one scheduled to cancel stays live until then.
    fn into_subscription(self) -> Subscription {
        let live = matches!(self.status.as_str(), "active" | "trialing" | "past_due");
        let ends_at = self
            .scheduled_change
            .filter(|c| live && c.action == "cancel")
            .map(|c| c.effective_at);
        let amount = self
            .items
            .iter()
            .map(|i| Some(i.price.unit_price.amount.parse::<i64>().ok()? * i.quantity))
            .sum::<Option<i64>>()
            .filter(|_| !self.items.is_empty());
        Subscription {
            subscription_id: self.id,
            customer_id: self.customer_id,
            status: self.status,
            live,
            ends_at,
            amount,
            currency: Some(self.currency_code.to_ascii_lowercase()),
            interval: self.billing_cycle.as_ref().map(|c| c.interval.clone()),
            interval_count: self.billing_cycle.map_or(1, |c| c.frequency),
            mrr_usd: None,
        }
    }
}

pub async fn check_key(client: &reqwest::Client, key: &str) -> anyhow::Result<()> {
    let env = detect_env(client, key).await?;
    let _: List<Value> = get(client, env, key, "/adjustments", &[("per_page", "1")]).await?;
    Ok(())
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
    let mut query = vec![
        ("status", "completed,paid"),
        ("order_by", "id[DESC]"),
        ("include", "adjustments"),
        ("per_page", PER_PAGE),
    ];
    if let Some(after) = &cur.after {
        query.push(("after", after));
    }
    let list = get(client, cur.env, key, "/transactions", &query).await?;
    cur.step(list)
}

/// Lists every subscription; None when the key may not read them.
pub async fn subscriptions(
    client: &reqwest::Client,
    key: &str,
) -> anyhow::Result<Option<Vec<Subscription>>> {
    let env = detect_env(client, key).await?;
    let mut out = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..MAX_SUBSCRIPTION_PAGES {
        let mut query = vec![("per_page", "200")];
        if let Some(a) = &after {
            query.push(("after", a));
        }
        let resp = client
            .get(format!("{}/subscriptions", env.base()))
            .bearer_auth(key)
            .query(&query)
            .send()
            .await?;
        match resp.status() {
            StatusCode::FORBIDDEN => return Ok(None),
            s if !s.is_success() => return Err(api_error(s)),
            _ => {}
        }
        let list: List<PaddleSubscription> = resp.json().await?;
        after = list.data.last().map(|s| s.id.clone());
        let more = list.meta.pagination.has_more;
        out.extend(
            list.data
                .into_iter()
                .map(PaddleSubscription::into_subscription),
        );
        if !more {
            break;
        }
    }
    Ok(Some(out))
}

fn vid(map: &Map<String, Value>) -> Option<String> {
    super::VID_KEYS.iter().find_map(|k| match map.get(*k) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    })
}

async fn detect_env(client: &reqwest::Client, key: &str) -> anyhow::Result<Env> {
    let envs = if key.starts_with("pdl_sdbx_") {
        [Env::Sandbox, Env::Live]
    } else {
        [Env::Live, Env::Sandbox]
    };
    let mut status = StatusCode::UNAUTHORIZED;
    for env in envs {
        status = client
            .get(format!("{}/transactions", env.base()))
            .bearer_auth(key)
            .query(&[("per_page", "1")])
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
    query: &[(&str, &str)],
) -> anyhow::Result<T> {
    let resp = client
        .get(format!("{}{path}", env.base()))
        .bearer_auth(key)
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
            "Paddle did not accept this API key. Copy it again from Developer tools > Authentication in Paddle."
        ),
        StatusCode::FORBIDDEN => anyhow::anyhow!(
            "This Paddle API key is missing a permission. Create a key with transaction.read and adjustment.read, plus subscription.read for retention."
        ),
        StatusCode::TOO_MANY_REQUESTS => {
            anyhow::anyhow!("Paddle rate limit reached, sync will retry shortly.")
        }
        s => anyhow::anyhow!("Paddle returned HTTP {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, e2e};
    use time::macros::datetime;

    const TRANSACTIONS: &str = include_str!("fixtures/paddle_transactions.json");

    fn list() -> List<Transaction> {
        serde_json::from_str(TRANSACTIONS).unwrap()
    }

    fn items() -> Vec<Value> {
        let v: Value = serde_json::from_str(TRANSACTIONS).unwrap();
        v["data"].as_array().unwrap().clone()
    }

    #[test]
    fn maps_transactions() {
        let now = datetime!(2026-10-01 00:00 UTC).unix_timestamp();
        let page = Cursor::backfill(Env::Live, now).step(list()).unwrap();
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
                ("txn_05", "paid", 2000, 0, true),
                ("txn_04", "partially_refunded", 4900, 1000, false),
                ("txn_03", "refunded", 1900, 1900, false),
                ("txn_01", TRIAL, 0, 0, false),
            ]
        );
        let first = &page.payments[1];
        assert_eq!(first.visitor_id.as_deref(), Some("vid_paddle"));
        assert_eq!(first.currency, "eur");
        assert_eq!(first.customer_id.as_deref(), Some("ctm_1"));
        assert_eq!(page.payments[0].ts, datetime!(2026-09-30 12:00:05 UTC));
        assert!(!page.more);
    }

    #[test]
    fn stops_at_floor_pages_on_and_rescans_for_refunds() {
        let now = datetime!(2026-10-01 00:00 UTC).unix_timestamp();
        let mut v: Value = serde_json::from_str(TRANSACTIONS).unwrap();
        v["meta"]["pagination"]["has_more"] = true.into();
        let full = Cursor::backfill(Env::Live, now)
            .step(serde_json::from_value(v.clone()).unwrap())
            .unwrap();
        assert!(full.more, "another page while above the floor");
        let next: Cursor = serde_json::from_str(full.cursor.as_deref().unwrap()).unwrap();
        assert_eq!(next.after.as_deref(), Some("txn_01"));

        let mut cur = Cursor::backfill(Env::Live, now);
        cur.floor = datetime!(2026-09-29 00:00 UTC).unix_timestamp();
        let page = cur.step(serde_json::from_value(v).unwrap()).unwrap();
        assert_eq!(page.payments.len(), 1);
        assert!(!page.more, "older than the floor ends the scan");
        let idle: Cursor = serde_json::from_str(page.cursor.as_deref().unwrap()).unwrap();
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
        let mut first = items()[1].clone();
        first["adjustments"] = Value::Array(vec![]);
        let mut renewal = items()[0].clone();
        renewal["customer_id"] = "ctm_1".into();
        let payments: Vec<Fetched> = [first, renewal]
            .into_iter()
            .filter_map(|t| {
                serde_json::from_value::<Transaction>(t)
                    .unwrap()
                    .into_fetched()
            })
            .collect();
        assert!(payments[1].is_renewal && payments[1].visitor_id.is_none());
        e2e::site(Provider::Paddle, payments, "vid_paddle").assert_all_reddit();
    }

    #[test]
    fn subscription_status_decides_live() {
        let list: List<PaddleSubscription> =
            serde_json::from_str(include_str!("fixtures/paddle_subscriptions.json")).unwrap();
        let subs: Vec<Subscription> = list
            .data
            .into_iter()
            .map(PaddleSubscription::into_subscription)
            .collect();
        let got: Vec<(&str, bool, Option<f64>)> = subs
            .iter()
            .map(|s| (s.status.as_str(), s.live, s.monthly()))
            .collect();
        assert_eq!(
            got,
            [
                ("active", true, Some(3000.0)),
                ("active", true, Some(1000.0)),
                ("canceled", false, Some(2900.0)),
            ]
        );
        assert_eq!(subs[1].ends_at, Some(datetime!(2026-11-01 00:00 UTC)));
    }

    #[test]
    fn cursor_round_trips() {
        let cur = Cursor::backfill(Env::Sandbox, 1_790_000_000);
        let json = serde_json::to_string(&cur).unwrap();
        assert!(json.contains("\"env\":\"sandbox\""));
        assert_eq!(serde_json::from_str::<Cursor>(&json).unwrap(), cur);
    }
}
