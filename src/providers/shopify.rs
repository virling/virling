use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use super::{Fetched, Page, Subscription};
use crate::fx::major_to_minor;

const VERSION: &str = "2026-07";
const LIMIT: &str = "250";
const BACKFILL: Duration = Duration::days(60);
const OVERLAP: Duration = Duration::minutes(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    /// Orders created since the last one read, oldest first.
    New,
    /// Orders updated since the last round, which brings refunds and payment changes.
    Updated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cursor {
    phase: Phase,
    /// Highest order ID read; None until the backfill has read one.
    since_id: Option<i64>,
    /// Unix seconds the backfill starts from.
    floor: i64,
    /// Unix seconds; orders updated after this are read again in the Updated phase.
    updated_after: i64,
    /// Next page of the Updated phase, from the Link header.
    page_info: Option<String>,
    /// When the current round started.
    started: i64,
}

#[derive(Debug, Deserialize)]
struct Orders {
    orders: Vec<Order>,
}

#[derive(Debug, Deserialize)]
struct Order {
    id: i64,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    processed_at: Option<OffsetDateTime>,
    total_price: String,
    currency: String,
    financial_status: Option<String>,
    customer: Option<Customer>,
    #[serde(default)]
    note_attributes: Vec<NoteAttribute>,
    #[serde(default)]
    refunds: Vec<Refund>,
    source_name: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Customer {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct NoteAttribute {
    name: String,
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Refund {
    #[serde(default)]
    transactions: Vec<Transaction>,
}

#[derive(Debug, Deserialize)]
struct Transaction {
    amount: String,
    kind: String,
    status: String,
}

impl Order {
    fn into_fetched(self) -> Option<Fetched> {
        let status = match self.financial_status.as_deref()? {
            "paid" => "paid",
            "partially_refunded" => "partially_refunded",
            "refunded" => "refunded",
            "voided" => "failed",
            _ => return None,
        };
        let amount = major_to_minor(self.total_price.parse().ok()?, &self.currency);
        if amount <= 0 {
            return None;
        }
        let refunded = match status {
            "refunded" => amount,
            _ => self
                .refunds
                .iter()
                .flat_map(|r| &r.transactions)
                .filter(|t| t.kind == "refund" && t.status == "success")
                .filter_map(|t| t.amount.parse::<f64>().ok())
                .map(|a| major_to_minor(a, &self.currency))
                .sum::<i64>()
                .min(amount),
        };
        let visitor_id = super::VID_KEYS.iter().find_map(|k| {
            self.note_attributes
                .iter()
                .find(|a| a.name == *k)?
                .value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        });
        Some(Fetched {
            provider_payment_id: self.id.to_string(),
            ts: self.processed_at.unwrap_or(self.created_at),
            amount,
            currency: self.currency.to_ascii_lowercase(),
            refunded,
            status: status.into(),
            visitor_id,
            customer_id: self.customer.map(|c| c.id.to_string()),
            // Orders from Shopify Subscriptions renewals come from the subscription contract.
            is_renewal: self.source_name.as_deref() == Some("subscription_contract"),
            email: self.email.filter(|e| !e.trim().is_empty()),
        })
    }
}

/// The `page_info` of the Link header's next page.
fn next_page(link: Option<&str>) -> Option<String> {
    link?.split(',').find_map(|part| {
        let (url, rel) = part.split_once(';')?;
        if !rel.contains(r#"rel="next""#) {
            return None;
        }
        let url = url.trim().trim_start_matches('<').trim_end_matches('>');
        url::Url::parse(url)
            .ok()?
            .query_pairs()
            .find(|(k, _)| k == "page_info")
            .map(|(_, v)| v.into_owned())
    })
}

fn rfc3339(unix: i64) -> anyhow::Result<String> {
    Ok(OffsetDateTime::from_unix_timestamp(unix)?
        .format(&time::format_description::well_known::Rfc3339)?)
}

pub async fn check_key(client: &reqwest::Client, key: &str, store: &str) -> anyhow::Result<()> {
    shop(store)?;
    get(client, key, store, &[("limit", "1"), ("status", "any")]).await?;
    Ok(())
}

pub async fn fetch(
    client: &reqwest::Client,
    key: &str,
    store: &str,
    cursor: Option<&str>,
) -> anyhow::Result<Page> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let mut cur = match cursor {
        Some(c) => serde_json::from_str(c)?,
        None => Cursor {
            phase: Phase::New,
            since_id: None,
            floor: now - BACKFILL.whole_seconds(),
            updated_after: now,
            page_info: None,
            started: now,
        },
    };
    let (orders, link) = match cur.phase {
        Phase::New => {
            let since = cur.since_id.map(|id| id.to_string());
            let floor = rfc3339(cur.floor)?;
            let mut query = vec![("status", "any"), ("limit", LIMIT)];
            match &since {
                Some(id) => query.push(("since_id", id)),
                None => query.push(("created_at_min", &floor)),
            }
            get(client, key, store, &query).await?
        }
        Phase::Updated => {
            let after = rfc3339(cur.updated_after)?;
            let query = match &cur.page_info {
                Some(p) => vec![("limit", LIMIT), ("page_info", p.as_str())],
                None => vec![
                    ("status", "any"),
                    ("limit", LIMIT),
                    ("updated_at_min", after.as_str()),
                ],
            };
            get(client, key, store, &query).await?
        }
    };
    cur.step(orders, next_page(link.as_deref()), now)
}

impl Cursor {
    fn step(&mut self, orders: Vec<Order>, next: Option<String>, now: i64) -> anyhow::Result<Page> {
        let full = orders.len() >= LIMIT.parse::<usize>()?;
        if let Some(max) = orders.iter().map(|o| o.id).max() {
            self.since_id = Some(self.since_id.map_or(max, |s| s.max(max)));
        }
        let payments = orders.into_iter().filter_map(Order::into_fetched).collect();
        let more = match self.phase {
            Phase::New if full => true,
            Phase::New => {
                self.phase = Phase::Updated;
                self.page_info = None;
                true
            }
            Phase::Updated if next.is_some() => {
                self.page_info = next;
                true
            }
            Phase::Updated => {
                // The next round reads orders updated since this one started, with a small overlap.
                self.phase = Phase::New;
                self.page_info = None;
                self.updated_after = self.started - OVERLAP.whole_seconds();
                self.started = now;
                false
            }
        };
        Ok(Page {
            payments,
            more,
            cursor: Some(serde_json::to_string(self)?),
        })
    }
}

/// Shopify does not offer subscription status through this API, so retention uses payment recency.
pub async fn subscriptions(
    _client: &reqwest::Client,
    _key: &str,
    _store: &str,
) -> anyhow::Result<Option<Vec<Subscription>>> {
    Ok(None)
}

/// The Admin API only answers on the store's myshopify.com address.
fn shop(store: &str) -> anyhow::Result<()> {
    let host = url::Url::parse(store)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default();
    anyhow::ensure!(
        host.ends_with(".myshopify.com"),
        "Use your store's myshopify.com address, such as https://your-store.myshopify.com."
    );
    Ok(())
}

async fn get(
    client: &reqwest::Client,
    key: &str,
    store: &str,
    query: &[(&str, &str)],
) -> anyhow::Result<(Vec<Order>, Option<String>)> {
    let resp = client
        .get(format!("{store}/admin/api/{VERSION}/orders.json"))
        .header("X-Shopify-Access-Token", key)
        .query(query)
        .send()
        .await?;
    match resp.status() {
        s if s.is_success() => {
            let link = resp
                .headers()
                .get("link")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            Ok((resp.json::<Orders>().await?.orders, link))
        }
        StatusCode::UNAUTHORIZED => anyhow::bail!(
            "Shopify did not accept this token. Paste the Admin API access token of your custom app."
        ),
        StatusCode::FORBIDDEN => anyhow::bail!(
            "This Shopify token cannot read orders. Give the custom app the read_orders scope."
        ),
        StatusCode::TOO_MANY_REQUESTS => {
            anyhow::bail!("Shopify rate limit reached, sync will retry shortly.")
        }
        s => anyhow::bail!("Shopify returned HTTP {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, e2e};
    use time::macros::datetime;

    const ORDERS: &str = include_str!("fixtures/shopify_orders.json");

    fn orders() -> Vec<Order> {
        serde_json::from_str::<Orders>(ORDERS).unwrap().orders
    }

    #[test]
    fn maps_orders() {
        let got: Vec<(String, String, i64, i64, bool)> = orders()
            .into_iter()
            .filter_map(Order::into_fetched)
            .map(|p| {
                (
                    p.provider_payment_id,
                    p.status,
                    p.amount,
                    p.refunded,
                    p.is_renewal,
                )
            })
            .collect();
        let want: Vec<(String, String, i64, i64, bool)> = [
            ("1001", "paid", 4900, 0, false),
            ("1002", "partially_refunded", 2000, 500, false),
            ("1003", "refunded", 1900, 1900, false),
            ("1004", "failed", 900, 0, false),
            ("1006", "paid", 2000, 0, true),
        ]
        .into_iter()
        .map(|(a, b, c, d, e)| (a.into(), b.into(), c, d, e))
        .collect();
        assert_eq!(got, want);
        let first = orders().remove(0).into_fetched().unwrap();
        assert_eq!(first.visitor_id.as_deref(), Some("vid_shop"));
        assert_eq!(first.customer_id.as_deref(), Some("207119551"));
        assert_eq!(first.ts, datetime!(2026-09-28 10:05:00 UTC));
        assert_eq!(first.currency, "eur");
        assert_eq!(first.email.as_deref(), Some("ada@example.com"));
    }

    #[test]
    fn rounds_read_new_orders_then_updated_ones() {
        let now = datetime!(2026-10-01 00:00 UTC).unix_timestamp();
        let mut cur = Cursor {
            phase: Phase::New,
            since_id: None,
            floor: now - BACKFILL.whole_seconds(),
            updated_after: now,
            page_info: None,
            started: now,
        };
        let page = cur.step(orders(), None, now).unwrap();
        assert!(page.more);
        assert_eq!((cur.phase, cur.since_id), (Phase::Updated, Some(1006)));
        cur.step(Vec::new(), Some("abc".into()), now).unwrap();
        assert_eq!(cur.page_info.as_deref(), Some("abc"));
        let later = now + 60;
        let page = cur.step(Vec::new(), None, later).unwrap();
        assert!(!page.more);
        assert_eq!(cur.phase, Phase::New);
        assert_eq!(cur.updated_after, now - OVERLAP.whole_seconds());
        assert_eq!(cur.started, later);
        let json = serde_json::to_string(&cur).unwrap();
        assert_eq!(serde_json::from_str::<Cursor>(&json).unwrap(), cur);
    }

    #[test]
    fn link_header_gives_the_next_page() {
        let link = r#"<https://s.myshopify.com/admin/api/2026-07/orders.json?limit=250&page_info=prev1>; rel="previous", <https://s.myshopify.com/admin/api/2026-07/orders.json?limit=250&page_info=next2>; rel="next""#;
        assert_eq!(next_page(Some(link)).as_deref(), Some("next2"));
        assert_eq!(next_page(None), None);
        assert!(shop("https://s.myshopify.com").is_ok());
        assert!(shop("https://shop.example.com").is_err());
    }

    #[test]
    fn renewals_keep_the_first_channel() {
        let mut all = orders();
        let renewal = all.pop().unwrap();
        let first = all.remove(0);
        let payments: Vec<Fetched> = [first, renewal]
            .into_iter()
            .filter_map(Order::into_fetched)
            .collect();
        e2e::site(Provider::Shopify, payments, "vid_shop").assert_all_reddit();
    }
}
