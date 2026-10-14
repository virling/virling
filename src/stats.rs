use duckdb::{Connection, named_params};
use serde::Serialize;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};

use crate::channel::source_name;

/// Net USD of a payment `p`; NULL when it has no USD conversion, so it counts as a payment but not revenue.
const NET_USD: &str = "p.amount_usd * (p.amount - p.refunded) / nullif(p.amount, 0)";

/// Who paid: customer_id, else visitor_id, else the payment itself.
const CUSTOMER: &str = "coalesce('c:' || p.customer_id, 'v:' || p.visitor_id, 'p:' || p.provider || ':' || p.provider_payment_id)";

/// Local dates `$from..=$to` in IANA zone `$tz` as a half-open UTC range [lo, hi).
const BOUNDS: &str = "bounds AS (
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
}

impl Dimension {
    /// (events column, attribution column)
    fn columns(self) -> (&'static str, &'static str) {
        match self {
            Dimension::Channel => ("channel", "channel"),
            Dimension::Referrer => ("referrer_host", "referrer_host"),
            Dimension::LandingPage => ("path", "landing_page"),
            Dimension::Country => ("country", "country"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Revenue,
    Visitors,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BreakdownRow {
    /// Raw value (source name for referrers); None for "(none)" and for the Unattributed row.
    pub key: Option<String>,
    pub label: String,
    pub unattributed: bool,
    pub visitors: u64,
    pub payments: u64,
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
    pub ts: OffsetDateTime,
    pub channel: String,
    pub referrer_host: Option<String>,
    pub source: String,
}

/// The session credited for a payment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Touch {
    pub session_id: String,
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

fn source(referrer_host: Option<&str>, channel: &str) -> String {
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
    sort: Sort,
) -> anyhow::Result<Vec<BreakdownRow>> {
    let (ecol, acol) = dimension.columns();
    let sql = format!(
        "WITH {BOUNDS},
        touches AS (
            SELECT visitor_id, {ecol} AS key
            FROM events, bounds
            WHERE site_id = $site AND kind = 'pageview' AND ts >= lo AND ts < hi
            QUALIFY row_number() OVER (PARTITION BY session_id ORDER BY ts, path) = 1
        )
        SELECT unattributed, key, sum(visitors)::BIGINT, sum(payments)::BIGINT, sum(revenue)
        FROM (
            SELECT false AS unattributed, key, count(DISTINCT visitor_id) AS visitors,
                   0 AS payments, 0.0 AS revenue
            FROM touches GROUP BY key
            UNION ALL
            SELECT a.session_id IS NULL, CASE WHEN a.session_id IS NOT NULL THEN a.{acol} END,
                   0, count(*), coalesce(sum({NET_USD}), 0)
            FROM payments p
            JOIN bounds ON p.ts >= lo AND p.ts < hi
            LEFT JOIN attribution a ON a.site_id = p.site_id AND a.provider = p.provider
                AND a.provider_payment_id = p.provider_payment_id
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
            ))
        },
    )?;
    let mut out: Vec<BreakdownRow> = Vec::new();
    for row in rows {
        let (unattributed, mut key, visitors, payments, revenue_usd) = row?;
        if dimension == Dimension::Referrer {
            key = key.as_deref().map(source_name);
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
            existing.revenue_usd += revenue_usd;
        } else {
            out.push(BreakdownRow {
                key,
                label,
                unattributed,
                visitors: visitors as u64,
                payments: payments as u64,
                revenue_usd,
                revenue_per_visitor: None,
            });
        }
    }
    for r in &mut out {
        r.revenue_per_visitor = ratio(r.revenue_usd, r.visitors);
    }
    sort_rows(&mut out, sort);
    Ok(out)
}

pub fn sort_rows(rows: &mut [BreakdownRow], sort: Sort) {
    rows.sort_by(|a, b| {
        let by_revenue = b.revenue_usd.total_cmp(&a.revenue_usd);
        let by_visitors = b.visitors.cmp(&a.visitors);
        match sort {
            Sort::Revenue => by_revenue.then(by_visitors),
            Sort::Visitors => by_visitors.then(by_revenue),
        }
        .then_with(|| a.label.cmp(&b.label))
    });
}

const PAYMENT_COLUMNS: &str =
    "p.provider, p.provider_payment_id, epoch_us(p.ts), p.amount, p.currency,
    p.amount_usd, p.refunded, p.status, p.visitor_id, p.customer_id, p.is_renewal,
    a.session_id, epoch_us(a.touch_ts), a.channel, a.referrer_host, a.campaign, a.landing_page,
    a.country, a.device, epoch_us(f.ts), f.channel, f.referrer_host";

const ATTRIBUTION_JOIN: &str = "LEFT JOIN attribution a ON a.site_id = p.site_id
    AND a.provider = p.provider AND a.provider_payment_id = p.provider_payment_id";

/// The first pageview of the payer, or of the credited visitor when the payment has no visitor ID.
const FIRST_TOUCH_JOIN: &str = "LEFT JOIN LATERAL (
        SELECT e.ts, e.channel, e.referrer_host FROM events e
        WHERE e.site_id = p.site_id AND e.kind = 'pageview' AND e.ts <= p.ts
            AND e.visitor_id = coalesce(p.visitor_id,
                (SELECT c.visitor_id FROM events c WHERE c.site_id = p.site_id AND c.session_id = a.session_id LIMIT 1))
        ORDER BY e.ts, e.path
        LIMIT 1
    ) f ON true";

fn payment_row(r: &duckdb::Row) -> duckdb::Result<PaymentRow> {
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
    let ts = duckdb::types::Value::Timestamp(
        duckdb::types::TimeUnit::Microsecond,
        (payment.ts.unix_timestamp_nanos() / 1000) as i64,
    );
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
    let mut stmt = conn.prepare(
        "SELECT country, browser, device FROM events
        WHERE site_id = $site AND kind = 'pageview' AND ts <= $ts
            AND (visitor_id = $visitor OR session_id = $credited)
        ORDER BY ts DESC LIMIT 1",
    )?;
    let last = stmt
        .query_map(
            named_params! {
                "site": site_id,
                "visitor": payment.visitor_id,
                "credited": credited,
                "ts": ts,
            },
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?
        .next()
        .transpose()?;
    let (country, browser, device) = last.unwrap_or((None, None, None));
    Ok(Some(Journey {
        payment,
        sessions,
        country,
        browser,
        device,
    }))
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
    let dims = |d| breakdown(conn, site_id, period, d, Sort::Revenue);
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
        let rows = store
            .breakdown("s", &period(), Dimension::Channel, Sort::Revenue)
            .unwrap();
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
            .breakdown("s", &period(), Dimension::Referrer, Sort::Visitors)
            .unwrap();
        assert_eq!(referrers[0].label, "Hacker News");
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
        assert!(store.journey("s", "stripe", "nope", 90).unwrap().is_none());
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
        assert_eq!(r.referrers[0].label, "Hacker News");
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
}
