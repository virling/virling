use std::collections::HashMap;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use super::{Fetched, MAX_SUBSCRIPTION_PAGES, Page, Subscription};

const API: &str = "https://api.lemonsqueezy.com/v1";
const PAGE_SIZE: u32 = 100;
const BACKFILL: Duration = Duration::days(90);
/// How far back a refund rescan reaches, since refunds do not change `created_at`.
const REFUND_WINDOW: Duration = Duration::days(60);
const REFUND_SCAN_EVERY: Duration = Duration::hours(6);
const OVERLAP: Duration = Duration::minutes(10);

pub async fn check_key(client: &reqwest::Client, key: &str) -> anyhow::Result<()> {
    get(client, key, "users/me", &[]).await?;
    Ok(())
}

pub async fn fetch(
    client: &reqwest::Client,
    key: &str,
    cursor: Option<&str>,
) -> anyhow::Result<Page> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let cur = match cursor {
        None => Cursor::backfill(now),
        Some(s) => serde_json::from_str::<Cursor>(s)?.resume(now),
    };
    let path = match cur.stage {
        Stage::Orders => "orders",
        Stage::Invoices | Stage::Idle => "subscription-invoices",
    };
    let body = get(
        client,
        key,
        path,
        &[("page[number]", cur.page), ("page[size]", PAGE_SIZE)],
    )
    .await?;
    step(cur, serde_json::from_str(&body)?)
}

/// Lists every subscription; LemonSqueezy keys can always read them.
pub async fn subscriptions(
    client: &reqwest::Client,
    key: &str,
) -> anyhow::Result<Option<Vec<Subscription>>> {
    let mut out = Vec::new();
    for page in 1..=MAX_SUBSCRIPTION_PAGES {
        let body = get(
            client,
            key,
            "subscriptions",
            &[("page[number]", page), ("page[size]", PAGE_SIZE)],
        )
        .await?;
        let list: SubscriptionList = serde_json::from_str(&body)?;
        out.extend(list.data.into_iter().filter_map(to_subscription));
        if list.links.next.is_none() {
            break;
        }
    }
    Ok(Some(out))
}

/// on_trial, active and past_due are live, and so is cancelled until its ends_at (its grace period).
fn to_subscription(r: SubscriptionResource) -> Option<Subscription> {
    let a = r.attributes;
    let (live, ends_at) = match a.status.as_str() {
        "on_trial" | "active" | "past_due" => (true, None),
        "cancelled" => (a.ends_at.is_some(), a.ends_at),
        _ => (false, None),
    };
    Some(Subscription {
        subscription_id: r.id,
        customer_id: a.customer_id?.to_string(),
        status: a.status,
        live,
        ends_at,
    })
}

async fn get(
    client: &reqwest::Client,
    key: &str,
    path: &str,
    query: &[(&str, u32)],
) -> anyhow::Result<String> {
    let resp = client
        .get(format!("{API}/{path}"))
        .bearer_auth(key)
        .header(reqwest::header::ACCEPT, "application/vnd.api+json")
        .query(query)
        .send()
        .await?;
    match resp.status() {
        s if s.is_success() => Ok(resp.text().await?),
        StatusCode::UNAUTHORIZED => anyhow::bail!(
            "LemonSqueezy did not accept this API key. Copy it again from Settings > API; it may have been deleted or expired."
        ),
        StatusCode::FORBIDDEN => anyhow::bail!(
            "This LemonSqueezy API key is not allowed to read orders. Create a new key under Settings > API."
        ),
        StatusCode::TOO_MANY_REQUESTS => {
            anyhow::bail!("LemonSqueezy rate limit reached; sync will retry shortly.")
        }
        s => anyhow::bail!("LemonSqueezy returned HTTP {s}. Try again in a minute."),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Stage {
    Orders,
    Invoices,
    Idle,
}

/// Scans orders then subscription invoices newest first, down to `floor`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cursor {
    stage: Stage,
    page: u32,
    /// Unix seconds; items created before this end the scan.
    floor: i64,
    /// When the current (or, while idle, the last) scan started.
    started: i64,
    /// When the last refund rescan started.
    refund_scan: i64,
}

impl Cursor {
    fn backfill(now: i64) -> Self {
        Cursor {
            stage: Stage::Orders,
            page: 1,
            floor: now - BACKFILL.whole_seconds(),
            started: now,
            refund_scan: now,
        }
    }

    fn resume(self, now: i64) -> Self {
        if self.stage != Stage::Idle {
            return self;
        }
        let mut floor = self.started - OVERLAP.whole_seconds();
        let mut refund_scan = self.refund_scan;
        if now - refund_scan >= REFUND_SCAN_EVERY.whole_seconds() {
            floor = floor.min(now - REFUND_WINDOW.whole_seconds());
            refund_scan = now;
        }
        Cursor {
            stage: Stage::Orders,
            page: 1,
            floor,
            started: now,
            refund_scan,
        }
    }
}

fn step(mut cur: Cursor, list: List) -> anyhow::Result<Page> {
    let invoices = cur.stage != Stage::Orders;
    let reached_floor = list
        .data
        .last()
        .is_none_or(|r| r.attributes.created_at.unix_timestamp() < cur.floor);
    let payments = list
        .data
        .into_iter()
        .filter(|r| r.attributes.created_at.unix_timestamp() >= cur.floor)
        .filter_map(|r| to_fetched(r, invoices))
        .collect();
    if list.links.next.is_some() && !reached_floor {
        cur.page += 1;
    } else if cur.stage == Stage::Orders {
        cur.stage = Stage::Invoices;
        cur.page = 1;
    } else {
        cur.stage = Stage::Idle;
        cur.page = 1;
    }
    Ok(Page {
        payments,
        more: cur.stage != Stage::Idle,
        cursor: Some(serde_json::to_string(&cur)?),
    })
}

fn to_fetched(r: Resource, invoice: bool) -> Option<Fetched> {
    let a = r.attributes;
    // A subscription's first charge is also an order.
    if invoice && a.billing_reason.as_deref() == Some("initial") {
        return None;
    }
    let status = match a.status.as_str() {
        "paid" => "paid",
        "refunded" => "refunded",
        "partial_refund" => "partially_refunded",
        "failed" | "fraudulent" | "void" => "failed",
        _ => return None,
    };
    let refunded = if status == "refunded" {
        a.total
    } else {
        a.refunded_amount
    };
    let visitor_id = r
        .meta
        .and_then(|m| m.custom_data.get("virling_vid").cloned())
        .and_then(|v| v.as_str().map(str::to_owned));
    Some(Fetched {
        provider_payment_id: format!("{}_{}", if invoice { "invoice" } else { "order" }, r.id),
        ts: a.created_at,
        amount: a.total,
        currency: a.currency.to_ascii_lowercase(),
        refunded,
        status: status.to_string(),
        visitor_id,
        customer_id: a.customer_id.map(|id| id.to_string()),
        is_renewal: invoice && matches!(a.billing_reason.as_deref(), Some("renewal" | "updated")),
    })
}

#[derive(Deserialize)]
struct List {
    data: Vec<Resource>,
    #[serde(default)]
    links: Links,
}

#[derive(Deserialize, Default)]
struct Links {
    next: Option<String>,
}

#[derive(Deserialize)]
struct SubscriptionList {
    data: Vec<SubscriptionResource>,
    #[serde(default)]
    links: Links,
}

#[derive(Deserialize)]
struct SubscriptionResource {
    id: String,
    attributes: SubscriptionAttributes,
}

#[derive(Deserialize)]
struct SubscriptionAttributes {
    customer_id: Option<i64>,
    status: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    ends_at: Option<OffsetDateTime>,
}

#[derive(Deserialize)]
struct Resource {
    id: String,
    attributes: Attributes,
    meta: Option<ResourceMeta>,
}

#[derive(Deserialize)]
struct ResourceMeta {
    #[serde(default)]
    custom_data: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct Attributes {
    customer_id: Option<i64>,
    currency: String,
    total: i64,
    #[serde(default)]
    refunded_amount: i64,
    status: String,
    billing_reason: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_status_decides_live() {
        let list: SubscriptionList =
            serde_json::from_str(include_str!("fixtures/lemonsqueezy_subscriptions.json")).unwrap();
        assert!(list.links.next.is_none());
        let subs: Vec<(String, bool, bool)> = list
            .data
            .into_iter()
            .filter_map(to_subscription)
            .map(|s| (s.status, s.live, s.ends_at.is_some()))
            .collect();
        let want: Vec<(String, bool, bool)> = [
            ("active", true, false),
            ("on_trial", true, false),
            ("past_due", true, false),
            ("cancelled", true, true),
            ("expired", false, false),
            ("paused", false, false),
            ("unpaid", false, false),
        ]
        .iter()
        .map(|(s, l, e)| (s.to_string(), *l, *e))
        .collect();
        assert_eq!(subs, want);
    }
    use crate::providers::{Provider, e2e};
    use time::macros::datetime;

    const ORDERS: &str = include_str!("fixtures/lemonsqueezy_orders.json");
    const INVOICES: &str = include_str!("fixtures/lemonsqueezy_invoices.json");

    fn cursor(stage: Stage, floor: OffsetDateTime) -> Cursor {
        Cursor {
            stage,
            page: 1,
            floor: floor.unix_timestamp(),
            started: 0,
            refund_scan: 0,
        }
    }

    fn decode(page: &Page) -> Cursor {
        serde_json::from_str(page.cursor.as_deref().unwrap()).unwrap()
    }

    #[test]
    fn parses_orders() {
        let page = step(
            cursor(Stage::Orders, datetime!(2026-09-01 0:00 UTC)),
            serde_json::from_str(ORDERS).unwrap(),
        )
        .unwrap();
        assert_eq!(
            page.payments,
            vec![
                Fetched {
                    provider_payment_id: "order_104".into(),
                    ts: datetime!(2026-10-10 08:15:02 UTC),
                    amount: 2460,
                    currency: "eur".into(),
                    refunded: 0,
                    status: "paid".into(),
                    visitor_id: Some("v_8c1f2e".into()),
                    customer_id: Some("31".into()),
                    is_renewal: false,
                },
                Fetched {
                    provider_payment_id: "order_103".into(),
                    ts: datetime!(2026-10-08 17:45:53 UTC),
                    amount: 1199,
                    currency: "usd".into(),
                    refunded: 1199,
                    status: "refunded".into(),
                    visitor_id: None,
                    customer_id: Some("30".into()),
                    is_renewal: false,
                },
                Fetched {
                    provider_payment_id: "order_102".into(),
                    ts: datetime!(2026-10-05 09:00 UTC),
                    amount: 5000,
                    currency: "gbp".into(),
                    refunded: 1500,
                    status: "partially_refunded".into(),
                    visitor_id: None,
                    customer_id: Some("29".into()),
                    is_renewal: false,
                },
            ]
        );
        assert!(page.more);
        assert_eq!(decode(&page).stage, Stage::Orders);
        assert_eq!(decode(&page).page, 2);
    }

    fn resource(json: &str, id: &str) -> Resource {
        let list: List = serde_json::from_str(json).unwrap();
        list.data.into_iter().find(|r| r.id == id).unwrap()
    }

    #[test]
    fn renewals_and_upgrades_keep_the_first_channel() {
        let mut upgrade = resource(INVOICES, "77");
        upgrade.id = "78".into();
        upgrade.attributes.billing_reason = Some("updated".into());
        upgrade.attributes.created_at = datetime!(2026-10-11 09:00 UTC);
        let payments = vec![
            to_fetched(resource(ORDERS, "104"), false).unwrap(),
            to_fetched(upgrade, true).unwrap(),
            to_fetched(resource(INVOICES, "77"), true).unwrap(),
        ];
        assert_eq!(payments[0].visitor_id.as_deref(), Some("v_8c1f2e"));
        assert!(
            payments[1..]
                .iter()
                .all(|p| p.is_renewal && p.visitor_id.is_none())
        );
        e2e::site(Provider::LemonSqueezy, payments, "v_8c1f2e").assert_all_reddit();
    }

    #[test]
    fn renewal_without_customer_is_unattributed() {
        let mut orphan = resource(INVOICES, "77");
        orphan.attributes.customer_id = None;
        let payments = vec![
            to_fetched(resource(ORDERS, "104"), false).unwrap(),
            to_fetched(orphan, true).unwrap(),
        ];
        let site = e2e::site(Provider::LemonSqueezy, payments, "v_8c1f2e");
        assert!(site.touch("order_104").is_some());
        assert_eq!(site.touch("invoice_77"), None);
    }

    #[test]
    fn parses_renewal_invoices_and_skips_initial() {
        let page = step(
            cursor(Stage::Invoices, datetime!(2026-05-01 0:00 UTC)),
            serde_json::from_str(INVOICES).unwrap(),
        )
        .unwrap();
        let ids: Vec<_> = page
            .payments
            .iter()
            .map(|p| p.provider_payment_id.as_str())
            .collect();
        assert_eq!(ids, ["invoice_77", "invoice_75"]);
        let renewal = &page.payments[0];
        assert!(renewal.is_renewal);
        assert_eq!(renewal.amount, 999);
        assert_eq!(renewal.customer_id.as_deref(), Some("31"));
        let partial = &page.payments[1];
        assert_eq!(
            (
                partial.status.as_str(),
                partial.refunded,
                partial.currency.as_str()
            ),
            ("partially_refunded", 900, "eur")
        );
        assert!(!page.more);
        assert_eq!(decode(&page).stage, Stage::Idle);
    }

    #[test]
    fn stops_at_floor() {
        let page = step(
            cursor(Stage::Orders, datetime!(2026-10-06 0:00 UTC)),
            serde_json::from_str(ORDERS).unwrap(),
        )
        .unwrap();
        assert_eq!(page.payments.len(), 2);
        assert!(page.more);
        let next = decode(&page);
        assert_eq!((next.stage, next.page), (Stage::Invoices, 1));
    }

    #[test]
    fn idle_cursor_restarts_with_bounded_floor() {
        let now = 1_800_000_000;
        let idle = Cursor {
            stage: Stage::Idle,
            page: 1,
            floor: 0,
            started: now - 60,
            refund_scan: now - 60,
        };
        let light = idle.clone().resume(now);
        assert_eq!((light.stage, light.page), (Stage::Orders, 1));
        assert_eq!(light.floor, now - 60 - OVERLAP.whole_seconds());
        assert_eq!(light.refund_scan, now - 60);

        let stale = Cursor {
            refund_scan: now - REFUND_SCAN_EVERY.whole_seconds(),
            ..idle
        };
        let deep = stale.resume(now);
        assert_eq!(deep.floor, now - REFUND_WINDOW.whole_seconds());
        assert_eq!(deep.refund_scan, now);
    }

    #[test]
    fn first_sync_backfills_90_days() {
        let c = Cursor::backfill(1_800_000_000);
        assert_eq!(c.floor, 1_800_000_000 - 90 * 86_400);
        assert_eq!(c.stage, Stage::Orders);
    }
}
