use std::path::Path;
use std::sync::Mutex;

use duckdb::types::{TimeUnit, Value};
use duckdb::{Connection, named_params, params};
use time::{OffsetDateTime, UtcOffset};

use crate::providers::Subscription;
use crate::retention::{self, Retention};
use crate::stats::{
    self, Activity, GoalCount, GoalRow, Journey, Overview, PaymentFilter, PaymentPage,
    PaymentSummary, Period, Report, Who,
};
use crate::stats::{BreakdownRow, Dimension, Filter, GoalStat, ProviderSplit, Step, StepCount};
use crate::visitors::{self, Facets, Timeline, VisitorFilter, VisitorPage};

#[derive(Debug, Clone)]
pub struct Event {
    pub site_id: String,
    pub ts: OffsetDateTime,
    pub visitor_id: String,
    pub session_id: String,
    pub kind: String,
    pub name: Option<String>,
    pub path: String,
    pub referrer_host: Option<String>,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub utm_content: Option<String>,
    pub utm_term: Option<String>,
    pub channel: String,
    pub country: Option<String>,
    pub device: String,
    pub browser: String,
    pub os: String,
    pub region: Option<String>,
    pub city: Option<String>,
    /// Page hostname, lowercase.
    pub host: Option<String>,
    /// Browser language as "en" or "en-US".
    pub language: Option<String>,
}

/// A payment as stored, one row per provider payment ID so re-syncs never double count.
#[derive(Debug, Clone, PartialEq)]
pub struct Payment {
    pub site_id: String,
    pub provider: String,
    pub provider_payment_id: String,
    pub ts: OffsetDateTime,
    /// Amount in the currency's minor unit (cents).
    pub amount: i64,
    pub currency: String,
    pub amount_usd: Option<f64>,
    /// Refunded and charged-back amount in minor units; net revenue is amount minus this.
    pub refunded: i64,
    /// paid, refunded, partially_refunded, failed
    pub status: String,
    pub visitor_id: Option<String>,
    /// Provider's opaque customer ID, used so renewals inherit the first payment's source.
    pub customer_id: Option<String>,
    pub is_renewal: bool,
}

/// Storage for events and payments. DuckDB today; ClickHouse can implement this later.
pub trait EventStore: Send + Sync {
    fn insert_events(&self, events: &[Event]) -> anyhow::Result<()>;
    /// Inserts or replaces payments keyed by (site_id, provider, provider_payment_id).
    fn upsert_payments(&self, payments: &[Payment]) -> anyhow::Result<()>;
    /// Replaces the stored subscription states of one provider of a site.
    fn replace_subscriptions(
        &self,
        site_id: &str,
        provider: &str,
        subscriptions: &[Subscription],
    ) -> anyhow::Result<()>;
    /// Customers acquired in the period and how many still pay, by first channel, referrer and campaign.
    fn retention(
        &self,
        site_id: &str,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Retention>;
    #[cfg(test)]
    fn count_events(&self, site_id: &str) -> anyhow::Result<u64>;
    fn latest_pageview(&self, site_id: &str) -> anyhow::Result<Option<LatestPageview>>;
    /// When the site's first event arrived.
    fn first_event(&self, site_id: &str) -> anyhow::Result<Option<OffsetDateTime>>;
    /// Distinct visitors seen in the last five minutes.
    fn live_visitors(&self, site_id: &str) -> anyhow::Result<u64>;
    /// Current UTC offset of IANA zone `tz`.
    fn utc_offset(&self, tz: &str) -> anyhow::Result<UtcOffset>;
    /// Recomputes the credited touch of every payment of a site; unattributed payments get a row with a NULL session.
    fn rebuild_attribution(
        &self,
        site_id: &str,
        model: &str,
        window_days: i64,
    ) -> anyhow::Result<()>;
    fn overview(&self, site_id: &str, period: &Period) -> anyhow::Result<Overview> {
        self.overview_in(site_id, period, &Filter::default())
    }
    fn overview_in(&self, site_id: &str, period: &Period, f: &Filter) -> anyhow::Result<Overview>;
    #[cfg(test)]
    fn timeseries(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<stats::DayPoint>>;
    fn breakdown(
        &self,
        site_id: &str,
        period: &Period,
        dimension: Dimension,
    ) -> anyhow::Result<Vec<BreakdownRow>> {
        self.breakdown_in(site_id, period, dimension, &Filter::default())
    }
    fn breakdown_in(
        &self,
        site_id: &str,
        period: &Period,
        dimension: Dimension,
        f: &Filter,
    ) -> anyhow::Result<Vec<BreakdownRow>>;
    /// Pages viewed last before paying, by the revenue of those payments.
    fn last_pages_in(
        &self,
        site_id: &str,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Vec<BreakdownRow>>;
    fn payment_list(
        &self,
        site_id: &str,
        period: &Period,
        filter: PaymentFilter,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<PaymentPage> {
        self.payment_list_in(site_id, period, filter, limit, offset, &Filter::default())
    }
    fn payment_list_in(
        &self,
        site_id: &str,
        period: &Period,
        filter: PaymentFilter,
        limit: u32,
        offset: u32,
        f: &Filter,
    ) -> anyhow::Result<PaymentPage>;
    /// The payer's sessions in the window before the payment plus the credited one, oldest first.
    fn journey(
        &self,
        site_id: &str,
        provider: &str,
        provider_payment_id: &str,
        window_days: i64,
    ) -> anyhow::Result<Option<Journey>>;
    /// Totals, current and previous daily series, the four breakdowns and setup facts.
    fn report(&self, site_id: &str, period: &Period) -> anyhow::Result<Report> {
        self.report_in(site_id, period, &Filter::default())
    }
    fn report_in(&self, site_id: &str, period: &Period, f: &Filter) -> anyhow::Result<Report>;
    /// Per local hour of the period's first day.
    fn hourly(
        &self,
        site_id: &str,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Vec<stats::DayPoint>>;
    fn payment_summary(&self, site_id: &str, period: &Period) -> anyhow::Result<PaymentSummary>;
    /// Payments per provider, new against renewals and why unmatched ones did not match.
    fn payment_split(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<ProviderSplit>>;
    /// Visitors who reached each step in order.
    fn funnel(
        &self,
        site_id: &str,
        period: &Period,
        steps: &[Step],
        f: &Filter,
    ) -> anyhow::Result<Vec<StepCount>>;
    /// Custom goals and views of `pages`, with completions and visitors by channel.
    fn goal_table(
        &self,
        site_id: &str,
        period: &Period,
        pages: &[String],
        f: &Filter,
    ) -> anyhow::Result<Vec<GoalStat>>;
    /// Last seen device and sessions per day before each moment, in zone `tz`.
    fn activity(&self, site_id: &str, tz: &str, who: &[Who]) -> anyhow::Result<Vec<Activity>>;
    fn goal_counts(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<GoalCount>>;
    /// Completions of goal `goal`, newest first, and their total.
    fn goal_list(
        &self,
        site_id: &str,
        period: &Period,
        goal: &str,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<(u64, Vec<GoalRow>)>;
    /// Visitors seen in the range, last seen first.
    fn visitor_list(
        &self,
        site_id: &str,
        period: &Period,
        filter: &VisitorFilter,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<VisitorPage>;
    fn visitor_facets(&self, site_id: &str, period: &Period) -> anyhow::Result<Facets>;
    /// Every visit, pageview, goal, exit and payment of one visitor, oldest first.
    fn timeline(&self, site_id: &str, visitor_id: &str) -> anyhow::Result<Option<Timeline>>;
}

/// Sessions as touches (first pageview's fields), then the credited session per payment, then renewal inheritance.
const REBUILD_ATTRIBUTION: &str = "
INSERT INTO attribution
WITH touches AS (
    SELECT visitor_id, session_id, ts, channel, referrer_host, utm_campaign, path, country, device
    FROM events
    WHERE site_id = $site AND kind = 'pageview'
    QUALIFY row_number() OVER (PARTITION BY session_id ORDER BY ts, path) = 1
),
pays AS (
    SELECT provider, provider_payment_id, ts, visitor_id, customer_id, is_renewal, status
    FROM payments WHERE site_id = $site
),
own AS (
    SELECT p.provider, p.provider_payment_id, t.*
    FROM pays p
    JOIN touches t ON t.visitor_id = p.visitor_id
        AND t.ts <= p.ts
        AND epoch_us(t.ts) >= epoch_us(p.ts) - $window_days * 86400000000
    QUALIFY row_number() OVER (
        PARTITION BY p.provider, p.provider_payment_id
        ORDER BY
            CASE WHEN $model = 'first_touch' THEN epoch_us(t.ts)
                 ELSE (t.channel = 'Direct')::BIGINT END,
            CASE WHEN $model = 'first_touch' THEN 0 ELSE -epoch_us(t.ts) END,
            t.session_id
    ) = 1
),
firsts AS (
    SELECT provider, customer_id, provider_payment_id
    FROM pays
    WHERE customer_id IS NOT NULL AND status <> 'failed'
    QUALIFY row_number() OVER (PARTITION BY provider, customer_id ORDER BY ts, provider_payment_id) = 1
)
SELECT $site, p.provider, p.provider_payment_id, $model,
    coalesce(inh.session_id, o.session_id),
    coalesce(inh.channel, o.channel),
    CASE WHEN inh.session_id IS NOT NULL THEN inh.referrer_host ELSE o.referrer_host END,
    CASE WHEN inh.session_id IS NOT NULL THEN inh.utm_campaign ELSE o.utm_campaign END,
    coalesce(inh.path, o.path),
    CASE WHEN inh.session_id IS NOT NULL THEN inh.country ELSE o.country END,
    coalesce(inh.device, o.device),
    coalesce(inh.ts, o.ts)
FROM pays p
LEFT JOIN own o USING (provider, provider_payment_id)
LEFT JOIN firsts f ON p.is_renewal AND f.provider = p.provider AND f.customer_id = p.customer_id
LEFT JOIN own inh ON inh.provider = f.provider AND inh.provider_payment_id = f.provider_payment_id
";

#[derive(Debug, Clone)]
pub struct LatestPageview {
    pub path: String,
    pub country: Option<String>,
    pub device: String,
    pub referrer_host: Option<String>,
}

pub struct DuckStore {
    conn: Mutex<Connection>,
}

impl DuckStore {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        // Install and load ICU up front (autoloading it mid-query fails that query); keep naive timestamps in UTC.
        conn.execute_batch(
            "INSTALL icu;
            LOAD icu;
            SET TimeZone = 'UTC';
            CREATE TABLE IF NOT EXISTS events (
                site_id VARCHAR NOT NULL,
                ts TIMESTAMPTZ NOT NULL,
                visitor_id VARCHAR NOT NULL,
                session_id VARCHAR NOT NULL,
                kind VARCHAR NOT NULL,
                name VARCHAR,
                path VARCHAR NOT NULL,
                referrer_host VARCHAR,
                utm_source VARCHAR,
                utm_medium VARCHAR,
                utm_campaign VARCHAR,
                utm_content VARCHAR,
                utm_term VARCHAR,
                channel VARCHAR NOT NULL,
                country VARCHAR,
                device VARCHAR NOT NULL,
                browser VARCHAR NOT NULL,
                os VARCHAR,
                region VARCHAR,
                city VARCHAR,
                host VARCHAR,
                language VARCHAR
            );
            ALTER TABLE events ADD COLUMN IF NOT EXISTS os VARCHAR;
            ALTER TABLE events ADD COLUMN IF NOT EXISTS region VARCHAR;
            ALTER TABLE events ADD COLUMN IF NOT EXISTS city VARCHAR;
            ALTER TABLE events ADD COLUMN IF NOT EXISTS host VARCHAR;
            ALTER TABLE events ADD COLUMN IF NOT EXISTS language VARCHAR;
            CREATE TABLE IF NOT EXISTS payments (
                site_id VARCHAR NOT NULL,
                provider VARCHAR NOT NULL,
                provider_payment_id VARCHAR NOT NULL,
                ts TIMESTAMPTZ NOT NULL,
                amount BIGINT NOT NULL,
                currency VARCHAR NOT NULL,
                amount_usd DOUBLE,
                refunded BIGINT NOT NULL,
                status VARCHAR NOT NULL,
                visitor_id VARCHAR,
                customer_id VARCHAR,
                is_renewal BOOLEAN NOT NULL,
                PRIMARY KEY (site_id, provider, provider_payment_id)
            );
            CREATE TABLE IF NOT EXISTS attribution (
                site_id VARCHAR NOT NULL,
                provider VARCHAR NOT NULL,
                provider_payment_id VARCHAR NOT NULL,
                model VARCHAR NOT NULL,
                session_id VARCHAR,
                channel VARCHAR,
                referrer_host VARCHAR,
                campaign VARCHAR,
                landing_page VARCHAR,
                country VARCHAR,
                device VARCHAR,
                touch_ts TIMESTAMPTZ,
                PRIMARY KEY (site_id, provider, provider_payment_id)
            );
            CREATE TABLE IF NOT EXISTS subscriptions (
                site_id VARCHAR NOT NULL,
                provider VARCHAR NOT NULL,
                subscription_id VARCHAR NOT NULL,
                customer_id VARCHAR NOT NULL,
                status VARCHAR NOT NULL,
                live BOOLEAN NOT NULL,
                ends_at TIMESTAMPTZ,
                PRIMARY KEY (site_id, provider, subscription_id)
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl EventStore for DuckStore {
    fn insert_events(&self, events: &[Event]) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut app = tx.appender("events")?;
            for e in events {
                app.append_row(params![
                    e.site_id,
                    Value::Timestamp(
                        TimeUnit::Microsecond,
                        (e.ts.unix_timestamp_nanos() / 1000) as i64
                    ),
                    e.visitor_id,
                    e.session_id,
                    e.kind,
                    e.name,
                    e.path,
                    e.referrer_host,
                    e.utm_source,
                    e.utm_medium,
                    e.utm_campaign,
                    e.utm_content,
                    e.utm_term,
                    e.channel,
                    e.country,
                    e.device,
                    e.browser,
                    e.os,
                    e.region,
                    e.city,
                    e.host,
                    e.language,
                ])?;
            }
            app.flush()?;
        }
        tx.commit()?;
        Ok(())
    }

    fn replace_subscriptions(
        &self,
        site_id: &str,
        provider: &str,
        subscriptions: &[Subscription],
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM subscriptions WHERE site_id = ? AND provider = ?",
            params![site_id, provider],
        )?;
        {
            let mut stmt =
                tx.prepare("INSERT OR REPLACE INTO subscriptions VALUES (?, ?, ?, ?, ?, ?, ?)")?;
            for s in subscriptions {
                stmt.execute(params![
                    site_id,
                    provider,
                    s.subscription_id,
                    s.customer_id,
                    s.status,
                    s.live,
                    s.ends_at.map(|t| Value::Timestamp(
                        TimeUnit::Microsecond,
                        (t.unix_timestamp_nanos() / 1000) as i64
                    )),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn retention(
        &self,
        site_id: &str,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Retention> {
        retention::retention(&self.conn.lock().unwrap(), site_id, period, now)
    }

    #[cfg(test)]
    fn count_events(&self, site_id: &str) -> anyhow::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT count(*) FROM events WHERE site_id = ?",
            params![site_id],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    fn upsert_payments(&self, payments: &[Payment]) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO payments VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )?;
            for p in payments {
                stmt.execute(params![
                    p.site_id,
                    p.provider,
                    p.provider_payment_id,
                    Value::Timestamp(
                        TimeUnit::Microsecond,
                        (p.ts.unix_timestamp_nanos() / 1000) as i64
                    ),
                    p.amount,
                    p.currency,
                    p.amount_usd,
                    p.refunded,
                    p.status,
                    p.visitor_id,
                    p.customer_id,
                    p.is_renewal,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn latest_pageview(&self, site_id: &str) -> anyhow::Result<Option<LatestPageview>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT path, country, device, referrer_host FROM events WHERE site_id = ? AND kind = 'pageview' ORDER BY ts DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![site_id], |r| {
            Ok(LatestPageview {
                path: r.get(0)?,
                country: r.get(1)?,
                device: r.get(2)?,
                referrer_host: r.get(3)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    fn first_event(&self, site_id: &str) -> anyhow::Result<Option<OffsetDateTime>> {
        let conn = self.conn.lock().unwrap();
        let us: Option<i64> = conn.query_row(
            "SELECT epoch_us(min(ts)) FROM events WHERE site_id = ?",
            params![site_id],
            |r| r.get(0),
        )?;
        Ok(us.map(stats::from_micros))
    }

    fn live_visitors(&self, site_id: &str) -> anyhow::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT count(DISTINCT visitor_id) FROM events WHERE site_id = ? AND kind <> 'leave' AND ts > now() - INTERVAL 5 MINUTE",
            params![site_id],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    fn utc_offset(&self, tz: &str) -> anyhow::Result<UtcOffset> {
        let conn = self.conn.lock().unwrap();
        let secs: i64 = conn.query_row(
            "SELECT (epoch_us(timezone(?, now())) - epoch_us(timezone('UTC', now()))) // 1000000",
            params![tz],
            |r| r.get(0),
        )?;
        Ok(UtcOffset::from_whole_seconds(secs as i32)?)
    }

    fn rebuild_attribution(
        &self,
        site_id: &str,
        model: &str,
        window_days: i64,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            matches!(model, "last_non_direct" | "first_touch"),
            "unknown attribution model {model}"
        );
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM attribution WHERE site_id = ?",
            params![site_id],
        )?;
        tx.execute(
            REBUILD_ATTRIBUTION,
            named_params! { "site": site_id, "model": model, "window_days": window_days },
        )?;
        tx.commit()?;
        Ok(())
    }

    fn overview_in(&self, site_id: &str, period: &Period, f: &Filter) -> anyhow::Result<Overview> {
        stats::overview(&self.conn.lock().unwrap(), site_id, period, f)
    }

    #[cfg(test)]
    fn timeseries(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<stats::DayPoint>> {
        stats::timeseries(
            &self.conn.lock().unwrap(),
            site_id,
            period,
            &Filter::default(),
        )
    }

    fn breakdown_in(
        &self,
        site_id: &str,
        period: &Period,
        dimension: Dimension,
        f: &Filter,
    ) -> anyhow::Result<Vec<BreakdownRow>> {
        stats::breakdown(&self.conn.lock().unwrap(), site_id, period, dimension, f)
    }

    fn last_pages_in(
        &self,
        site_id: &str,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Vec<BreakdownRow>> {
        stats::last_pages(&self.conn.lock().unwrap(), site_id, period, f)
    }

    fn payment_list_in(
        &self,
        site_id: &str,
        period: &Period,
        filter: PaymentFilter,
        limit: u32,
        offset: u32,
        f: &Filter,
    ) -> anyhow::Result<PaymentPage> {
        stats::payment_list(
            &self.conn.lock().unwrap(),
            site_id,
            period,
            filter,
            limit,
            offset,
            f,
        )
    }

    fn journey(
        &self,
        site_id: &str,
        provider: &str,
        provider_payment_id: &str,
        window_days: i64,
    ) -> anyhow::Result<Option<Journey>> {
        stats::journey(
            &self.conn.lock().unwrap(),
            site_id,
            provider,
            provider_payment_id,
            window_days,
        )
    }

    fn report_in(&self, site_id: &str, period: &Period, f: &Filter) -> anyhow::Result<Report> {
        stats::report(&self.conn.lock().unwrap(), site_id, period, f)
    }

    fn hourly(
        &self,
        site_id: &str,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Vec<stats::DayPoint>> {
        stats::hourly(&self.conn.lock().unwrap(), site_id, period, f)
    }

    fn payment_split(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<ProviderSplit>> {
        stats::payment_split(&self.conn.lock().unwrap(), site_id, period)
    }

    fn funnel(
        &self,
        site_id: &str,
        period: &Period,
        steps: &[Step],
        f: &Filter,
    ) -> anyhow::Result<Vec<StepCount>> {
        stats::funnel(&self.conn.lock().unwrap(), site_id, period, steps, f)
    }

    fn goal_table(
        &self,
        site_id: &str,
        period: &Period,
        pages: &[String],
        f: &Filter,
    ) -> anyhow::Result<Vec<GoalStat>> {
        stats::goal_table(&self.conn.lock().unwrap(), site_id, period, pages, f)
    }

    fn payment_summary(&self, site_id: &str, period: &Period) -> anyhow::Result<PaymentSummary> {
        stats::payment_summary(&self.conn.lock().unwrap(), site_id, period)
    }

    fn activity(&self, site_id: &str, tz: &str, who: &[Who]) -> anyhow::Result<Vec<Activity>> {
        stats::activity(&self.conn.lock().unwrap(), site_id, tz, who)
    }

    fn goal_counts(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<GoalCount>> {
        stats::goal_counts(&self.conn.lock().unwrap(), site_id, period)
    }

    fn goal_list(
        &self,
        site_id: &str,
        period: &Period,
        goal: &str,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<(u64, Vec<GoalRow>)> {
        stats::goal_list(
            &self.conn.lock().unwrap(),
            site_id,
            period,
            goal,
            limit,
            offset,
        )
    }

    fn visitor_list(
        &self,
        site_id: &str,
        period: &Period,
        filter: &VisitorFilter,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<VisitorPage> {
        visitors::visitor_list(
            &self.conn.lock().unwrap(),
            site_id,
            period,
            filter,
            limit,
            offset,
        )
    }

    fn visitor_facets(&self, site_id: &str, period: &Period) -> anyhow::Result<Facets> {
        visitors::facets(&self.conn.lock().unwrap(), site_id, period)
    }

    fn timeline(&self, site_id: &str, visitor_id: &str) -> anyhow::Result<Option<Timeline>> {
        visitors::timeline(&self.conn.lock().unwrap(), site_id, visitor_id)
    }
}

#[cfg(test)]
type EventRow = (
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);

#[cfg(test)]
impl DuckStore {
    /// (path, channel, referrer_host, kind, name, country, device, visitor_id) for every event, oldest first.
    pub fn dump(&self) -> Vec<EventRow> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT path, channel, referrer_host, kind, name, country, device, visitor_id FROM events ORDER BY ts, path")
            .unwrap();
        stmt.query_map([], |r| {
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
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    }
}

#[cfg(test)]
mod payment_tests {
    use super::*;

    fn payment(id: &str, refunded: i64) -> Payment {
        Payment {
            site_id: "s".into(),
            provider: "stripe".into(),
            provider_payment_id: id.into(),
            ts: OffsetDateTime::from_unix_timestamp(1_760_000_000).unwrap(),
            amount: 2900,
            currency: "usd".into(),
            amount_usd: Some(29.0),
            refunded,
            status: if refunded > 0 {
                "refunded".into()
            } else {
                "paid".into()
            },
            visitor_id: Some("v1".into()),
            customer_id: Some("cus_1".into()),
            is_renewal: false,
        }
    }

    #[test]
    fn upserts_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        store
            .upsert_payments(&[payment("pi_1", 0), payment("pi_2", 0)])
            .unwrap();
        store.upsert_payments(&[payment("pi_1", 2900)]).unwrap();
        let period = Period {
            from: time::macros::date!(2025 - 01 - 01),
            to: time::macros::date!(2026 - 12 - 31),
            tz: "UTC".into(),
        };
        let rows = store
            .payment_list("s", &period, PaymentFilter::All, 10, 0)
            .unwrap()
            .rows;
        assert_eq!(rows.len(), 2);
        let pi_1 = rows
            .iter()
            .find(|p| p.provider_payment_id == "pi_1")
            .unwrap();
        assert_eq!((pi_1.refunded, pi_1.net_usd), (2900, Some(0.0)));
    }
}
