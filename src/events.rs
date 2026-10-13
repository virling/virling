use std::path::Path;
use std::sync::Mutex;

use duckdb::types::{TimeUnit, Value};
use duckdb::{Connection, params};
use time::OffsetDateTime;

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
    fn payments(&self, site_id: &str) -> anyhow::Result<Vec<Payment>>;
    #[cfg(test)]
    fn count_events(&self, site_id: &str) -> anyhow::Result<u64>;
    fn latest_pageview(&self, site_id: &str) -> anyhow::Result<Option<LatestPageview>>;
}

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
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
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
                browser VARCHAR NOT NULL
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
                ])?;
            }
            app.flush()?;
        }
        tx.commit()?;
        Ok(())
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

    fn payments(&self, site_id: &str) -> anyhow::Result<Vec<Payment>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT site_id, provider, provider_payment_id, epoch_us(ts), amount, currency, amount_usd, refunded, status,
                    visitor_id, customer_id, is_renewal
             FROM payments WHERE site_id = ? ORDER BY ts",
        )?;
        let rows = stmt.query_map(params![site_id], |r| {
            let micros: i64 = r.get(3)?;
            Ok(Payment {
                site_id: r.get(0)?,
                provider: r.get(1)?,
                provider_payment_id: r.get(2)?,
                ts: OffsetDateTime::from_unix_timestamp_nanos(micros as i128 * 1000)
                    .unwrap_or(OffsetDateTime::UNIX_EPOCH),
                amount: r.get(4)?,
                currency: r.get(5)?,
                amount_usd: r.get(6)?,
                refunded: r.get(7)?,
                status: r.get(8)?,
                visitor_id: r.get(9)?,
                customer_id: r.get(10)?,
                is_renewal: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
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
        let rows = store.payments("s").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows.iter()
                .find(|p| p.provider_payment_id == "pi_1")
                .unwrap(),
            &payment("pi_1", 2900)
        );
    }
}
