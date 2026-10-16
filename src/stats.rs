use duckdb::{Connection, named_params};
use serde::Serialize;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};

use crate::channel::{source_domain, source_name};

/// Net USD of a payment `p`; NULL when it has no USD conversion, so it counts as a payment but not revenue.
pub(crate) const NET_USD: &str = "p.amount_usd * (p.amount - p.refunded) / nullif(p.amount, 0)";

/// Who paid: provider and customer_id, else visitor_id, else the payment itself.
pub(crate) const CUSTOMER: &str = "coalesce('c:' || p.provider || ':' || p.customer_id, 'v:' || p.visitor_id, 'p:' || p.provider || ':' || p.provider_payment_id)";

/// Local dates `$from..=$to` in IANA zone `$tz` as a half-open UTC range [lo, hi).
pub(crate) const BOUNDS: &str = "bounds AS (
    SELECT timezone($tz, CAST($from AS DATE)::TIMESTAMP) AS lo,
           timezone($tz, (CAST($to AS DATE) + 1)::TIMESTAMP) AS hi
)";

/// Inclusive range of local dates in the site's timezone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Period {
    pub from: Date,
    pub to: Date,
    pub tz: String,
}

impl Period {
    /// The equal-length period that ends the day before this one starts.
    pub fn previous(&self) -> Period {
        let days = (self.to - self.from).whole_days() + 1;
        Period {
            from: self.from - Duration::days(days),
            to: self.from - Duration::days(1),
            tz: self.tz.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Totals {
    pub visitors: u64,
    pub sessions: u64,
    /// Non-failed payments, including those without a USD amount.
    pub payments: u64,
    /// Distinct customer_id, else visitor_id, else payment, among non-failed payments.
    pub customers: u64,
    pub revenue_usd: f64,
    /// Customers whose payment matches a visitor tracked in the period.
    pub converted: u64,
    /// Payments, and their net USD, whose visitor was tracked in the period.
    pub matched_payments: u64,
    pub matched_revenue_usd: f64,
    /// Matched revenue per visitor; None until a payment is matched.
    pub revenue_per_visitor: Option<f64>,
    /// Converted customers per visitor; None until a payment is matched.
    pub conversion: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Overview {
    pub current: Totals,
    pub previous: Totals,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayPoint {
    pub date: Date,
    pub visitors: u64,
    pub payments: u64,
    pub customers: u64,
    pub revenue_usd: f64,
    pub converted: u64,
    pub matched_revenue_usd: f64,
    pub conversion: Option<f64>,
    pub revenue_per_visitor: Option<f64>,
    /// Revenue by credited channel, largest first; None is Unattributed.
    pub by_channel: Vec<(Option<String>, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Channel,
    Referrer,
    LandingPage,
    Country,
    Campaign,
    Keyword,
    /// "channel|referrer_host", to list each channel's sources.
    ChannelSource,
    /// Every page viewed, not only the first of a session.
    Page,
    Browser,
    Os,
    Device,
}

impl Dimension {
    /// (event kind, key expression, whether only the session's first such event counts)
    fn source(self) -> (&'static str, &'static str, bool) {
        let first = |key| ("pageview", key, true);
        match self {
            Dimension::Channel => first("channel"),
            Dimension::Referrer => first("referrer_host"),
            Dimension::LandingPage => first("path"),
            Dimension::Country => first("country"),
            Dimension::Campaign => first("utm_campaign"),
            Dimension::Keyword => first("utm_term"),
            Dimension::ChannelSource => first("channel || '|' || coalesce(referrer_host, '')"),
            Dimension::Browser => first("browser"),
            Dimension::Os => first("os"),
            Dimension::Device => first("device"),
            Dimension::Page => ("pageview", "path", false),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BreakdownRow {
    /// Raw value (source name for referrers); None for "(none)" and for the Unattributed row.
    pub key: Option<String>,
    pub label: String,
    pub unattributed: bool,
    pub visitors: u64,
    pub payments: u64,
    /// Distinct paying customers credited to this row.
    pub customers: u64,
    pub revenue_usd: f64,
    pub revenue_per_visitor: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentFilter {
    All,
    Attributed,
    Unattributed,
    Refunded,
}

impl PaymentFilter {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "all" => PaymentFilter::All,
            "attributed" => PaymentFilter::Attributed,
            "unattributed" => PaymentFilter::Unattributed,
            "refunded" => PaymentFilter::Refunded,
            _ => return None,
        })
    }

    fn sql(self) -> &'static str {
        match self {
            PaymentFilter::All => "true",
            PaymentFilter::Attributed => "a.session_id IS NOT NULL",
            PaymentFilter::Unattributed => "a.session_id IS NULL",
            PaymentFilter::Refunded => "p.refunded > 0",
        }
    }
}

/// The payer's earliest visit at or before the payment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FirstTouch {
    #[serde(with = "time::serde::rfc3339")]
    pub ts: OffsetDateTime,
    pub channel: String,
    pub referrer_host: Option<String>,
    pub source: String,
}

/// The session credited for a payment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Touch {
    pub session_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub ts: OffsetDateTime,
    pub channel: String,
    pub referrer_host: Option<String>,
    /// Display name of the referrer, or the channel when there is none.
    pub source: String,
    pub campaign: Option<String>,
    pub landing_page: Option<String>,
    pub country: Option<String>,
    pub device: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaymentRow {
    pub provider: String,
    pub provider_payment_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub ts: OffsetDateTime,
    pub amount: i64,
    pub currency: String,
    pub amount_usd: Option<f64>,
    pub refunded: i64,
    pub net_usd: Option<f64>,
    pub status: String,
    pub visitor_id: Option<String>,
    pub customer_id: Option<String>,
    pub is_renewal: bool,
    /// Whose timeline shows this payment: its visitor, else the credited visit's, else the customer's.
    pub visitor: Option<String>,
    /// None means Unattributed.
    pub touch: Option<Touch>,
    pub first_touch: Option<FirstTouch>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaymentPage {
    pub total: u64,
    pub rows: Vec<PaymentRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JourneySession {
    pub session_id: String,
    pub ts: OffsetDateTime,
    pub channel: String,
    pub referrer_host: Option<String>,
    pub source: String,
    pub campaign: Option<String>,
    pub paths: Vec<String>,
    pub goals: Vec<String>,
    pub credited: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Journey {
    pub payment: PaymentRow,
    /// Oldest first.
    pub sessions: Vec<JourneySession>,
    /// From the payer's last pageview at or before the payment.
    pub country: Option<String>,
    pub browser: Option<String>,
    pub device: Option<String>,
    /// Path of that last pageview.
    pub last_page: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PaymentSummary {
    pub payments: u64,
    pub attributed: u64,
    pub unattributed: u64,
    pub refunded: u64,
    pub customers: u64,
    pub net_usd: f64,
    pub refunded_usd: f64,
}

/// Site-wide facts for the empty and new-site states, regardless of range.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Setup {
    pub first_event: Option<OffsetDateTime>,
    pub has_goals: bool,
    pub payments: u64,
    /// Earliest payment credited to a visit.
    pub first_match: Option<PaymentRow>,
}

/// Everything the dashboard shows for a range, read in one go.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub overview: Overview,
    pub days: Vec<DayPoint>,
    pub previous_days: Vec<DayPoint>,
    pub channels: Vec<BreakdownRow>,
    pub referrers: Vec<BreakdownRow>,
    pub landing_pages: Vec<BreakdownRow>,
    pub countries: Vec<BreakdownRow>,
    /// Up to three referrer source names per channel, most visitors first.
    pub channel_sources: Vec<(String, Vec<String>)>,
    pub setup: Setup,
}

pub(crate) fn from_micros(us: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(us as i128 * 1000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

pub(crate) fn source(referrer_host: Option<&str>, channel: &str) -> String {
    referrer_host.map_or_else(|| channel.to_string(), source_name)
}

fn ratio(n: f64, d: u64) -> Option<f64> {
    (d > 0).then(|| n / d as f64)
}

fn totals(conn: &Connection, site_id: &str, period: &Period) -> anyhow::Result<Totals> {
    let sql = format!(
        "WITH {BOUNDS},
        tracked AS (
            SELECT DISTINCT visitor_id FROM events, bounds
            WHERE site_id = $site AND ts >= lo AND ts < hi
        )
        SELECT
            (SELECT count(*) FROM tracked),
            (SELECT count(DISTINCT session_id) FROM events, bounds
             WHERE site_id = $site AND ts >= lo AND ts < hi),
            count(*),
            count(DISTINCT {CUSTOMER}),
            coalesce(sum({NET_USD}), 0),
            count(DISTINCT {CUSTOMER}) FILTER (WHERE p.visitor_id IN (SELECT visitor_id FROM tracked)),
            count(*) FILTER (WHERE p.visitor_id IN (SELECT visitor_id FROM tracked)),
            coalesce(sum({NET_USD}) FILTER (WHERE p.visitor_id IN (SELECT visitor_id FROM tracked)), 0)
        FROM payments p, bounds
        WHERE p.site_id = $site AND p.ts >= lo AND p.ts < hi AND p.status <> 'failed'"
    );
    #[allow(clippy::type_complexity)]
    let (visitors, sessions, payments, customers, revenue_usd, converted, matched, matched_usd): (
        i64,
        i64,
        i64,
        i64,
        f64,
        i64,
        i64,
        f64,
    ) = conn.query_row(
        &sql,
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        },
    )?;
    let visitors = visitors as u64;
    let some = matched > 0;
    Ok(Totals {
        visitors,
        sessions: sessions as u64,
        payments: payments as u64,
        customers: customers as u64,
        revenue_usd,
        converted: converted as u64,
        matched_payments: matched as u64,
        matched_revenue_usd: matched_usd,
        revenue_per_visitor: ratio(matched_usd, visitors).filter(|_| some),
        conversion: ratio(converted as f64, visitors).filter(|_| some),
    })
}

pub(crate) fn overview(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<Overview> {
    Ok(Overview {
        current: totals(conn, site_id, period)?,
        previous: totals(conn, site_id, &period.previous())?,
    })
}

pub(crate) fn timeseries(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<Vec<DayPoint>> {
    let sql = format!(
        "WITH {BOUNDS},
        days AS (
            SELECT CAST(generate_series AS DATE) AS day
            FROM generate_series(CAST($from AS DATE)::TIMESTAMP, CAST($to AS DATE)::TIMESTAMP, INTERVAL 1 DAY)
        ),
        v AS (
            SELECT CAST(timezone($tz, ts) AS DATE) AS day, count(DISTINCT visitor_id) AS n
            FROM events, bounds WHERE site_id = $site AND ts >= lo AND ts < hi GROUP BY 1
        ),
        r AS (
            SELECT CAST(timezone($tz, p.ts) AS DATE) AS day, count(*) AS n,
                count(DISTINCT {CUSTOMER}) AS customers, coalesce(sum({NET_USD}), 0) AS rev,
                count(DISTINCT {CUSTOMER}) FILTER (WHERE matched) AS converted,
                coalesce(sum({NET_USD}) FILTER (WHERE matched), 0) AS matched_rev
            FROM (
                SELECT p.*, EXISTS (
                    SELECT 1 FROM events e WHERE e.site_id = $site AND e.visitor_id = p.visitor_id
                    AND CAST(timezone($tz, e.ts) AS DATE) = CAST(timezone($tz, p.ts) AS DATE)) AS matched
                FROM payments p WHERE p.site_id = $site
            ) p, bounds
            WHERE p.site_id = $site AND p.ts >= lo AND p.ts < hi AND p.status <> 'failed' GROUP BY 1
        )
        SELECT strftime(d.day, '%Y-%m-%d'), coalesce(v.n, 0), coalesce(r.n, 0),
            coalesce(r.customers, 0), coalesce(r.rev, 0), coalesce(r.converted, 0),
            coalesce(r.matched_rev, 0)
        FROM days d LEFT JOIN v USING (day) LEFT JOIN r USING (day)
        ORDER BY d.day"
    );
    let params = named_params! {
        "site": site_id,
        "tz": period.tz,
        "from": period.from.to_string(),
        "to": period.to.to_string(),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, f64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, f64>(6)?,
        ))
    })?;
    let ymd = format_description!("[year]-[month]-[day]");
    let mut out = Vec::new();
    for row in rows {
        let (day, visitors, payments, customers, revenue_usd, converted, matched_usd) = row?;
        let visitors = visitors as u64;
        out.push(DayPoint {
            date: Date::parse(&day, ymd)?,
            visitors,
            payments: payments as u64,
            customers: customers as u64,
            revenue_usd,
            converted: converted as u64,
            matched_revenue_usd: matched_usd,
            conversion: ratio(converted as f64, visitors),
            revenue_per_visitor: ratio(matched_usd, visitors),
            by_channel: Vec::new(),
        });
    }
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS}
        SELECT strftime(CAST(timezone($tz, p.ts) AS DATE), '%Y-%m-%d'),
            CASE WHEN a.session_id IS NOT NULL THEN a.channel END, coalesce(sum({NET_USD}), 0)
        FROM payments p
        JOIN bounds ON p.ts >= lo AND p.ts < hi
        {ATTRIBUTION_JOIN}
        WHERE p.site_id = $site AND p.status <> 'failed'
        GROUP BY ALL"
    ))?;
    let rows = stmt.query_map(params, |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, f64>(2)?,
        ))
    })?;
    for row in rows {
        let (day, channel, revenue) = row?;
        let date = Date::parse(&day, ymd)?;
        if let Some(p) = out.iter_mut().find(|p| p.date == date) {
            p.by_channel.push((channel, revenue));
        }
    }
    for p in &mut out {
        p.by_channel
            .sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    }
    Ok(out)
}

pub(crate) fn breakdown(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    dimension: Dimension,
) -> anyhow::Result<Vec<BreakdownRow>> {
    let (kind, key, first) = dimension.source();
    let only_first = if first {
        "QUALIFY row_number() OVER (PARTITION BY session_id ORDER BY ts, path) = 1"
    } else {
        ""
    };
    // Visitors count sessions in range; payments follow their credited session, wherever it falls.
    let sql = format!(
        "WITH {BOUNDS},
        keyed AS (
            SELECT session_id, visitor_id, ts, {key} AS key
            FROM events
            WHERE site_id = $site AND kind = '{kind}'
            {only_first}
        )
        SELECT unattributed, key, sum(visitors)::BIGINT, sum(payments)::BIGINT, sum(revenue),
            sum(customers)::BIGINT
        FROM (
            SELECT false AS unattributed, key, count(DISTINCT visitor_id) AS visitors,
                   0 AS payments, 0.0 AS revenue, 0 AS customers
            FROM keyed, bounds WHERE ts >= lo AND ts < hi GROUP BY key
            UNION ALL
            SELECT a.session_id IS NULL, k.key, 0, count(*), coalesce(sum({NET_USD}), 0),
                   count(DISTINCT {CUSTOMER})
            FROM payments p
            JOIN bounds ON p.ts >= lo AND p.ts < hi
            LEFT JOIN attribution a ON a.site_id = p.site_id AND a.provider = p.provider
                AND a.provider_payment_id = p.provider_payment_id
            LEFT JOIN (SELECT DISTINCT session_id, key FROM keyed) k ON k.session_id = a.session_id
            WHERE p.site_id = $site AND p.status <> 'failed'
            GROUP BY ALL
        )
        GROUP BY ALL"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            Ok((
                r.get::<_, bool>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        },
    )?;
    let mut out: Vec<BreakdownRow> = Vec::new();
    for row in rows {
        let (unattributed, mut key, visitors, payments, revenue_usd, customers) = row?;
        match dimension {
            Dimension::Referrer => key = key.as_deref().map(source_domain),
            Dimension::ChannelSource => {
                key = key.map(|k| match k.split_once('|') {
                    Some((c, h)) if !h.is_empty() => format!("{c}|{}", source_domain(h)),
                    _ => k,
                })
            }
            _ => {}
        }
        let label = match (&key, unattributed) {
            (_, true) => "Unattributed".to_string(),
            (Some(k), false) => k.clone(),
            (None, false) => "(none)".to_string(),
        };
        if let Some(existing) = out
            .iter_mut()
            .find(|r| r.unattributed == unattributed && r.key == key)
        {
            existing.visitors += visitors as u64;
            existing.payments += payments as u64;
            existing.customers += customers as u64;
            existing.revenue_usd += revenue_usd;
        } else {
            out.push(BreakdownRow {
                key,
                label,
                unattributed,
                visitors: visitors as u64,
                payments: payments as u64,
                customers: customers as u64,
                revenue_usd,
                revenue_per_visitor: None,
            });
        }
    }
    for r in &mut out {
        r.revenue_per_visitor = ratio(r.revenue_usd, r.visitors);
    }
    sort_rows(&mut out);
    Ok(out)
}

/// Most revenue first, then most visitors, then by label.
pub fn sort_rows(rows: &mut [BreakdownRow]) {
    rows.sort_by(|a, b| {
        b.revenue_usd
            .total_cmp(&a.revenue_usd)
            .then(b.visitors.cmp(&a.visitors))
            .then_with(|| a.label.cmp(&b.label))
    });
}

pub(crate) const PAYMENT_COLUMNS: &str =
    "p.provider, p.provider_payment_id, epoch_us(p.ts), p.amount, p.currency,
    p.amount_usd, p.refunded, p.status, p.visitor_id, p.customer_id, p.is_renewal,
    a.session_id, epoch_us(a.touch_ts), a.channel, a.referrer_host, a.campaign, a.landing_page,
    a.country, a.device, epoch_us(f.ts), f.channel, f.referrer_host,
    coalesce(p.visitor_id,
        (SELECT c.visitor_id FROM events c WHERE c.site_id = p.site_id AND c.session_id = a.session_id LIMIT 1),
        (SELECT q.visitor_id FROM payments q WHERE q.site_id = p.site_id AND q.provider = p.provider
            AND q.customer_id = p.customer_id AND q.visitor_id IS NOT NULL ORDER BY q.ts LIMIT 1))";

pub(crate) const ATTRIBUTION_JOIN: &str = "LEFT JOIN attribution a ON a.site_id = p.site_id
    AND a.provider = p.provider AND a.provider_payment_id = p.provider_payment_id";

/// The first pageview of the payer, or of the credited visitor when the payment has no visitor ID.
pub(crate) const FIRST_TOUCH_JOIN: &str = "LEFT JOIN LATERAL (
        SELECT e.ts, e.channel, e.referrer_host FROM events e
        WHERE e.site_id = p.site_id AND e.kind = 'pageview' AND e.ts <= p.ts
            AND e.visitor_id = coalesce(p.visitor_id,
                (SELECT c.visitor_id FROM events c WHERE c.site_id = p.site_id AND c.session_id = a.session_id LIMIT 1))
        ORDER BY e.ts, e.path
        LIMIT 1
    ) f ON true";

pub(crate) fn payment_row(r: &duckdb::Row) -> duckdb::Result<PaymentRow> {
    let amount: i64 = r.get(3)?;
    let amount_usd: Option<f64> = r.get(5)?;
    let refunded: i64 = r.get(6)?;
    let session_id: Option<String> = r.get(11)?;
    let touch = match session_id {
        Some(session_id) => {
            let channel: String = r.get(13)?;
            let referrer_host: Option<String> = r.get(14)?;
            Some(Touch {
                session_id,
                ts: from_micros(r.get(12)?),
                source: source(referrer_host.as_deref(), &channel),
                channel,
                referrer_host,
                campaign: r.get(15)?,
                landing_page: r.get(16)?,
                country: r.get(17)?,
                device: r.get(18)?,
            })
        }
        None => None,
    };
    let first_touch = match r.get::<_, Option<i64>>(19)? {
        Some(us) => {
            let channel: String = r.get(20)?;
            let referrer_host: Option<String> = r.get(21)?;
            Some(FirstTouch {
                ts: from_micros(us),
                source: source(referrer_host.as_deref(), &channel),
                channel,
                referrer_host,
            })
        }
        None => None,
    };
    Ok(PaymentRow {
        provider: r.get(0)?,
        provider_payment_id: r.get(1)?,
        ts: from_micros(r.get(2)?),
        amount,
        currency: r.get(4)?,
        amount_usd,
        refunded,
        net_usd: amount_usd
            .filter(|_| amount != 0)
            .map(|usd| usd * (amount - refunded) as f64 / amount as f64),
        status: r.get(7)?,
        visitor_id: r.get(8)?,
        customer_id: r.get(9)?,
        is_renewal: r.get(10)?,
        visitor: r.get(22)?,
        touch,
        first_touch,
    })
}

pub(crate) fn payment_list(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    filter: PaymentFilter,
    limit: u32,
    offset: u32,
) -> anyhow::Result<PaymentPage> {
    let from = format!(
        "FROM payments p
        JOIN bounds ON p.ts >= lo AND p.ts < hi
        {ATTRIBUTION_JOIN}
        WHERE p.site_id = $site AND p.status <> 'failed' AND {}",
        filter.sql()
    );
    let from_first = from.replacen(
        ATTRIBUTION_JOIN,
        &format!("{ATTRIBUTION_JOIN} {FIRST_TOUCH_JOIN}"),
        1,
    );
    let total: i64 = conn.query_row(
        &format!("WITH {BOUNDS} SELECT count(*) {from}"),
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| r.get(0),
    )?;
    let sql = format!(
        "WITH {BOUNDS} SELECT {PAYMENT_COLUMNS} {from_first}
        ORDER BY p.ts DESC, p.provider, p.provider_payment_id
        LIMIT $limit OFFSET $offset"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
            "limit": limit as i64,
            "offset": offset as i64,
        },
        payment_row,
    )?;
    Ok(PaymentPage {
        total: total as u64,
        rows: rows.collect::<Result<_, _>>()?,
    })
}

pub(crate) fn journey(
    conn: &Connection,
    site_id: &str,
    provider: &str,
    provider_payment_id: &str,
    window_days: i64,
) -> anyhow::Result<Option<Journey>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PAYMENT_COLUMNS} FROM payments p {ATTRIBUTION_JOIN} {FIRST_TOUCH_JOIN}
        WHERE p.site_id = $site AND p.provider = $provider AND p.provider_payment_id = $id"
    ))?;
    let params = named_params! { "site": site_id, "provider": provider, "id": provider_payment_id };
    let Some(payment) = stmt.query_map(params, payment_row)?.next().transpose()? else {
        return Ok(None);
    };
    let credited = payment.touch.as_ref().map(|t| t.session_id.clone());
    let mut stmt = conn.prepare(
        "WITH firsts AS (
            SELECT session_id, ts, channel, referrer_host, utm_campaign
            FROM events
            WHERE site_id = $site AND kind = 'pageview' AND (visitor_id = $visitor OR session_id = $credited)
            QUALIFY row_number() OVER (PARTITION BY session_id ORDER BY ts, path) = 1
        ),
        picked AS (
            SELECT * FROM firsts
            WHERE (ts <= $ts AND epoch_us(ts) >= epoch_us($ts) - $window_days * 86400000000)
               OR session_id = $credited
        )
        SELECT k.session_id, epoch_us(k.ts), k.channel, k.referrer_host, k.utm_campaign,
            to_json(list(e.path ORDER BY e.ts, e.path) FILTER (WHERE e.kind = 'pageview'))::VARCHAR,
            to_json(list(e.name ORDER BY e.ts, e.name) FILTER (WHERE e.kind = 'goal'))::VARCHAR
        FROM picked k
        JOIN events e ON e.site_id = $site AND e.session_id = k.session_id
        GROUP BY ALL
        ORDER BY epoch_us(k.ts), k.session_id",
    )?;
    let ts = micros(payment.ts);
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "visitor": payment.visitor_id,
            "credited": credited,
            "ts": ts,
            "window_days": window_days,
        },
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        },
    )?;
    let mut sessions = Vec::new();
    for row in rows {
        let (session_id, us, channel, referrer_host, campaign, paths, goals) = row?;
        let list = |json: Option<String>| -> anyhow::Result<Vec<String>> {
            Ok(match json {
                Some(j) => serde_json::from_str(&j)?,
                None => Vec::new(),
            })
        };
        sessions.push(JourneySession {
            credited: credited.as_deref() == Some(session_id.as_str()),
            session_id,
            ts: from_micros(us),
            source: source(referrer_host.as_deref(), &channel),
            channel,
            referrer_host,
            campaign,
            paths: list(paths)?,
            goals: list(goals)?,
        });
    }
    let seen = last_seen(
        conn,
        site_id,
        &Who {
            visitor_id: payment.visitor_id.clone(),
            session_id: credited,
            ts: payment.ts,
        },
    )?;
    Ok(Some(Journey {
        payment,
        sessions,
        country: seen.country,
        browser: seen.browser,
        device: seen.device,
        last_page: seen.last_page,
    }))
}

/// A visitor, or the session credited to them, at the moment they paid or completed a goal.
#[derive(Debug, Clone, PartialEq)]
pub struct Who {
    pub visitor_id: Option<String>,
    pub session_id: Option<String>,
    pub ts: OffsetDateTime,
}

impl Who {
    pub fn of(p: &PaymentRow) -> Self {
        Who {
            visitor_id: p.visitor_id.clone(),
            session_id: p.touch.as_ref().map(|t| t.session_id.clone()),
            ts: p.ts,
        }
    }
}

/// Days in an activity strip, ending on the day of the payment or goal.
pub const ACTIVITY_DAYS: usize = 14;

/// Where someone was and how often they came back before a payment or goal.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Activity {
    pub country: Option<String>,
    pub device: Option<String>,
    pub os: Option<String>,
    pub browser: Option<String>,
    /// Path of the last pageview at or before the moment.
    pub last_page: Option<String>,
    /// Sessions per local day, oldest first; the last entry is the day of the payment or goal.
    pub days: Vec<u32>,
}

fn micros(ts: OffsetDateTime) -> duckdb::types::Value {
    duckdb::types::Value::Timestamp(
        duckdb::types::TimeUnit::Microsecond,
        (ts.unix_timestamp_nanos() / 1000) as i64,
    )
}

/// Country, device, OS and browser of the last pageview at or before `who.ts`.
fn last_seen(conn: &Connection, site_id: &str, who: &Who) -> anyhow::Result<Activity> {
    let mut stmt = conn.prepare(
        "SELECT country, device, os, browser, path FROM events
        WHERE site_id = $site AND kind = 'pageview' AND ts <= $ts
            AND (visitor_id = $visitor OR session_id = $session)
        ORDER BY ts DESC, path DESC LIMIT 1",
    )?;
    let row = stmt
        .query_map(
            named_params! {
                "site": site_id,
                "visitor": who.visitor_id,
                "session": who.session_id,
                "ts": micros(who.ts),
            },
            |r| {
                Ok(Activity {
                    country: r.get(0)?,
                    device: r.get(1)?,
                    os: r.get(2)?,
                    browser: r.get(3)?,
                    last_page: r.get(4)?,
                    days: Vec::new(),
                })
            },
        )?
        .next()
        .transpose()?;
    Ok(row.unwrap_or_default())
}

pub(crate) fn activity(
    conn: &Connection,
    site_id: &str,
    tz: &str,
    who: &[Who],
) -> anyhow::Result<Vec<Activity>> {
    let mut stmt = conn.prepare(
        "SELECT CAST(timezone($tz, $ts) AS DATE) - CAST(timezone($tz, ts) AS DATE) AS ago,
            count(DISTINCT session_id)
        FROM events
        WHERE site_id = $site AND kind = 'pageview' AND ts <= $ts
            AND (visitor_id = $visitor OR session_id = $session)
            AND epoch_us(ts) > epoch_us($ts) - 15 * 86400000000
        GROUP BY ago",
    )?;
    let mut out = Vec::with_capacity(who.len());
    for w in who {
        let mut a = last_seen(conn, site_id, w)?;
        a.days = vec![0; ACTIVITY_DAYS];
        let rows = stmt.query_map(
            named_params! {
                "site": site_id,
                "tz": tz,
                "visitor": w.visitor_id,
                "session": w.session_id,
                "ts": micros(w.ts),
            },
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        for row in rows {
            let (ago, n) = row?;
            if (0..ACTIVITY_DAYS as i64).contains(&ago) {
                a.days[ACTIVITY_DAYS - 1 - ago as usize] = n as u32;
            }
        }
        out.push(a);
    }
    Ok(out)
}

/// Pages by the revenue of payments whose payer viewed them last before paying; visitors count every view in range.
pub(crate) fn last_pages(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<Vec<BreakdownRow>> {
    let sql = format!(
        "WITH {BOUNDS},
        last AS (
            SELECT {NET_USD} AS usd, {CUSTOMER} AS customer,
                (SELECT e.path FROM events e
                 WHERE e.site_id = p.site_id AND e.kind = 'pageview' AND e.ts <= p.ts
                    AND (e.visitor_id = p.visitor_id OR e.session_id = a.session_id)
                 ORDER BY e.ts DESC, e.path DESC LIMIT 1) AS path
            FROM payments p
            JOIN bounds ON p.ts >= lo AND p.ts < hi
            {ATTRIBUTION_JOIN}
            WHERE p.site_id = $site AND p.status <> 'failed'
        ),
        seen AS (
            SELECT path, count(DISTINCT visitor_id) AS visitors FROM events, bounds
            WHERE site_id = $site AND kind = 'pageview' AND ts >= lo AND ts < hi
            GROUP BY path
        )
        SELECT l.path, coalesce(any_value(s.visitors), 0)::BIGINT, count(*), coalesce(sum(l.usd), 0),
            count(DISTINCT l.customer)
        FROM last l LEFT JOIN seen s ON s.path = l.path
        WHERE l.path IS NOT NULL
        GROUP BY l.path"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            let path: String = r.get(0)?;
            let visitors = r.get::<_, i64>(1)? as u64;
            let revenue_usd: f64 = r.get(3)?;
            Ok(BreakdownRow {
                key: Some(path.clone()),
                label: path,
                unattributed: false,
                visitors,
                payments: r.get::<_, i64>(2)? as u64,
                customers: r.get::<_, i64>(4)? as u64,
                revenue_usd,
                revenue_per_visitor: ratio(revenue_usd, visitors),
            })
        },
    )?;
    let mut out: Vec<BreakdownRow> = rows.collect::<Result<_, _>>()?;
    sort_rows(&mut out);
    Ok(out)
}

/// A goal and how often it was completed in a range.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalCount {
    pub name: String,
    pub completions: u64,
}

pub(crate) fn goal_counts(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<Vec<GoalCount>> {
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS}
        SELECT name, count(*) FROM events, bounds
        WHERE site_id = $site AND kind = 'goal' AND ts >= lo AND ts < hi
        GROUP BY name ORDER BY count(*) DESC, name"
    ))?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            Ok(GoalCount {
                name: r.get(0)?,
                completions: r.get::<_, i64>(1)? as u64,
            })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// One completion of a goal, with the session it happened in and what the visitor has spent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalRow {
    pub visitor_id: String,
    pub session_id: String,
    pub ts: OffsetDateTime,
    pub channel: String,
    pub referrer_host: Option<String>,
    pub source: String,
    /// The visitor's first pageview.
    pub first_ts: Option<OffsetDateTime>,
    /// Net USD the visitor has paid so far.
    pub spent_usd: f64,
    /// "provider:customer_id" of the visitor's first payment, if any.
    pub customer: Option<String>,
}

pub(crate) fn goal_list(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    goal: &str,
    limit: u32,
    offset: u32,
) -> anyhow::Result<(u64, Vec<GoalRow>)> {
    let total: i64 = conn.query_row(
        &format!(
            "WITH {BOUNDS} SELECT count(*) FROM events, bounds
            WHERE site_id = $site AND kind = 'goal' AND name = $goal AND ts >= lo AND ts < hi"
        ),
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
            "goal": goal,
        },
        |r| r.get(0),
    )?;
    let params = named_params! {
        "site": site_id,
        "tz": period.tz,
        "from": period.from.to_string(),
        "to": period.to.to_string(),
        "goal": goal,
        "limit": limit as i64,
        "offset": offset as i64,
    };
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS},
        done AS (
            SELECT visitor_id, session_id, ts, channel, referrer_host FROM events, bounds
            WHERE site_id = $site AND kind = 'goal' AND name = $goal AND ts >= lo AND ts < hi
            ORDER BY ts DESC, session_id LIMIT $limit OFFSET $offset
        )
        SELECT d.visitor_id, d.session_id, epoch_us(d.ts),
            coalesce(l.channel, d.channel), CASE WHEN l.session_id IS NULL THEN d.referrer_host ELSE l.referrer_host END,
            (SELECT epoch_us(min(e.ts)) FROM events e
                WHERE e.site_id = $site AND e.kind = 'pageview' AND e.visitor_id = d.visitor_id),
            (SELECT coalesce(sum({NET_USD}), 0) FROM payments p
                WHERE p.site_id = $site AND p.visitor_id = d.visitor_id AND p.status <> 'failed'),
            (SELECT p.provider || ':' || p.customer_id FROM payments p
                WHERE p.site_id = $site AND p.visitor_id = d.visitor_id AND p.customer_id IS NOT NULL
                ORDER BY p.ts LIMIT 1)
        FROM done d
        LEFT JOIN LATERAL (
            SELECT session_id, channel, referrer_host FROM events e
            WHERE e.site_id = $site AND e.session_id = d.session_id AND e.kind = 'pageview'
            ORDER BY e.ts, e.path LIMIT 1
        ) l ON true
        ORDER BY d.ts DESC, d.session_id"
    ))?;
    let rows = stmt.query_map(params, |r| {
        let channel: String = r.get(3)?;
        let referrer_host: Option<String> = r.get(4)?;
        Ok(GoalRow {
            visitor_id: r.get(0)?,
            session_id: r.get(1)?,
            ts: from_micros(r.get(2)?),
            source: source(referrer_host.as_deref(), &channel),
            channel,
            referrer_host,
            first_ts: r.get::<_, Option<i64>>(5)?.map(from_micros),
            spent_usd: r.get(6)?,
            customer: r.get(7)?,
        })
    })?;
    Ok((total as u64, rows.collect::<Result<_, _>>()?))
}

pub(crate) fn payment_summary(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<PaymentSummary> {
    let sql = format!(
        "WITH {BOUNDS}
        SELECT count(*), count(a.session_id), count(*) FILTER (WHERE a.session_id IS NULL),
            count(*) FILTER (WHERE p.refunded > 0), count(DISTINCT {CUSTOMER}),
            coalesce(sum({NET_USD}), 0),
            coalesce(sum(p.amount_usd * p.refunded / nullif(p.amount, 0)), 0)
        FROM payments p
        JOIN bounds ON p.ts >= lo AND p.ts < hi
        {ATTRIBUTION_JOIN}
        WHERE p.site_id = $site AND p.status <> 'failed'"
    );
    let row: (i64, i64, i64, i64, i64, f64, f64) = conn.query_row(
        &sql,
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        },
    )?;
    Ok(PaymentSummary {
        payments: row.0 as u64,
        attributed: row.1 as u64,
        unattributed: row.2 as u64,
        refunded: row.3 as u64,
        customers: row.4 as u64,
        net_usd: row.5,
        refunded_usd: row.6,
    })
}

fn channel_sources(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    let sql = format!(
        "WITH {BOUNDS},
        touches AS (
            SELECT visitor_id, channel, referrer_host
            FROM events, bounds
            WHERE site_id = $site AND kind = 'pageview' AND ts >= lo AND ts < hi
            QUALIFY row_number() OVER (PARTITION BY session_id ORDER BY ts, path) = 1
        )
        SELECT channel, referrer_host, count(DISTINCT visitor_id) AS n
        FROM touches WHERE referrer_host IS NOT NULL
        GROUP BY ALL
        ORDER BY channel, n DESC, referrer_host"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )?;
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for row in rows {
        let (channel, host) = row?;
        if out.last().is_none_or(|(c, _)| *c != channel) {
            out.push((channel, Vec::new()));
        }
        let names = &mut out.last_mut().unwrap().1;
        let name = source_name(&host);
        if names.len() < 3 && !names.contains(&name) {
            names.push(name);
        }
    }
    Ok(out)
}

fn setup(conn: &Connection, site_id: &str) -> anyhow::Result<Setup> {
    let (first, has_goals, payments): (Option<i64>, bool, i64) = conn.query_row(
        "SELECT
            (SELECT epoch_us(min(ts)) FROM events WHERE site_id = $site),
            EXISTS (SELECT 1 FROM events WHERE site_id = $site AND kind = 'goal'),
            (SELECT count(*) FROM payments WHERE site_id = $site AND status <> 'failed')",
        named_params! { "site": site_id },
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {PAYMENT_COLUMNS} FROM payments p {ATTRIBUTION_JOIN} {FIRST_TOUCH_JOIN}
        WHERE p.site_id = $site AND p.status <> 'failed' AND a.session_id IS NOT NULL
        ORDER BY p.ts, p.provider, p.provider_payment_id
        LIMIT 1"
    ))?;
    let first_match = stmt
        .query_map(named_params! { "site": site_id }, payment_row)?
        .next()
        .transpose()?;
    Ok(Setup {
        first_event: first.map(from_micros),
        has_goals,
        payments: payments as u64,
        first_match,
    })
}

pub(crate) fn report(conn: &Connection, site_id: &str, period: &Period) -> anyhow::Result<Report> {
    let dims = |d| breakdown(conn, site_id, period, d);
    Ok(Report {
        overview: overview(conn, site_id, period)?,
        days: timeseries(conn, site_id, period)?,
        previous_days: timeseries(conn, site_id, &period.previous())?,
        channels: dims(Dimension::Channel)?,
        referrers: dims(Dimension::Referrer)?,
        landing_pages: dims(Dimension::LandingPage)?,
        countries: dims(Dimension::Country)?,
        channel_sources: channel_sources(conn, site_id, period)?,
        setup: setup(conn, site_id)?,
    })
}

#[cfg(test)]
mod tests {
    use time::macros::{date, datetime};

    use super::*;
    use crate::events::{DuckStore, Event, EventStore, Payment};

    const T: OffsetDateTime = datetime!(2026-10-01 12:00 UTC);

    fn store() -> (tempfile::TempDir, DuckStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        (dir, store)
    }

    /// One pageview starting session `session` of `visitor` at `ts` from `channel`.
    fn visit(visitor: &str, session: &str, ts: OffsetDateTime, channel: &str) -> Event {
        let referrer_host = match channel {
            "Social" => Some("news.ycombinator.com".to_string()),
            "Referral" => Some("someblog.dev".to_string()),
            _ => None,
        };
        Event {
            site_id: "s".into(),
            ts,
            visitor_id: visitor.into(),
            session_id: session.into(),
            kind: "pageview".into(),
            name: None,
            path: format!("/{session}"),
            referrer_host,
            utm_source: None,
            utm_medium: None,
            utm_campaign: (channel == "Paid").then(|| "launch".to_string()),
            utm_content: None,
            utm_term: None,
            channel: channel.into(),
            country: Some("DE".into()),
            device: "Desktop".into(),
            browser: "Firefox".into(),
            os: "macOS".into(),
            region: None,
            city: None,
            host: None,
        }
    }

    fn pay(id: &str, ts: OffsetDateTime, visitor: Option<&str>, customer: Option<&str>) -> Payment {
        Payment {
            site_id: "s".into(),
            provider: "stripe".into(),
            provider_payment_id: id.into(),
            ts,
            amount: 10000,
            currency: "usd".into(),
            amount_usd: Some(100.0),
            refunded: 0,
            status: "paid".into(),
            visitor_id: visitor.map(Into::into),
            customer_id: customer.map(Into::into),
            is_renewal: false,
        }
    }

    fn period() -> Period {
        Period {
            from: date!(2026 - 09 - 01),
            to: date!(2026 - 10 - 31),
            tz: "UTC".into(),
        }
    }

    fn touches(store: &DuckStore) -> Vec<(String, Option<String>)> {
        store
            .payment_list("s", &period(), PaymentFilter::All, 100, 0)
            .unwrap()
            .rows
            .into_iter()
            .map(|p| (p.provider_payment_id, p.touch.map(|t| t.session_id)))
            .collect()
    }

    fn credited(store: &DuckStore) -> Option<String> {
        touches(store).into_iter().next().unwrap().1
    }

    #[test]
    fn last_non_direct_and_first_touch() {
        let (_dir, store) = store();
        store
            .insert_events(&[
                visit("v1", "social", T - Duration::days(10), "Social"),
                visit("v1", "referral", T - Duration::days(5), "Referral"),
                visit("v1", "direct", T - Duration::days(1), "Direct"),
                visit("v1", "after", T + Duration::hours(1), "Paid"),
            ])
            .unwrap();
        store
            .upsert_payments(&[pay("p1", T, Some("v1"), None)])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        assert_eq!(credited(&store).as_deref(), Some("referral"));
        store.rebuild_attribution("s", "first_touch", 90).unwrap();
        assert_eq!(credited(&store).as_deref(), Some("social"));
        let touch = store
            .payment_list("s", &period(), PaymentFilter::All, 1, 0)
            .unwrap()
            .rows[0]
            .touch
            .clone()
            .unwrap();
        assert_eq!(touch.source, "Hacker News");
        assert_eq!(touch.landing_page.as_deref(), Some("/social"));
        assert!(store.rebuild_attribution("s", "linear", 90).is_err());
    }

    #[test]
    fn all_direct_falls_back_to_latest_direct() {
        let (_dir, store) = store();
        store
            .insert_events(&[
                visit("v1", "d1", T - Duration::days(10), "Direct"),
                visit("v1", "d2", T - Duration::days(2), "Direct"),
            ])
            .unwrap();
        store
            .upsert_payments(&[pay("p1", T, Some("v1"), None)])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        assert_eq!(credited(&store).as_deref(), Some("d2"));
    }

    #[test]
    fn sessions_outside_window_are_ignored() {
        let (_dir, store) = store();
        store
            .insert_events(&[
                visit("v1", "old", T - Duration::days(40), "Social"),
                visit("v1", "recent", T - Duration::days(1), "Direct"),
            ])
            .unwrap();
        store
            .upsert_payments(&[pay("p1", T, Some("v1"), None)])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 30)
            .unwrap();
        assert_eq!(credited(&store).as_deref(), Some("recent"));
        store
            .rebuild_attribution("s", "last_non_direct", 60)
            .unwrap();
        assert_eq!(credited(&store).as_deref(), Some("old"));
    }

    #[test]
    fn renewals_inherit_first_payment() {
        let (_dir, store) = store();
        store
            .insert_events(&[
                visit("v1", "first", T - Duration::days(3), "Social"),
                visit("v2", "other", T + Duration::days(29), "Referral"),
            ])
            .unwrap();
        let mut renewal = pay("p2", T + Duration::days(30), Some("v2"), Some("cus_1"));
        renewal.is_renewal = true;
        let mut orphan = pay("p3", T + Duration::days(30), None, Some("cus_2"));
        orphan.is_renewal = true;
        store
            .upsert_payments(&[pay("p1", T, Some("v1"), Some("cus_1")), renewal, orphan])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let mut got = touches(&store);
        got.sort();
        assert_eq!(
            got,
            vec![
                ("p1".into(), Some("first".into())),
                ("p2".into(), Some("first".into())),
                ("p3".into(), None),
            ]
        );
    }

    #[test]
    fn renewals_copy_the_models_choice_for_the_first_payment() {
        let (_dir, store) = store();
        store
            .insert_events(&[
                visit("v1", "social", T - Duration::days(10), "Social"),
                visit("v1", "referral", T - Duration::days(2), "Referral"),
                visit("v1", "direct", T - Duration::days(1), "Direct"),
            ])
            .unwrap();
        let mut renewal = pay("p2", T + Duration::days(29), None, Some("cus_1"));
        renewal.is_renewal = true;
        store
            .upsert_payments(&[pay("p1", T, Some("v1"), Some("cus_1")), renewal])
            .unwrap();
        for (model, session) in [("last_non_direct", "referral"), ("first_touch", "social")] {
            store.rebuild_attribution("s", model, 90).unwrap();
            let mut got = touches(&store);
            got.sort();
            let want = Some(session.to_string());
            assert_eq!(
                got,
                vec![("p1".into(), want.clone()), ("p2".into(), want)],
                "{model}"
            );
        }
    }

    #[test]
    fn renewal_of_unattributed_first_payment_stays_unattributed() {
        let (_dir, store) = store();
        store
            .insert_events(&[visit("v1", "s1", T - Duration::days(1), "Social")])
            .unwrap();
        let mut renewal = pay("p2", T + Duration::days(29), None, Some("cus_1"));
        renewal.is_renewal = true;
        store
            .upsert_payments(&[pay("p1", T, None, Some("cus_1")), renewal])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        assert!(touches(&store).iter().all(|(_, t)| t.is_none()));
        let r = store.report("s", &period()).unwrap();
        let unattributed = r.channels.iter().find(|c| c.unattributed).unwrap();
        assert_eq!(
            (unattributed.payments, unattributed.revenue_usd),
            (2, 200.0)
        );
        assert!(r.channels.iter().all(|c| c.unattributed || c.payments == 0));
    }

    #[test]
    fn same_customer_on_two_providers_is_two_customers() {
        let (_dir, store) = store();
        store
            .insert_events(&[visit("v1", "s1", T - Duration::days(1), "Social")])
            .unwrap();
        let mut moved = pay("p2", T + Duration::days(29), None, Some("cus_1"));
        moved.provider = "polar".into();
        moved.is_renewal = true;
        store
            .upsert_payments(&[pay("p1", T, Some("v1"), Some("cus_1")), moved])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let mut got = touches(&store);
        got.sort();
        assert_eq!(
            got,
            vec![("p1".into(), Some("s1".into())), ("p2".into(), None)]
        );
        assert_eq!(store.overview("s", &period()).unwrap().current.customers, 2);
        assert_eq!(store.payment_summary("s", &period()).unwrap().customers, 2);
    }

    #[test]
    fn unattributed_payments_still_count() {
        let (_dir, store) = store();
        store
            .insert_events(&[visit("v1", "s1", T - Duration::days(1), "Social")])
            .unwrap();
        store
            .upsert_payments(&[
                pay("p1", T, Some("v1"), None),
                pay("p2", T, None, Some("cus_9")),
                pay("p3", T, Some("never-seen"), None),
            ])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let page = store
            .payment_list("s", &period(), PaymentFilter::Unattributed, 10, 0)
            .unwrap();
        assert_eq!(page.total, 2);
        let o = store.overview("s", &period()).unwrap();
        assert_eq!(o.current.payments, 3);
        assert_eq!(o.current.revenue_usd, 300.0);
        assert_eq!(o.current.customers, 3);
        assert_eq!(o.current.visitors, 1);
    }

    #[test]
    fn refunds_reduce_net_revenue() {
        let (_dir, store) = store();
        let mut partial = pay("p1", T, None, None);
        partial.refunded = 2500;
        let mut full = pay("p2", T, None, None);
        full.refunded = 10000;
        let mut no_fx = pay("p3", T, None, None);
        no_fx.amount_usd = None;
        store.upsert_payments(&[partial, full, no_fx]).unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let o = store.overview("s", &period()).unwrap();
        assert_eq!(o.current.payments, 3);
        assert_eq!(o.current.revenue_usd, 75.0);
        let refunded = store
            .payment_list("s", &period(), PaymentFilter::Refunded, 10, 0)
            .unwrap();
        assert_eq!(refunded.total, 2);
        let net: Vec<_> = refunded.rows.iter().map(|p| p.net_usd).collect();
        assert!(net.contains(&Some(75.0)) && net.contains(&Some(0.0)));
    }

    #[test]
    fn channel_breakdown_sums_to_overview() {
        let (_dir, store) = store();
        store
            .insert_events(&[
                visit("v1", "a", T - Duration::days(2), "Social"),
                visit("v2", "b", T - Duration::days(2), "Paid"),
                visit("v3", "c", T - Duration::days(2), "Direct"),
                visit("v4", "d", T - Duration::days(2), "Social"),
            ])
            .unwrap();
        let mut refunded = pay("p2", T, Some("v2"), None);
        refunded.refunded = 5000;
        let mut no_fx = pay("p4", T, Some("v1"), None);
        no_fx.amount_usd = None;
        store
            .upsert_payments(&[
                pay("p1", T, Some("v1"), None),
                refunded,
                pay("p3", T, None, None),
                no_fx,
            ])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let o = store.overview("s", &period()).unwrap();
        let rows = store.breakdown("s", &period(), Dimension::Channel).unwrap();
        let revenue: f64 = rows.iter().map(|r| r.revenue_usd).sum();
        let payments: u64 = rows.iter().map(|r| r.payments).sum();
        assert_eq!(revenue, o.current.revenue_usd);
        assert_eq!(payments, o.current.payments);
        assert_eq!(rows[0].label, "Social");
        assert_eq!(rows[0].visitors, 2);
        assert_eq!(rows[0].revenue_per_visitor, Some(50.0));
        assert!(
            rows.iter()
                .any(|r| r.unattributed && r.revenue_usd == 100.0)
        );
        let referrers = store
            .breakdown("s", &period(), Dimension::Referrer)
            .unwrap();
        let mut referrers = referrers;
        referrers.sort_by_key(|r| std::cmp::Reverse(r.visitors));
        assert_eq!(referrers[0].label, "news.ycombinator.com");
    }

    #[test]
    fn referrer_aliases_share_one_domain_row() {
        let (_dir, store) = store();
        let mut events = Vec::new();
        for (i, host) in ["t.co", "twitter.com", "www.x.com"].iter().enumerate() {
            let mut e = visit(&format!("v{i}"), &format!("s{i}"), T, "Social");
            e.referrer_host = Some((*host).to_string());
            events.push(e);
        }
        store.insert_events(&events).unwrap();
        let referrers = store
            .breakdown("s", &period(), Dimension::Referrer)
            .unwrap();
        let mut referrers = referrers;
        referrers.sort_by_key(|r| std::cmp::Reverse(r.visitors));
        assert_eq!(referrers.len(), 1);
        assert_eq!(referrers[0].label, "x.com");
        assert_eq!(referrers[0].visitors, 3);
    }

    #[test]
    fn ai_referrers_carry_revenue() {
        let (_dir, store) = store();
        let mut events = Vec::new();
        for (i, host) in ["chatgpt.com", "chat.openai.com", "claude.ai"]
            .iter()
            .enumerate()
        {
            let mut e = visit(&format!("v{i}"), &format!("s{i}"), T, "AI");
            e.referrer_host = Some((*host).to_string());
            events.push(e);
        }
        store.insert_events(&events).unwrap();
        store
            .upsert_payments(&[
                pay("p1", T, Some("v0"), None),
                pay("p2", T, Some("v1"), None),
                pay("p3", T, Some("v2"), None),
            ])
            .unwrap();
        store.rebuild_attribution("s", "first_touch", 90).unwrap();
        let referrers = store
            .breakdown("s", &period(), Dimension::Referrer)
            .unwrap();
        let row = |label: &str| referrers.iter().find(|r| r.label == label).unwrap();
        assert_eq!(
            (row("chatgpt.com").visitors, row("chatgpt.com").revenue_usd),
            (2, 200.0)
        );
        assert_eq!(row("claude.ai").revenue_usd, 100.0);
    }

    #[test]
    fn journey_lists_sessions_in_order() {
        let (_dir, store) = store();
        let mut goal = visit(
            "v1",
            "s2",
            T - Duration::days(1) + Duration::minutes(2),
            "Direct",
        );
        goal.kind = "goal".into();
        goal.name = Some("signup".into());
        let mut second_page = visit(
            "v1",
            "s2",
            T - Duration::days(1) + Duration::minutes(1),
            "Direct",
        );
        second_page.path = "/pricing".into();
        store
            .insert_events(&[
                goal,
                second_page,
                visit("v1", "s2", T - Duration::days(1), "Direct"),
                visit("v1", "s1", T - Duration::days(4), "Social"),
                visit("v1", "s0", T - Duration::days(200), "Paid"),
            ])
            .unwrap();
        store
            .upsert_payments(&[pay("p1", T, Some("v1"), None)])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let j = store.journey("s", "stripe", "p1", 90).unwrap().unwrap();
        let ids: Vec<_> = j.sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, ["s1", "s2"]);
        assert!(j.sessions[0].credited && !j.sessions[1].credited);
        assert_eq!(j.sessions[0].source, "Hacker News");
        assert_eq!(j.sessions[1].paths, ["/s2", "/pricing"]);
        assert_eq!(j.sessions[1].goals, ["signup"]);
        assert_eq!(j.last_page.as_deref(), Some("/pricing"));
        assert!(store.journey("s", "stripe", "nope", 90).unwrap().is_none());
    }

    #[test]
    fn last_pages_credit_the_page_viewed_before_paying() {
        let (_dir, store) = store();
        let page = |v: &str, s: &str, ts, path: &str| {
            let mut e = visit(v, s, ts, "Direct");
            e.path = path.into();
            e
        };
        store
            .insert_events(&[
                page("v1", "a", T - Duration::hours(2), "/"),
                page("v1", "a", T - Duration::hours(1), "/pricing"),
                page("v1", "a", T + Duration::minutes(5), "/thanks"),
                page("v2", "b", T - Duration::hours(1), "/pricing"),
                page("v3", "c", T - Duration::hours(3), "/compare"),
            ])
            .unwrap();
        store
            .upsert_payments(&[
                pay("p1", T, Some("v1"), None),
                pay("p2", T, Some("v3"), None),
                pay("p3", T, None, None),
            ])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let rows = store.last_pages("s", &period()).unwrap();
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.label.as_str(), r.visitors, r.payments, r.revenue_usd))
            .collect();
        assert_eq!(got, [("/pricing", 2, 1, 100.0), ("/compare", 1, 1, 100.0)]);
        assert_eq!(rows[0].revenue_per_visitor, Some(50.0));
    }

    #[test]
    fn report_loads_everything_for_a_range() {
        let (_dir, store) = store();
        let mut goal = visit("v3", "c", T, "Direct");
        goal.kind = "goal".into();
        goal.name = Some("signup".into());
        store
            .insert_events(&[
                visit("v1", "a", T - Duration::days(2), "Social"),
                visit("v1", "a2", T - Duration::hours(1), "Direct"),
                visit("v2", "b", T - Duration::days(2), "Paid"),
                visit("v3", "c", T, "Direct"),
                goal,
            ])
            .unwrap();
        store
            .upsert_payments(&[pay("p1", T, Some("v1"), None), pay("p2", T, None, None)])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let r = store.report("s", &period()).unwrap();
        assert_eq!(r.overview.current.sessions, 4);
        assert_eq!(r.overview.current.conversion, Some(1.0 / 3.0));
        assert_eq!(r.days.len(), 61);
        assert_eq!(r.previous_days.len(), 61);
        assert_eq!(r.previous_days[0].date, date!(2026 - 07 - 02));
        let day = r.days.iter().find(|d| d.date == T.date()).unwrap();
        assert_eq!(
            day.by_channel,
            vec![(None, 100.0), (Some("Social".to_string()), 100.0)]
        );
        let split: f64 = day.by_channel.iter().map(|c| c.1).sum();
        assert_eq!(split, day.revenue_usd);
        assert_eq!((day.visitors, day.customers), (2, 2));
        assert_eq!(day.conversion, Some(0.5));
        assert_eq!(day.revenue_per_visitor, Some(50.0), "matched revenue only");
        let cur = &r.overview.current;
        assert_eq!((cur.converted, cur.matched_payments), (1, 1));
        assert_eq!(cur.revenue_per_visitor, Some(100.0 / 3.0));
        assert_eq!(r.channels[0].label, "Social");
        assert_eq!(r.referrers[0].label, "news.ycombinator.com");
        assert!(r.landing_pages.iter().any(|p| p.label == "/a"));
        assert_eq!(r.countries[0].key.as_deref(), Some("DE"));
        assert!(
            r.channel_sources
                .contains(&("Social".into(), vec!["Hacker News".into()]))
        );
        assert_eq!(r.setup.first_event, Some(T - Duration::days(2)));
        assert!(r.setup.has_goals);
        assert_eq!(r.setup.payments, 2);
        assert_eq!(
            r.setup
                .first_match
                .map(|p| p.provider_payment_id)
                .as_deref(),
            Some("p1")
        );
    }

    #[test]
    fn empty_site_has_no_setup_facts() {
        let (_dir, store) = store();
        let r = store.report("s", &period()).unwrap();
        assert_eq!(r.setup.first_event, None);
        assert!(!r.setup.has_goals && r.setup.first_match.is_none());
        assert!(r.days.iter().all(|d| d.conversion.is_none()));
    }

    #[test]
    fn payments_carry_first_touch_and_journey_device() {
        let (_dir, store) = store();
        let mut later = visit("v1", "s2", T - Duration::days(1), "Referral");
        later.browser = "Chrome".into();
        later.country = Some("IN".into());
        store
            .insert_events(&[visit("v1", "s1", T - Duration::days(6), "Social"), later])
            .unwrap();
        let mut renewal = pay("p2", T + Duration::days(30), None, Some("cus_1"));
        renewal.is_renewal = true;
        store
            .upsert_payments(&[
                pay("p1", T, Some("v1"), Some("cus_1")),
                renewal,
                pay("p3", T, None, None),
            ])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let rows = store
            .payment_list("s", &period(), PaymentFilter::All, 10, 0)
            .unwrap()
            .rows;
        let first = |id: &str| {
            rows.iter()
                .find(|p| p.provider_payment_id == id)
                .unwrap()
                .first_touch
                .as_ref()
                .map(|f| f.source.clone())
        };
        assert_eq!(first("p1").as_deref(), Some("Hacker News"));
        assert_eq!(first("p2").as_deref(), Some("Hacker News"));
        assert_eq!(first("p3"), None);
        let j = store.journey("s", "stripe", "p1", 90).unwrap().unwrap();
        assert_eq!(j.country.as_deref(), Some("IN"));
        assert_eq!(j.browser.as_deref(), Some("Chrome"));
        assert_eq!(j.device.as_deref(), Some("Desktop"));
        assert_eq!(j.sessions.len(), 2);
    }

    #[test]
    fn payment_summary_matches_filters() {
        let (_dir, store) = store();
        store
            .insert_events(&[visit("v1", "s1", T - Duration::days(1), "Social")])
            .unwrap();
        let mut partial = pay("p2", T, None, Some("cus_1"));
        partial.refunded = 2500;
        let mut full = pay("p3", T, None, Some("cus_1"));
        full.refunded = 10000;
        store
            .upsert_payments(&[pay("p1", T, Some("v1"), None), partial, full])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let s = store.payment_summary("s", &period()).unwrap();
        assert_eq!(
            (s.payments, s.attributed, s.unattributed, s.refunded),
            (3, 1, 2, 2)
        );
        assert_eq!(s.customers, 2);
        assert_eq!((s.net_usd, s.refunded_usd), (175.0, 125.0));
        for (filter, n) in [
            (PaymentFilter::Attributed, s.attributed),
            (PaymentFilter::Unattributed, s.unattributed),
            (PaymentFilter::Refunded, s.refunded),
        ] {
            assert_eq!(
                store
                    .payment_list("s", &period(), filter, 0, 0)
                    .unwrap()
                    .total,
                n
            );
        }
    }

    #[test]
    fn days_follow_site_timezone() {
        let (_dir, store) = store();
        store
            .insert_events(&[visit("v1", "s1", datetime!(2026-10-01 20:00 UTC), "Direct")])
            .unwrap();
        store
            .upsert_payments(&[pay("p1", datetime!(2026-10-01 20:00 UTC), None, None)])
            .unwrap();
        let p = Period {
            from: date!(2026 - 10 - 01),
            to: date!(2026 - 10 - 02),
            tz: "Asia/Kolkata".into(),
        };
        let days = store.timeseries("s", &p).unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!((days[0].visitors, days[0].revenue_usd), (0, 0.0));
        assert_eq!(
            (days[1].date, days[1].visitors, days[1].revenue_usd),
            (date!(2026 - 10 - 02), 1, 100.0)
        );
        assert_eq!(p.previous().from, date!(2026 - 09 - 29));
    }

    #[test]
    fn activity_counts_sessions_per_day_before_the_payment() {
        let (_dir, store) = store();
        let mut late = visit("v1", "c", T - Duration::hours(1), "Direct");
        late.os = "iOS".into();
        late.device = "Mobile".into();
        store
            .insert_events(&[
                visit("v1", "a", T - Duration::days(20), "Social"),
                visit("v1", "b", T - Duration::days(3), "Referral"),
                late,
                visit("v1", "d", T + Duration::hours(1), "Direct"),
            ])
            .unwrap();
        let who = Who {
            visitor_id: Some("v1".into()),
            session_id: None,
            ts: T,
        };
        let nobody = Who {
            visitor_id: None,
            session_id: None,
            ts: T,
        };
        let got = store.activity("s", "UTC", &[who, nobody]).unwrap();
        let mut want = vec![0; ACTIVITY_DAYS];
        want[ACTIVITY_DAYS - 4] = 1;
        want[ACTIVITY_DAYS - 1] = 1;
        assert_eq!(got[0].days, want, "older and later visits are left out");
        assert_eq!(got[0].os.as_deref(), Some("iOS"));
        assert_eq!(got[0].device.as_deref(), Some("Mobile"));
        assert_eq!(got[0].country.as_deref(), Some("DE"));
        assert_eq!(got[1].days, vec![0; ACTIVITY_DAYS]);
        assert_eq!(got[1].os, None);
    }

    #[test]
    fn goal_completions_list_source_and_spend() {
        let (_dir, store) = store();
        let mut goal = visit("v1", "b", T + Duration::minutes(5), "Direct");
        goal.kind = "goal".into();
        goal.name = Some("signup".into());
        goal.referrer_host = None;
        store
            .insert_events(&[
                visit("v1", "a", T - Duration::days(2), "Direct"),
                visit("v1", "b", T, "Social"),
                goal,
            ])
            .unwrap();
        store
            .upsert_payments(&[pay("p1", T + Duration::hours(1), Some("v1"), None)])
            .unwrap();
        let counts = store.goal_counts("s", &period()).unwrap();
        assert_eq!(
            counts,
            vec![GoalCount {
                name: "signup".into(),
                completions: 1
            }]
        );
        let (total, rows) = store.goal_list("s", &period(), "signup", 10, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(
            rows[0].source, "Hacker News",
            "the session's landing source"
        );
        assert_eq!(rows[0].first_ts, Some(T - Duration::days(2)));
        assert_eq!(rows[0].spent_usd, 100.0);
        assert!(
            store
                .goal_list("s", &period(), "nope", 10, 0)
                .unwrap()
                .1
                .is_empty()
        );
    }

    #[test]
    fn breakdowns_cover_every_card_tab() {
        let (_dir, store) = store();
        let mut a = visit("v1", "a", T, "Social");
        a.utm_campaign = Some("launch".into());
        a.utm_term = Some("invoices".into());
        a.region = Some("Bavaria".into());
        a.city = Some("Munich".into());
        a.host = Some("docs.example.com".into());
        a.os = "iOS".into();
        let mut second = visit("v1", "a", T + Duration::minutes(1), "Social");
        second.path = "/pricing".into();
        let mut exit = visit("v1", "a", T + Duration::minutes(2), "Social");
        exit.kind = "exit".into();
        exit.name = Some("github.com/acme".into());
        let b = visit("v2", "b", T, "Direct");
        store.insert_events(&[a, second, exit, b]).unwrap();
        store
            .upsert_payments(&[pay("p1", T + Duration::hours(1), Some("v1"), Some("c1"))])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 30)
            .unwrap();
        let rows = |d| store.breakdown("s", &period(), d).unwrap();
        let find = |d, key: &str| {
            rows(d)
                .into_iter()
                .find(|r| r.key.as_deref() == Some(key))
                .unwrap_or_else(|| panic!("{d:?} {key}"))
        };
        let campaign = find(Dimension::Campaign, "launch");
        assert_eq!((campaign.visitors, campaign.customers), (1, 1));
        assert_eq!(campaign.revenue_usd, 100.0);
        assert_eq!(find(Dimension::Keyword, "invoices").visitors, 1);
        assert_eq!(find(Dimension::Page, "/pricing").revenue_usd, 100.0);
        assert_eq!(find(Dimension::LandingPage, "/a").revenue_usd, 100.0);
        assert!(
            rows(Dimension::LandingPage)
                .iter()
                .all(|r| r.key.as_deref() != Some("/pricing"))
        );
        assert_eq!(find(Dimension::Os, "iOS").visitors, 1);
        assert_eq!(find(Dimension::Os, "macOS").revenue_usd, 0.0);
        assert_eq!(
            find(Dimension::ChannelSource, "Social|news.ycombinator.com").revenue_usd,
            100.0
        );
        assert_eq!(find(Dimension::ChannelSource, "Direct|").visitors, 1);
    }
}
