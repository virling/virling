use anyhow::Context;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::{Duration, OffsetDateTime};

use super::{Fetched, MAX_SUBSCRIPTION_PAGES, Page, Subscription};

const LIMIT: u32 = 100;
const RECHECK_PER_CALL: usize = 10;
const PENDING_CHECK_EVERY: i64 = 600;
const PENDING_MAX_AGE: Duration = Duration::days(30);
const PENDING_CAP: usize = 500;
const REFUND_HOLD_MAX: Duration = Duration::days(7);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Env {
    Production,
    Sandbox,
}

impl Env {
    fn base(self) -> &'static str {
        match self {
            Env::Production => "https://api.polar.sh",
            Env::Sandbox => "https://sandbox-api.polar.sh",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Orders,
    Refunds,
    Recheck,
}

/// Sync state: new orders by creation time, then new refunds, then re-fetch the orders they touched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cursor {
    env: Env,
    phase: Phase,
    #[serde(with = "time::serde::rfc3339")]
    orders_after: OffsetDateTime,
    orders_page: u32,
    #[serde(with = "time::serde::rfc3339::option")]
    orders_seen: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    refunds_after: OffsetDateTime,
    refunds_page: u32,
    #[serde(with = "time::serde::rfc3339::option")]
    refunds_seen: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    refund_hold: Option<OffsetDateTime>,
    queue: Vec<String>,
    /// Unpaid orders as (id, first seen unix time), re-checked every few minutes.
    pending: Vec<(String, i64)>,
    pending_checked: i64,
}

#[derive(Debug, Deserialize)]
struct List<T> {
    items: Vec<T>,
    pagination: Pagination,
}

#[derive(Debug, Deserialize)]
struct Pagination {
    max_page: u32,
}

#[derive(Debug, Deserialize)]
struct Order {
    id: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    status: String,
    billing_reason: String,
    net_amount: i64,
    refunded_amount: i64,
    currency: String,
    customer_id: String,
    #[serde(default)]
    metadata: Map<String, Value>,
    #[serde(default)]
    custom_field_data: Option<Map<String, Value>>,
    #[serde(default)]
    customer: Option<OrderCustomer>,
}

#[derive(Debug, Deserialize)]
struct OrderCustomer {
    #[serde(default)]
    metadata: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct Refund {
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    status: String,
    order_id: String,
}

#[derive(Debug, Deserialize)]
struct PolarSubscription {
    id: String,
    status: String,
    customer_id: String,
    amount: Option<i64>,
    currency: Option<String>,
    recurring_interval: Option<String>,
    recurring_interval_count: Option<i64>,
}

impl PolarSubscription {
    /// Active, trialing and past_due are live; a cancel at period end stays active until then.
    fn into_subscription(self) -> Subscription {
        Subscription {
            live: matches!(self.status.as_str(), "active" | "trialing" | "past_due"),
            subscription_id: self.id,
            customer_id: self.customer_id,
            status: self.status,
            ends_at: None,
            amount: self.amount,
            currency: self.currency,
            interval: self.recurring_interval,
            interval_count: self.recurring_interval_count.unwrap_or(1),
            mrr_usd: None,
        }
    }
}

enum Mapped {
    Payment(Fetched),
    Pending,
    Skip,
}

pub async fn check_key(client: &reqwest::Client, key: &str) -> anyhow::Result<()> {
    let env = detect_env(client, key).await?;
    let _: List<Refund> = get(
        client,
        env,
        key,
        "/v1/refunds/",
        &[("limit", "1".into())],
        "refunds:read",
    )
    .await?;
    Ok(())
}

/// Lists every subscription; None when the token lacks the optional subscriptions:read scope.
pub async fn subscriptions(
    client: &reqwest::Client,
    key: &str,
) -> anyhow::Result<Option<Vec<Subscription>>> {
    let env = detect_env(client, key).await?;
    let mut out = Vec::new();
    for page in 1..=MAX_SUBSCRIPTION_PAGES {
        let resp = client
            .get(format!("{}/v1/subscriptions/", env.base()))
            .bearer_auth(key)
            .query(&[("limit", LIMIT), ("page", page)])
            .send()
            .await
            .context("could not reach Polar")?;
        match resp.status() {
            StatusCode::FORBIDDEN => return Ok(None),
            s if !s.is_success() => return Err(api_error(s, "subscriptions:read")),
            _ => {}
        }
        let list: List<PolarSubscription> = resp
            .json()
            .await
            .context("unexpected response from Polar")?;
        let last = page >= list.pagination.max_page;
        out.extend(
            list.items
                .into_iter()
                .map(PolarSubscription::into_subscription),
        );
        if last {
            break;
        }
    }
    Ok(Some(out))
}

pub async fn fetch(
    client: &reqwest::Client,
    key: &str,
    cursor: Option<&str>,
) -> anyhow::Result<Page> {
    let now = OffsetDateTime::now_utc();
    let mut cur = match cursor {
        Some(c) => serde_json::from_str(c).context("invalid Polar sync cursor")?,
        None => Cursor::new(detect_env(client, key).await?, now),
    };
    let (payments, more) = match cur.phase {
        Phase::Orders => {
            let query = [
                ("sorting", "created_at".to_string()),
                ("limit", LIMIT.to_string()),
                ("page", cur.orders_page.to_string()),
                ("created_after", format_ts(cur.orders_after)?),
            ];
            let list = get(client, cur.env, key, "/v1/orders/", &query, "orders:read").await?;
            cur.orders_step(list, now)
        }
        Phase::Refunds => {
            let query = [
                ("sorting", "-created_at".to_string()),
                ("limit", LIMIT.to_string()),
                ("page", cur.refunds_page.to_string()),
            ];
            let list = get(client, cur.env, key, "/v1/refunds/", &query, "refunds:read").await?;
            (Vec::new(), cur.refunds_step(list, now))
        }
        Phase::Recheck => {
            let take = cur.queue.len().min(RECHECK_PER_CALL);
            let checked: Vec<String> = cur.queue.drain(..take).collect();
            let mut orders = Vec::with_capacity(checked.len());
            for id in &checked {
                if let Some(order) = get_order(client, cur.env, key, id).await? {
                    orders.push(order);
                }
            }
            cur.recheck_step(&checked, orders, now)
        }
    };
    Ok(Page {
        payments,
        cursor: Some(serde_json::to_string(&cur)?),
        more,
    })
}

impl Cursor {
    fn new(env: Env, now: OffsetDateTime) -> Self {
        Cursor {
            env,
            phase: Phase::Orders,
            orders_after: now - Duration::days(90),
            orders_page: 1,
            orders_seen: None,
            // The backfill reads current refund totals, so only refunds from now on matter.
            refunds_after: now,
            refunds_page: 1,
            refunds_seen: None,
            refund_hold: None,
            queue: Vec::new(),
            pending: Vec::new(),
            pending_checked: now.unix_timestamp(),
        }
    }

    fn orders_step(&mut self, list: List<Order>, now: OffsetDateTime) -> (Vec<Fetched>, bool) {
        let full = list.items.len() as u32 >= LIMIT;
        let mut out = Vec::new();
        for order in list.items {
            let created = order.created_at;
            self.orders_seen = Some(self.orders_seen.map_or(created, |s| s.max(created)));
            let id = order.id.clone();
            match map_order(order) {
                Mapped::Payment(f) => out.push(f),
                Mapped::Pending => self.track_pending(&id, now),
                Mapped::Skip => {}
            }
        }
        if full && self.orders_page < list.pagination.max_page {
            self.orders_page += 1;
            return (out, true);
        }
        if let Some(seen) = self.orders_seen.take() {
            self.orders_after = seen;
        }
        self.orders_page = 1;
        self.phase = Phase::Refunds;
        (out, true)
    }

    fn refunds_step(&mut self, list: List<Refund>, now: OffsetDateTime) -> bool {
        let full = list.items.len() as u32 >= LIMIT;
        let mut reached_old = false;
        for refund in list.items {
            let created = refund.created_at;
            if created <= self.refunds_after {
                reached_old = true;
                break;
            }
            self.refunds_seen = Some(self.refunds_seen.map_or(created, |s| s.max(created)));
            match refund.status.as_str() {
                "succeeded" => {
                    if !self.queue.contains(&refund.order_id) {
                        self.queue.push(refund.order_id);
                    }
                }
                "pending" if now - created < REFUND_HOLD_MAX => {
                    self.refund_hold = Some(self.refund_hold.map_or(created, |h| h.min(created)));
                }
                _ => {}
            }
        }
        if !reached_old && full && self.refunds_page < list.pagination.max_page {
            self.refunds_page += 1;
            return true;
        }
        // A pending refund holds the watermark just below it so it is read again once settled.
        self.refunds_after = match self.refund_hold.take() {
            Some(hold) => hold - Duration::nanoseconds(1),
            None => self.refunds_seen.unwrap_or(self.refunds_after),
        };
        self.refunds_seen = None;
        self.refunds_page = 1;
        if now.unix_timestamp() - self.pending_checked >= PENDING_CHECK_EVERY {
            self.pending_checked = now.unix_timestamp();
            for (id, _) in &self.pending {
                if !self.queue.contains(id) {
                    self.queue.push(id.clone());
                }
            }
        }
        if self.queue.is_empty() {
            self.phase = Phase::Orders;
            false
        } else {
            self.phase = Phase::Recheck;
            true
        }
    }

    fn recheck_step(
        &mut self,
        checked: &[String],
        orders: Vec<Order>,
        now: OffsetDateTime,
    ) -> (Vec<Fetched>, bool) {
        let still_pending: Vec<&str> = orders
            .iter()
            .filter(|o| matches!(o.status.as_str(), "draft" | "pending"))
            .map(|o| o.id.as_str())
            .collect();
        self.pending
            .retain(|(id, _)| !checked.contains(id) || still_pending.contains(&id.as_str()));
        let mut out = Vec::new();
        for order in orders {
            let id = order.id.clone();
            match map_order(order) {
                Mapped::Payment(f) => out.push(f),
                Mapped::Pending => self.track_pending(&id, now),
                Mapped::Skip => {}
            }
        }
        let oldest = (now - PENDING_MAX_AGE).unix_timestamp();
        self.pending.retain(|(_, since)| *since >= oldest);
        if self.queue.is_empty() {
            self.phase = Phase::Orders;
            (out, false)
        } else {
            (out, true)
        }
    }

    fn track_pending(&mut self, id: &str, now: OffsetDateTime) {
        if self.pending.iter().any(|(p, _)| p == id) {
            return;
        }
        if self.pending.len() >= PENDING_CAP {
            self.pending.remove(0);
        }
        self.pending.push((id.to_string(), now.unix_timestamp()));
    }
}

fn map_order(order: Order) -> Mapped {
    let status = match order.status.as_str() {
        "paid" | "refunded" | "partially_refunded" => order.status.clone(),
        "void" => "failed".to_string(),
        "draft" | "pending" => return Mapped::Pending,
        _ => return Mapped::Skip,
    };
    if order.net_amount == 0 {
        return Mapped::Skip;
    }
    let visitor_id = vid(&order.metadata)
        .or_else(|| order.custom_field_data.as_ref().and_then(vid))
        .or_else(|| order.customer.as_ref().and_then(|c| vid(&c.metadata)));
    Mapped::Payment(Fetched {
        provider_payment_id: order.id,
        ts: order.created_at,
        amount: order.net_amount,
        currency: order.currency.to_ascii_lowercase(),
        refunded: order.refunded_amount,
        status,
        visitor_id,
        customer_id: Some(order.customer_id),
        is_renewal: matches!(
            order.billing_reason.as_str(),
            "subscription_cycle" | "subscription_update"
        ),
    })
}

fn vid(map: &Map<String, Value>) -> Option<String> {
    match map.get("virling_vid") {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

fn format_ts(ts: OffsetDateTime) -> anyhow::Result<String> {
    Ok(ts.format(&time::format_description::well_known::Rfc3339)?)
}

async fn detect_env(client: &reqwest::Client, key: &str) -> anyhow::Result<Env> {
    for env in [Env::Production, Env::Sandbox] {
        let status = client
            .get(format!("{}/v1/orders/", env.base()))
            .bearer_auth(key)
            .query(&[("limit", "1")])
            .send()
            .await
            .context("could not reach Polar")?
            .status();
        if status == StatusCode::UNAUTHORIZED {
            continue;
        }
        if status.is_success() {
            return Ok(env);
        }
        return Err(api_error(status, "orders:read"));
    }
    Err(api_error(StatusCode::UNAUTHORIZED, "orders:read"))
}

async fn get<T: DeserializeOwned>(
    client: &reqwest::Client,
    env: Env,
    key: &str,
    path: &str,
    query: &[(&str, String)],
    scope: &str,
) -> anyhow::Result<T> {
    let resp = client
        .get(format!("{}{path}", env.base()))
        .bearer_auth(key)
        .query(query)
        .send()
        .await
        .context("could not reach Polar")?;
    if !resp.status().is_success() {
        return Err(api_error(resp.status(), scope));
    }
    resp.json().await.context("unexpected response from Polar")
}

async fn get_order(
    client: &reqwest::Client,
    env: Env,
    key: &str,
    id: &str,
) -> anyhow::Result<Option<Order>> {
    let resp = client
        .get(format!("{}/v1/orders/{id}", env.base()))
        .bearer_auth(key)
        .send()
        .await
        .context("could not reach Polar")?;
    match resp.status() {
        StatusCode::NOT_FOUND => Ok(None),
        s if s.is_success() => Ok(Some(
            resp.json()
                .await
                .context("unexpected response from Polar")?,
        )),
        s => Err(api_error(s, "orders:read")),
    }
}

fn api_error(status: StatusCode, scope: &str) -> anyhow::Error {
    match status {
        StatusCode::UNAUTHORIZED => anyhow::anyhow!(
            "Polar did not accept this token. Paste an Organization Access Token from your Polar organization settings."
        ),
        StatusCode::FORBIDDEN => anyhow::anyhow!(
            "This Polar token is missing the {scope} scope. Create an Organization Access Token with orders:read, refunds:read and subscriptions:read."
        ),
        StatusCode::TOO_MANY_REQUESTS => {
            anyhow::anyhow!("Polar rate limit reached, sync will retry shortly.")
        }
        s => anyhow::anyhow!("Polar returned HTTP {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_status_decides_live() {
        let list: List<PolarSubscription> =
            serde_json::from_str(include_str!("fixtures/polar_subscriptions.json")).unwrap();
        assert_eq!(list.pagination.max_page, 1);
        let subs: Vec<Subscription> = list
            .items
            .into_iter()
            .map(PolarSubscription::into_subscription)
            .collect();
        assert_eq!(
            (
                subs[0].amount,
                subs[0].currency.as_deref(),
                subs[0].monthly()
            ),
            (Some(1500), Some("usd"), Some(1500.0))
        );
        assert_eq!(
            (subs[1].interval_count, subs[1].monthly()),
            (1, Some(1250.0))
        );
        let subs: Vec<(String, bool)> = subs.into_iter().map(|s| (s.status, s.live)).collect();
        let want: Vec<(String, bool)> = [
            ("active", true),
            ("trialing", true),
            ("past_due", true),
            ("canceled", false),
            ("unpaid", false),
            ("incomplete", false),
        ]
        .iter()
        .map(|(s, l)| (s.to_string(), *l))
        .collect();
        assert_eq!(subs, want);
    }
    use crate::providers::{Provider, e2e};
    use time::macros::datetime;

    const ORDERS: &str = include_str!("fixtures/polar_orders.json");
    const REFUNDS: &str = include_str!("fixtures/polar_refunds.json");

    fn orders() -> List<Order> {
        serde_json::from_str(ORDERS).unwrap()
    }

    fn cursor(now: OffsetDateTime) -> Cursor {
        Cursor::new(Env::Production, now)
    }

    #[test]
    fn maps_orders_from_fixture() {
        let now = datetime!(2026-09-08 00:00 UTC);
        let mut cur = cursor(now);
        let (payments, more) = cur.orders_step(orders(), now);
        assert!(more);
        assert_eq!(payments.len(), 4);

        let plain = &payments[0];
        assert_eq!(
            plain.provider_payment_id,
            "8f1c2d4e-5a6b-4c7d-8e9f-0a1b2c3d4e5f"
        );
        assert_eq!(plain.ts, datetime!(2026-09-01 10:15:30.123456 UTC));
        assert_eq!((plain.amount, plain.refunded), (4900, 0));
        assert_eq!(plain.currency, "usd");
        assert_eq!(plain.status, "paid");
        assert_eq!(plain.visitor_id, None);
        assert_eq!(
            plain.customer_id.as_deref(),
            Some("c0000000-0000-4000-8000-000000000001")
        );
        assert!(!plain.is_renewal);

        let partial = &payments[1];
        assert_eq!(partial.status, "partially_refunded");
        assert_eq!((partial.amount, partial.refunded), (9000, 3000));
        assert_eq!(partial.currency, "eur");
        assert_eq!(partial.visitor_id.as_deref(), Some("vid_abc123"));

        let refunded = &payments[2];
        assert_eq!(refunded.status, "refunded");
        assert_eq!(refunded.refunded, refunded.amount);
        assert_eq!(refunded.visitor_id.as_deref(), Some("vid_from_field"));

        let renewal = &payments[3];
        assert!(renewal.is_renewal);
        assert_eq!(renewal.amount, 2000);
        assert_eq!(renewal.visitor_id.as_deref(), Some("vid_customer"));
    }

    #[test]
    fn orders_step_advances_watermark_and_tracks_pending() {
        let now = datetime!(2026-09-08 00:00 UTC);
        let mut cur = cursor(now);
        cur.orders_step(orders(), now);
        assert_eq!(cur.phase, Phase::Refunds);
        assert_eq!(cur.orders_after, datetime!(2026-09-07 00:00 UTC));
        assert_eq!(cur.orders_page, 1);
        assert_eq!(
            cur.pending,
            vec![(
                "cc000000-0000-4000-8000-000000000005".to_string(),
                now.unix_timestamp()
            )]
        );
    }

    #[test]
    fn orders_step_pages_when_full() {
        let now = datetime!(2026-09-08 00:00 UTC);
        let mut cur = cursor(now);
        let one = &orders_json_items()[0];
        let items: Vec<Value> = (0..LIMIT).map(|_| one.clone()).collect();
        let body =
            serde_json::json!({"items": items, "pagination": {"total_count": 250, "max_page": 3}});
        let (payments, more) = cur.orders_step(serde_json::from_value(body).unwrap(), now);
        assert!(more);
        assert_eq!(payments.len(), LIMIT as usize);
        assert_eq!(cur.phase, Phase::Orders);
        assert_eq!(cur.orders_page, 2);
        assert_eq!(cur.orders_after, now - Duration::days(90));
    }

    fn orders_json_items() -> Vec<Value> {
        let v: Value = serde_json::from_str(ORDERS).unwrap();
        v["items"].as_array().unwrap().clone()
    }

    /// The fixture's trial order as a paid one; other reasons get `id`, `created_at` and no visitor ID.
    fn subscription_order(id: &str, created_at: &str, reason: &str) -> Fetched {
        let mut v = orders_json_items()[5].clone();
        v["net_amount"] = 2000.into();
        if reason != "subscription_create" {
            v["id"] = id.into();
            v["created_at"] = created_at.into();
            v["billing_reason"] = reason.into();
            v["metadata"] = Value::Object(Map::new());
        }
        match map_order(serde_json::from_value(v).unwrap()) {
            Mapped::Payment(f) => f,
            _ => panic!("{id} is not a payment"),
        }
    }

    #[test]
    fn renewals_and_upgrades_keep_the_first_channel() {
        let payments = vec![
            subscription_order("first", "", "subscription_create"),
            subscription_order("upgrade", "2026-09-20T00:00:00Z", "subscription_update"),
            subscription_order("renewal", "2026-12-07T00:00:00Z", "subscription_cycle"),
        ];
        assert_eq!(payments[0].visitor_id.as_deref(), Some("vid_trial"));
        assert!(
            payments[1..]
                .iter()
                .all(|p| p.is_renewal && p.visitor_id.is_none())
        );
        e2e::site(Provider::Polar, payments, "vid_trial").assert_all_reddit();
    }

    #[test]
    fn refunds_queue_orders_and_hold_on_pending() {
        let now = datetime!(2026-09-11 00:00 UTC);
        let mut cur = cursor(now);
        cur.phase = Phase::Refunds;
        cur.refunds_after = datetime!(2026-09-05 00:00 UTC);
        let more = cur.refunds_step(serde_json::from_str(REFUNDS).unwrap(), now);
        assert!(more);
        assert_eq!(cur.phase, Phase::Recheck);
        assert_eq!(cur.queue, vec!["9a2b3c4d-1111-4222-8333-444455556666"]);
        assert_eq!(
            cur.refunds_after,
            datetime!(2026-09-10 12:00 UTC) - Duration::nanoseconds(1)
        );
    }

    #[test]
    fn refunds_without_news_finish_round() {
        let now = datetime!(2026-09-11 00:00 UTC);
        let mut cur = cursor(now);
        cur.phase = Phase::Refunds;
        let more = cur.refunds_step(serde_json::from_str(REFUNDS).unwrap(), now);
        assert!(!more);
        assert_eq!(cur.phase, Phase::Orders);
        assert!(cur.queue.is_empty());
        assert_eq!(cur.refunds_after, now);
    }

    #[test]
    fn recheck_emits_refunded_order_and_resolves_pending() {
        let now = datetime!(2026-09-20 00:00 UTC);
        let mut cur = cursor(now);
        cur.phase = Phase::Recheck;
        cur.pending = vec![
            (
                "cc000000-0000-4000-8000-000000000005".into(),
                now.unix_timestamp(),
            ),
            ("gone".into(), now.unix_timestamp()),
        ];
        let items = orders_json_items();
        let mut paid = items[4].clone();
        paid["status"] = "paid".into();
        let fetched: Vec<Order> = vec![
            serde_json::from_value(items[1].clone()).unwrap(),
            serde_json::from_value(paid).unwrap(),
        ];
        let checked = vec![
            "9a2b3c4d-1111-4222-8333-444455556666".to_string(),
            "cc000000-0000-4000-8000-000000000005".to_string(),
            "gone".to_string(),
        ];
        let (payments, more) = cur.recheck_step(&checked, fetched, now);
        assert!(!more);
        assert_eq!(cur.phase, Phase::Orders);
        assert_eq!(payments.len(), 2);
        assert_eq!(payments[0].refunded, 3000);
        assert!(payments[1].is_renewal);
        assert!(cur.pending.is_empty());
    }

    #[test]
    fn cursor_round_trips() {
        let now = datetime!(2026-09-11 00:00:00.5 UTC);
        let mut cur = Cursor::new(Env::Sandbox, now);
        cur.refund_hold = Some(now);
        let json = serde_json::to_string(&cur).unwrap();
        assert!(json.contains("\"env\":\"sandbox\""));
        assert_eq!(serde_json::from_str::<Cursor>(&json).unwrap(), cur);
    }
}
