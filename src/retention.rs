use duckdb::{Connection, named_params};
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::channel::{source_domain, source_name};
use crate::stats::{BOUNDS, CUSTOMER, NET_USD, Period};

/// Extra time after a billing interval before a customer without subscription status stops counting as paying.
pub const GRACE: Duration = Duration::days(7);
/// Billing interval assumed for a customer with a single payment and no subscription status.
pub const DEFAULT_INTERVAL: Duration = Duration::days(30);
/// Payments spanning fewer days than this lead with refunds and churn, since LTV needs time.
pub const LTV_DAYS: i64 = 60;
/// Fewer acquired customers than this, or fewer still paying than `THIN_PAYING`, and there is no headline.
const THIN_CUSTOMERS: u64 = 20;
const THIN_PAYING: u64 = 5;
/// A row needs this many customers to be named in the headline.
const HEADLINE_MIN_ROW: u64 = 5;
/// Smallest gap between share of customers and share of still-paying customers worth a headline.
const HEADLINE_GAP: f64 = 0.15;

/// One customer's lifetime, keyed by the attribution of their first payment.
#[derive(Debug, Clone, PartialEq)]
pub struct Customer {
    pub first: OffsetDateTime,
    pub first_payment_usd: f64,
    pub net_usd: f64,
    pub refunded_usd: f64,
    pub any_refund: bool,
    /// Latest payment that was not fully refunded.
    pub last_kept: Option<OffsetDateTime>,
    /// Time between the last two payments that were not fully refunded.
    pub gap: Option<Duration>,
    /// Some when the provider reported a subscription for this customer: whether any is live now.
    pub subscribed: Option<bool>,
    pub unattributed: bool,
    pub channel: Option<String>,
    pub referrer_host: Option<String>,
    pub campaign: Option<String>,
    pub in_range: bool,
}

impl Customer {
    /// The provider's live subscription status, else a kept payment within the last billing interval plus `GRACE`.
    pub fn still_paying(&self, now: OffsetDateTime) -> bool {
        if let Some(live) = self.subscribed {
            return live;
        }
        let interval = self.gap.map_or(DEFAULT_INTERVAL, |g| {
            g.clamp(Duration::days(7), Duration::days(366))
        });
        self.last_kept.is_some_and(|t| t >= now - interval - GRACE)
    }

    /// First paid more than 30 and at most 60 days before `now`.
    pub fn in_churn_cohort(&self, now: OffsetDateTime) -> bool {
        self.first > now - Duration::days(60) && self.first <= now - Duration::days(30)
    }

    /// No live subscription and no kept payment in the 30 days before `now`.
    pub fn churned(&self, now: OffsetDateTime) -> bool {
        self.subscribed != Some(true)
            && self.last_kept.is_none_or(|t| t <= now - Duration::days(30))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RetentionRow {
    /// Channel, referrer domain or campaign; None for "(none)" and for the Unattributed row.
    pub key: Option<String>,
    /// Display name: the channel, the referrer's source name such as Hacker News, or the campaign.
    pub label: String,
    pub unattributed: bool,
    /// Customers whose first payment was in the range.
    pub customers: u64,
    pub still_paying: u64,
    /// Gross USD of those customers' first payments.
    pub first_payment_usd: f64,
    /// USD refunded or charged back on any of their payments.
    pub refunded_usd: f64,
    /// Every payment they made so far, net of refunds.
    pub net_ltv_usd: f64,
    pub avg_ltv_usd: Option<f64>,
    /// Share of customers with any refund.
    pub refund_rate: Option<f64>,
    /// Customers who first paid 30 to 60 days ago, whatever the range.
    pub churn_customers: u64,
    pub churned: u64,
    /// Share of `churn_customers` with no payment and no live subscription in the last 30 days.
    pub churn_30d: Option<f64>,
    pub share_customers: Option<f64>,
    pub share_still_paying: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusSource {
    pub provider: String,
    /// True when still-paying comes from the provider's subscription status, false when from payment recency only.
    pub subscription_status: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Retention {
    /// Days from the site's first payment to now.
    pub days_of_data: i64,
    /// Under `LTV_DAYS` of payments: lifetime value is not meaningful yet.
    pub day_one: bool,
    pub total: RetentionRow,
    pub channels: Vec<RetentionRow>,
    pub referrers: Vec<RetentionRow>,
    pub campaigns: Vec<RetentionRow>,
    pub headline: Option<String>,
    pub status_sources: Vec<StatusSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    Channel,
    Referrer,
    Campaign,
}

/// First payments that are not renewals, with lifetime totals, the latest kept payment and live subscription status.
const CUSTOMERS: &str = "
pays AS (
    SELECT {CUSTOMER} AS cust, p.provider, p.customer_id, p.provider_payment_id, p.ts,
        epoch_us(p.ts) AS us, p.is_renewal, p.amount, p.refunded, p.amount_usd,
        {NET_USD} AS net, p.amount_usd * p.refunded / nullif(p.amount, 0) AS refunded_usd
    FROM payments p
    WHERE p.site_id = $site AND p.status <> 'failed' AND epoch_us(p.ts) <= $now
),
firsts AS (
    SELECT * FROM pays
    QUALIFY row_number() OVER (PARTITION BY cust ORDER BY ts, provider_payment_id) = 1
),
kept AS (
    SELECT cust, us - lag(us) OVER (PARTITION BY cust ORDER BY us, provider_payment_id) AS gap
    FROM pays WHERE refunded < amount
    QUALIFY row_number() OVER (PARTITION BY cust ORDER BY us DESC, provider_payment_id DESC) = 1
),
totals AS (
    SELECT cust, coalesce(sum(net), 0) AS net, coalesce(sum(refunded_usd), 0) AS refunded_usd,
        bool_or(refunded > 0) AS any_refund, max(us) FILTER (WHERE refunded < amount) AS last_kept
    FROM pays GROUP BY cust
),
subs AS (
    SELECT provider, customer_id,
        bool_or(live AND (ends_at IS NULL OR epoch_us(ends_at) > $now)) AS live
    FROM subscriptions WHERE site_id = $site GROUP BY ALL
)
SELECT f.us, coalesce(f.amount_usd, 0), t.net, t.refunded_usd, t.any_refund, t.last_kept, k.gap,
    s.live, a.session_id IS NULL, a.channel, a.referrer_host, a.campaign,
    f.ts >= b.lo AND f.ts < b.hi
FROM firsts f
CROSS JOIN bounds b
JOIN totals t USING (cust)
LEFT JOIN kept k USING (cust)
LEFT JOIN subs s ON s.provider = f.provider AND s.customer_id = f.customer_id
LEFT JOIN attribution a ON a.site_id = $site AND a.provider = f.provider
    AND a.provider_payment_id = f.provider_payment_id
WHERE NOT f.is_renewal AND ((f.ts >= b.lo AND f.ts < b.hi) OR f.us > $now - 60 * 86400000000)
";

fn micros(t: OffsetDateTime) -> i64 {
    (t.unix_timestamp_nanos() / 1000) as i64
}

pub(crate) fn customers(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    now: OffsetDateTime,
) -> anyhow::Result<Vec<Customer>> {
    let sql = format!(
        "WITH {BOUNDS},{}",
        CUSTOMERS
            .replace("{CUSTOMER}", CUSTOMER)
            .replace("{NET_USD}", NET_USD)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
            "now": micros(now),
        },
        |r| {
            Ok(Customer {
                first: crate::stats::from_micros(r.get(0)?),
                first_payment_usd: r.get(1)?,
                net_usd: r.get(2)?,
                refunded_usd: r.get(3)?,
                any_refund: r.get(4)?,
                last_kept: r.get::<_, Option<i64>>(5)?.map(crate::stats::from_micros),
                gap: r.get::<_, Option<i64>>(6)?.map(Duration::microseconds),
                subscribed: r.get(7)?,
                unattributed: r.get(8)?,
                channel: r.get(9)?,
                referrer_host: r.get(10)?,
                campaign: r.get(11)?,
                in_range: r.get(12)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub(crate) fn retention(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    now: OffsetDateTime,
) -> anyhow::Result<Retention> {
    let list = customers(conn, site_id, period, now)?;
    let first: Option<i64> = conn.query_row(
        "SELECT epoch_us(min(ts)) FROM payments WHERE site_id = ? AND status <> 'failed'",
        [site_id],
        |r| r.get(0),
    )?;
    let days_of_data = first.map_or(0, |us| (now - crate::stats::from_micros(us)).whole_days());
    let mut stmt = conn.prepare(
        "SELECT p.provider, bool_or(s.provider IS NOT NULL)
         FROM (SELECT DISTINCT provider FROM payments WHERE site_id = $site) p
         LEFT JOIN (SELECT DISTINCT provider FROM subscriptions WHERE site_id = $site) s USING (provider)
         GROUP BY ALL ORDER BY 1",
    )?;
    let status_sources = stmt
        .query_map(named_params! { "site": site_id }, |r| {
            Ok(StatusSource {
                provider: r.get(0)?,
                subscription_status: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(summarize(&list, now, days_of_data, status_sources))
}

/// Aggregates customers into the total and the three breakdowns, then picks the headline.
pub fn summarize(
    list: &[Customer],
    now: OffsetDateTime,
    days_of_data: i64,
    status_sources: Vec<StatusSource>,
) -> Retention {
    let total = aggregate(list, now, None)
        .into_iter()
        .next()
        .unwrap_or_default();
    let mut r = Retention {
        days_of_data,
        day_one: days_of_data < LTV_DAYS,
        channels: shares(aggregate(list, now, Some(By::Channel)), &total),
        referrers: shares(aggregate(list, now, Some(By::Referrer)), &total),
        campaigns: shares(aggregate(list, now, Some(By::Campaign)), &total),
        total,
        headline: None,
        status_sources,
    };
    r.headline = headline(&r);
    r
}

fn key_of(c: &Customer, by: By) -> (bool, Option<String>) {
    if c.unattributed {
        return (true, None);
    }
    (
        false,
        match by {
            By::Channel => c.channel.clone(),
            By::Referrer => c.referrer_host.as_deref().map(source_domain),
            By::Campaign => c.campaign.clone(),
        },
    )
}

fn ratio(n: f64, d: u64) -> Option<f64> {
    (d > 0).then(|| n / d as f64)
}

/// One row per key with customers acquired in the range; `by` None gives the single total row.
fn aggregate(list: &[Customer], now: OffsetDateTime, by: Option<By>) -> Vec<RetentionRow> {
    let mut rows: Vec<RetentionRow> = Vec::new();
    for c in list {
        let (unattributed, key) = by.map_or((false, None), |b| key_of(c, b));
        let i = match rows
            .iter()
            .position(|r| r.unattributed == unattributed && r.key == key)
        {
            Some(i) => i,
            None => {
                rows.push(RetentionRow {
                    label: match (&key, unattributed, by) {
                        (_, true, _) => "Unattributed".into(),
                        (_, _, None) => "All customers".into(),
                        (Some(k), _, Some(By::Referrer)) => source_name(k),
                        (Some(k), _, _) => k.clone(),
                        (None, _, _) => "(none)".into(),
                    },
                    key,
                    unattributed,
                    ..Default::default()
                });
                rows.len() - 1
            }
        };
        let r = &mut rows[i];
        if c.in_range {
            r.customers += 1;
            r.still_paying += c.still_paying(now) as u64;
            r.first_payment_usd += c.first_payment_usd;
            r.refunded_usd += c.refunded_usd;
            r.net_ltv_usd += c.net_usd;
            r.refund_rate = Some(r.refund_rate.unwrap_or(0.0) + c.any_refund as u8 as f64);
        }
        if c.in_churn_cohort(now) {
            r.churn_customers += 1;
            r.churned += c.churned(now) as u64;
        }
    }
    rows.retain(|r| r.customers > 0 || by.is_none());
    for r in &mut rows {
        r.avg_ltv_usd = ratio(r.net_ltv_usd, r.customers);
        r.refund_rate = ratio(r.refund_rate.unwrap_or(0.0), r.customers);
        r.churn_30d = ratio(r.churned as f64, r.churn_customers);
    }
    rows.sort_by(|a, b| {
        a.unattributed
            .cmp(&b.unattributed)
            .then(b.customers.cmp(&a.customers))
            .then(b.net_ltv_usd.total_cmp(&a.net_ltv_usd))
            .then(a.label.cmp(&b.label))
    });
    rows
}

fn shares(mut rows: Vec<RetentionRow>, total: &RetentionRow) -> Vec<RetentionRow> {
    for r in &mut rows {
        r.share_customers = ratio(r.customers as f64, total.customers);
        r.share_still_paying = ratio(r.still_paying as f64, total.still_paying);
    }
    rows
}

fn pct(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}

/// The most striking true statement about who stays, or None when the data is too thin to say one.
pub fn headline(r: &Retention) -> Option<String> {
    let t = &r.total;
    if t.customers < THIN_CUSTOMERS || t.still_paying < THIN_PAYING {
        return None;
    }
    let named = |rows: &[RetentionRow]| -> Vec<RetentionRow> {
        rows.iter()
            .filter(|x| !x.unattributed && x.key.is_some() && x.customers >= HEADLINE_MIN_ROW)
            .cloned()
            .collect()
    };
    let candidates: Vec<RetentionRow> = [named(&r.referrers), named(&r.channels)].concat();
    let mut best: Option<(f64, String)> = None;
    for x in &candidates {
        let (Some(sc), Some(sp)) = (x.share_customers, x.share_still_paying) else {
            continue;
        };
        let gap = sc - sp;
        if gap.abs() < HEADLINE_GAP || best.as_ref().is_some_and(|b| b.0 >= gap.abs()) {
            continue;
        }
        let text = if gap > 0.0 {
            format!(
                "{} sent {} of your customers but only {} of the ones still paying.",
                x.label,
                pct(sc),
                pct(sp)
            )
        } else {
            format!(
                "{} sent {} of your customers but {} of the ones still paying.",
                x.label,
                pct(sc),
                pct(sp)
            )
        };
        best = Some((gap.abs(), text));
    }
    if let Some((_, text)) = best {
        return Some(text);
    }
    if r.day_one {
        return None;
    }
    let biggest = candidates.iter().max_by_key(|x| x.customers)?;
    let richest = candidates
        .iter()
        .filter(|x| x.label != biggest.label)
        .max_by(|a, b| {
            a.avg_ltv_usd
                .unwrap_or(0.0)
                .total_cmp(&b.avg_ltv_usd.unwrap_or(0.0))
        })?;
    let (rich, big) = (richest.avg_ltv_usd?, biggest.avg_ltv_usd?);
    (big > 0.0 && rich / big >= 2.0).then(|| {
        format!(
            "A customer from {} is worth {:.1}x one from {} so far.",
            richest.label,
            rich / big,
            biggest.label
        )
    })
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;
    use crate::events::{DuckStore, Event, EventStore, Payment};
    use crate::providers::Subscription;

    const NOW: OffsetDateTime = datetime!(2026-10-16 12:00 UTC);

    fn customer() -> Customer {
        Customer {
            first: NOW - Duration::days(45),
            first_payment_usd: 10.0,
            net_usd: 10.0,
            refunded_usd: 0.0,
            any_refund: false,
            last_kept: Some(NOW - Duration::days(45)),
            gap: None,
            subscribed: None,
            unattributed: false,
            channel: Some("Social".into()),
            referrer_host: Some("x.com".into()),
            campaign: None,
            in_range: true,
        }
    }

    #[test]
    fn still_paying_trusts_the_provider_then_payment_recency() {
        let days = |n| Some(NOW - Duration::days(n));
        let c = Customer {
            last_kept: days(36),
            ..customer()
        };
        assert!(c.still_paying(NOW), "monthly within interval plus grace");
        let c = Customer {
            last_kept: days(38),
            ..customer()
        };
        assert!(!c.still_paying(NOW));
        let c = Customer {
            last_kept: days(38),
            subscribed: Some(true),
            ..customer()
        };
        assert!(c.still_paying(NOW), "a live subscription wins");
        let c = Customer {
            last_kept: days(1),
            subscribed: Some(false),
            ..customer()
        };
        assert!(!c.still_paying(NOW), "a cancelled subscription wins");
        let c = Customer {
            last_kept: days(300),
            gap: Some(Duration::days(365)),
            ..customer()
        };
        assert!(c.still_paying(NOW), "yearly interval from the payment gap");
        let c = Customer {
            last_kept: None,
            ..customer()
        };
        assert!(!c.still_paying(NOW), "every payment refunded");
    }

    #[test]
    fn churn_window_edges() {
        let at = |n: i64| Customer {
            first: NOW - Duration::days(n),
            last_kept: Some(NOW - Duration::days(n)),
            ..customer()
        };
        assert!(at(30).in_churn_cohort(NOW));
        assert!(!at(60).in_churn_cohort(NOW));
        assert!(at(59).in_churn_cohort(NOW));
        assert!(!at(29).in_churn_cohort(NOW));
        assert!(
            at(30).churned(NOW),
            "the first payment itself is not activity"
        );
        let renewed = Customer {
            last_kept: Some(NOW - Duration::days(30) + Duration::seconds(1)),
            ..at(45)
        };
        assert!(!renewed.churned(NOW));
        let live = Customer {
            subscribed: Some(true),
            ..at(45)
        };
        assert!(!live.churned(NOW));
    }

    fn row(label: &str, customers: u64, still: u64, avg: f64) -> RetentionRow {
        RetentionRow {
            key: Some(label.into()),
            label: label.into(),
            customers,
            still_paying: still,
            avg_ltv_usd: Some(avg),
            ..Default::default()
        }
    }

    fn report(rows: Vec<RetentionRow>, day_one: bool) -> Retention {
        let total = RetentionRow {
            customers: rows.iter().map(|r| r.customers).sum(),
            still_paying: rows.iter().map(|r| r.still_paying).sum(),
            ..Default::default()
        };
        let rows = shares(rows, &total);
        Retention {
            days_of_data: if day_one { 20 } else { 200 },
            day_one,
            total,
            channels: Vec::new(),
            referrers: rows,
            campaigns: Vec::new(),
            headline: None,
            status_sources: Vec::new(),
        }
    }

    #[test]
    fn headline_picks_the_biggest_true_gap() {
        let r = report(
            vec![
                row("X", 60, 5, 20.0),
                row("Hacker News", 30, 12, 90.0),
                row("Google", 10, 3, 40.0),
            ],
            false,
        );
        assert_eq!(
            headline(&r).as_deref(),
            Some("X sent 60% of your customers but only 25% of the ones still paying.")
        );
        let r = report(
            vec![
                row("X", 50, 18, 20.0),
                row("Hacker News", 40, 2, 90.0),
                row("Google", 10, 0, 40.0),
            ],
            false,
        );
        assert_eq!(
            headline(&r).as_deref(),
            Some("X sent 50% of your customers but 90% of the ones still paying.")
        );
    }

    #[test]
    fn headline_falls_back_to_ltv_and_hides_when_thin() {
        let even = vec![
            row("X", 40, 10, 20.0),
            row("Hacker News", 30, 8, 64.0),
            row("Google", 30, 7, 30.0),
        ];
        assert_eq!(
            headline(&report(even.clone(), false)).as_deref(),
            Some("A customer from Hacker News is worth 3.2x one from X so far.")
        );
        assert_eq!(headline(&report(even, true)), None, "LTV waits on day one");
        let thin = vec![row("X", 12, 1, 20.0), row("Hacker News", 3, 3, 90.0)];
        assert_eq!(headline(&report(thin, false)), None);
    }

    fn store() -> (tempfile::TempDir, DuckStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        (dir, store)
    }

    fn visit(visitor: &str, ts: OffsetDateTime, host: &str, channel: &str) -> Event {
        Event {
            site_id: "s".into(),
            ts,
            visitor_id: visitor.into(),
            session_id: format!("{visitor}-{}", ts.unix_timestamp()),
            kind: "pageview".into(),
            name: None,
            path: "/".into(),
            referrer_host: Some(host.into()),
            utm_source: None,
            utm_medium: None,
            utm_campaign: Some("launch".into()),
            utm_content: None,
            utm_term: None,
            channel: channel.into(),
            country: None,
            device: "Desktop".into(),
            browser: "Firefox".into(),
            os: "macOS".into(),
            region: None,
            city: None,
            host: None,
        }
    }

    fn pay(
        id: &str,
        customer: &str,
        visitor: Option<&str>,
        days_ago: i64,
        refunded: i64,
    ) -> Payment {
        Payment {
            site_id: "s".into(),
            provider: "stripe".into(),
            provider_payment_id: id.into(),
            ts: NOW - Duration::days(days_ago),
            amount: 1000,
            currency: "usd".into(),
            amount_usd: Some(10.0),
            refunded,
            status: match refunded {
                0 => "paid",
                1000 => "refunded",
                _ => "partially_refunded",
            }
            .into(),
            visitor_id: visitor.map(Into::into),
            customer_id: Some(customer.into()),
            is_renewal: false,
        }
    }

    fn renewal(p: Payment) -> Payment {
        Payment {
            is_renewal: true,
            ..p
        }
    }

    fn period() -> Period {
        Period {
            from: (NOW - Duration::days(89)).date(),
            to: NOW.date(),
            tz: "UTC".into(),
        }
    }

    #[test]
    fn ltv_is_net_of_refunds_and_renewals_stay_with_the_first_channel() {
        let (_d, store) = store();
        store
            .insert_events(&[
                visit(
                    "v_hn",
                    NOW - Duration::days(81),
                    "news.ycombinator.com",
                    "Social",
                ),
                visit("v_x", NOW - Duration::days(81), "x.com", "Social"),
                visit(
                    "v_hn",
                    NOW - Duration::days(21),
                    "www.google.com",
                    "Organic search",
                ),
            ])
            .unwrap();
        store
            .upsert_payments(&[
                pay("a1", "cus_hn", Some("v_hn"), 80, 0),
                renewal(pay("a2", "cus_hn", None, 50, 1000)),
                renewal(pay("a3", "cus_hn", Some("v_hn"), 20, 500)),
                pay("b1", "cus_x", Some("v_x"), 80, 0),
                renewal(pay("old", "cus_old", None, 10, 0)),
            ])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 30)
            .unwrap();
        let r = store.retention("s", &period(), NOW).unwrap();
        assert_eq!(
            r.total.customers, 2,
            "a customer first seen on a renewal is not acquired here"
        );
        let hn = r
            .referrers
            .iter()
            .find(|x| x.label == "Hacker News")
            .unwrap();
        assert_eq!((hn.customers, hn.still_paying), (1, 1));
        assert!((hn.net_ltv_usd - 15.0).abs() < 1e-9, "{hn:?}");
        assert!((hn.refunded_usd - 15.0).abs() < 1e-9);
        assert!((hn.first_payment_usd - 10.0).abs() < 1e-9);
        assert_eq!(hn.refund_rate, Some(1.0));
        assert!(r.referrers.iter().all(|x| x.label != "Google"));
        let x = r.referrers.iter().find(|x| x.label == "X").unwrap();
        assert_eq!((x.customers, x.still_paying, x.net_ltv_usd), (1, 0, 10.0));
        assert_eq!(x.refund_rate, Some(0.0));
        assert_eq!(r.channels.len(), 1);
        assert_eq!(r.channels[0].customers, 2);
        assert_eq!(r.channels[0].share_still_paying, Some(1.0));
        assert_eq!(r.campaigns[0].label, "launch");
        assert_eq!(
            r.status_sources,
            [StatusSource {
                provider: "stripe".into(),
                subscription_status: false
            }]
        );
        assert_eq!((r.days_of_data, r.day_one), (80, false));
    }

    #[test]
    fn subscription_status_overrides_payment_recency() {
        let (_d, store) = store();
        store
            .upsert_payments(&[
                pay("a1", "cus_live", None, 45, 0),
                pay("b1", "cus_cancelled", None, 45, 0),
                renewal(pay("b2", "cus_cancelled", None, 15, 0)),
                pay("c1", "cus_grace", None, 45, 0),
                pay("d1", "cus_ended", None, 45, 0),
                pay("e1", "cus_refunded", None, 40, 1000),
            ])
            .unwrap();
        let sub = |id: &str, customer: &str, live: bool, ends: Option<i64>| Subscription {
            subscription_id: id.into(),
            customer_id: customer.into(),
            status: "x".into(),
            live,
            ends_at: ends.map(|d| NOW + Duration::days(d)),
        };
        store
            .replace_subscriptions(
                "s",
                "stripe",
                &[
                    sub("s1", "cus_live", true, None),
                    sub("s2", "cus_cancelled", false, None),
                    sub("s3", "cus_grace", true, Some(3)),
                    sub("s4", "cus_ended", true, Some(-1)),
                ],
            )
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 30)
            .unwrap();
        let r = store.retention("s", &period(), NOW).unwrap();
        let t = &r.total;
        assert_eq!((t.customers, t.still_paying), (5, 2), "live and grace");
        assert_eq!(
            (t.churn_customers, t.churned),
            (5, 2),
            "cancelled but paid 15 days ago"
        );
        assert_eq!(t.churn_30d, Some(0.4));
        assert_eq!(r.channels[0].label, "Unattributed");
        assert!(r.status_sources[0].subscription_status);
        assert!(r.day_one);
        store.replace_subscriptions("s", "stripe", &[]).unwrap();
        let r = store.retention("s", &period(), NOW).unwrap();
        assert_eq!(
            r.total.still_paying, 1,
            "only the renewal 15 days ago is recent"
        );
    }
}
