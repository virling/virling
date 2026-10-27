//! How long and how far visitors read each page, and whether those who paid read differently.

use duckdb::{Connection, named_params};
use serde::Serialize;

use crate::stats::{BOUNDS, Period};

/// Pages listed on the card.
const SHOWN: usize = 6;
/// Scrolled at least this far counts as reading to the end.
const END: u8 = 90;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Depth {
    pub views: u64,
    /// Average seconds the page was visible.
    pub secs: f64,
    /// Average furthest scroll in percent.
    pub scroll: f64,
    /// Share of views that reached the end, in percent.
    pub ended: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadRow {
    pub path: String,
    pub all: Depth,
    /// Visitors who paid and everyone else; None when that group has no views of the page.
    pub buyers: Option<Depth>,
    pub others: Option<Depth>,
}

pub(crate) fn reading(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<Vec<ReadRow>> {
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS},
        payers AS (
            SELECT DISTINCT hash(visitor_id) AS vh FROM payments
            WHERE site_id = $site AND visitor_id IS NOT NULL AND status <> 'failed'
        ),
        r AS (
            SELECT path, secs, scroll, (scroll >= {END})::INTEGER AS ended,
                vh IN (SELECT vh FROM payers) AS buyer
            FROM reads, bounds WHERE site_id = $site AND ts >= lo AND ts < hi
        )
        SELECT path,
            count(*), avg(secs), avg(scroll), avg(ended) * 100,
            count(*) FILTER (WHERE buyer), avg(secs) FILTER (WHERE buyer),
            avg(scroll) FILTER (WHERE buyer), avg(ended) FILTER (WHERE buyer) * 100,
            count(*) FILTER (WHERE NOT buyer), avg(secs) FILTER (WHERE NOT buyer),
            avg(scroll) FILTER (WHERE NOT buyer), avg(ended) FILTER (WHERE NOT buyer) * 100
        FROM r GROUP BY path ORDER BY count(*) DESC, path LIMIT {SHOWN}"
    ))?;
    let rows = stmt.query_map(
        named_params! {
            "site": site_id,
            "tz": period.tz,
            "from": period.from.to_string(),
            "to": period.to.to_string(),
        },
        |r| {
            let depth = |i: usize| -> duckdb::Result<Option<Depth>> {
                let views = r.get::<_, i64>(i)? as u64;
                let avg =
                    |j: usize| Ok::<_, duckdb::Error>(r.get::<_, Option<f64>>(j)?.unwrap_or(0.0));
                Ok((views > 0).then_some(Depth {
                    views,
                    secs: avg(i + 1)?,
                    scroll: avg(i + 2)?,
                    ended: avg(i + 3)?,
                }))
            };
            Ok(ReadRow {
                path: r.get(0)?,
                all: depth(1)?.unwrap_or_default(),
                buyers: depth(5)?,
                others: depth(9)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub struct ReadCard {
    pub rows: Vec<ReadView>,
    /// The Buyers tab, shown once some paying visitor read a listed page.
    pub buyers: bool,
    pub on_buyers: bool,
    /// Links to the two tabs.
    pub tabs: (String, String),
}

pub struct ReadView {
    pub path: String,
    pub views: String,
    pub time: String,
    pub scroll: String,
    pub ended: String,
    pub buyers: (String, String),
    pub others: (String, String),
}

fn seconds(s: f64) -> String {
    let s = s.round() as u64;
    if s >= 60 {
        format!("{}m {}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

fn pair(d: &Option<Depth>) -> (String, String) {
    d.as_ref().map_or_else(
        || ("-".into(), "-".into()),
        |d| (seconds(d.secs), format!("{:.0}%", d.scroll)),
    )
}

/// None until some page has been read.
pub fn card(rows: &[ReadRow], on_buyers: bool, tabs: (String, String)) -> Option<ReadCard> {
    if rows.is_empty() {
        return None;
    }
    let buyers = rows.iter().any(|r| r.buyers.is_some());
    Some(ReadCard {
        rows: rows
            .iter()
            .map(|r| ReadView {
                path: r.path.clone(),
                views: crate::dashboard::group(r.all.views),
                time: seconds(r.all.secs),
                scroll: format!("{:.0}%", r.all.scroll),
                ended: format!("{:.0}%", r.all.ended),
                buyers: pair(&r.buyers),
                others: pair(&r.others),
            })
            .collect(),
        buyers,
        on_buyers: on_buyers && buyers,
        tabs,
    })
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;
    use crate::events::{DuckStore, EventStore, Payment, Read};

    fn read(visitor: &str, path: &str, secs: u16, scroll: u8) -> Read {
        Read {
            site_id: "s".into(),
            ts: OffsetDateTime::from_unix_timestamp(1_760_000_000).unwrap(),
            visitor_id: visitor.into(),
            path: path.into(),
            secs,
            scroll,
        }
    }

    fn period() -> Period {
        Period {
            from: time::macros::date!(2025 - 01 - 01),
            to: time::macros::date!(2026 - 12 - 31),
            tz: "UTC".into(),
        }
    }

    #[test]
    fn averages_per_page_and_splits_buyers_from_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        assert!(store.reading("s", &period()).unwrap().is_empty());
        store
            .insert_reads(&[
                read("a", "/blog", 60, 100),
                read("b", "/blog", 20, 40),
                read("c", "/blog", 10, 10),
                read("a", "/docs", 30, 50),
            ])
            .unwrap();
        let none = store.reading("s", &period()).unwrap();
        assert_eq!(none[0].path, "/blog");
        assert_eq!(none[0].all.views, 3);
        assert_eq!(none[0].all.secs, 30.0);
        assert!((none[0].all.ended - 100.0 / 3.0).abs() < 1e-9);
        assert!(none[0].buyers.is_none());
        store
            .upsert_payments(&[Payment {
                site_id: "s".into(),
                provider: "stripe".into(),
                provider_payment_id: "p1".into(),
                ts: OffsetDateTime::from_unix_timestamp(1_760_000_100).unwrap(),
                amount: 2900,
                currency: "usd".into(),
                amount_usd: Some(29.0),
                refunded: 0,
                status: "paid".into(),
                visitor_id: Some("a".into()),
                customer_id: None,
                is_renewal: false,
            }])
            .unwrap();
        let rows = store.reading("s", &period()).unwrap();
        let blog = &rows[0];
        assert_eq!(
            blog.buyers.as_ref().map(|d| (d.views, d.secs)),
            Some((1, 60.0))
        );
        assert_eq!(
            blog.others.as_ref().map(|d| (d.views, d.secs)),
            Some((2, 15.0))
        );
        let card = card(&rows, true, Default::default()).unwrap();
        assert!(card.buyers && card.on_buyers);
        assert_eq!(card.rows[0].time, "30s");
        assert_eq!(card.rows[1].others.0, "-");
    }

    #[test]
    fn an_empty_site_has_no_card() {
        assert!(card(&[], false, Default::default()).is_none());
        assert_eq!(seconds(95.0), "1m 35s");
    }
}
