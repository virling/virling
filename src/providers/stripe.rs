use std::collections::{BTreeMap, HashMap};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{Fetched, Page};

const API: &str = "https://api.stripe.com/v1";
// Last version where Charge.invoice exists, so one expanded list call yields billing_reason.
const API_VERSION: &str = "2025-02-24.acacia";
const PAGE_SIZE: &str = "100";
const BACKFILL_SECS: i64 = 90 * 86_400;
// Re-read this many seconds below the newest object seen, in case Stripe lists one late.
const OVERLAP_SECS: i64 = 60;
const VID_KEY: &str = "virling_vid";
const EVENT_TYPES: [&str; 6] = [
    "charge.succeeded",
    "charge.failed",
    "charge.captured",
    "charge.refunded",
    "charge.refund.updated",
    "charge.dispute.closed",
];

pub async fn check_key(client: &reqwest::Client, key: &str) -> anyhow::Result<()> {
    if let Some(msg) = key_format_error(key) {
        anyhow::bail!(msg);
    }
    let query = [
        ("limit", "1".into()),
        ("expand[]", "data.payment_intent".into()),
    ];
    let resp = request(client, key, "/charges", &query).await?;
    match resp.status().as_u16() {
        200..=299 => Ok(()),
        401 => anyhow::bail!(
            "Stripe rejected this key. Check it was copied in full and has not been deleted or rolled."
        ),
        403 => anyhow::bail!(
            "This key cannot read Charges or PaymentIntents. Give it read access to Events, Checkout Sessions, Charges, PaymentIntents, Invoices, Subscriptions and Customers."
        ),
        code => anyhow::bail!("Stripe returned an unexpected error ({code}). Try again shortly."),
    }
}

fn key_format_error(key: &str) -> Option<&'static str> {
    if key.starts_with("sk_") {
        Some("Use a restricted read-only key, not a secret key.")
    } else if key.starts_with("rk_live_") || key.starts_with("rk_test_") {
        None
    } else {
        Some(
            "That does not look like a Stripe restricted key. It should start with rk_live_ or rk_test_.",
        )
    }
}

/// Each round sweeps Charges newest-first down to a floor, then Events (refunds, disputes, late settlements) since their own floor, then raises both floors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cursor {
    phase: Phase,
    charges: Sweep,
    events: Sweep,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Charges,
    Events,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Sweep {
    floor: i64,
    after: Option<String>,
    next_floor: Option<i64>,
}

impl Sweep {
    fn new(floor: i64) -> Self {
        Sweep {
            floor,
            after: None,
            next_floor: None,
        }
    }

    /// Records one newest-first page; returns true when the sweep is finished.
    fn advance(&mut self, newest: Option<i64>, last_id: Option<&str>, has_more: bool) -> bool {
        if self.after.is_none() {
            self.next_floor = newest.map(|t| (t - OVERLAP_SECS).max(self.floor));
        }
        match last_id {
            Some(id) if has_more => {
                self.after = Some(id.to_string());
                false
            }
            _ => {
                self.floor = self.next_floor.take().unwrap_or(self.floor);
                self.after = None;
                true
            }
        }
    }
}

pub async fn fetch(
    client: &reqwest::Client,
    key: &str,
    cursor: Option<&str>,
) -> anyhow::Result<Page> {
    let mut cur = match cursor {
        Some(c) => serde_json::from_str(c)?,
        None => {
            let now = OffsetDateTime::now_utc().unix_timestamp();
            Cursor {
                phase: Phase::Charges,
                charges: Sweep::new(now - BACKFILL_SECS),
                events: Sweep::new(now - OVERLAP_SECS),
            }
        }
    };
    let mut payments = Vec::new();
    let more = match cur.phase {
        Phase::Charges => {
            let mut query = vec![("created[gte]", cur.charges.floor.to_string())];
            query.extend(page_query(cur.charges.after.as_deref()));
            query.push(("expand[]", "data.invoice".into()));
            query.push(("expand[]", "data.payment_intent".into()));
            let list: List<Charge> = get(client, key, "/charges", &query).await?;
            for charge in &list.data {
                payments.extend(resolve(client, key, charge, 0).await?);
            }
            let done = cur.charges.advance(
                list.data.first().map(|c| c.created),
                list.data.last().map(|c| c.id.as_str()),
                list.has_more,
            );
            if done {
                cur.phase = Phase::Events;
            }
            true
        }
        Phase::Events => {
            let mut query = vec![("created[gte]", cur.events.floor.to_string())];
            query.extend(page_query(cur.events.after.as_deref()));
            query.extend(EVENT_TYPES.iter().map(|t| ("types[]", t.to_string())));
            let list: List<Event> = get(client, key, "/events", &query).await?;
            for (id, lost) in charges_to_refetch(&list.data, cur.charges.floor) {
                let path = format!("/charges/{id}");
                let expand = [
                    ("expand[]", "invoice".into()),
                    ("expand[]", "payment_intent".into()),
                ];
                let charge: Charge = get(client, key, &path, &expand).await?;
                payments.extend(resolve(client, key, &charge, lost).await?);
            }
            let done = cur.events.advance(
                list.data.first().map(|e| e.created),
                list.data.last().map(|e| e.id.as_str()),
                list.has_more,
            );
            if done {
                cur.phase = Phase::Charges;
            }
            !done
        }
    };
    Ok(Page {
        payments,
        cursor: Some(serde_json::to_string(&cur)?),
        more,
    })
}

fn page_query(after: Option<&str>) -> Vec<(&'static str, String)> {
    let mut query = vec![("limit", PAGE_SIZE.to_string())];
    if let Some(after) = after {
        query.push(("starting_after", after.to_string()));
    }
    query
}

/// Charge ids an event changed, with any lost-dispute amount; charges at or above the floor are left to the next sweep.
fn charges_to_refetch(events: &[Event], charge_floor: i64) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for event in events {
        let obj = &event.data.object;
        let (id, lost) = match (obj.object.as_str(), &obj.charge) {
            ("charge", _) if obj.created.is_some_and(|c| c >= charge_floor) => continue,
            ("charge", _) => (obj.id.clone(), 0),
            ("refund", Some(c)) => (c.clone(), 0),
            ("dispute", Some(c)) if obj.status.as_deref() == Some("lost") => {
                (c.clone(), obj.amount)
            }
            ("dispute", Some(c)) => (c.clone(), 0),
            _ => continue,
        };
        let entry = out.entry(id).or_insert(0);
        *entry = (*entry).max(lost);
    }
    out
}

/// Looks up the Checkout Session only when the charge itself carries no visitor ID.
async fn resolve(
    client: &reqwest::Client,
    key: &str,
    charge: &Charge,
    lost: i64,
) -> anyhow::Result<Option<Fetched>> {
    let Some(fetched) = to_fetched(charge, None, lost) else {
        return Ok(None);
    };
    if non_empty(charge.metadata.get(VID_KEY)).is_some()
        || fetched.is_renewal
        || fetched.status == "failed"
    {
        return Ok(Some(fetched));
    }
    let filter = match &charge.invoice {
        Some(inv) if inv.billing_reason.as_deref() == Some("subscription_create") => {
            inv.subscription.clone().map(|s| ("subscription", s))
        }
        Some(_) => None,
        None => charge
            .payment_intent
            .as_ref()
            .map(|p| ("payment_intent", p.id())),
    };
    let Some(filter) = filter else {
        return Ok(Some(fetched));
    };
    let query = [filter, ("limit", "1".into())];
    let list: List<Session> = get(client, key, "/checkout/sessions", &query).await?;
    Ok(to_fetched(charge, list.data.first(), lost))
}

/// Maps a charge to a payment; None while it is pending or uncaptured.
fn to_fetched(charge: &Charge, session: Option<&Session>, lost: i64) -> Option<Fetched> {
    let status = match charge.status.as_str() {
        "failed" => "failed",
        "succeeded" if charge.captured => "paid",
        _ => return None,
    };
    let refunded = if status == "failed" {
        0
    } else {
        (charge.amount_refunded + lost).min(charge.amount)
    };
    let status = match refunded {
        0 => status,
        r if r >= charge.amount => "refunded",
        _ => "partially_refunded",
    };
    let billing_reason = charge
        .invoice
        .as_ref()
        .and_then(|i| i.billing_reason.as_deref());
    Some(Fetched {
        provider_payment_id: charge.id.clone(),
        ts: OffsetDateTime::from_unix_timestamp(charge.created).ok()?,
        amount: charge.amount,
        currency: charge.currency.to_ascii_lowercase(),
        refunded,
        status: status.to_string(),
        visitor_id: non_empty(charge.metadata.get(VID_KEY))
            .or_else(|| session.and_then(session_visitor))
            .or_else(|| charge.payment_intent.as_ref().and_then(intent_visitor)),
        customer_id: charge.customer.clone(),
        is_renewal: matches!(
            billing_reason,
            Some("subscription_cycle" | "subscription_update")
        ),
    })
}

fn session_visitor(session: &Session) -> Option<String> {
    non_empty(session.metadata.get(VID_KEY))
        .or_else(|| non_empty(session.client_reference_id.as_ref()))
}

fn intent_visitor(intent: &IntentRef) -> Option<String> {
    match intent {
        IntentRef::Expanded(pi) => non_empty(pi.metadata.get(VID_KEY)),
        IntentRef::Id(_) => None,
    }
}

fn non_empty(s: Option<&String>) -> Option<String> {
    s.filter(|s| !s.is_empty()).cloned()
}

async fn request(
    client: &reqwest::Client,
    key: &str,
    path: &str,
    query: &[(&str, String)],
) -> anyhow::Result<reqwest::Response> {
    Ok(client
        .get(format!("{API}{path}"))
        .bearer_auth(key)
        .header("Stripe-Version", API_VERSION)
        .query(query)
        .send()
        .await?)
}

async fn get<T: DeserializeOwned>(
    client: &reqwest::Client,
    key: &str,
    path: &str,
    query: &[(&str, String)],
) -> anyhow::Result<T> {
    let resp = request(client, key, path, query).await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await?;
        let message = serde_json::from_str::<ErrorBody>(&body)
            .ok()
            .and_then(|b| b.error.message)
            .unwrap_or(body);
        anyhow::bail!("Stripe {path} returned {status}: {message}");
    }
    Ok(resp.json().await?)
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct List<T> {
    data: Vec<T>,
    has_more: bool,
}

#[derive(Debug, Deserialize)]
struct Charge {
    id: String,
    amount: i64,
    amount_refunded: i64,
    currency: String,
    created: i64,
    customer: Option<String>,
    status: String,
    captured: bool,
    #[serde(default)]
    metadata: HashMap<String, String>,
    payment_intent: Option<IntentRef>,
    invoice: Option<Invoice>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IntentRef {
    Id(String),
    Expanded(PaymentIntent),
}

impl IntentRef {
    fn id(&self) -> String {
        match self {
            IntentRef::Id(id) => id.clone(),
            IntentRef::Expanded(pi) => pi.id.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct PaymentIntent {
    id: String,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Invoice {
    billing_reason: Option<String>,
    subscription: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Session {
    client_reference_id: Option<String>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Event {
    id: String,
    created: i64,
    data: EventData,
}

#[derive(Debug, Deserialize)]
struct EventData {
    object: EventObject,
}

/// Fields shared by the charge, refund and dispute objects these events carry.
#[derive(Debug, Deserialize)]
struct EventObject {
    id: String,
    object: String,
    created: Option<i64>,
    charge: Option<String>,
    #[serde(default)]
    amount: i64,
    status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, e2e};

    fn charge(id: &str) -> Charge {
        let list: List<Charge> =
            serde_json::from_str(include_str!("fixtures/stripe_charges.json")).unwrap();
        assert!(!list.has_more);
        list.data.into_iter().find(|c| c.id == id).unwrap()
    }

    fn sessions() -> Vec<Session> {
        let list: List<Session> =
            serde_json::from_str(include_str!("fixtures/stripe_checkout_sessions.json")).unwrap();
        list.data
    }

    #[test]
    fn plain_charge() {
        let f = to_fetched(&charge("ch_3MmlLrLkdIwHu7ix0snN0B15"), None, 0).unwrap();
        assert_eq!(f.provider_payment_id, "ch_3MmlLrLkdIwHu7ix0snN0B15");
        assert_eq!(f.ts.unix_timestamp(), 1679090539);
        assert_eq!(f.amount, 1099);
        assert_eq!(f.currency, "usd");
        assert_eq!(f.refunded, 0);
        assert_eq!(f.status, "paid");
        assert_eq!(f.visitor_id, None);
        assert_eq!(f.customer_id, None);
        assert!(!f.is_renewal);
    }

    #[test]
    fn refunded_charges() {
        let full = to_fetched(&charge("ch_3NrefundFull0000000000"), None, 0).unwrap();
        assert_eq!((full.refunded, full.status.as_str()), (2000, "refunded"));
        let part = to_fetched(&charge("ch_3NrefundPart0000000000"), None, 0).unwrap();
        assert_eq!(
            (part.refunded, part.status.as_str()),
            (500, "partially_refunded")
        );
        assert_eq!(part.currency, "eur");
    }

    #[test]
    fn lost_dispute_counts_as_refunded() {
        let f = to_fetched(&charge("ch_3MmlLrLkdIwHu7ix0snN0B15"), None, 1099).unwrap();
        assert_eq!((f.refunded, f.status.as_str()), (1099, "refunded"));
    }

    #[test]
    fn client_reference_id_from_checkout_session() {
        let c = charge("ch_3NcheckoutLink00000000");
        let f = to_fetched(&c, sessions().first(), 0).unwrap();
        assert_eq!(f.visitor_id.as_deref(), Some("v_4f2a9c1e"));
        assert_eq!(f.customer_id.as_deref(), Some("cus_NffrFeUfNV2Hib"));
        assert_eq!(
            c.payment_intent.map(|p| p.id()).as_deref(),
            Some("pi_3NcheckoutLink00000000")
        );
    }

    #[test]
    fn session_metadata_beats_client_reference_id() {
        let s = sessions();
        assert_eq!(session_visitor(&s[1]).as_deref(), Some("v_meta_session"));
    }

    #[test]
    fn metadata_virling_vid_on_charge() {
        let c = charge("ch_3NmetadataVid000000000");
        let f = to_fetched(&c, sessions().first(), 0).unwrap();
        assert_eq!(f.visitor_id.as_deref(), Some("v_from_charge_meta"));
    }

    #[test]
    fn payment_intent_metadata_is_last_resort() {
        let c = charge("ch_3NintentMeta0000000000");
        assert_eq!(
            c.payment_intent.as_ref().unwrap().id(),
            "pi_3NintentMeta0000000000"
        );
        let f = to_fetched(&c, None, 0).unwrap();
        assert_eq!(f.visitor_id.as_deref(), Some("v_from_intent_meta"));
        let f = to_fetched(&c, sessions().first(), 0).unwrap();
        assert_eq!(f.visitor_id.as_deref(), Some("v_4f2a9c1e"));
    }

    #[test]
    fn renewal_invoice() {
        let renewal = to_fetched(&charge("ch_3NrenewalCycle00000000"), None, 0).unwrap();
        assert!(renewal.is_renewal);
        assert_eq!(renewal.customer_id.as_deref(), Some("cus_NffrFeUfNV2Hib"));
        let first = charge("ch_3NsubCreate00000000000");
        assert!(!to_fetched(&first, None, 0).unwrap().is_renewal);
        assert_eq!(
            first.invoice.unwrap().subscription.as_deref(),
            Some("sub_1MowQVLkdIwHu7ixeRlqHVzs")
        );
    }

    fn upgrade() -> Charge {
        let mut c = charge("ch_3NrenewalCycle00000000");
        c.id = "ch_3NsubUpdate00000000000".into();
        c.created -= 30 * 86_400;
        c.invoice.as_mut().unwrap().billing_reason = Some("subscription_update".into());
        c
    }

    #[test]
    fn renewals_and_upgrades_keep_the_first_channel() {
        let first = charge("ch_3NsubCreate00000000000");
        let payments = vec![
            to_fetched(&first, sessions().first(), 0).unwrap(),
            to_fetched(&upgrade(), None, 0).unwrap(),
            to_fetched(&charge("ch_3NrenewalCycle00000000"), None, 0).unwrap(),
        ];
        assert_eq!(payments[0].visitor_id.as_deref(), Some("v_4f2a9c1e"));
        assert!(payments[1..].iter().all(|p| p.visitor_id.is_none()));
        e2e::site(Provider::Stripe, payments, "v_4f2a9c1e").assert_all_reddit();
    }

    #[test]
    fn failed_first_attempt_is_not_the_first_payment() {
        let mut failed = charge("ch_3NsubCreate00000000000");
        failed.id = "ch_3NsubCreateFailed00000".into();
        failed.status = "failed".into();
        failed.created -= 3600;
        let payments = vec![
            to_fetched(&failed, None, 0).unwrap(),
            to_fetched(&charge("ch_3NsubCreate00000000000"), sessions().first(), 0).unwrap(),
            to_fetched(&charge("ch_3NrenewalCycle00000000"), None, 0).unwrap(),
        ];
        let site = e2e::site(Provider::Stripe, payments, "v_4f2a9c1e");
        let renewal = site.touch("ch_3NrenewalCycle00000000");
        assert_eq!(renewal.map(|t| t.session_id).as_deref(), Some("reddit"));
    }

    #[test]
    fn renewal_without_attributed_first_or_customer_is_unattributed() {
        let first = charge("ch_3NsubCreate00000000000");
        let renewal = charge("ch_3NrenewalCycle00000000");
        let payments = vec![
            to_fetched(&first, None, 0).unwrap(),
            to_fetched(&renewal, None, 0).unwrap(),
        ];
        let site = e2e::site(Provider::Stripe, payments, "v_4f2a9c1e");
        assert_eq!(site.touch("ch_3NsubCreate00000000000"), None);
        assert_eq!(site.touch("ch_3NrenewalCycle00000000"), None);

        let mut orphan = charge("ch_3NrenewalCycle00000000");
        orphan.customer = None;
        let payments = vec![
            to_fetched(&first, sessions().first(), 0).unwrap(),
            to_fetched(&orphan, None, 0).unwrap(),
        ];
        let site = e2e::site(Provider::Stripe, payments, "v_4f2a9c1e");
        assert!(site.touch("ch_3NsubCreate00000000000").is_some());
        assert_eq!(site.touch("ch_3NrenewalCycle00000000"), None);
    }

    #[test]
    fn failed_pending_and_uncaptured() {
        let failed = to_fetched(&charge("ch_3Nfailed000000000000000"), None, 0).unwrap();
        assert_eq!((failed.status.as_str(), failed.refunded), ("failed", 0));
        assert!(to_fetched(&charge("ch_3Npending00000000000000"), None, 0).is_none());
        assert!(to_fetched(&charge("ch_3Nuncaptured0000000000"), None, 0).is_none());
    }

    #[test]
    fn events_pick_changed_charges() {
        let list: List<Event> =
            serde_json::from_str(include_str!("fixtures/stripe_events.json")).unwrap();
        let ids = charges_to_refetch(&list.data, 1_690_000_000);
        let expected: BTreeMap<String, i64> = [
            ("ch_1AZtxr2eZvKYlo2CJDX8whov".to_string(), 1000),
            ("ch_3NrefundFull0000000000".to_string(), 0),
            ("ch_3NwonDispute0000000000".to_string(), 0),
        ]
        .into();
        assert_eq!(ids, expected);
    }

    #[test]
    fn sweep_pages_then_raises_floor() {
        let mut s = Sweep::new(1000);
        assert!(!s.advance(Some(5000), Some("ch_b"), true));
        assert_eq!(s.after.as_deref(), Some("ch_b"));
        assert!(s.advance(Some(3000), Some("ch_a"), false));
        assert_eq!(s, Sweep::new(5000 - OVERLAP_SECS));
        assert!(s.advance(None, None, false));
        assert_eq!(s.floor, 5000 - OVERLAP_SECS);
    }

    #[test]
    fn cursor_round_trips() {
        let c = Cursor {
            phase: Phase::Events,
            charges: Sweep::new(1),
            events: Sweep {
                floor: 2,
                after: Some("evt_1".into()),
                next_floor: Some(3),
            },
        };
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Cursor>(&s).unwrap(), c);
    }

    #[test]
    fn rejects_wrong_keys() {
        assert_eq!(
            key_format_error("sk_live_abc"),
            Some("Use a restricted read-only key, not a secret key.")
        );
        assert!(key_format_error("pk_live_abc").is_some());
        assert!(key_format_error("rk_live_abc").is_none());
        assert!(key_format_error("rk_test_abc").is_none());
    }
}
