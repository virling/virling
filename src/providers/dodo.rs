use std::collections::HashMap;

use anyhow::{Context, anyhow, bail};
use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use super::{Fetched, MAX_SUBSCRIPTION_PAGES, Page, Subscription};
use crate::events::TRIAL;

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
const SUBSCRIPTION_PAGE: usize = 100;

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
struct DodoSubscriptionRow {
    subscription_id: String,
    status: String,
    customer: Customer,
    recurring_pre_tax_amount: Option<i64>,
    currency: Option<String>,
    payment_frequency_interval: Option<String>,
    payment_frequency_count: Option<i64>,
}

impl DodoSubscriptionRow {
    /// Active is live, and so is on_hold while Dodo waits for a new payment method after a failed renewal.
    fn into_subscription(self) -> Subscription {
        Subscription {
            live: matches!(self.status.as_str(), "active" | "on_hold"),
            subscription_id: self.subscription_id,
            customer_id: self.customer.customer_id,
            status: self.status,
            ends_at: None,
            amount: self.recurring_pre_tax_amount,
            currency: self.currency,
            interval: self
                .payment_frequency_interval
                .map(|i| i.to_ascii_lowercase()),
            interval_count: self.payment_frequency_count.unwrap_or(1),
            mrr_usd: None,
        }
    }
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
            visitor_id: super::VID_KEYS
                .iter()
                .filter_map(|k| self.metadata.get(*k)?.as_str())
                .find(|v| !v.is_empty())
                .map(str::to_string),
            customer_id: Some(self.customer.customer_id.clone()),
            is_renewal,
        }
    }
}

/// The earliest paid payment of a subscription, by time then id; a free trial's $0 start does not count.
fn first_paid(payments: &[DodoPayment]) -> Option<(OffsetDateTime, String)> {
    payments
        .iter()
        .filter(|p| p.total_amount > 0)
        .map(|p| (p.created_at, p.payment_id.clone()))
        .min()
}

/// A subscription payment is a renewal when the subscription was paid before it.
fn is_renewal(p: &DodoPayment, first: Option<&(OffsetDateTime, String)>) -> bool {
    first.is_some_and(|(at, id)| (p.created_at, &p.payment_id) > (*at, id))
}

/// A subscription's $0 payment, such as a free trial start, kept only to credit the subscription's later payments.
fn trial_start(p: &DodoPayment) -> Option<Fetched> {
    (p.total_amount == 0 && p.subscription_id.is_some()).then(|| Fetched {
        status: TRIAL.into(),
        ..p.to_fetched(0, false)
    })
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

fn list_query(w: Window) -> anyhow::Result<[(&'static str, String); 5]> {
    Ok([
        ("status", "succeeded".to_string()),
        ("created_at_gte", rfc3339(w.gte)?),
        ("created_at_lte", rfc3339(w.lte)?),
        ("page_size", PAGE_SIZE.to_string()),
        ("page_number", w.page.to_string()),
    ])
}

/// Any non-2xx or non-JSON body is an error, so sync records it instead of seeing no payments.
fn parse<T: DeserializeOwned>(path: &str, status: StatusCode, body: &str) -> anyhow::Result<T> {
    if !status.is_success() {
        let detail = body.trim();
        if detail.is_empty() || matches!(status.as_u16(), 401 | 403 | 429) {
            bail!("{}", error_message(status));
        }
        let detail: String = detail.chars().take(200).collect();
        bail!("{} on {path}: {detail}", error_message(status));
    }
    serde_json::from_str(body).map_err(|e| {
        anyhow!(
            "Dodo Payments returned HTTP {} with an unreadable response on {path}: {e}",
            status.as_u16()
        )
    })
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
    first_paid: HashMap<String, Option<(OffsetDateTime, String)>>,
}

impl Api<'_> {
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> anyhow::Result<T> {
        let res = request(self.client, self.key, self.base, path, query).await?;
        let status = res.status();
        let body = res
            .text()
            .await
            .context("could not read Dodo Payments response")?;
        parse(path, status, &body)
    }

    async fn list<T: DeserializeOwned>(&self, path: &str, w: Window) -> anyhow::Result<Vec<T>> {
        let list: List<T> = self.get(path, &list_query(w)?).await?;
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
        if !self.first_paid.contains_key(sub) {
            let mut firsts = Vec::new();
            for page in 0..MAX_SUBSCRIPTION_PAGES {
                self.lookups += 1;
                let query = [
                    ("subscription_id", sub.clone()),
                    ("status", "succeeded".into()),
                    ("page_size", SUBSCRIPTION_PAGE.to_string()),
                    ("page_number", page.to_string()),
                ];
                let list: List<DodoPayment> = self.get("/payments", &query).await?;
                let done = list.items.len() < SUBSCRIPTION_PAGE;
                firsts.extend(first_paid(&list.items));
                if done {
                    break;
                }
            }
            self.first_paid
                .insert(sub.clone(), firsts.into_iter().min());
        }
        Ok(is_renewal(p, self.first_paid[sub].as_ref()))
    }

    async fn payments_page(&mut self, w: Window) -> anyhow::Result<(usize, Vec<Fetched>)> {
        let items: Vec<DodoPayment> = self.list("/payments", w).await?;
        let mut out = Vec::with_capacity(items.len());
        for item in &items {
            out.extend(trial_start(item));
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

/// Lists every subscription; None when the key may not read them.
pub async fn subscriptions(
    client: &reqwest::Client,
    key: &str,
) -> anyhow::Result<Option<Vec<Subscription>>> {
    let key = key.trim();
    let base = detect_mode(client, key).await?.base();
    let mut out = Vec::new();
    for page in 0..MAX_SUBSCRIPTION_PAGES {
        let query = [
            ("page_size", SUBSCRIPTION_PAGE.to_string()),
            ("page_number", page.to_string()),
        ];
        let res = request(client, key, base, "/subscriptions", &query).await?;
        let status = res.status();
        if status == StatusCode::FORBIDDEN {
            return Ok(None);
        }
        let body = res
            .text()
            .await
            .context("could not read Dodo Payments response")?;
        let list: List<DodoSubscriptionRow> = parse("/subscriptions", status, &body)?;
        let n = list.items.len();
        out.extend(
            list.items
                .into_iter()
                .map(DodoSubscriptionRow::into_subscription),
        );
        if n < SUBSCRIPTION_PAGE {
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
        first_paid: HashMap::new(),
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

    #[test]
    fn subscription_status_decides_live() {
        let list: List<DodoSubscriptionRow> =
            serde_json::from_str(include_str!("fixtures/dodo_subscriptions.json")).unwrap();
        let subs: Vec<Subscription> = list
            .items
            .into_iter()
            .map(DodoSubscriptionRow::into_subscription)
            .collect();
        assert_eq!(
            (
                subs[0].amount,
                subs[0].currency.as_deref(),
                subs[0].interval.as_deref()
            ),
            (Some(1500), Some("USD"), Some("month"))
        );
        assert_eq!(subs[0].monthly(), Some(1500.0));
        assert_eq!(
            (subs[1].interval_count, subs[1].monthly()),
            (2, Some(3000.0))
        );
        let subs: Vec<(String, String, bool)> = subs
            .into_iter()
            .map(|s| (s.customer_id, s.status, s.live))
            .collect();
        let want = [
            ("cus_Wn4cTb6Jk1Ps", "active", true),
            ("cus_Hold00000001", "on_hold", true),
            ("cus_Gone00000001", "cancelled", false),
            ("cus_Gone00000002", "expired", false),
            ("cus_Gone00000003", "failed", false),
            ("cus_Pend00000001", "pending", false),
        ];
        let want: Vec<(String, String, bool)> = want
            .iter()
            .map(|(c, s, l)| (c.to_string(), s.to_string(), *l))
            .collect();
        assert_eq!(subs, want);
    }
    use crate::providers::{Provider, e2e};
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

    fn subscription_payments() -> Vec<DodoPayment> {
        let list: List<DodoPayment> =
            serde_json::from_str(include_str!("fixtures/dodo_subscription_payments.json")).unwrap();
        list.items
    }

    #[test]
    fn first_charge_after_a_free_trial_is_not_a_renewal() {
        let sub: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/dodo_subscription.json")).unwrap();
        assert_eq!(sub["trial_period_days"], 14);
        let paid = subscription_payments();
        let first = first_paid(&paid);
        assert_eq!(
            first,
            Some((
                datetime!(2026-09-15 09:00:04 UTC),
                "pay_Fz2kT7bN4wQ8vC1mX5rL9".into()
            )),
            "the $0 trial start is not the first payment"
        );
        let flags: Vec<_> = paid.iter().map(|p| is_renewal(p, first.as_ref())).collect();
        assert_eq!(flags, [true, false, false]);
        assert!(!is_renewal(&paid[0], None), "no earlier payment found");
        let f = paid[0].to_fetched(0, true);
        assert!(f.is_renewal);
        assert_eq!(f.customer_id.as_deref(), Some("cus_Wn4cTb6Jk1Ps"));
    }

    #[test]
    fn compat_metadata_key_is_a_fallback() {
        let mut p = subscription_payments().remove(0);
        p.metadata.remove("virling_vid");
        p.metadata
            .insert("datafast_visitor_id".into(), "v_alias".into());
        assert_eq!(
            p.to_fetched(0, false).visitor_id.as_deref(),
            Some("v_alias")
        );
        p.metadata.insert("virling_vid".into(), "v_own".into());
        assert_eq!(p.to_fetched(0, false).visitor_id.as_deref(), Some("v_own"));
    }

    #[test]
    fn renewals_and_plan_changes_keep_the_first_channel() {
        let paid = subscription_payments();
        let mut first = subscription_payments().remove(1);
        first.metadata.insert("virling_vid".into(), "v_sub".into());
        let mut upgrade = subscription_payments().remove(0);
        upgrade.payment_id = "pay_upgrade".into();
        upgrade.created_at = datetime!(2026-09-20 14:00 UTC);
        let anchor = first_paid(&paid);
        let fetched: Vec<Fetched> = [first, upgrade, subscription_payments().remove(0)]
            .iter()
            .map(|p| p.to_fetched(0, is_renewal(p, anchor.as_ref())))
            .collect();
        let flags: Vec<_> = fetched.iter().map(|f| f.is_renewal).collect();
        assert_eq!(flags, [false, true, true]);
        assert!(fetched[1..].iter().all(|f| f.visitor_id.is_none()));
        e2e::site(Provider::Dodo, fetched, "v_sub").assert_all_reddit();
    }

    #[test]
    fn free_trial_start_credits_the_later_payments() {
        assert!(
            trial_start(&payments()[0]).is_none(),
            "one-time payments are not trials"
        );
        let mut start = payments().remove(3);
        start.payment_id = "pay_trial".into();
        start.total_amount = 0;
        start.created_at = datetime!(2026-09-01 09:00 UTC);
        start.metadata.insert("virling_vid".into(), "v_sub".into());
        let trial = trial_start(&start).unwrap();
        assert_eq!((trial.status.as_str(), trial.amount), (TRIAL, 0));
        let mut first = payments().remove(3);
        first.payment_id = "pay_first".into();
        first.created_at = datetime!(2026-09-15 09:00 UTC);
        let fetched = vec![
            trial,
            first.to_fetched(0, false),
            payments()[3].to_fetched(0, true),
        ];
        let site = e2e::site(Provider::Dodo, fetched, "v_sub");
        site.assert_all_reddit();
        assert_eq!(site.payments(), 2, "the trial start is not a payment");
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
    fn list_query_sends_rfc3339_dates() {
        let w = Window {
            gte: 1_785_062_742,
            lte: 1_785_149_142,
            page: 2,
        };
        let q = list_query(w).unwrap();
        assert!(q.contains(&("created_at_gte", "2026-07-26T10:45:42Z".to_string())));
        assert!(q.contains(&("created_at_lte", "2026-07-27T10:45:42Z".to_string())));
        assert!(q.contains(&("page_number", "2".to_string())));
    }

    #[test]
    fn old_epoch_cursor_still_decodes() {
        let c = Cursor::decode("v1:live:r:1791972942,0,0:1785062742,0,0").unwrap();
        assert_eq!(c.phase, Phase::Refunds);
        assert_eq!(rfc3339(c.refunds.gte).unwrap(), "2026-07-26T10:45:42Z");
    }

    #[test]
    fn bad_responses_are_errors() {
        let err = parse::<List<DodoPayment>>(
            "/payments",
            StatusCode::BAD_REQUEST,
            "Failed to deserialize query string: created_at_gte: input contains invalid characters",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("HTTP 400"), "{err}");
        assert!(err.contains("created_at_gte"), "{err}");
        let err = parse::<List<DodoPayment>>("/payments", StatusCode::OK, "<html>oops</html>")
            .unwrap_err()
            .to_string();
        assert!(err.contains("HTTP 200"), "{err}");
        let ok: List<DodoRefund> = parse("/refunds", StatusCode::OK, r#"{"items":[]}"#).unwrap();
        assert!(ok.items.is_empty());
    }

    #[test]
    fn auth_errors_are_readable() {
        assert!(error_message(StatusCode::UNAUTHORIZED).contains("rejected this API key"));
        assert!(error_message(StatusCode::FORBIDDEN).contains("not allowed to read"));
    }
}
