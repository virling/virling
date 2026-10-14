use std::collections::HashMap;

use anyhow::{Context, anyhow, bail};
use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use super::{Fetched, Page};

const LIVE_URL: &str = "https://live.dodopayments.com";
const TEST_URL: &str = "https://test.dodopayments.com";
const BACKFILL: Duration = Duration::days(90);
/// Longest created_at window per listing, so offset pagination never goes deep.
const WINDOW: Duration = Duration::days(1);
/// Each incremental window re-reads this much so payments that succeed late are not missed.
const OVERLAP: Duration = Duration::minutes(30);
const CAUGHT_UP: Duration = Duration::minutes(5);
const PAGE_SIZE: usize = 20;
const MAX_PAGE: u32 = 1000;
/// Extra GETs per fetch before ending the round, keeping a round under Dodo's 240 requests/minute.
const LOOKUP_BUDGET: usize = 2;
/// A subscription payment made this long after the subscription started is a renewal.
const RENEWAL_GAP: Duration = Duration::hours(12);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Live,
    Test,
}

impl Mode {
    fn base(self) -> &'static str {
        match self {
            Mode::Live => LIVE_URL,
            Mode::Test => TEST_URL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Payments,
    Refunds,
}

/// A created_at window being paged through; lte 0 means it is fixed on first use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Window {
    gte: i64,
    lte: i64,
    page: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cursor {
    mode: Mode,
    phase: Phase,
    payments: Window,
    refunds: Window,
}

impl Cursor {
    fn start(mode: Mode, now: OffsetDateTime) -> Self {
        let w = Window {
            gte: (now - BACKFILL).unix_timestamp(),
            lte: 0,
            page: 0,
        };
        Cursor {
            mode,
            phase: Phase::Payments,
            payments: w,
            refunds: w,
        }
    }

    fn encode(&self) -> String {
        let mode = match self.mode {
            Mode::Live => "live",
            Mode::Test => "test",
        };
        let phase = match self.phase {
            Phase::Payments => "p",
            Phase::Refunds => "r",
        };
        let (p, r) = (self.payments, self.refunds);
        format!(
            "v1:{mode}:{phase}:{},{},{}:{},{},{}",
            p.gte, p.lte, p.page, r.gte, r.lte, r.page
        )
    }

    fn decode(s: &str) -> anyhow::Result<Self> {
        let bad = || anyhow!("invalid Dodo sync cursor {s:?}");
        let parts: Vec<&str> = s.split(':').collect();
        let ["v1", mode, phase, p, r] = parts[..] else {
            return Err(bad());
        };
        let window = |w: &str| -> anyhow::Result<Window> {
            let n: Vec<&str> = w.split(',').collect();
            let [gte, lte, page] = n[..] else {
                return Err(bad());
            };
            Ok(Window {
                gte: gte.parse().map_err(|_| bad())?,
                lte: lte.parse().map_err(|_| bad())?,
                page: page.parse().map_err(|_| bad())?,
            })
        };
        Ok(Cursor {
            mode: match mode {
                "live" => Mode::Live,
                "test" => Mode::Test,
                _ => return Err(bad()),
            },
            phase: match phase {
                "p" => Phase::Payments,
                "r" => Phase::Refunds,
                _ => return Err(bad()),
            },
            payments: window(p)?,
            refunds: window(r)?,
        })
    }

    fn window_mut(&mut self) -> &mut Window {
        match self.phase {
            Phase::Payments => &mut self.payments,
            Phase::Refunds => &mut self.refunds,
        }
    }

    /// Fixes the open end of the current window and returns it.
    fn window(&mut self, now: OffsetDateTime) -> Window {
        let w = self.window_mut();
        if w.lte == 0 {
            w.lte = (w.gte + WINDOW.whole_seconds()).min(now.unix_timestamp());
        }
        *w
    }

    /// Moves past the page just read and reports whether more pages are available now.
    fn advance(&mut self, full_page: bool, now: OffsetDateTime) -> bool {
        let w = self.window_mut();
        if full_page {
            w.page += 1;
            return true;
        }
        let caught_up = w.lte >= (now - CAUGHT_UP).unix_timestamp();
        w.gte = if caught_up {
            w.lte - OVERLAP.whole_seconds()
        } else {
            w.lte
        };
        w.lte = 0;
        w.page = 0;
        if !caught_up {
            return true;
        }
        match self.phase {
            Phase::Payments => {
                self.phase = Phase::Refunds;
                true
            }
            Phase::Refunds => {
                self.phase = Phase::Payments;
                false
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct List<T> {
    items: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct Customer {
    customer_id: String,
}

/// A payment from the list endpoint, or the detail endpoint which adds refunds and disputes.
#[derive(Debug, Deserialize)]
struct DodoPayment {
    payment_id: String,
    total_amount: i64,
    currency: String,
    customer: Customer,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
    status: Option<String>,
    refund_status: Option<String>,
    dispute_status: Option<String>,
    subscription_id: Option<String>,
    #[serde(default)]
    refunds: Vec<DodoRefund>,
    #[serde(default)]
    disputes: Vec<DodoDispute>,
}

#[derive(Debug, Deserialize)]
struct DodoRefund {
    payment_id: String,
    amount: Option<i64>,
    status: String,
}

#[derive(Debug, Deserialize)]
struct DodoDispute {
    dispute_status: String,
}

#[derive(Debug, Deserialize)]
struct DodoSubscription {
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

/// Lost or accepted disputes mean the money went back to the customer.
fn dispute_lost(status: &str) -> bool {
    matches!(status, "dispute_lost" | "dispute_accepted")
}

impl DodoPayment {
    /// Refunded amount from list fields alone, or None when only the detail endpoint knows it.
    fn refunded_from_list(&self) -> Option<i64> {
        if self.dispute_status.as_deref().is_some_and(dispute_lost)
            || self.refund_status.as_deref() == Some("full")
        {
            return Some(self.total_amount);
        }
        match self.refund_status {
            None => Some(0),
            Some(_) => None,
        }
    }

    fn refunded_from_detail(&self) -> i64 {
        if self
            .disputes
            .iter()
            .any(|d| dispute_lost(&d.dispute_status))
            || self.refund_status.as_deref() == Some("full")
        {
            return self.total_amount;
        }
        let sum: i64 = self
            .refunds
            .iter()
            .filter(|r| r.status == "succeeded")
            .filter_map(|r| r.amount)
            .sum();
        sum.min(self.total_amount)
    }

    fn to_fetched(&self, refunded: i64, is_renewal: bool) -> Fetched {
        let status = if refunded >= self.total_amount {
            "refunded"
        } else if refunded > 0 {
            "partially_refunded"
        } else {
            "paid"
        };
        Fetched {
            provider_payment_id: self.payment_id.clone(),
            ts: self.created_at,
            amount: self.total_amount,
            currency: self.currency.clone(),
            refunded,
            status: status.to_string(),
            visitor_id: self
                .metadata
                .get("virling_vid")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(str::to_string),
            customer_id: Some(self.customer.customer_id.clone()),
            is_renewal,
        }
    }
}

fn is_renewal(payment: OffsetDateTime, subscription_created: OffsetDateTime) -> bool {
    payment - subscription_created > RENEWAL_GAP
}

/// Distinct payments touched by a page of refunds, in first-seen order.
fn refunded_payment_ids(refunds: &[DodoRefund]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for r in refunds {
        if !ids.contains(&r.payment_id) {
            ids.push(r.payment_id.clone());
        }
    }
    ids
}

fn rfc3339(unix: i64) -> anyhow::Result<String> {
    Ok(OffsetDateTime::from_unix_timestamp(unix)?.format(&Rfc3339)?)
}

fn error_message(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED => "Dodo Payments rejected this API key. Copy a fresh key from Developer > API Keys in your Dodo dashboard.".into(),
        StatusCode::FORBIDDEN => "This Dodo Payments API key is not allowed to read payments. Create a key with read access.".into(),
        StatusCode::TOO_MANY_REQUESTS => "Dodo Payments is rate limiting requests. Sync will retry in a minute.".into(),
        s => format!("Dodo Payments returned HTTP {}", s.as_u16()),
    }
}

async fn request(
    client: &reqwest::Client,
    key: &str,
    base: &str,
    path: &str,
    query: &[(&str, String)],
) -> anyhow::Result<reqwest::Response> {
    client
        .get(format!("{base}{path}"))
        .bearer_auth(key)
        .query(query)
        .send()
        .await
        .context("could not reach Dodo Payments")
}

struct Api<'a> {
    client: &'a reqwest::Client,
    key: &'a str,
    base: &'static str,
    lookups: usize,
    subscriptions: HashMap<String, OffsetDateTime>,
}

impl Api<'_> {
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> anyhow::Result<T> {
        let res = request(self.client, self.key, self.base, path, query).await?;
        if !res.status().is_success() {
            bail!("{}", error_message(res.status()));
        }
        res.json()
            .await
            .with_context(|| format!("unexpected Dodo Payments response from {path}"))
    }

    async fn list<T: DeserializeOwned>(&self, path: &str, w: Window) -> anyhow::Result<Vec<T>> {
        let list: List<T> = self
            .get(
                path,
                &[
                    ("status", "succeeded".to_string()),
                    ("created_at_gte", rfc3339(w.gte)?),
                    ("created_at_lte", rfc3339(w.lte)?),
                    ("page_size", PAGE_SIZE.to_string()),
                    ("page_number", w.page.to_string()),
                ],
            )
            .await?;
        Ok(list.items)
    }

    async fn payment(&mut self, id: &str) -> anyhow::Result<DodoPayment> {
        self.lookups += 1;
        self.get(&format!("/payments/{id}"), &[]).await
    }

    async fn renewal(&mut self, p: &DodoPayment) -> anyhow::Result<bool> {
        let Some(sub) = &p.subscription_id else {
            return Ok(false);
        };
        let created = match self.subscriptions.get(sub) {
            Some(t) => *t,
            None => {
                self.lookups += 1;
                let s: DodoSubscription = self.get(&format!("/subscriptions/{sub}"), &[]).await?;
                self.subscriptions.insert(sub.clone(), s.created_at);
                s.created_at
            }
        };
        Ok(is_renewal(p.created_at, created))
    }

    async fn payments_page(&mut self, w: Window) -> anyhow::Result<(usize, Vec<Fetched>)> {
        let items: Vec<DodoPayment> = self.list("/payments", w).await?;
        let mut out = Vec::with_capacity(items.len());
        for item in &items {
            if item.total_amount <= 0 {
                continue;
            }
            let renewal = self.renewal(item).await?;
            let refunded = match item.refunded_from_list() {
                Some(r) => r,
                None => self.payment(&item.payment_id).await?.refunded_from_detail(),
            };
            out.push(item.to_fetched(refunded, renewal));
        }
        Ok((items.len(), out))
    }

    async fn refunds_page(&mut self, w: Window) -> anyhow::Result<(usize, Vec<Fetched>)> {
        let items: Vec<DodoRefund> = self.list("/refunds", w).await?;
        let mut out = Vec::new();
        for id in refunded_payment_ids(&items) {
            let p = self.payment(&id).await?;
            if p.status.as_deref() != Some("succeeded") || p.total_amount <= 0 {
                continue;
            }
            let renewal = self.renewal(&p).await?;
            out.push(p.to_fetched(p.refunded_from_detail(), renewal));
        }
        Ok((items.len(), out))
    }
}

/// Finds whether the key belongs to live or test mode; each only works against its own host.
async fn detect_mode(client: &reqwest::Client, key: &str) -> anyhow::Result<Mode> {
    let probe = [("page_size", "1".to_string())];
    let mut worst = StatusCode::UNAUTHORIZED;
    for mode in [Mode::Live, Mode::Test] {
        let status = request(client, key, mode.base(), "/payments", &probe)
            .await?
            .status();
        if status.is_success() {
            return Ok(mode);
        }
        match status {
            StatusCode::UNAUTHORIZED => {}
            StatusCode::FORBIDDEN => worst = status,
            s => bail!("{}", error_message(s)),
        }
    }
    bail!("{}", error_message(worst))
}

pub async fn check_key(client: &reqwest::Client, key: &str) -> anyhow::Result<()> {
    detect_mode(client, key.trim()).await.map(|_| ())
}

pub async fn fetch(
    client: &reqwest::Client,
    key: &str,
    cursor: Option<&str>,
) -> anyhow::Result<Page> {
    let key = key.trim();
    let now = OffsetDateTime::now_utc();
    let mut cursor = match cursor {
        Some(c) => Cursor::decode(c)?,
        None => Cursor::start(detect_mode(client, key).await?, now),
    };
    let w = cursor.window(now);
    if w.page > MAX_PAGE {
        bail!("Dodo Payments returned more than {MAX_PAGE} pages for one day");
    }
    let mut api = Api {
        client,
        key,
        base: cursor.mode.base(),
        lookups: 0,
        subscriptions: HashMap::new(),
    };
    let (n, payments) = match cursor.phase {
        Phase::Payments => api.payments_page(w).await?,
        Phase::Refunds => api.refunds_page(w).await?,
    };
    let more = cursor.advance(n >= PAGE_SIZE, now) && api.lookups <= LOOKUP_BUDGET;
    Ok(Page {
        payments,
        cursor: Some(cursor.encode()),
        more,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn payments() -> Vec<DodoPayment> {
        serde_json::from_str::<List<DodoPayment>>(include_str!("fixtures/dodo_payments.json"))
            .unwrap()
            .items
    }

    #[test]
    fn plain_payment_with_visitor_id() {
        let p = &payments()[0];
        let f = p.to_fetched(p.refunded_from_list().unwrap(), false);
        assert_eq!(
            f,
            Fetched {
                provider_payment_id: "pay_gr4RizvMOXFJ6xca3y2tU".into(),
                ts: datetime!(2026-09-01 10:30 UTC),
                amount: 2999,
                currency: "USD".into(),
                refunded: 0,
                status: "paid".into(),
                visitor_id: Some("v_3f9a1c".into()),
                customer_id: Some("cus_Gq9lbBQ4pZ7c".into()),
                is_renewal: false,
            }
        );
    }

    #[test]
    fn fully_refunded_from_list() {
        let p = &payments()[1];
        let f = p.to_fetched(p.refunded_from_list().unwrap(), false);
        assert_eq!(f.refunded, 4900);
        assert_eq!(f.status, "refunded");
        assert_eq!(f.currency, "EUR");
        assert_eq!(f.visitor_id, None);
    }

    #[test]
    fn partial_refund_needs_detail() {
        assert_eq!(payments()[2].refunded_from_list(), None);
        let p: DodoPayment =
            serde_json::from_str(include_str!("fixtures/dodo_payment_partial.json")).unwrap();
        let f = p.to_fetched(p.refunded_from_detail(), false);
        assert_eq!(f.refunded, 2500);
        assert_eq!(f.status, "partially_refunded");
        assert_eq!(f.visitor_id.as_deref(), Some("v_3f9a1c"));
    }

    #[test]
    fn lost_dispute_counts_as_refunded() {
        let mut p = payments().remove(3);
        assert_eq!(p.refunded_from_list(), Some(0));
        p.dispute_status = Some("dispute_lost".into());
        assert_eq!(p.refunded_from_list(), Some(1500));
        p.disputes = vec![DodoDispute {
            dispute_status: "dispute_accepted".into(),
        }];
        assert_eq!(p.refunded_from_detail(), 1500);
    }

    #[test]
    fn subscription_renewal() {
        let p = &payments()[3];
        let sub: DodoSubscription =
            serde_json::from_str(include_str!("fixtures/dodo_subscription.json")).unwrap();
        assert_eq!(p.subscription_id.as_deref(), Some("sub_Hq3nK8wZ2xV5bM"));
        assert!(is_renewal(p.created_at, sub.created_at));
        assert!(!is_renewal(
            sub.created_at + Duration::minutes(3),
            sub.created_at
        ));
        let f = p.to_fetched(0, true);
        assert!(f.is_renewal);
        assert_eq!(f.customer_id.as_deref(), Some("cus_Wn4cTb6Jk1Ps"));
    }

    #[test]
    fn refunds_page_dedupes_payments() {
        let r: List<DodoRefund> =
            serde_json::from_str(include_str!("fixtures/dodo_refunds.json")).unwrap();
        assert_eq!(
            refunded_payment_ids(&r.items),
            ["pay_Pt5sR9vLmQ2wXy7kZ3aB4", "pay_7Hj2KdQwXz1mNbV5cR8tY"]
        );
    }

    #[test]
    fn cursor_round_trips() {
        let mut c = Cursor::start(Mode::Test, datetime!(2026-10-14 00:00 UTC));
        c.phase = Phase::Refunds;
        c.refunds.page = 3;
        assert_eq!(Cursor::decode(&c.encode()).unwrap(), c);
        assert!(Cursor::decode("garbage").is_err());
    }

    #[test]
    fn backfills_then_polls_both_streams() {
        let now = datetime!(2026-10-14 00:00 UTC);
        let ts = now.unix_timestamp();
        let mut c = Cursor::start(Mode::Live, now);
        let start = c.payments.gte;
        assert_eq!(c.window(now).lte, start + 86_400);
        assert!(c.advance(true, now));
        assert_eq!(c.payments.page, 1);
        assert!(c.advance(false, now));
        assert_eq!(c.payments.gte, start + 86_400);
        for _ in 0..89 {
            assert_eq!(c.phase, Phase::Payments);
            c.window(now);
            assert!(c.advance(false, now));
        }
        assert_eq!(c.phase, Phase::Refunds);
        assert_eq!(c.payments.gte, ts - 1800);
        c.refunds.gte = ts - 600;
        assert_eq!(c.window(now).lte, ts);
        assert!(!c.advance(false, now));
        assert_eq!(c.phase, Phase::Payments);
        assert_eq!(c.refunds.gte, ts - 1800);
    }

    #[test]
    fn auth_errors_are_readable() {
        assert!(error_message(StatusCode::UNAUTHORIZED).contains("rejected this API key"));
        assert!(error_message(StatusCode::FORBIDDEN).contains("not allowed to read"));
    }
}
