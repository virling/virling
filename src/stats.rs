use duckdb::{Connection, named_params};
use serde::Serialize;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};

use crate::channel::{source_domain, source_name};

/// Net USD of a payment `p`; NULL when it has no USD conversion, so it counts as a payment but not revenue.
pub(crate) const NET_USD: &str =
    "round(p.amount_usd * (p.amount - p.refunded) / nullif(p.amount, 0), 2)";

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

/// Limits numbers to sessions whose first pageview matches every condition, and payments to those credited to them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    /// Column of the session's first pageview and the values it may hold; None matches NULL.
    pub first: Vec<(&'static str, Vec<Option<String>>)>,
    /// A path viewed anywhere in the session.
    pub viewed: Option<String>,
}

/// `s` as a SQL string literal.
pub(crate) fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Whether `e.path` is page `p`; a trailing `*` matches every path starting with the rest.
fn page_is(p: &str) -> String {
    match p.strip_suffix('*') {
        Some(prefix) => format!("starts_with(e.path, {})", lit(prefix)),
        None => format!("e.path = {}", lit(p)),
    }
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.first.is_empty() && self.viewed.is_none()
    }

    /// A `scope` CTE of matching session IDs, led by a comma; empty without conditions.
    pub(crate) fn cte(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut conds: Vec<String> = self
            .first
            .iter()
            .map(|(col, vals)| {
                let mut any: Vec<String> = Vec::new();
                let set: Vec<String> = vals.iter().flatten().map(|v| lit(v)).collect();
                if !set.is_empty() {
                    any.push(format!("{col} IN ({})", set.join(", ")));
                }
                if vals.iter().any(Option::is_none) {
                    any.push(format!("{col} IS NULL"));
                }
                if any.is_empty() {
                    "false".into()
                } else {
                    format!("({})", any.join(" OR "))
                }
            })
            .collect();
        if let Some(path) = &self.viewed {
            conds.push(format!(
                "session_id IN (SELECT session_id FROM events WHERE site_id = $site AND kind = 'pageview' AND path = {})",
                lit(path)
            ));
        }
        format!(
            ",
        scope AS (
            SELECT session_id FROM touches WHERE site_id = $site AND {}
        )",
            conds.join(" AND ")
        )
    }

    /// Keeps rows whose session column `col` is in scope.
    pub(crate) fn within(&self, col: &str) -> String {
        if self.is_empty() {
            String::new()
        } else {
            format!(" AND {col} IN (SELECT session_id FROM scope)")
        }
    }

    /// Keeps payments `p` credited to a session in scope.
    pub(crate) fn paid(&self) -> String {
        if self.is_empty() {
            String::new()
        } else {
            " AND EXISTS (SELECT 1 FROM attribution x WHERE x.site_id = p.site_id AND x.provider = p.provider
                AND x.provider_payment_id = p.provider_payment_id AND x.session_id IN (SELECT session_id FROM scope))".into()
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
    /// Distinct customers among first, not renewal, payments.
    pub new_customers: u64,
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
    /// Mean seconds from first to last event of visits that began in the period and have two or more events.
    pub avg_visit_secs: Option<f64>,
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
    /// Local hour of the day for hourly series.
    pub hour: Option<u8>,
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
    /// "path|channel|referrer_host", the sources of each landing page.
    PageSource,
    /// "country|city".
    CountryCity,
    /// "device|os".
    DeviceOs,
    Language,
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
            Dimension::PageSource => {
                first("path || '|' || channel || '|' || coalesce(referrer_host, '')")
            }
            Dimension::CountryCity => first("country || '|' || city"),
            Dimension::DeviceOs => first("device || '|' || os"),
            Dimension::Language => first("language"),
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
    /// Payments credited to this row that were refunded in whole or part.
    pub refunded: u64,
    pub revenue_usd: f64,
    pub revenue_per_visitor: Option<f64>,
    /// Channel rows only: as `Totals::avg_visit_secs`, over the channel's visits.
    pub avg_visit_secs: Option<f64>,
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

/// What one first payment's payer did before paying.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PathPay {
    pub days: f64,
    pub visits: u64,
    pub pages: u64,
    pub first_channel: String,
    pub credited_channel: Option<String>,
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

/// `usd` rounded to whole cents, as NET_USD rounds.
pub(crate) fn cents(usd: f64) -> f64 {
    (usd * 100.0).round() / 100.0
}

pub(crate) fn micros_of(ts: OffsetDateTime) -> i64 {
    (ts.unix_timestamp_nanos() / 1000) as i64
}

pub(crate) fn source(referrer_host: Option<&str>, channel: &str) -> String {
    referrer_host.map_or_else(|| channel.to_string(), source_name)
}

pub(crate) fn ratio(n: f64, d: u64) -> Option<f64> {
    (d > 0).then(|| n / d as f64)
}

fn totals(conn: &Connection, site_id: &str, period: &Period, f: &Filter) -> anyhow::Result<Totals> {
    let (scope, within, paid) = (f.cte(), f.within("session_id"), f.paid());
    let sql = format!(
        "WITH {BOUNDS}{scope},
        tracked AS (
            SELECT DISTINCT visitor_id FROM events, bounds
            WHERE site_id = $site AND ts >= lo AND ts < hi{within}
        )
        SELECT
            (SELECT count(*) FROM tracked),
            (SELECT count(DISTINCT session_id) FROM events, bounds
             WHERE site_id = $site AND ts >= lo AND ts < hi{within}),
            count(*),
            count(DISTINCT {CUSTOMER}),
            count(DISTINCT {CUSTOMER}) FILTER (WHERE NOT p.is_renewal),
            coalesce(sum({NET_USD}), 0),
            count(DISTINCT {CUSTOMER}) FILTER (WHERE p.visitor_id IN (SELECT visitor_id FROM tracked)),
            count(*) FILTER (WHERE p.visitor_id IN (SELECT visitor_id FROM tracked)),
            coalesce(sum({NET_USD}) FILTER (WHERE p.visitor_id IN (SELECT visitor_id FROM tracked)), 0)
        FROM payments p, bounds
        WHERE p.site_id = $site AND p.ts >= lo AND p.ts < hi AND p.status <> 'failed'{paid}"
    );
    #[allow(clippy::type_complexity)]
    let (
        visitors,
        sessions,
        payments,
        customers,
        new_customers,
        revenue_usd,
        converted,
        matched,
        matched_usd,
    ): (i64, i64, i64, i64, i64, f64, i64, i64, f64) = conn.query_row(
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
                r.get(8)?,
            ))
        },
    )?;
    let visitors = visitors as u64;
    let some = matched > 0;
    let visits = visit_times(conn, site_id, period, f)?;
    Ok(Totals {
        visitors,
        sessions: sessions as u64,
        payments: payments as u64,
        customers: customers as u64,
        new_customers: new_customers as u64,
        revenue_usd,
        converted: converted as u64,
        matched_payments: matched as u64,
        matched_revenue_usd: matched_usd,
        revenue_per_visitor: ratio(matched_usd, visitors).filter(|_| some),
        conversion: ratio(converted as f64, visitors).filter(|_| some),
        avg_visit_secs: mean(visits.iter().map(|v| (v.1, v.2))),
    })
}

/// (channel, seconds, visits) over visits that began in the period and have two or more events.
fn visit_times(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    f: &Filter,
) -> anyhow::Result<Vec<(Option<String>, f64, u64)>> {
    let (scope, within) = (f.cte(), f.within("t.session_id"));
    let sql = format!(
        "WITH {BOUNDS}{scope}
        SELECT channel, sum(secs), count(*) FROM (
            SELECT t.channel, epoch(max(e.ts)) - epoch(min(e.ts)) AS secs
            FROM touches t
            JOIN events e ON e.site_id = t.site_id AND e.session_id = t.session_id, bounds
            WHERE t.site_id = $site AND t.ts >= lo AND t.ts < hi{within}
            GROUP BY t.session_id, t.channel
            HAVING max(e.ts) > min(e.ts)
        ) GROUP BY channel"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64)),
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Mean of (seconds, visits) sums.
pub(crate) fn mean(sums: impl Iterator<Item = (f64, u64)>) -> Option<f64> {
    let (secs, n) = sums.fold((0.0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    ratio(secs, n)
}

pub(crate) fn overview(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    f: &Filter,
) -> anyhow::Result<Overview> {
    Ok(Overview {
        current: totals(conn, site_id, period, f)?,
        previous: totals(conn, site_id, &period.previous(), f)?,
    })
}

pub(crate) fn timeseries(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    f: &Filter,
) -> anyhow::Result<Vec<DayPoint>> {
    let (scope, within, paid) = (f.cte(), f.within("session_id"), f.paid());
    let sql = format!(
        "WITH {BOUNDS}{scope},
        days AS (
            SELECT CAST(generate_series AS DATE) AS day
            FROM generate_series(CAST($from AS DATE)::TIMESTAMP, CAST($to AS DATE)::TIMESTAMP, INTERVAL 1 DAY)
        ),
        v AS (
            SELECT CAST(timezone($tz, ts) AS DATE) AS day, count(DISTINCT visitor_id) AS n
            FROM events, bounds WHERE site_id = $site AND ts >= lo AND ts < hi{within} GROUP BY 1
        ),
        r AS (
            SELECT CAST(timezone($tz, p.ts) AS DATE) AS day, count(*) AS n,
                count(DISTINCT {CUSTOMER}) AS customers, coalesce(sum({NET_USD}), 0) AS rev,
                count(DISTINCT {CUSTOMER}) FILTER (WHERE matched) AS converted,
                coalesce(sum({NET_USD}) FILTER (WHERE matched), 0) AS matched_rev
            FROM (
                SELECT p.*, m.visitor_id IS NOT NULL AS matched
                FROM payments p
                LEFT JOIN (
                    SELECT DISTINCT visitor_id, CAST(timezone($tz, ts) AS DATE) AS day FROM events, bounds
                    WHERE site_id = $site AND ts >= lo AND ts < hi
                ) m ON m.visitor_id = p.visitor_id AND m.day = CAST(timezone($tz, p.ts) AS DATE)
                WHERE p.site_id = $site
            ) p, bounds
            WHERE p.site_id = $site AND p.ts >= lo AND p.ts < hi AND p.status <> 'failed'{paid} GROUP BY 1
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
            hour: None,
            by_channel: Vec::new(),
        });
    }
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS}{scope}
        SELECT strftime(CAST(timezone($tz, p.ts) AS DATE), '%Y-%m-%d'),
            CASE WHEN a.session_id IS NOT NULL THEN a.channel END, coalesce(sum({NET_USD}), 0)
        FROM payments p
        JOIN bounds ON p.ts >= lo AND p.ts < hi
        {ATTRIBUTION_JOIN}
        WHERE p.site_id = $site AND p.status <> 'failed'{paid}
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

/// Revenue and visitors per local hour of a one-day period.
pub(crate) fn hourly(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    f: &Filter,
) -> anyhow::Result<Vec<DayPoint>> {
    let (scope, within, paid) = (f.cte(), f.within("session_id"), f.paid());
    let params = named_params! {
        "site": site_id,
        "tz": period.tz,
        "from": period.from.to_string(),
        "to": period.from.to_string(),
    };
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS}{scope},
        hours AS (SELECT unnest(range(24)) AS h),
        v AS (
            SELECT hour(timezone($tz, ts)) AS h, count(DISTINCT visitor_id) AS n
            FROM events, bounds WHERE site_id = $site AND ts >= lo AND ts < hi{within} GROUP BY 1
        ),
        r AS (
            SELECT hour(timezone($tz, p.ts)) AS h, count(*) AS n, count(DISTINCT {CUSTOMER}) AS c,
                coalesce(sum({NET_USD}), 0) AS rev
            FROM payments p, bounds
            WHERE p.site_id = $site AND p.ts >= lo AND p.ts < hi AND p.status <> 'failed'{paid} GROUP BY 1
        )
        SELECT hours.h, coalesce(v.n, 0), coalesce(r.n, 0), coalesce(r.c, 0), coalesce(r.rev, 0)
        FROM hours LEFT JOIN v USING (h) LEFT JOIN r USING (h) ORDER BY 1"
    ))?;
    let rows = stmt.query_map(params, |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, f64>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (h, visitors, payments, customers, revenue_usd) = row?;
        out.push(DayPoint {
            date: period.from,
            visitors: visitors as u64,
            payments: payments as u64,
            customers: customers as u64,
            revenue_usd,
            converted: 0,
            matched_revenue_usd: 0.0,
            conversion: None,
            revenue_per_visitor: None,
            hour: Some(h as u8),
            by_channel: Vec::new(),
        });
    }
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS}{scope}
        SELECT hour(timezone($tz, p.ts)), CASE WHEN a.session_id IS NOT NULL THEN a.channel END,
            coalesce(sum({NET_USD}), 0)
        FROM payments p
        JOIN bounds ON p.ts >= lo AND p.ts < hi
        {ATTRIBUTION_JOIN}
        WHERE p.site_id = $site AND p.status <> 'failed'{paid}
        GROUP BY ALL"
    ))?;
    let rows = stmt.query_map(params, |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, f64>(2)?,
        ))
    })?;
    for row in rows {
        let (h, channel, revenue) = row?;
        if let Some(p) = out.get_mut(h as usize) {
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
    f: &Filter,
) -> anyhow::Result<Vec<BreakdownRow>> {
    let (kind, key, first) = dimension.source();
    let (scope, within, paid) = (f.cte(), f.within("session_id"), f.paid());
    let (table, kind, distinct) = if first {
        ("touches", "true".to_string(), "")
    } else {
        ("events", format!("kind = '{kind}'"), "DISTINCT")
    };
    // Visitors count sessions in range; payments follow their credited session, wherever it falls.
    let sql = format!(
        "WITH {BOUNDS}{scope}
        SELECT unattributed, key, sum(visitors)::BIGINT, sum(payments)::BIGINT, sum(revenue),
            sum(customers)::BIGINT, sum(refunded)::BIGINT
        FROM (
            SELECT false AS unattributed, {key} AS key, count(DISTINCT visitor_id) AS visitors,
                   0 AS payments, 0.0 AS revenue, 0 AS customers, 0 AS refunded
            FROM {table}, bounds
            WHERE site_id = $site AND {kind} AND ts >= lo AND ts < hi{within}
            GROUP BY 2
            UNION ALL
            SELECT a.session_id IS NULL, k.key, 0, count(*), coalesce(sum({NET_USD}), 0),
                   count(DISTINCT {CUSTOMER}), count(*) FILTER (WHERE p.refunded > 0)
            FROM payments p
            JOIN bounds ON p.ts >= lo AND p.ts < hi
            LEFT JOIN attribution a ON a.site_id = p.site_id AND a.provider = p.provider
                AND a.provider_payment_id = p.provider_payment_id
            LEFT JOIN (
                SELECT {distinct} session_id, {key} AS key FROM {table}
                WHERE site_id = $site AND {kind}{within}
                    AND session_id IN (SELECT session_id FROM attribution WHERE site_id = $site)
            ) k ON k.session_id = a.session_id
            WHERE p.site_id = $site AND p.status <> 'failed'{paid}
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
                r.get::<_, i64>(6)?,
            ))
        },
    )?;
    let mut out = breakdown_rows(dimension, rows.collect::<Result<Vec<_>, _>>()?);
    if dimension == Dimension::Channel {
        for (key, secs, n) in visit_times(conn, site_id, period, f)? {
            if let Some(r) = out.iter_mut().find(|r| !r.unattributed && r.key == key) {
                r.avg_visit_secs = ratio(secs, n);
            }
        }
    }
    Ok(out)
}

/// Merges (unattributed, key, visitors, payments, revenue, customers, refunded) rows into a sorted breakdown.
#[allow(clippy::type_complexity)]
pub(crate) fn breakdown_rows(
    dimension: Dimension,
    rows: Vec<(bool, Option<String>, i64, i64, f64, i64, i64)>,
) -> Vec<BreakdownRow> {
    let mut out: Vec<BreakdownRow> = Vec::new();
    for row in rows {
        let (unattributed, mut key, visitors, payments, revenue_usd, customers, refunded) = row;
        match dimension {
            Dimension::Referrer => key = key.as_deref().map(source_domain),
            Dimension::ChannelSource | Dimension::PageSource => {
                key = key.map(|k| match k.rsplit_once('|') {
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
            existing.refunded += refunded as u64;
            existing.revenue_usd += revenue_usd;
        } else {
            out.push(BreakdownRow {
                key,
                label,
                unattributed,
                visitors: visitors as u64,
                payments: payments as u64,
                customers: customers as u64,
                refunded: refunded as u64,
                revenue_usd,
                revenue_per_visitor: None,
                avg_visit_secs: None,
            });
        }
    }
    for r in &mut out {
        r.revenue_per_visitor = ratio(r.revenue_usd, r.visitors);
    }
    sort_rows(&mut out);
    out
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
    coalesce(p.visitor_id, sv.visitor_id, cv.visitor_id)";

pub(crate) const ATTRIBUTION_JOIN: &str = "LEFT JOIN attribution a ON a.site_id = p.site_id
    AND a.provider = p.provider AND a.provider_payment_id = p.provider_payment_id";

/// The first pageview of the payer, or of the credited visitor when the payment has no visitor ID,
/// plus the credited session's visitor (sv) and the customer's earliest visitor (cv) for PAYMENT_COLUMNS.
pub(crate) const FIRST_TOUCH_JOIN: &str =
    "LEFT JOIN touches sv ON sv.site_id = p.site_id AND sv.session_id = a.session_id
    LEFT JOIN (
        SELECT site_id, provider, customer_id, arg_min(visitor_id, ts) AS visitor_id FROM payments
        WHERE visitor_id IS NOT NULL AND customer_id IS NOT NULL GROUP BY ALL
    ) cv ON cv.site_id = p.site_id AND cv.provider = p.provider AND cv.customer_id = p.customer_id
    LEFT JOIN first_touches f ON f.site_id = p.site_id
        AND f.visitor_id = coalesce(p.visitor_id, sv.visitor_id) AND f.ts <= p.ts";

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
            .map(|usd| cents(usd * (amount - refunded) as f64 / amount as f64)),
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
    f: &Filter,
) -> anyhow::Result<PaymentPage> {
    let scope = f.cte();
    let from = format!(
        "FROM payments p
        JOIN bounds ON p.ts >= lo AND p.ts < hi
        {ATTRIBUTION_JOIN}
        WHERE p.site_id = $site AND p.status <> 'failed' AND {}{}",
        filter.sql(),
        f.paid()
    );
    let from_first = from.replacen(
        ATTRIBUTION_JOIN,
        &format!("{ATTRIBUTION_JOIN} {FIRST_TOUCH_JOIN}"),
        1,
    );
    let total: i64 = conn.query_row(
        &format!("WITH {BOUNDS}{scope} SELECT count(*) {from}"),
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| r.get(0),
    )?;
    let sql = format!(
        "WITH {BOUNDS}{scope} SELECT {PAYMENT_COLUMNS} {from_first}
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

/// Visits and pageviews up to each first payment in the range, from the payer's first visit.
pub(crate) fn path_to_purchase(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<Vec<PathPay>> {
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS},
        pays AS (
            SELECT p.ts, coalesce(p.visitor_id, t.visitor_id) AS visitor, a.channel
            FROM payments p
            JOIN bounds ON p.ts >= lo AND p.ts < hi
            LEFT JOIN attribution a ON a.site_id = p.site_id AND a.provider = p.provider
                AND a.provider_payment_id = p.provider_payment_id
            LEFT JOIN touches t ON t.site_id = p.site_id AND t.session_id = a.session_id
            WHERE p.site_id = $site AND p.status <> 'failed' AND NOT p.is_renewal
        )
        SELECT epoch_us(p.ts) - epoch_us(f.ts), f.channel, p.channel,
            (SELECT count(*) FROM touches s WHERE s.site_id = $site AND s.visitor_id = p.visitor AND s.ts <= p.ts),
            (SELECT count(*) FROM events e WHERE e.site_id = $site AND e.visitor_id = p.visitor
                AND e.kind = 'pageview' AND e.ts <= p.ts)
        FROM pays p
        JOIN first_touches f ON f.site_id = $site AND f.visitor_id = p.visitor AND f.ts <= p.ts"
    ))?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            Ok(PathPay {
                days: r.get::<_, i64>(0)? as f64 / 86_400_000_000.0,
                first_channel: r.get(1)?,
                credited_channel: r.get(2)?,
                visits: r.get::<_, i64>(3)? as u64,
                pages: r.get::<_, i64>(4)? as u64,
            })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
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
            FROM touches
            WHERE site_id = $site AND (visitor_id = $visitor OR session_id = $credited)
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
    if who.is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<serde_json::Value> = who
        .iter()
        .enumerate()
        .map(|(i, w)| {
            serde_json::json!({
                "i": i,
                "v": w.visitor_id,
                "s": w.session_id,
                "t": (w.ts.unix_timestamp_nanos() / 1000) as i64,
            })
        })
        .collect();
    // Pageviews of each visitor or credited session at or before its moment, all moments in one pass.
    let seen = r#"WITH w AS (
            SELECT i, v, s, make_timestamp(t)::TIMESTAMPTZ AS moment FROM (
                SELECT unnest(from_json($who, '[{"i":"BIGINT","v":"VARCHAR","s":"VARCHAR","t":"BIGINT"}]'), recursive := true)
            )
        ),
        seen AS (
            SELECT w.i, w.moment, e.ts, e.path, e.session_id, e.country, e.device, e.os, e.browser
            FROM w JOIN events e ON e.site_id = $site AND e.kind = 'pageview' AND e.visitor_id = w.v AND e.ts <= w.moment
            UNION
            SELECT w.i, w.moment, e.ts, e.path, e.session_id, e.country, e.device, e.os, e.browser
            FROM w JOIN events e ON e.site_id = $site AND e.kind = 'pageview' AND e.session_id = w.s AND e.ts <= w.moment
        )"#;
    let who_json = serde_json::to_string(&list)?;
    let mut out = vec![
        Activity {
            days: vec![0; ACTIVITY_DAYS],
            ..Default::default()
        };
        who.len()
    ];
    let mut stmt = conn.prepare(&format!(
        "{seen}
        SELECT i, country, device, os, browser, path FROM seen
        QUALIFY row_number() OVER (PARTITION BY i ORDER BY ts DESC, path DESC) = 1"
    ))?;
    let rows = stmt.query_map(named_params! { "site": site_id, "who": who_json }, |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        ))
    })?;
    for row in rows {
        let (i, country, device, os, browser, last_page) = row?;
        let a = &mut out[i as usize];
        (a.country, a.device, a.os, a.browser, a.last_page) =
            (country, device, os, browser, last_page);
    }
    let mut stmt = conn.prepare(&format!(
        "{seen}
        SELECT i, CAST(timezone($tz, moment) AS DATE) - CAST(timezone($tz, ts) AS DATE) AS ago,
            count(DISTINCT session_id)
        FROM seen
        WHERE epoch_us(ts) > epoch_us(moment) - 15 * 86400000000
        GROUP BY i, ago"
    ))?;
    let rows = stmt.query_map(
        named_params! { "site": site_id, "tz": tz, "who": who_json },
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        },
    )?;
    for row in rows {
        let (i, ago, n) = row?;
        if (0..ACTIVITY_DAYS as i64).contains(&ago) {
            out[i as usize].days[ACTIVITY_DAYS - 1 - ago as usize] = n as u32;
        }
    }
    Ok(out)
}

/// Pages by the revenue of payments whose payer viewed them last before paying; visitors count every view in range.
pub(crate) fn last_pages(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    f: &Filter,
) -> anyhow::Result<Vec<BreakdownRow>> {
    let (scope, within, paid) = (f.cte(), f.within("session_id"), f.paid());
    let sql = format!(
        "WITH {BOUNDS}{scope},
        pays AS (
            SELECT p.provider, p.provider_payment_id, p.ts, p.visitor_id, a.session_id,
                {NET_USD} AS usd, {CUSTOMER} AS customer
            FROM payments p
            JOIN bounds ON p.ts >= lo AND p.ts < hi
            {ATTRIBUTION_JOIN}
            WHERE p.site_id = $site AND p.status <> 'failed'{paid}
        ),
        viewed AS (
            SELECT y.provider, y.provider_payment_id, e.ts, e.path FROM pays y
            JOIN events e ON e.site_id = $site AND e.kind = 'pageview' AND e.visitor_id = y.visitor_id AND e.ts <= y.ts
            UNION
            SELECT y.provider, y.provider_payment_id, e.ts, e.path FROM pays y
            JOIN events e ON e.site_id = $site AND e.kind = 'pageview' AND e.session_id = y.session_id AND e.ts <= y.ts
        ),
        last AS (
            SELECT y.usd, y.customer, l.path FROM pays y
            LEFT JOIN (
                SELECT provider, provider_payment_id, path FROM viewed
                QUALIFY row_number() OVER (PARTITION BY provider, provider_payment_id ORDER BY ts DESC, path DESC) = 1
            ) l USING (provider, provider_payment_id)
        ),
        seen AS (
            SELECT path, count(DISTINCT visitor_id) AS visitors FROM events, bounds
            WHERE site_id = $site AND kind = 'pageview' AND ts >= lo AND ts < hi{within}
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
                refunded: 0,
                revenue_usd,
                revenue_per_visitor: ratio(revenue_usd, visitors),
                avg_visit_secs: None,
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
            epoch_us(ft.ts), coalesce(spent.usd, 0), firstpay.customer
        FROM done d
        LEFT JOIN first_touches ft ON ft.site_id = $site AND ft.visitor_id = d.visitor_id
        LEFT JOIN (
            SELECT p.visitor_id, coalesce(sum({NET_USD}), 0) AS usd FROM payments p
            WHERE p.site_id = $site AND p.status <> 'failed'
            GROUP BY 1
        ) spent ON spent.visitor_id = d.visitor_id
        LEFT JOIN (
            SELECT p.visitor_id, arg_min(p.provider || ':' || p.customer_id, p.ts) AS customer FROM payments p
            WHERE p.site_id = $site AND p.customer_id IS NOT NULL
            GROUP BY 1
        ) firstpay ON firstpay.visitor_id = d.visitor_id
        LEFT JOIN touches l ON l.site_id = $site AND l.session_id = d.session_id
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
            coalesce(sum(p.amount_usd - {NET_USD}), 0)
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

/// One provider's payments in a range: new against renewals, and why unmatched ones did not match.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ProviderSplit {
    pub provider: String,
    pub first_payments: u64,
    pub first_usd: f64,
    pub renewals: u64,
    pub renewal_usd: f64,
    /// Unmatched payments that carried no visitor ID.
    pub no_visitor: u64,
    /// Unmatched payments whose visitor ID has no visit in the window.
    pub unseen: u64,
    /// Net USD credited to visits that started before the range.
    pub earlier_usd: f64,
}

pub(crate) fn payment_split(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<Vec<ProviderSplit>> {
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS}
        SELECT p.provider,
            count(*) FILTER (WHERE NOT p.is_renewal), coalesce(sum({NET_USD}) FILTER (WHERE NOT p.is_renewal), 0),
            count(*) FILTER (WHERE p.is_renewal), coalesce(sum({NET_USD}) FILTER (WHERE p.is_renewal), 0),
            count(*) FILTER (WHERE a.session_id IS NULL AND p.visitor_id IS NULL),
            count(*) FILTER (WHERE a.session_id IS NULL AND p.visitor_id IS NOT NULL),
            coalesce(sum({NET_USD}) FILTER (WHERE a.touch_ts < lo), 0)
        FROM payments p
        JOIN bounds ON p.ts >= lo AND p.ts < hi
        {ATTRIBUTION_JOIN}
        WHERE p.site_id = $site AND p.status <> 'failed'
        GROUP BY 1 ORDER BY 1"
    ))?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            Ok(ProviderSplit {
                provider: r.get(0)?,
                first_payments: r.get::<_, i64>(1)? as u64,
                first_usd: r.get(2)?,
                renewals: r.get::<_, i64>(3)? as u64,
                renewal_usd: r.get(4)?,
                no_visitor: r.get::<_, i64>(5)? as u64,
                unseen: r.get::<_, i64>(6)? as u64,
                earlier_usd: r.get(7)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// A step of a funnel; the first step is always any visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Visit,
    Page(String),
    Goal(String),
    Paid,
}

/// Visitors who reached a step after every step before it, and the median seconds since the step before.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepCount {
    pub visitors: u64,
    pub median_secs: Option<f64>,
}

pub(crate) fn funnel(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    steps: &[Step],
    f: &Filter,
) -> anyhow::Result<Vec<StepCount>> {
    let (scope, within) = (f.cte(), f.within("session_id"));
    let mut ctes = vec![format!(
        "s0 AS (SELECT visitor_id, min(ts) AS t FROM events, bounds
            WHERE site_id = $site AND kind = 'pageview' AND ts >= lo AND ts < hi{within} GROUP BY 1)"
    )];
    let mut cols = vec!["(SELECT count(*) FROM s0), NULL::DOUBLE".to_string()];
    for (i, step) in steps.iter().enumerate().skip(1) {
        let b = i - 1;
        ctes.push(match step {
            Step::Paid => format!(
                "s{i} AS (SELECT p.visitor_id, min(p.ts) AS t FROM s{b} b
                JOIN payments p ON p.site_id = $site AND p.visitor_id = b.visitor_id AND p.ts >= b.t, bounds
                WHERE p.ts < hi AND p.status <> 'failed' GROUP BY 1)"
            ),
            _ => {
                let pred = match step {
                    Step::Page(path) => format!("e.kind = 'pageview' AND {}", page_is(path)),
                    Step::Goal(name) => format!("e.kind = 'goal' AND e.name = {}", lit(name)),
                    _ => "e.kind = 'pageview'".into(),
                };
                format!(
                    "s{i} AS (SELECT e.visitor_id, min(e.ts) AS t FROM s{b} b
                    JOIN events e ON e.site_id = $site AND e.visitor_id = b.visitor_id AND e.ts >= b.t, bounds
                    WHERE e.ts < hi AND {pred} GROUP BY 1)"
                )
            }
        });
        cols.push(format!(
            "(SELECT count(*) FROM s{i}),
            (SELECT median(epoch_us(x.t) - epoch_us(y.t))::DOUBLE / 1e6 FROM s{i} x JOIN s{b} y USING (visitor_id))"
        ));
    }
    let sql = format!(
        "WITH {BOUNDS}{scope}, {} SELECT {}",
        ctes.join(",\n"),
        cols.join(", ")
    );
    conn.query_row(
        &sql,
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            (0..steps.len().max(1))
                .map(|i| {
                    Ok(StepCount {
                        visitors: r.get::<_, i64>(i * 2)? as u64,
                        median_secs: r.get(i * 2 + 1)?,
                    })
                })
                .collect()
        },
    )
    .map_err(Into::into)
}

/// Completions of a custom goal ("goal:name") or a page ("page:/path") in a range, split by the session's channel.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GoalStat {
    pub key: String,
    pub completions: u64,
    pub visitors: u64,
    /// Channel and visitors, most first.
    pub channels: Vec<(String, u64)>,
}

pub(crate) fn goal_table(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    pages: &[String],
    f: &Filter,
) -> anyhow::Result<Vec<GoalStat>> {
    let (scope, within) = (f.cte(), f.within("e.session_id"));
    let keys: String = pages
        .iter()
        .map(|p| format!(" WHEN {} THEN {}", page_is(p), lit(&format!("page:{p}"))))
        .collect();
    let pages = if pages.is_empty() {
        "false".to_string()
    } else {
        let any: Vec<String> = pages.iter().map(|p| page_is(p)).collect();
        format!("({})", any.join(" OR "))
    };
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS}{scope},
        done AS (
            SELECT CASE WHEN e.kind = 'goal' THEN 'goal:' || e.name{keys} END AS key,
                e.visitor_id, coalesce(f.channel, e.channel) AS channel
            FROM events e
            JOIN bounds ON e.ts >= lo AND e.ts < hi
            LEFT JOIN touches f ON f.site_id = $site AND f.session_id = e.session_id
            WHERE e.site_id = $site AND ((e.kind = 'goal' AND e.name IS NOT NULL)
                OR (e.kind = 'pageview' AND {pages})){within}
        )
        SELECT key, channel, count(*), count(DISTINCT visitor_id), grouping(channel)
        FROM done GROUP BY GROUPING SETS ((key), (key, channel))
        ORDER BY grouping(channel) DESC, count(DISTINCT visitor_id) DESC, key, channel"
    ))?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)? as u64,
                r.get::<_, i64>(3)? as u64,
                r.get::<_, i64>(4)? == 1,
            ))
        },
    )?;
    let mut out: Vec<GoalStat> = Vec::new();
    for row in rows {
        let (key, channel, completions, visitors, total) = row?;
        if total {
            out.push(GoalStat {
                key,
                completions,
                visitors,
                channels: Vec::new(),
            });
        } else if let Some(g) = out.iter_mut().find(|g| g.key == key) {
            g.channels.push((channel.unwrap_or_default(), visitors));
        }
    }
    Ok(out)
}

pub(crate) fn channel_sources(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    f: &Filter,
) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    let (scope, within) = (f.cte(), f.within("session_id"));
    let sql = format!(
        "WITH {BOUNDS}{scope},
        touches AS (
            SELECT visitor_id, channel, referrer_host
            FROM events, bounds
            WHERE site_id = $site AND kind = 'pageview' AND ts >= lo AND ts < hi{within}
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

/// Earliest payment credited to a visit.
pub(crate) fn first_match(conn: &Connection, site_id: &str) -> anyhow::Result<Option<PaymentRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PAYMENT_COLUMNS} FROM payments p {ATTRIBUTION_JOIN} {FIRST_TOUCH_JOIN}
        WHERE p.site_id = $site AND p.status <> 'failed' AND a.session_id IS NOT NULL
        ORDER BY p.ts, p.provider, p.provider_payment_id
        LIMIT 1"
    ))?;
    Ok(stmt
        .query_map(named_params! { "site": site_id }, payment_row)?
        .next()
        .transpose()?)
}

pub(crate) fn setup(conn: &Connection, site_id: &str) -> anyhow::Result<Setup> {
    let (first, has_goals, payments): (Option<i64>, bool, i64) = conn.query_row(
        "SELECT
            (SELECT epoch_us(min(ts)) FROM events WHERE site_id = $site),
            EXISTS (SELECT 1 FROM events WHERE site_id = $site AND kind = 'goal'),
            (SELECT count(*) FROM payments WHERE site_id = $site AND status <> 'failed')",
        named_params! { "site": site_id },
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let first_match = first_match(conn, site_id)?;
    Ok(Setup {
        first_event: first.map(from_micros),
        has_goals,
        payments: payments as u64,
        first_match,
    })
}

/// Runs each part on its own connection from `conn`, all at once.
pub(crate) fn report<C: std::ops::Deref<Target = Connection>>(
    conn: &(dyn Fn() -> anyhow::Result<C> + Sync),
    site_id: &str,
    period: &Period,
    f: &Filter,
) -> anyhow::Result<Report> {
    let previous = period.previous();
    std::thread::scope(|s| {
        let dims = |d| s.spawn(move || breakdown(&*conn()?, site_id, period, d, f));
        let overview = s.spawn(|| overview(&*conn()?, site_id, period, f));
        let days = s.spawn(|| timeseries(&*conn()?, site_id, period, f));
        let previous_days = s.spawn(|| timeseries(&*conn()?, site_id, &previous, f));
        let channels = dims(Dimension::Channel);
        let referrers = dims(Dimension::Referrer);
        let landing_pages = dims(Dimension::LandingPage);
        let countries = dims(Dimension::Country);
        let channel_sources = s.spawn(|| channel_sources(&*conn()?, site_id, period, f));
        let setup = s.spawn(|| setup(&*conn()?, site_id));
        Ok(Report {
            overview: overview.join().unwrap()?,
            days: days.join().unwrap()?,
            previous_days: previous_days.join().unwrap()?,
            channels: channels.join().unwrap()?,
            referrers: referrers.join().unwrap()?,
            landing_pages: landing_pages.join().unwrap()?,
            countries: countries.join().unwrap()?,
            channel_sources: channel_sources.join().unwrap()?,
            setup: setup.join().unwrap()?,
        })
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
            language: None,
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
        let rows = store
            .last_pages_in("s", &period(), &Filter::default())
            .unwrap();
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.label.as_str(), r.visitors, r.payments, r.revenue_usd))
            .collect();
        assert_eq!(got, [("/pricing", 2, 1, 100.0), ("/compare", 1, 1, 100.0)]);
        assert_eq!(rows[0].revenue_per_visitor, Some(50.0));
    }

    #[test]
    fn visit_time_averages_visits_with_two_events() {
        let (_dir, store) = store();
        let mut more = visit("v1", "a", T + Duration::minutes(4), "Social");
        more.path = "/more".into();
        let mut longer = visit("v2", "b", T + Duration::minutes(10), "Social");
        longer.path = "/more".into();
        store
            .insert_events(&[
                visit("v1", "a", T, "Social"),
                more,
                visit("v2", "b", T, "Social"),
                longer,
                visit("v3", "c", T, "Paid"),
            ])
            .unwrap();
        let o = store.overview("s", &period()).unwrap();
        assert_eq!(o.current.avg_visit_secs, Some(420.0));
        assert_eq!(o.previous.avg_visit_secs, None);
        let rows = store.breakdown("s", &period(), Dimension::Channel).unwrap();
        let secs = |l: &str| rows.iter().find(|r| r.label == l).unwrap().avg_visit_secs;
        assert_eq!(secs("Social"), Some(420.0));
        assert_eq!(secs("Paid"), None);
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
        a.language = Some("de-DE".into());
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
        let de = find(Dimension::Language, "de-DE");
        assert_eq!((de.visitors, de.revenue_usd), (1, 100.0));
        assert_eq!(
            find(Dimension::ChannelSource, "Social|news.ycombinator.com").revenue_usd,
            100.0
        );
        assert_eq!(find(Dimension::ChannelSource, "Direct|").visitors, 1);
    }

    #[test]
    fn filters_scope_numbers_to_matching_sessions() {
        let (_dir, store) = store();
        let mut pricing = visit("v1", "a", T + Duration::minutes(1), "Social");
        pricing.path = "/pricing".into();
        let mut goal = visit("v1", "a", T + Duration::minutes(2), "Social");
        goal.kind = "goal".into();
        goal.name = Some("signup".into());
        store
            .insert_events(&[
                visit("v1", "a", T, "Social"),
                pricing,
                goal,
                visit("v2", "b", T, "Direct"),
            ])
            .unwrap();
        store
            .upsert_payments(&[
                pay("p1", T + Duration::hours(1), Some("v1"), None),
                pay("p2", T + Duration::hours(1), None, None),
            ])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 30)
            .unwrap();
        let social = Filter {
            first: vec![("channel", vec![Some("Social".into())])],
            viewed: None,
        };
        let t = store.overview_in("s", &period(), &social).unwrap().current;
        assert_eq!((t.visitors, t.payments, t.revenue_usd), (1, 1, 100.0));
        let quoted = Filter {
            first: vec![("channel", vec![Some("x' OR '1'='1".into())])],
            viewed: None,
        };
        assert_eq!(
            store
                .overview_in("s", &period(), &quoted)
                .unwrap()
                .current
                .visitors,
            0
        );
        let viewed = Filter {
            first: Vec::new(),
            viewed: Some("/pricing".into()),
        };
        let rows = store
            .breakdown_in("s", &period(), Dimension::Channel, &viewed)
            .unwrap();
        assert_eq!(rows.len(), 1);
        let list = store
            .payment_list_in("s", &period(), PaymentFilter::All, 10, 0, &social)
            .unwrap();
        assert_eq!(list.total, 1);

        let steps = [
            Step::Visit,
            Step::Page("/pricing".into()),
            Step::Goal("signup".into()),
            Step::Paid,
        ];
        let f = store
            .funnel("s", &period(), &steps, &Filter::default())
            .unwrap();
        let counts: Vec<u64> = f.iter().map(|c| c.visitors).collect();
        assert_eq!(counts, [2, 1, 1, 1]);
        assert_eq!(f[3].median_secs, Some(3480.0));
        let goals = store
            .goal_table("s", &period(), &["/pricing".into()], &Filter::default())
            .unwrap();
        let signup = goals.iter().find(|g| g.key == "goal:signup").unwrap();
        assert_eq!((signup.completions, signup.visitors), (1, 1));
        assert_eq!(signup.channels, [("Social".to_string(), 1)]);

        let split = store.payment_split("s", &period()).unwrap();
        assert_eq!(
            (
                split[0].first_payments,
                split[0].no_visitor,
                split[0].unseen
            ),
            (2, 1, 0)
        );
        let day = Period {
            from: T.date(),
            to: T.date(),
            tz: "UTC".into(),
        };
        let hours = store.hourly("s", &day, &Filter::default()).unwrap();
        assert_eq!(hours.len(), 24);
        assert_eq!((hours[12].visitors, hours[13].revenue_usd), (2, 200.0));
    }
}
