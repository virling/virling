use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use time::{Duration, OffsetDateTime};

use super::{Fetched, Page, Subscription};
use crate::events::TRIAL;

const API: &str = "https://api.revenuecat.com/v2";
const BACKFILL: Duration = Duration::days(90);
/// Customers read per sync round while backfilling, two requests each, well under the 480 a minute limit.
const CUSTOMERS_PER_ROUND: u32 = 40;
/// Events whose payloads carry a transaction and the customer's attributes, and with them the visitor ID.
const WEBHOOK_EVENTS: [&str; 4] = [
    "initial_purchase",
    "renewal",
    "non_renewing_purchase",
    "cancellation",
];

/// Backfill state: the project, and the last customer read; done once every customer was read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cursor {
    project: String,
    after: Option<String>,
    done: bool,
    /// Unix seconds; purchases before this are not imported.
    floor: i64,
}

#[derive(Debug, Deserialize)]
struct List<T> {
    items: Vec<T>,
    next_page: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Project {
    id: String,
}

#[derive(Debug, Deserialize)]
struct Customer {
    id: String,
}

#[derive(Debug, Deserialize)]
struct Purchase {
    purchased_at: i64,
    revenue_in_usd: Option<Revenue>,
    status: String,
    store_purchase_identifier: Option<Value>,
    customer_id: String,
}

#[derive(Debug, Deserialize)]
struct Revenue {
    gross: f64,
}

#[derive(Debug, Deserialize)]
struct Attribute {
    name: String,
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Hook {
    event: Event,
}

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    transaction_id: Option<String>,
    original_app_user_id: Option<String>,
    app_user_id: Option<String>,
    price_in_purchased_currency: Option<f64>,
    currency: Option<String>,
    purchased_at_ms: Option<i64>,
    period_type: Option<String>,
    cancel_reason: Option<String>,
    #[serde(default)]
    subscriber_attributes: Map<String, Value>,
}

pub async fn check_key(client: &reqwest::Client, key: &str) -> anyhow::Result<()> {
    let project = project(client, key).await?;
    let _: List<Customer> = get(
        client,
        key,
        &format!("/projects/{project}/customers"),
        &[("limit", "1")],
    )
    .await?;
    Ok(())
}

/// Reads one-time purchases of the last 90 days customer by customer, once; new payments arrive by webhook.
pub async fn fetch(
    client: &reqwest::Client,
    key: &str,
    cursor: Option<&str>,
) -> anyhow::Result<Page> {
    let mut cur = match cursor {
        Some(c) => serde_json::from_str(c)?,
        None => Cursor {
            project: project(client, key).await?,
            after: None,
            done: false,
            floor: (OffsetDateTime::now_utc() - BACKFILL).unix_timestamp(),
        },
    };
    let mut payments = Vec::new();
    if !cur.done {
        let limit = CUSTOMERS_PER_ROUND.to_string();
        let mut query = vec![("limit", limit.as_str())];
        if let Some(after) = &cur.after {
            query.push(("starting_after", after));
        }
        let customers: List<Customer> = get(
            client,
            key,
            &format!("/projects/{}/customers", cur.project),
            &query,
        )
        .await?;
        for c in &customers.items {
            let path = format!("/projects/{}/customers/{}", cur.project, c.id);
            let purchases: List<Purchase> = get(
                client,
                key,
                &format!("{path}/purchases"),
                &[("limit", "100")],
            )
            .await?;
            if purchases.items.is_empty() {
                continue;
            }
            let attributes: List<Attribute> = get(
                client,
                key,
                &format!("{path}/attributes"),
                &[("limit", "100")],
            )
            .await?;
            let vid = super::VID_KEYS.iter().find_map(|k| {
                attributes
                    .items
                    .iter()
                    .find(|a| a.name == *k)
                    .and_then(|a| a.value.clone())
                    .filter(|v| !v.trim().is_empty())
            });
            payments.extend(
                purchases
                    .items
                    .into_iter()
                    .filter_map(|p| map_purchase(p, cur.floor, vid.clone())),
            );
        }
        cur.after = customers.items.last().map(|c| c.id.clone());
        cur.done = customers.next_page.is_none();
    }
    Ok(Page {
        payments,
        cursor: Some(serde_json::to_string(&cur)?),
        more: false,
    })
}

/// RevenueCat reports subscriptions per customer only, so retention uses payment recency.
pub async fn subscriptions(
    _client: &reqwest::Client,
    _key: &str,
) -> anyhow::Result<Option<Vec<Subscription>>> {
    Ok(None)
}

/// A one-time purchase as a payment in USD, keyed by its store transaction like webhook events.
fn map_purchase(p: Purchase, floor: i64, visitor_id: Option<String>) -> Option<Fetched> {
    let ts = millis(p.purchased_at);
    let id = match p.store_purchase_identifier? {
        Value::String(s) => s,
        v => v.to_string(),
    };
    let gross = p.revenue_in_usd?.gross;
    if ts.unix_timestamp() < floor || gross <= 0.0 {
        return None;
    }
    let amount = (gross * 100.0).round() as i64;
    let refunded = p.status == "refunded";
    Some(Fetched {
        provider_payment_id: id,
        ts,
        amount,
        currency: "usd".into(),
        refunded: if refunded { amount } else { 0 },
        status: if refunded { "refunded" } else { "paid" }.into(),
        visitor_id,
        customer_id: Some(p.customer_id),
        is_renewal: false,
    })
}

/// Adds a webhook for purchases, renewals and refunds that sends `secret` as its Authorization header.
pub async fn register_webhook(
    client: &reqwest::Client,
    key: &str,
    url: &str,
    secret: &str,
) -> anyhow::Result<()> {
    let project = project(client, key).await?;
    let resp = client
        .post(format!("{API}/projects/{project}/integrations/webhooks"))
        .bearer_auth(key)
        .json(&json!({
            "name": "Virling",
            "url": url,
            "authorization_header": secret,
            "event_types": WEBHOOK_EVENTS,
        }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "RevenueCat did not accept the webhook (HTTP {}). Give the key read and write access to Integrations; Virling also needs a public https address.",
            resp.status()
        );
    }
    Ok(())
}

/// The payment, renewal or refund a webhook reports, with the visitor ID from the customer's attributes.
pub fn webhook_payment(body: &[u8]) -> anyhow::Result<Option<Fetched>> {
    let e = serde_json::from_slice::<Hook>(body)?.event;
    let renewal = match e.kind.as_str() {
        "INITIAL_PURCHASE" | "NON_RENEWING_PURCHASE" => false,
        "RENEWAL" => true,
        "CANCELLATION" => false,
        _ => return Ok(None),
    };
    let (Some(id), Some(price), Some(currency), Some(at)) = (
        e.transaction_id,
        e.price_in_purchased_currency,
        e.currency,
        e.purchased_at_ms,
    ) else {
        return Ok(None);
    };
    let amount = crate::fx::major_to_minor(price.abs(), &currency);
    // A refund is a cancellation by customer support, or one with a negative price.
    let refund = e.kind == "CANCELLATION";
    if refund && price >= 0.0 && e.cancel_reason.as_deref() != Some("CUSTOMER_SUPPORT") {
        return Ok(None);
    }
    let status = if refund {
        "refunded"
    } else if amount == 0 && e.period_type.as_deref() == Some("TRIAL") {
        TRIAL
    } else if amount == 0 {
        return Ok(None);
    } else {
        "paid"
    };
    let visitor_id = super::VID_KEYS.iter().find_map(|k| {
        match e.subscriber_attributes.get(*k)?.get("value")? {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        }
    });
    Ok(Some(Fetched {
        provider_payment_id: id,
        ts: millis(at),
        amount,
        currency: currency.to_ascii_lowercase(),
        refunded: if refund { amount } else { 0 },
        status: status.into(),
        visitor_id,
        customer_id: e.original_app_user_id.or(e.app_user_id),
        is_renewal: renewal,
    }))
}

fn millis(ms: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

/// The project the key belongs to; a v2 secret key covers one.
async fn project(client: &reqwest::Client, key: &str) -> anyhow::Result<String> {
    let projects: List<Project> = get(client, key, "/projects", &[("limit", "1")]).await?;
    projects
        .items
        .into_iter()
        .next()
        .map(|p| p.id)
        .ok_or_else(|| anyhow::anyhow!("This RevenueCat key has no project."))
}

async fn get<T: DeserializeOwned>(
    client: &reqwest::Client,
    key: &str,
    path: &str,
    query: &[(&str, &str)],
) -> anyhow::Result<T> {
    let resp = client
        .get(format!("{API}{path}"))
        .bearer_auth(key)
        .query(query)
        .send()
        .await?;
    match resp.status() {
        s if s.is_success() => Ok(resp.json().await?),
        StatusCode::UNAUTHORIZED => anyhow::bail!(
            "RevenueCat did not accept this key. Paste a V2 secret API key from your project settings."
        ),
        StatusCode::FORBIDDEN => anyhow::bail!(
            "This RevenueCat key is missing a permission. Give it read access to Projects and to Customer information (customers and purchases), plus read and write access to Integrations."
        ),
        StatusCode::TOO_MANY_REQUESTS => {
            anyhow::bail!("RevenueCat rate limit reached, sync will retry shortly.")
        }
        s => anyhow::bail!("RevenueCat returned HTTP {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, e2e};

    const HOOK: &str = include_str!("fixtures/revenuecat_webhook.json");

    fn event(edit: impl FnOnce(&mut Value)) -> Option<Fetched> {
        let mut v: Value = serde_json::from_str(HOOK).unwrap();
        edit(&mut v["event"]);
        webhook_payment(v.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn initial_purchase_carries_the_visitor() {
        let p = event(|_| {}).unwrap();
        assert_eq!(p.provider_payment_id, "170000123456789");
        assert_eq!((p.amount, p.currency.as_str()), (999, "eur"));
        assert_eq!(p.visitor_id.as_deref(), Some("vid_rc"));
        assert_eq!(p.customer_id.as_deref(), Some("$RCAnonymousID:abc"));
        assert_eq!((p.status.as_str(), p.is_renewal), ("paid", false));
        let alias = event(|e| {
            e["subscriber_attributes"] =
                json!({ "datafast_visitor_id": { "value": "vid_alias", "updated_at_ms": 1 } })
        })
        .unwrap();
        assert_eq!(alias.visitor_id.as_deref(), Some("vid_alias"));
    }

    #[test]
    fn renewals_trials_refunds_and_other_events() {
        let renewal = event(|e| e["type"] = "RENEWAL".into()).unwrap();
        assert!(renewal.is_renewal);
        let trial = event(|e| {
            e["price_in_purchased_currency"] = 0.into();
            e["period_type"] = "TRIAL".into();
        })
        .unwrap();
        assert_eq!(trial.status, TRIAL);
        let refund = event(|e| {
            e["type"] = "CANCELLATION".into();
            e["price_in_purchased_currency"] = (-9.99).into();
        })
        .unwrap();
        assert_eq!((refund.status.as_str(), refund.refunded), ("refunded", 999));
        let unsubscribed = event(|e| {
            e["type"] = "CANCELLATION".into();
            e["cancel_reason"] = "UNSUBSCRIBE".into();
        });
        assert!(
            unsubscribed.is_none(),
            "turning off renewal is not a refund"
        );
        assert!(event(|e| e["type"] = "EXPIRATION".into()).is_none());
    }

    #[test]
    fn renewals_keep_the_first_channel() {
        let first = event(|_| {}).unwrap();
        let mut renewal = event(|e| {
            e["type"] = "RENEWAL".into();
            e["transaction_id"] = "170000123456790".into();
            e["purchased_at_ms"] = 1_790_000_000_000_i64.into();
            e["subscriber_attributes"] = json!({});
        })
        .unwrap();
        renewal.currency = "usd".into();
        e2e::site(Provider::RevenueCat, vec![first, renewal], "vid_rc").assert_all_reddit();
    }

    #[test]
    fn backfill_maps_one_time_purchases() {
        let list: List<Purchase> =
            serde_json::from_str(include_str!("fixtures/revenuecat_purchases.json")).unwrap();
        let floor = 1_780_000_000;
        let got: Vec<Fetched> = list
            .items
            .into_iter()
            .filter_map(|p| map_purchase(p, floor, Some("vid_rc".into())))
            .collect();
        assert_eq!(got.len(), 2, "old and free purchases are skipped");
        assert_eq!(
            (
                got[0].provider_payment_id.as_str(),
                got[0].amount,
                got[0].status.as_str()
            ),
            ("GPA.1234", 999, "paid")
        );
        assert_eq!((got[1].status.as_str(), got[1].refunded), ("refunded", 499));
        assert_eq!(got[1].provider_payment_id, "12345678");
    }

    #[test]
    fn cursor_round_trips() {
        let cur = Cursor {
            project: "proj1".into(),
            after: Some("c1".into()),
            done: false,
            floor: 1,
        };
        let json = serde_json::to_string(&cur).unwrap();
        assert_eq!(serde_json::from_str::<Cursor>(&json).unwrap(), cur);
    }
}
