//! Core Web Vitals from real visits, per page, next to how the visitors who loaded the page converted.

use duckdb::{Connection, named_params};
use serde::Serialize;

use crate::stats::{BOUNDS, NET_USD, Period};

/// Pages with the most loads considered, then listed slowest first.
const TOP: usize = 20;
const SHOWN: usize = 6;
/// Largest contentful paint, slowest interaction and layout shift (x1000) past which a load is poor.
const LCP_POOR: f64 = 4000.0;
const LCP_GOOD: f64 = 2500.0;
const INP_POOR: f64 = 500.0;
const CLS_POOR: f64 = 250.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeedRow {
    pub path: String,
    pub loads: u64,
    /// 75th percentile in ms; None when no load of the page reported it.
    pub lcp: Option<f64>,
    pub inp: Option<f64>,
    /// 75th percentile layout shift times 1000.
    pub cls: Option<f64>,
    pub visitors: u64,
    pub buyers: u64,
    pub revenue_usd: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Speed {
    pub rows: Vec<SpeedRow>,
    /// Visitors with a load slower than 2.5 s, and how many of them paid.
    pub slow: (u64, u64),
    /// Visitors whose loads were all faster, and how many of them paid.
    pub fast: (u64, u64),
}

pub(crate) fn speed(conn: &Connection, site_id: &str, period: &Period) -> anyhow::Result<Speed> {
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS},
        v AS (
            SELECT path, vh, lcp, inp, cls FROM vitals, bounds
            WHERE site_id = $site AND ts >= lo AND ts < hi
        ),
        payers AS (
            SELECT hash(p.visitor_id) AS vh, sum({NET_USD}) AS rev FROM payments p, bounds
            WHERE p.site_id = $site AND p.visitor_id IS NOT NULL AND p.status <> 'failed'
              AND p.ts >= lo AND p.ts < hi
            GROUP BY 1
        ),
        top AS (
            SELECT path, count(*) AS loads,
                quantile_cont(lcp, 0.75) FILTER (WHERE lcp > 0) AS lcp,
                quantile_cont(inp, 0.75) FILTER (WHERE lcp > 0) AS inp,
                quantile_cont(cls, 0.75) FILTER (WHERE lcp > 0) AS cls
            FROM v GROUP BY path ORDER BY loads DESC, path LIMIT {TOP}
        ),
        who AS (SELECT DISTINCT v.path, v.vh FROM v JOIN top USING (path)),
        money AS (
            SELECT w.path, count(*) AS visitors, count(p.vh) AS buyers, coalesce(sum(p.rev), 0) AS rev
            FROM who w LEFT JOIN payers p USING (vh) GROUP BY w.path
        ),
        per AS (SELECT vh, max(lcp) > {LCP_GOOD} AS slow FROM v WHERE lcp > 0 GROUP BY vh),
        ranked AS (
            SELECT t.path, t.loads, t.lcp, t.inp, t.cls, m.visitors, m.buyers, m.rev,
                row_number() OVER (ORDER BY t.lcp DESC NULLS LAST, t.loads DESC, t.path) AS n
            FROM top t JOIN money m USING (path)
        )
        SELECT path, loads, lcp, inp, cls, visitors, buyers, rev, NULL::BOOLEAN AS slow, n
        FROM ranked WHERE n <= {SHOWN}
        UNION ALL
        SELECT NULL, 0, NULL, NULL, NULL, count(*), count(p.vh), 0.0, per.slow, 0
        FROM per LEFT JOIN payers p USING (vh) GROUP BY per.slow
        ORDER BY n"
    ))?;
    let params = named_params! {
        "site": site_id,
        "tz": period.tz,
        "from": period.from.to_string(),
        "to": period.to.to_string(),
    };
    let mut out = Speed::default();
    let rows = stmt.query_map(params, |r| {
        let (visitors, buyers) = (r.get::<_, i64>(5)? as u64, r.get::<_, i64>(6)? as u64);
        Ok(match r.get::<_, Option<bool>>(8)? {
            Some(slow) => (Some((slow, visitors, buyers)), None),
            None => (
                None,
                Some(SpeedRow {
                    path: r.get(0)?,
                    loads: r.get::<_, i64>(1)? as u64,
                    lcp: r.get(2)?,
                    inp: r.get(3)?,
                    cls: r.get(4)?,
                    visitors,
                    buyers,
                    revenue_usd: r.get(7)?,
                }),
            ),
        })
    })?;
    for row in rows {
        match row? {
            (Some((true, v, b)), _) => out.slow = (v, b),
            (Some((false, v, b)), _) => out.fast = (v, b),
            (_, Some(row)) => out.rows.push(row),
            _ => {}
        }
    }
    Ok(out)
}

pub struct SpeedView {
    pub path: String,
    pub lcp: (String, bool),
    pub inp: (String, bool),
    pub cls: (String, bool),
    pub conversion: String,
    pub revenue: String,
}

pub struct SpeedCard {
    pub rows: Vec<SpeedView>,
    /// How slow loads converted against fast ones; empty without enough of both.
    pub line: String,
}

fn pct(n: u64, d: u64) -> String {
    if d == 0 {
        "-".into()
    } else {
        format!("{:.1}%", n as f64 * 100.0 / d as f64)
    }
}

/// A value and whether it is poor; a dash when the browsers did not report it.
fn shown(v: Option<f64>, poor: f64, fmt: impl Fn(f64) -> String) -> (String, bool) {
    v.map_or_else(|| ("-".into(), false), |v| (fmt(v), v > poor))
}

/// Visitors on each side before the comparison line is shown.
const ENOUGH: u64 = 30;

/// None until some page load reported vitals.
pub fn card(s: &Speed) -> Option<SpeedCard> {
    if s.rows.is_empty() {
        return None;
    }
    let ((sv, sb), (fv, fb)) = (s.slow, s.fast);
    let line = if sv >= ENOUGH && fv >= ENOUGH {
        format!(
            "Visitors who waited over 2.5 s for a page converted at {}, against {} for faster loads.",
            pct(sb, sv),
            pct(fb, fv)
        )
    } else {
        String::new()
    };
    Some(SpeedCard {
        rows: s
            .rows
            .iter()
            .map(|r| SpeedView {
                path: r.path.clone(),
                lcp: shown(r.lcp, LCP_POOR, |v| format!("{:.1} s", v / 1000.0)),
                inp: shown(r.inp, INP_POOR, |v| {
                    // Browsers report interactions from 104 ms, so a lower value means fast.
                    if v < 100.0 {
                        "< 100 ms".into()
                    } else {
                        format!("{v:.0} ms")
                    }
                }),
                cls: shown(r.cls, CLS_POOR, |v| format!("{:.2}", v / 1000.0)),
                conversion: pct(r.buyers, r.visitors),
                revenue: crate::dashboard::money(r.revenue_usd),
            })
            .collect(),
        line,
    })
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use time::macros::date;

    use super::*;
    use crate::events::{DuckStore, EventStore, Payment, Read};

    fn load(visitor: &str, path: &str, lcp: u32) -> Read {
        Read {
            site_id: "s".into(),
            ts: OffsetDateTime::from_unix_timestamp(1_760_000_000).unwrap(),
            visitor_id: visitor.into(),
            path: path.into(),
            secs: 30,
            scroll: 50,
            vitals: Some([lcp, 120, 50]),
        }
    }

    #[test]
    fn pages_slowest_first_with_who_paid() {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        let mut reads = vec![
            load("v1", "/pricing", 5200),
            load("v2", "/pricing", 4800),
            load("v3", "/", 900),
            load("v1", "/", 1200),
        ];
        reads.push(Read {
            vitals: None,
            ..load("v4", "/blog", 0)
        });
        store.insert_reads(&reads).unwrap();
        store
            .upsert_payments(&[Payment {
                site_id: "s".into(),
                provider: "stripe".into(),
                provider_payment_id: "pi_1".into(),
                ts: OffsetDateTime::from_unix_timestamp(1_760_000_100).unwrap(),
                amount: 4900,
                currency: "usd".into(),
                amount_usd: Some(49.0),
                refunded: 0,
                status: "paid".into(),
                visitor_id: Some("v3".into()),
                customer_id: None,
                is_renewal: false,
            }])
            .unwrap();
        let p = Period {
            from: date!(2025 - 10 - 01),
            to: date!(2025 - 10 - 31),
            tz: "UTC".into(),
        };
        let s = store.speed("s", &p).unwrap();
        let paths: Vec<&str> = s.rows.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            paths,
            ["/pricing", "/"],
            "a read without vitals adds no row"
        );
        assert_eq!((s.rows[0].visitors, s.rows[0].buyers), (2, 0));
        assert_eq!((s.rows[1].visitors, s.rows[1].buyers), (2, 1));
        assert_eq!(s.rows[1].revenue_usd, 49.0);
        assert!(s.rows[0].lcp.unwrap() > 4800.0);
        assert_eq!((s.slow, s.fast), ((2, 0), (1, 1)), "v1 had one slow load");
        let c = card(&s).unwrap();
        assert!(c.rows[0].lcp.1);
        assert_eq!(c.rows[1].conversion, "50.0%");
        assert!(c.line.is_empty(), "too few visitors to compare");
    }
}
