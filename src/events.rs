use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use duckdb::types::{TimeUnit, Value};
use duckdb::{Connection, named_params, params};
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::channel::Channel;
use crate::index::{self, Indexes, SiteIndex, Zones, goals, read, rows};
use crate::providers::Subscription;
use crate::retention::{self, Mrr, Retention};
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
    /// Monthly recurring revenue of paying subscriptions now, and the part from customers who first paid in the period; None until a subscription has a price.
    fn mrr(
        &self,
        site_id: &str,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Option<Mrr>>;
    /// Customers acquired in the period and how many still pay, by first channel, referrer and campaign.
    fn retention(
        &self,
        site_id: &str,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Retention>;
    #[cfg(test)]
    fn count_events(&self, site_id: &str) -> anyhow::Result<u64>;
    /// The first pageview of the newest visitor, as the dashboard shows that visitor.
    fn latest_pageview(&self, site_id: &str) -> anyhow::Result<Option<LatestPageview>>;
    /// When the site's first event arrived.
    fn first_event(&self, site_id: &str) -> anyhow::Result<Option<OffsetDateTime>>;
    /// Bumped on every write to the site; 0 for stores that do not count writes.
    fn generation(&self, _site_id: &str) -> u64 {
        0
    }
    /// Site ids as their data changes; None for stores that do not announce writes.
    fn subscribe(&self) -> Option<tokio::sync::broadcast::Receiver<String>> {
        None
    }
    /// Stores events flagged as bots, with why, apart from visits and every metric.
    fn insert_bots(&self, bots: &[(Event, &str)]) -> anyhow::Result<()>;
    /// Bot visitors filtered in the last `days` days.
    fn bots_filtered(&self, site_id: &str, days: i64) -> anyhow::Result<u64>;
    /// Current UTC offset of IANA zone `tz`.
    fn utc_offset(&self, tz: &str) -> anyhow::Result<UtcOffset>;

    /// Whether reads of the site no longer wait on a background load.
    fn settled(&self, _site_id: &str) -> bool {
        true
    }
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
WITH starts AS (
    SELECT visitor_id, session_id, ts, channel, referrer_host, utm_campaign, path, country, device
    FROM touches
    WHERE site_id = $site
),
pays AS (
    SELECT provider, provider_payment_id, ts, visitor_id, customer_id, is_renewal, status
    FROM payments WHERE site_id = $site
),
own AS (
    SELECT p.provider, p.provider_payment_id, t.*
    FROM pays p
    JOIN starts t ON t.visitor_id = p.visitor_id
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

/// Marks every payment after a customer's first kept payment, across all time, as a renewal.
const MARK_RENEWALS: &str = "
UPDATE payments SET is_renewal = true
FROM (
    SELECT site_id, provider, provider_payment_id
    FROM payments
    WHERE customer_id IS NOT NULL AND status <> 'failed' AND site_id = coalesce(?, site_id)
    QUALIFY row_number() OVER (PARTITION BY site_id, provider, customer_id ORDER BY ts, provider_payment_id) > 1
) later
WHERE NOT payments.is_renewal AND payments.site_id = later.site_id
    AND payments.provider = later.provider AND payments.provider_payment_id = later.provider_payment_id
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
    /// Writes `touches` and `first_touches` apart from events; writes that include touch rows hold it too.
    touch_conn: Arc<Mutex<Connection>>,
    pending: Arc<Mutex<Pending>>,
    /// Set while a flush of `pending` is scheduled.
    flush_queued: Arc<AtomicBool>,
    index: Arc<Indexes>,
    zones: Zones,
    /// Idle read connections, so reads never wait on each other or on writes.
    readers: Mutex<Vec<Connection>>,
    /// Events waiting to be written, so concurrent inserts share one transaction.
    queue: Mutex<Queue>,
    written: Condvar,
}

#[derive(Default)]
struct Queue {
    events: Vec<Event>,
    /// Batch the queued events go out in, the last batch written, and whether one is being written.
    next: u64,
    done: u64,
    writing: bool,
    /// Errors of failed batches.
    failed: HashMap<u64, String>,
}

/// A pooled read connection, returned to the pool on drop.
pub struct Reader<'a> {
    store: &'a DuckStore,
    conn: Option<Connection>,
}

impl std::ops::Deref for Reader<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.conn.as_ref().unwrap()
    }
}

impl Drop for Reader<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.store.readers.lock().unwrap().push(conn);
        }
    }
}

/// Memory budget of session indexes in bytes, from VIRLING_INDEX_MB (0 turns them off), else a quarter of memory and at least 512 MB.
pub fn index_cap() -> usize {
    match std::env::var("VIRLING_INDEX_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        Some(mb) => mb * 1_000_000,
        None => (memory_bytes().unwrap_or(0) / 4).max(512_000_000),
    }
}

/// Memory this process may use: the machine's (Linux, else macOS sysctl), or its container's limit when lower.
fn memory_bytes() -> Option<usize> {
    let total = match std::fs::read_to_string("/proc/meminfo") {
        Ok(info) => {
            info.lines()
                .find_map(|l| l.strip_prefix("MemTotal:"))?
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<usize>()
                .ok()?
                * 1024
        }
        Err(_) => {
            let out = std::process::Command::new("sysctl")
                .args(["-n", "hw.memsize"])
                .output()
                .ok()?;
            String::from_utf8(out.stdout).ok()?.trim().parse().ok()?
        }
    };
    let limit = std::fs::read_to_string("/sys/fs/cgroup/memory.max")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok());
    Some(limit.map_or(total, |l| l.min(total)))
}

/// A stored utm_medium, utm_source or referrer host.
type Tag = Option<String>;

/// Moves stored visits from community sites into Communities, which used to be Social; returns rows changed.
fn reclassify_channels(conn: &Connection) -> anyhow::Result<usize> {
    let mut stmt =
        conn.prepare("SELECT DISTINCT utm_medium, utm_source, referrer_host, channel FROM events")?;
    let rows: Vec<(Tag, Tag, Tag, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let stale: Vec<_> = rows
        .into_iter()
        .filter_map(|(m, s, h, c)| {
            let now = crate::channel::classify(m.as_deref(), s.as_deref(), h.as_deref());
            (now == Channel::Communities && c != now.as_str()).then_some((m, s, h, now.as_str()))
        })
        .collect();
    let mut changed = 0;
    for table in ["events", "touches", "first_touches"] {
        for (m, s, h, channel) in &stale {
            changed += conn.execute(
                &format!(
                    "UPDATE {table} SET channel = ? WHERE utm_medium IS NOT DISTINCT FROM ?
                     AND utm_source IS NOT DISTINCT FROM ? AND referrer_host IS NOT DISTINCT FROM ?"
                ),
                params![channel, m, s, h],
            )?;
        }
    }
    Ok(changed)
}

/// Columns of `touches` and `first_touches`: an event's fields without kind and name.
const TOUCH_COLUMNS: &str =
    "site_id, session_id, visitor_id, ts, path, referrer_host, utm_source, utm_medium,
    utm_campaign, utm_content, utm_term, channel, country, device, browser, os, region, city, host,
    language";

const TOUCH_TABLE: &str = "site_id VARCHAR NOT NULL, session_id VARCHAR NOT NULL,
    visitor_id VARCHAR NOT NULL, ts TIMESTAMPTZ NOT NULL, path VARCHAR NOT NULL, referrer_host VARCHAR,
    utm_source VARCHAR, utm_medium VARCHAR, utm_campaign VARCHAR, utm_content VARCHAR, utm_term VARCHAR,
    channel VARCHAR NOT NULL, country VARCHAR, device VARCHAR NOT NULL, browser VARCHAR NOT NULL,
    os VARCHAR, region VARCHAR, city VARCHAR, host VARCHAR, language VARCHAR";

/// The touch tables and their keys.
const TOUCH_TABLES: [(&str, &str); 2] =
    [("touches", "session_id"), ("first_touches", "visitor_id")];

/// Keeps the earlier (ts, path) pageview of a key in `touches` or `first_touches`.
fn keep_earlier(table: &str, key: &str) -> String {
    let set = TOUCH_COLUMNS
        .split(',')
        .map(str::trim)
        .map(|c| format!("{c} = excluded.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "ON CONFLICT (site_id, {key}) DO UPDATE SET {set}
        WHERE excluded.ts < {table}.ts OR (excluded.ts = {table}.ts AND excluded.path < {table}.path)"
    )
}

fn upsert_touches(table: &str, key: &str, rows: usize) -> String {
    let n = TOUCH_COLUMNS.split(',').count();
    let row = format!("({})", vec!["?"; n].join(", "));
    format!(
        "INSERT INTO {table} ({TOUCH_COLUMNS}) VALUES {} {}",
        vec![row; rows].join(", "),
        keep_earlier(table, key)
    )
}

fn ts_value(ts: OffsetDateTime) -> Value {
    Value::Timestamp(
        TimeUnit::Microsecond,
        (ts.unix_timestamp_nanos() / 1000) as i64,
    )
}

/// The earliest pageview, by (ts, path), per key of `events`.
fn earliest(events: &[Event], key: impl Fn(&Event) -> &str) -> Vec<&Event> {
    let mut first: std::collections::HashMap<(&str, &str), &Event> = Default::default();
    for e in events.iter().filter(|e| e.kind == "pageview") {
        first
            .entry((e.site_id.as_str(), key(e)))
            .and_modify(|f| {
                if (e.ts, &e.path) < (f.ts, &f.path) {
                    *f = e;
                }
            })
            .or_insert(e);
    }
    first.into_values().collect()
}

/// An event's values in TOUCH_COLUMNS order.
fn touch_values(e: &Event) -> [Value; 20] {
    let text = |v: &str| Value::Text(v.to_string());
    let opt = |v: &Option<String>| v.as_deref().map_or(Value::Null, text);
    [
        text(&e.site_id),
        text(&e.session_id),
        text(&e.visitor_id),
        ts_value(e.ts),
        text(&e.path),
        opt(&e.referrer_host),
        opt(&e.utm_source),
        opt(&e.utm_medium),
        opt(&e.utm_campaign),
        opt(&e.utm_content),
        opt(&e.utm_term),
        text(&e.channel),
        opt(&e.country),
        text(&e.device),
        text(&e.browser),
        text(&e.os),
        opt(&e.region),
        opt(&e.city),
        opt(&e.host),
        opt(&e.language),
    ]
}

/// Upserts `rows` into `table`, a few hundred per statement.
fn write_touches(
    tx: &duckdb::Transaction,
    table: &str,
    key: &str,
    rows: &[&Event],
) -> anyhow::Result<()> {
    for chunk in rows.chunks(500) {
        let mut stmt = tx.prepare(&upsert_touches(table, key, chunk.len()))?;
        let values: Vec<Value> = chunk.iter().flat_map(|e| touch_values(e)).collect();
        stmt.execute(duckdb::params_from_iter(values))?;
    }
    Ok(())
}

/// First pageviews stored in `events` but not yet in `touches` and `first_touches`, per table: new keys, then earlier ones.
#[derive(Default, Clone)]
struct Pending {
    new: [Vec<Event>; 2],
    earlier: [Vec<Event>; 2],
}

impl Pending {
    fn len(&self) -> usize {
        self.new.iter().chain(&self.earlier).map(Vec::len).sum()
    }

    fn extend(&mut self, other: Pending) {
        for (t, (new, earlier)) in other.new.into_iter().zip(other.earlier).enumerate() {
            self.new[t].extend(new);
            self.earlier[t].extend(earlier);
        }
    }

    /// Appends the new keys, then upserts the earliest of the earlier pageviews per key.
    fn write(&self, tx: &duckdb::Transaction) -> anyhow::Result<()> {
        for (t, (table, key)) in TOUCH_TABLES.into_iter().enumerate() {
            if !self.new[t].is_empty() {
                let mut app = tx.appender(table)?;
                for e in &self.new[t] {
                    app.append_row(touch_values(e))?;
                }
                app.flush()?;
            }
            let earlier = if t == 0 {
                earliest(&self.earlier[t], |e| &e.session_id)
            } else {
                earliest(&self.earlier[t], |e| &e.visitor_id)
            };
            write_touches(tx, table, key, &earlier)?;
        }
        Ok(())
    }
}

/// Writes the pending first pageviews on `conn`, the store's touch connection, while events keep arriving.
fn flush_touches(conn: &mut Connection, pending: &Mutex<Pending>) -> anyhow::Result<()> {
    let batch = std::mem::take(&mut *pending.lock().unwrap());
    if batch.len() == 0 {
        return Ok(());
    }
    let written = (|| {
        let tx = conn.transaction()?;
        batch.write(&tx)?;
        Ok(tx.commit()?)
    })();
    if written.is_err() {
        pending.lock().unwrap().extend(batch);
    }
    written
}

/// Live writes leave first pageviews pending this long, so a burst of events shares one write of them.
const FLUSH_AFTER: std::time::Duration = std::time::Duration::from_millis(20);
/// Most first pageviews left pending before a write includes them.
const MAX_PENDING: usize = 1000;

impl DuckStore {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        let derived: i64 = conn.query_row(
            "SELECT count(*) FROM duckdb_columns()
            WHERE table_name IN ('touches', 'first_touches') AND column_name = 'language'",
            [],
            |r| r.get(0),
        )?;
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
            CREATE TABLE IF NOT EXISTS bot_events (
                site_id VARCHAR NOT NULL,
                ts TIMESTAMPTZ NOT NULL,
                visitor_id VARCHAR NOT NULL,
                kind VARCHAR NOT NULL,
                path VARCHAR NOT NULL,
                referrer_host VARCHAR,
                country VARCHAR,
                device VARCHAR NOT NULL,
                browser VARCHAR NOT NULL,
                os VARCHAR,
                language VARCHAR,
                reason VARCHAR NOT NULL
            );
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
            );
            ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS amount BIGINT;
            ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS currency VARCHAR;
            ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS billing_interval VARCHAR;
            ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS interval_count INTEGER;
            ALTER TABLE subscriptions ADD COLUMN IF NOT EXISTS mrr_usd DOUBLE;",
        )?;
        if derived < 2 {
            // First pageview, by (ts, path), of each session and of each visitor; insert_events keeps them current.
            conn.execute_batch(&format!(
                "BEGIN;
                DROP TABLE IF EXISTS touches;
                DROP TABLE IF EXISTS first_touches;
                CREATE TABLE touches ({TOUCH_TABLE}, PRIMARY KEY (site_id, session_id));
                INSERT INTO touches SELECT {TOUCH_COLUMNS} FROM events WHERE kind = 'pageview'
                    QUALIFY row_number() OVER (PARTITION BY site_id, session_id ORDER BY ts, path) = 1;
                CREATE TABLE first_touches ({TOUCH_TABLE}, PRIMARY KEY (site_id, visitor_id));
                INSERT INTO first_touches SELECT {TOUCH_COLUMNS} FROM touches
                    QUALIFY row_number() OVER (PARTITION BY site_id, visitor_id ORDER BY ts, path) = 1;
                COMMIT;"
            ))?;
        }
        // Live writes leave their newest first pageviews pending in memory; fold the last two days back in.
        for (table, key) in TOUCH_TABLES {
            conn.execute_batch(&format!(
                "INSERT INTO {table} ({TOUCH_COLUMNS}) SELECT * FROM (
                    SELECT {TOUCH_COLUMNS} FROM events WHERE kind = 'pageview'
                        AND ts >= (SELECT max(ts) FROM events) - INTERVAL 2 DAY
                    QUALIFY row_number() OVER (PARTITION BY site_id, {key} ORDER BY ts, path) = 1
                ) {}",
                keep_earlier(table, key)
            ))?;
        }
        conn.execute(MARK_RENEWALS, params![None::<&str>])?;
        conn.execute(
            "UPDATE payments SET amount_usd = round(amount_usd, 2) WHERE amount_usd <> round(amount_usd, 2)",
            [],
        )?;
        let changed = reclassify_channels(&conn)?;
        if changed > 0 {
            tracing::info!(changed, "moved stored visits to their current channel");
        }
        Ok(Self {
            touch_conn: Arc::new(Mutex::new(conn.try_clone()?)),
            conn: Mutex::new(conn),
            pending: Arc::default(),
            flush_queued: Arc::default(),
            index: Arc::new(Indexes::new(index_cap())),
            zones: Zones::default(),
            readers: Mutex::default(),
            queue: Mutex::new(Queue {
                next: 1,
                ..Default::default()
            }),
            written: Condvar::new(),
        })
    }

    /// `read` over the site's session index, when it is loaded and fits the question.
    fn indexed<T>(
        &self,
        site_id: &str,
        read: impl FnOnce(&SiteIndex, &Zones, &Connection) -> anyhow::Result<Option<T>>,
    ) -> anyhow::Result<Option<T>> {
        let Some(ix) = self.index.get(site_id, || self.snapshot_conn())? else {
            return Ok(None);
        };
        let conn = self.reader()?;
        let ix = ix.read().unwrap();
        read(&ix, &self.zones, &conn)
    }

    /// Reads the site's index now instead of in the background.
    #[cfg(test)]
    pub fn load_index(&self, site_id: &str) {
        let conn = self.snapshot_conn().unwrap();
        self.index.load(site_id, &conn).unwrap();
    }

    #[cfg(test)]
    pub fn index_for_test(&self, site_id: &str) -> Arc<std::sync::RwLock<SiteIndex>> {
        self.index
            .get(site_id, || self.snapshot_conn())
            .unwrap()
            .expect("index loaded")
    }

    #[cfg(test)]
    pub fn conn_for_test(&self) -> Reader<'_> {
        self.read().unwrap()
    }

    /// A connection for reading the index from, after pending first pageviews are written so its snapshot has them.
    fn snapshot_conn(&self) -> anyhow::Result<Connection> {
        let conn = self.conn.lock().unwrap();
        flush_touches(&mut self.touch_conn.lock().unwrap(), &self.pending)?;
        Ok(conn.try_clone()?)
    }

    /// Writes the pending first pageviews soon, once a burst of events is likely over.
    fn flush_later(&self) {
        if self.flush_queued.swap(true, Ordering::AcqRel) {
            return;
        }
        // Weak, so a dropped store closes its database; reopening it folds anything pending back in.
        let (conn, pending, queued) = (
            Arc::downgrade(&self.touch_conn),
            Arc::downgrade(&self.pending),
            self.flush_queued.clone(),
        );
        std::thread::spawn(move || {
            std::thread::sleep(FLUSH_AFTER);
            queued.store(false, Ordering::Release);
            let (Some(conn), Some(pending)) = (conn.upgrade(), pending.upgrade()) else {
                return;
            };
            if let Err(err) = flush_touches(&mut conn.lock().unwrap(), &pending) {
                tracing::error!(%err, "writing first pageviews failed");
            }
        });
    }

    /// Writes events in one transaction, then adds them to loaded indexes; returns whether their first pageviews were left pending.
    fn write_events(&self, events: &[Event]) -> anyhow::Result<bool> {
        let _gate = self.index.gate();
        let mut conn = self.conn.lock().unwrap();
        // A loaded index knows every stored first pageview: append new keys, upsert only earlier ones.
        let mut ready: HashMap<&str, Option<Arc<std::sync::RwLock<SiteIndex>>>> = HashMap::new();
        let mut fresh = Pending::default();
        for (t, by_session) in [true, false].into_iter().enumerate() {
            let rows = if by_session {
                earliest(events, |e| &e.session_id)
            } else {
                earliest(events, |e| &e.visitor_id)
            };
            for e in rows {
                let ix = ready
                    .entry(&e.site_id)
                    .or_insert_with(|| self.index.ready(&e.site_id));
                let Some(ix) = ix else {
                    fresh.earlier[t].push(e.clone());
                    continue;
                };
                let ix = ix.read().unwrap();
                let id = if by_session {
                    &e.session_id
                } else {
                    &e.visitor_id
                };
                match ix.first_view(by_session, id) {
                    None => fresh.new[t].push(e.clone()),
                    Some((ts, path))
                        if (e.ts, e.path.as_str()) < (stats::from_micros(ts), path) =>
                    {
                        fresh.earlier[t].push(e.clone())
                    }
                    Some(_) => {}
                }
            }
        }
        // Live events leave their first pageviews pending unless an index is being read from a snapshot.
        let live = OffsetDateTime::now_utc() - Duration::hours(1);
        let defer = events.iter().all(|e| e.ts > live)
            && self.pending.lock().unwrap().len() + fresh.len() <= MAX_PENDING
            && !self.index.loading();
        // Writing touch rows here waits out a flush, so both never write the same key.
        let touches = (!defer).then(|| self.touch_conn.lock().unwrap());
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
        let left = defer && fresh.len() > 0;
        if defer {
            tx.commit()?;
            self.pending.lock().unwrap().extend(fresh);
        } else {
            let mut pending = self.pending.lock().unwrap();
            let mut all = pending.clone();
            all.extend(fresh);
            all.write(&tx)?;
            tx.commit()?;
            *pending = Pending::default();
        }
        drop(touches);
        self.index.apply(events);
        drop(conn);
        Ok(left)
    }

    /// A reader that sees every first pageview written so far.
    fn read(&self) -> anyhow::Result<Reader<'_>> {
        // Taking the lock also waits for a background flush that already took the pending rows.
        flush_touches(&mut self.touch_conn.lock().unwrap(), &self.pending)?;
        self.reader()
    }

    /// A reader for queries that do not read `touches` or `first_touches`.
    fn reader(&self) -> anyhow::Result<Reader<'_>> {
        let pooled = self.readers.lock().unwrap().pop();
        let conn = match pooled {
            Some(conn) => conn,
            None => {
                let conn = self.conn.lock().unwrap().try_clone()?;
                conn.execute_batch("SET TimeZone = 'UTC'")?;
                conn
            }
        };
        Ok(Reader {
            store: self,
            conn: Some(conn),
        })
    }
}

impl EventStore for DuckStore {
    fn insert_events(&self, events: &[Event]) -> anyhow::Result<()> {
        let mut q = self.queue.lock().unwrap();
        q.events.extend_from_slice(events);
        let mine = q.next;
        loop {
            if q.done >= mine {
                return match q.failed.get(&mine) {
                    Some(err) => Err(anyhow::anyhow!("{err}")),
                    None => Ok(()),
                };
            }
            if !q.writing {
                let batch = std::mem::take(&mut q.events);
                q.writing = true;
                q.next += 1;
                drop(q);
                let result = self.write_events(&batch);
                if matches!(result, Ok(true)) {
                    self.flush_later();
                }
                let result = result.map(|_| ());
                q = self.queue.lock().unwrap();
                q.writing = false;
                q.done = mine;
                if let Err(err) = &result {
                    q.failed.insert(mine, format!("{err:#}"));
                    q.failed.retain(|b, _| *b + 1000 > mine);
                }
                self.written.notify_all();
                return result;
            }
            q = self.written.wait(q).unwrap();
        }
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
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO subscriptions (site_id, provider, subscription_id, customer_id,
                    status, live, ends_at, amount, currency, billing_interval, interval_count, mrr_usd)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )?;
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
                    s.amount,
                    s.currency.as_deref().map(str::to_ascii_lowercase),
                    s.interval,
                    s.interval_count,
                    s.mrr_usd,
                ])?;
            }
        }
        tx.commit()?;
        self.index.payments_changed(site_id);
        Ok(())
    }

    fn mrr(
        &self,
        site_id: &str,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Option<Mrr>> {
        if let Some(m) = self.indexed(site_id, |ix, z, c| {
            index::retention::read::mrr(ix, z, c, period, now)
        })? {
            return Ok(m);
        }
        retention::mrr(&*self.read()?, site_id, period, now)
    }

    fn retention(
        &self,
        site_id: &str,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Retention> {
        if let Some(r) = self.indexed(site_id, |ix, z, c| {
            index::retention::read::retention(ix, z, c, period, now)
        })? {
            return Ok(r);
        }
        retention::retention(&*self.read()?, site_id, period, now)
    }

    #[cfg(test)]
    fn count_events(&self, site_id: &str) -> anyhow::Result<u64> {
        let conn = self.read()?;
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
            let sites: std::collections::BTreeSet<&str> =
                payments.iter().map(|p| p.site_id.as_str()).collect();
            for site in sites {
                tx.execute(MARK_RENEWALS, params![site])?;
            }
        }
        tx.commit()?;
        for p in payments {
            self.index.payments_changed(&p.site_id);
        }
        Ok(())
    }

    fn latest_pageview(&self, site_id: &str) -> anyhow::Result<Option<LatestPageview>> {
        let conn = self.read()?;
        let mut stmt = conn.prepare(
            "SELECT path, country, device, referrer_host FROM first_touches WHERE site_id = ? ORDER BY ts DESC LIMIT 1",
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
        if let Some(t) = self.indexed(site_id, |ix, _, _| Ok(Some(ix.first_event())))? {
            return Ok(t.map(stats::from_micros));
        }
        let conn = self.read()?;
        let us: Option<i64> = conn.query_row(
            "SELECT epoch_us(min(ts)) FROM events WHERE site_id = ?",
            params![site_id],
            |r| r.get(0),
        )?;
        Ok(us.map(stats::from_micros))
    }

    fn insert_bots(&self, bots: &[(Event, &str)]) -> anyhow::Result<()> {
        if bots.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        let mut app = conn.appender("bot_events")?;
        for (e, reason) in bots {
            app.append_row(params![
                e.site_id,
                Value::Timestamp(
                    TimeUnit::Microsecond,
                    (e.ts.unix_timestamp_nanos() / 1000) as i64
                ),
                e.visitor_id,
                e.kind,
                e.path,
                e.referrer_host,
                e.country,
                e.device,
                e.browser,
                e.os,
                e.language,
                reason,
            ])?;
        }
        app.flush()?;
        Ok(())
    }

    fn bots_filtered(&self, site_id: &str, days: i64) -> anyhow::Result<u64> {
        let conn = self.read()?;
        let n: i64 = conn.query_row(
            "SELECT count(DISTINCT visitor_id) FROM bot_events WHERE site_id = ? AND ts > now() - to_days(CAST(? AS INTEGER))",
            params![site_id, days],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    fn settled(&self, site_id: &str) -> bool {
        let get = self.index.get(site_id, || self.snapshot_conn());
        get.is_ok_and(|ix| ix.is_some()) || self.index.unfit(site_id)
    }

    fn utc_offset(&self, tz: &str) -> anyhow::Result<UtcOffset> {
        let conn = self.reader()?;
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
        flush_touches(&mut self.touch_conn.lock().unwrap(), &self.pending)?;
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
        self.index.payments_changed(site_id);
        Ok(())
    }

    fn overview_in(&self, site_id: &str, period: &Period, f: &Filter) -> anyhow::Result<Overview> {
        if let Some(o) = self.indexed(site_id, |ix, z, c| read::overview(ix, z, c, period, f))? {
            return Ok(o);
        }
        stats::overview(&*self.read()?, site_id, period, f)
    }

    #[cfg(test)]
    fn timeseries(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<stats::DayPoint>> {
        stats::timeseries(&*self.read()?, site_id, period, &Filter::default())
    }

    fn breakdown_in(
        &self,
        site_id: &str,
        period: &Period,
        dimension: Dimension,
        f: &Filter,
    ) -> anyhow::Result<Vec<BreakdownRow>> {
        if let Some(rows) = self.indexed(site_id, |ix, z, c| {
            read::breakdown(ix, z, c, period, dimension, f)
        })? {
            return Ok(rows);
        }
        stats::breakdown(&*self.read()?, site_id, period, dimension, f)
    }

    fn last_pages_in(
        &self,
        site_id: &str,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Vec<BreakdownRow>> {
        if let Some(rows) =
            self.indexed(site_id, |ix, z, c| read::last_pages(ix, z, c, period, f))?
        {
            return Ok(rows);
        }
        stats::last_pages(&*self.read()?, site_id, period, f)
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
        if let Some(page) = self.indexed(site_id, |ix, z, c| {
            rows::read::payment_list(ix, z, c, period, filter, limit, offset, f)
        })? {
            return Ok(page);
        }
        stats::payment_list(&*self.read()?, site_id, period, filter, limit, offset, f)
    }

    fn journey(
        &self,
        site_id: &str,
        provider: &str,
        provider_payment_id: &str,
        window_days: i64,
    ) -> anyhow::Result<Option<Journey>> {
        if let Some(j) = self.indexed(site_id, |ix, _, _| {
            Ok(ix
                .journey(provider, provider_payment_id, window_days)
                .map(Some))
        })? {
            return Ok(j);
        }
        stats::journey(
            &*self.read()?,
            site_id,
            provider,
            provider_payment_id,
            window_days,
        )
    }

    fn report_in(&self, site_id: &str, period: &Period, f: &Filter) -> anyhow::Result<Report> {
        let indexed = self.indexed(site_id, |ix, z, c| {
            let dims = |d| read::breakdown(ix, z, c, period, d, f);
            let (Some(days), Some(previous_days)) = (
                read::timeseries(ix, z, c, period, f)?,
                read::timeseries(ix, z, c, &period.previous(), f)?,
            ) else {
                return Ok(None);
            };
            Ok(Some(Report {
                overview: read::overview(ix, z, c, period, f)?.unwrap(),
                days,
                previous_days,
                channels: dims(Dimension::Channel)?.unwrap(),
                referrers: dims(Dimension::Referrer)?.unwrap(),
                landing_pages: dims(Dimension::LandingPage)?.unwrap(),
                countries: dims(Dimension::Country)?.unwrap(),
                channel_sources: read::channel_sources(ix, z, c, period, f)?.unwrap(),
                setup: read::setup(ix),
            }))
        })?;
        if let Some(r) = indexed {
            return Ok(r);
        }
        stats::report(&|| self.read(), site_id, period, f)
    }

    fn hourly(
        &self,
        site_id: &str,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Vec<stats::DayPoint>> {
        if let Some(h) = self.indexed(site_id, |ix, z, c| read::hourly(ix, z, c, period, f))? {
            return Ok(h);
        }
        stats::hourly(&*self.read()?, site_id, period, f)
    }

    fn payment_split(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<ProviderSplit>> {
        if let Some(s) = self.indexed(site_id, |ix, z, c| read::payment_split(ix, z, c, period))? {
            return Ok(s);
        }
        stats::payment_split(&*self.read()?, site_id, period)
    }

    fn funnel(
        &self,
        site_id: &str,
        period: &Period,
        steps: &[Step],
        f: &Filter,
    ) -> anyhow::Result<Vec<StepCount>> {
        if let Some(c) = self.indexed(site_id, |ix, z, c| {
            goals::read::funnel(ix, z, c, period, steps, f)
        })? {
            return Ok(c);
        }
        stats::funnel(&*self.read()?, site_id, period, steps, f)
    }

    fn goal_table(
        &self,
        site_id: &str,
        period: &Period,
        pages: &[String],
        f: &Filter,
    ) -> anyhow::Result<Vec<GoalStat>> {
        if let Some(t) = self.indexed(site_id, |ix, z, c| {
            goals::read::goal_table(ix, z, c, period, pages, f)
        })? {
            return Ok(t);
        }
        stats::goal_table(&*self.read()?, site_id, period, pages, f)
    }

    fn payment_summary(&self, site_id: &str, period: &Period) -> anyhow::Result<PaymentSummary> {
        if let Some(s) = self.indexed(site_id, |ix, z, c| {
            rows::read::payment_summary(ix, z, c, period)
        })? {
            return Ok(s);
        }
        stats::payment_summary(&*self.read()?, site_id, period)
    }

    fn activity(&self, site_id: &str, tz: &str, who: &[Who]) -> anyhow::Result<Vec<Activity>> {
        if let Some(a) =
            self.indexed(site_id, |ix, z, c| rows::read::activity(ix, z, c, tz, who))?
        {
            return Ok(a);
        }
        stats::activity(&*self.read()?, site_id, tz, who)
    }

    fn goal_counts(&self, site_id: &str, period: &Period) -> anyhow::Result<Vec<GoalCount>> {
        if let Some(g) = self.indexed(site_id, |ix, z, c| {
            rows::read::goal_counts(ix, z, c, period)
        })? {
            return Ok(g);
        }
        stats::goal_counts(&*self.read()?, site_id, period)
    }

    fn goal_list(
        &self,
        site_id: &str,
        period: &Period,
        goal: &str,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<(u64, Vec<GoalRow>)> {
        if let Some(g) = self.indexed(site_id, |ix, z, c| {
            rows::read::goal_list(ix, z, c, period, goal, limit, offset)
        })? {
            return Ok(g);
        }
        stats::goal_list(&*self.read()?, site_id, period, goal, limit, offset)
    }

    fn visitor_list(
        &self,
        site_id: &str,
        period: &Period,
        filter: &VisitorFilter,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<VisitorPage> {
        if let Some(page) = self.indexed(site_id, |ix, z, c| {
            rows::read::visitor_list(ix, z, c, period, filter, limit, offset)
        })? {
            return Ok(page);
        }
        visitors::visitor_list(&*self.read()?, site_id, period, filter, limit, offset)
    }

    fn visitor_facets(&self, site_id: &str, period: &Period) -> anyhow::Result<Facets> {
        if let Some(f) = self.indexed(site_id, |ix, z, c| read::facets(ix, z, c, period))? {
            return Ok(f);
        }
        visitors::facets(&*self.read()?, site_id, period)
    }

    fn timeline(&self, site_id: &str, visitor_id: &str) -> anyhow::Result<Option<Timeline>> {
        if let Some(t) = self.indexed(site_id, |ix, _, _| Ok(Some(ix.timeline(visitor_id))))? {
            return Ok(t);
        }
        visitors::timeline(&*self.read()?, site_id, visitor_id)
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
    #[test]
    fn a_repeat_customer_is_new_once_across_all_time() {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        let at = |d: time::Date| d.with_hms(12, 0, 0).unwrap().assume_utc();
        let (aug, oct) = (
            time::macros::date!(2026 - 08 - 21),
            time::macros::date!(2026 - 10 - 14),
        );
        let first = Payment {
            ts: at(aug),
            provider: "dodo".into(),
            ..payment("pay_aug", 0)
        };
        let again = Payment {
            ts: at(oct),
            provider_payment_id: "pay_oct".into(),
            ..first.clone()
        };
        // The later payment syncs first, as a range-limited sync would.
        store.upsert_payments(std::slice::from_ref(&again)).unwrap();
        store.upsert_payments(&[first, again.clone()]).unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        store.load_index("s");
        let conn = store.read().unwrap();
        for (from, new, renewals) in [(aug, 1, 1), (oct, 0, 1)] {
            let p = Period {
                from,
                to: oct,
                tz: "UTC".into(),
            };
            let split = store.payment_split("s", &p).unwrap();
            assert_eq!(
                (split[0].first_payments, split[0].renewals),
                (new, renewals),
                "{from}"
            );
            assert_eq!(split, stats::payment_split(&conn, "s", &p).unwrap());
            let f = Filter::default();
            let o = store.overview_in("s", &p, &f).unwrap();
            assert_eq!(o.current.new_customers, new, "{from}");
            assert_eq!(o, stats::overview(&conn, "s", &p, &f).unwrap());
        }
        drop(conn);
        drop(store);
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        store.upsert_payments(&[again]).unwrap();
        let renewal: bool = store
            .read()
            .unwrap()
            .query_row(
                "SELECT is_renewal FROM payments WHERE provider_payment_id = 'pay_oct'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(renewal, "a re-sync keeps the repeat payment a renewal");
    }

    #[test]
    fn inr_totals_equal_the_rows_shown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("e.duckdb");
        let day = |d: u8, h: u8| {
            time::macros::date!(2026 - 08 - 10)
                .replace_day(d)
                .unwrap()
                .with_hms(h, 0, 0)
                .unwrap()
                .assume_utc()
        };
        // Stored before conversions were rounded: each shows as $7.40, unrounded they sum to $22.21.
        let inr = |id: &str, ts: OffsetDateTime, refunded: i64| Payment {
            ts,
            amount: 65000,
            currency: "inr".into(),
            amount_usd: Some(7.4049),
            refunded,
            status: if refunded > 0 {
                "partially_refunded".into()
            } else {
                "paid".into()
            },
            ..payment(id, 0)
        };
        {
            let store = DuckStore::open(&path).unwrap();
            store
                .upsert_payments(&[
                    inr("pay_1", day(11, 9), 0),
                    inr("pay_2", day(11, 15), 0),
                    inr("pay_3", day(12, 9), 0),
                    inr("pay_4", day(13, 9), 21667),
                ])
                .unwrap();
        }
        let store = DuckStore::open(&path).unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let p = Period {
            from: time::macros::date!(2026 - 08 - 10),
            to: time::macros::date!(2026 - 08 - 16),
            tz: "Asia/Kolkata".into(),
        };
        let shown = |usd: f64| crate::dashboard::money(usd);
        let rows = store
            .payment_list("s", &p, PaymentFilter::All, 10, 0)
            .unwrap()
            .rows;
        assert!(rows.iter().all(|r| r.amount_usd == Some(7.4)));
        let row_cents: i64 = rows
            .iter()
            .map(|r| (r.net_usd.unwrap() * 100.0).round() as i64)
            .sum();
        let summary = store.payment_summary("s", &p).unwrap();
        assert_eq!(shown(summary.net_usd), shown(row_cents as f64 / 100.0));
        assert_eq!(shown(summary.net_usd), "$27.13");
        assert_eq!(
            shown(summary.net_usd + summary.refunded_usd),
            shown(4.0 * 7.4)
        );
        let days = stats::timeseries(&store.read().unwrap(), "s", &p, &Filter::default()).unwrap();
        let day_cents: i64 = days
            .iter()
            .map(|d| (d.revenue_usd * 100.0).round() as i64)
            .sum();
        let total = store.overview("s", &p).unwrap().current.revenue_usd;
        assert_eq!(shown(total), shown(day_cents as f64 / 100.0));
        assert_eq!(shown(total), "$27.13");
    }

    #[test]
    fn concurrent_appends_keep_touches_exact() {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        let (mut events, _, _) = crate::demo::seed("site", OffsetDateTime::now_utc());
        events.sort_by_key(|e| e.ts);
        let mut later = events.split_off(events.len() / 2);
        store.insert_events(&events).unwrap();
        store.load_index("site");
        // Earlier pageviews of stored sessions, then the rest out of order in mixed batches.
        let mut early: Vec<Event> = events
            .iter()
            .filter(|e| e.kind == "pageview")
            .step_by(7)
            .cloned()
            .collect();
        for (i, e) in early.iter_mut().enumerate() {
            e.ts -= time::Duration::seconds(1 + i as i64 % 3);
            e.path = format!("/early{}", i % 2);
        }
        later.extend(early);
        later.reverse();
        let total = (events.len() + later.len()) as u64;
        std::thread::scope(|scope| {
            for t in 0..8 {
                let mine: Vec<Event> = later.iter().skip(t).step_by(8).cloned().collect();
                let store = &store;
                scope.spawn(move || {
                    let mut rest = &mine[..];
                    let mut n = t + 1;
                    while !rest.is_empty() {
                        let (chunk, tail) = rest.split_at(n.min(rest.len()));
                        store.insert_events(chunk).unwrap();
                        rest = tail;
                        n = n % 20 + 1;
                    }
                });
            }
        });
        assert_eq!(store.count_events("site").unwrap(), total);
        let conn = store.read().unwrap();
        for (table, key) in [("touches", "session_id"), ("first_touches", "visitor_id")] {
            let rebuilt = format!(
                "SELECT {TOUCH_COLUMNS} FROM events WHERE kind = 'pageview'
                QUALIFY row_number() OVER (PARTITION BY site_id, {key} ORDER BY ts, path) = 1"
            );
            let differ: i64 = conn
                .query_row(
                    &format!(
                        "SELECT (SELECT count(*) FROM (SELECT {TOUCH_COLUMNS} FROM {table} EXCEPT {rebuilt}))
                            + (SELECT count(*) FROM ({rebuilt} EXCEPT SELECT {TOUCH_COLUMNS} FROM {table}))"
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(differ, 0, "{table}");
        }
        let today = OffsetDateTime::now_utc().date();
        let p = Period {
            from: today - time::Duration::days(29),
            to: today,
            tz: "UTC".into(),
        };
        let f = Filter::default();
        let counts = |rows: Vec<BreakdownRow>| {
            let mut v: Vec<_> = rows
                .into_iter()
                .map(|r| (r.label, r.visitors, r.payments))
                .collect();
            v.sort();
            v
        };
        let sql = stats::breakdown(&conn, "site", &p, Dimension::Page, &f).unwrap();
        assert!(!sql.is_empty());
        assert_eq!(
            counts(store.breakdown_in("site", &p, Dimension::Page, &f).unwrap()),
            counts(sql)
        );
        let (a, b) = (
            store.overview("site", &p).unwrap().current,
            stats::overview(&conn, "site", &p, &f).unwrap().current,
        );
        assert_eq!(
            (a.visitors, a.sessions, a.payments),
            (b.visitors, b.sessions, b.payments)
        );
    }
    #[test]
    fn events_written_while_the_index_loads_count_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        let (mut events, _, _) = crate::demo::seed("site", OffsetDateTime::now_utc());
        events.sort_by_key(|e| e.ts);
        let later = events.split_off(events.len() / 2);
        store.insert_events(&events).unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for chunk in later.chunks(5) {
                    store.insert_events(chunk).unwrap();
                }
            });
            while !store.settled("site") {
                std::thread::yield_now();
            }
        });
        let p = Period {
            from: time::macros::date!(2000 - 01 - 01),
            to: OffsetDateTime::now_utc().date(),
            tz: "UTC".into(),
        };
        let all = crate::visitors::VisitorFilter::default();
        let index = store.visitor_list("site", &p, &all, 1_000_000, 0).unwrap();
        let sql =
            crate::visitors::visitor_list(&store.read().unwrap(), "site", &p, &all, 1_000_000, 0)
                .unwrap();
        let views = |page: &crate::visitors::VisitorPage| -> u64 {
            page.rows.iter().map(|r| r.pageviews).sum()
        };
        assert_eq!((index.total, views(&index)), (sql.total, views(&sql)));
    }
}

#[cfg(test)]
mod channel_tests {
    use super::*;

    #[test]
    fn reopening_moves_stored_visits_to_their_current_channel() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("e.duckdb");
        let visit = |session: &str, host: &str, channel: &str| Event {
            site_id: "s".into(),
            ts: OffsetDateTime::from_unix_timestamp(1_760_000_000).unwrap(),
            visitor_id: session.into(),
            session_id: session.into(),
            kind: "pageview".into(),
            name: None,
            path: "/".into(),
            referrer_host: Some(host.into()),
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
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
            language: None,
        };
        DuckStore::open(&path)
            .unwrap()
            .insert_events(&[
                visit("a", "news.ycombinator.com", "Social"),
                visit("b", "x.com", "Social"),
            ])
            .unwrap();
        let store = DuckStore::open(&path).unwrap();
        let mut channels: Vec<String> = store.dump().into_iter().map(|r| r.1).collect();
        channels.sort();
        assert_eq!(channels, ["Communities", "Social"]);
        let conn = store.conn.lock().unwrap();
        let touched: String = conn
            .query_row(
                "SELECT channel FROM first_touches WHERE visitor_id = 'a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(touched, "Communities");
    }

    #[test]
    fn pending_first_pageviews_are_read_and_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("e.duckdb");
        let now = OffsetDateTime::now_utc();
        let mut events = crate::demo::seed("s", now).0;
        events.retain(|e| e.ts > now - Duration::minutes(30));
        let sessions: std::collections::HashSet<_> = events
            .iter()
            .filter(|e| e.kind == "pageview")
            .map(|e| &e.session_id)
            .collect();
        assert!(!sessions.is_empty());
        let touches = |conn: &Connection| -> usize {
            conn.query_row("SELECT count(*) FROM touches", [], |r| r.get::<_, i64>(0))
                .unwrap() as usize
        };
        let store = DuckStore::open(&path).unwrap();
        store.insert_events(&events).unwrap();
        assert_eq!(touches(&store.read().unwrap()), sessions.len());
        drop(store);
        let other = dir.path().join("f.duckdb");
        let store = DuckStore::open(&other).unwrap();
        store.insert_events(&events).unwrap();
        assert!(store.pending.lock().unwrap().len() > 0);
        drop(store);
        let store = DuckStore::open(&other).unwrap();
        assert_eq!(touches(&store.conn.lock().unwrap()), sessions.len());
    }
}
