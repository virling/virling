//! Cumulative revenue per visitor in the weeks after the first visit, by month of that visit and channel.

use std::collections::BTreeMap;

use duckdb::{Connection, named_params};
use serde::Serialize;
use time::OffsetDateTime;

use crate::stats::{ATTRIBUTION_JOIN, BOUNDS, NET_USD, Period, micros_of};

/// Weeks after the first visit the curve follows.
pub const WEEKS: usize = 12;
pub const WEEK_US: i64 = 7 * 86_400 * 1_000_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaybackRow {
    /// Local month of the first visit, as 2026-09.
    pub month: String,
    pub channel: String,
    pub visitors: u64,
    /// Net USD per visitor paid within 1..=WEEKS weeks of the first visit; None for weeks not reached yet.
    pub per_visitor: Vec<Option<f64>>,
}

/// A month and channel of first visits: (month, channel, start of the month in range, visitors).
pub type Cohort = (String, String, i64, u64);
/// Net USD paid in a week after the first visit: (month, channel, week, usd).
pub type Paid = (String, String, usize, f64);

/// Rows newest month first, then most visitors.
pub fn build(cohorts: Vec<Cohort>, paid: Vec<Paid>, now: OffsetDateTime) -> Vec<PaybackRow> {
    let now = micros_of(now);
    let mut weeks: BTreeMap<(String, String), [f64; WEEKS]> = BTreeMap::new();
    for (m, c, w, usd) in paid {
        weeks.entry((m, c)).or_insert([0.0; WEEKS])[w] += usd;
    }
    let mut rows: Vec<PaybackRow> = cohorts
        .into_iter()
        .map(|(month, channel, start, visitors)| {
            let paid = weeks
                .remove(&(month.clone(), channel.clone()))
                .unwrap_or([0.0; WEEKS]);
            let mut sum = 0.0;
            let per_visitor = (0..WEEKS)
                .map(|w| {
                    sum += paid[w];
                    (start + (w as i64 + 1) * WEEK_US <= now).then(|| sum / visitors as f64)
                })
                .collect();
            PaybackRow {
                month,
                channel,
                visitors,
                per_visitor,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.month
            .cmp(&a.month)
            .then(b.visitors.cmp(&a.visitors))
            .then_with(|| a.channel.cmp(&b.channel))
    });
    rows
}

pub(crate) fn payback(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    now: OffsetDateTime,
) -> anyhow::Result<Vec<PaybackRow>> {
    let cohort = format!(
        "WITH {BOUNDS},
        cohort AS (
            SELECT v.visitor_id, coalesce(v.channel, '') AS channel, epoch_us(v.ts) AS us,
                strftime(timezone($tz, v.ts), '%Y-%m') AS month,
                epoch_us(greatest(b.lo, timezone($tz, date_trunc('month', timezone($tz, v.ts))))) AS start
            FROM first_touches v, bounds b
            WHERE v.site_id = $site AND v.ts >= b.lo AND v.ts < b.hi
        )"
    );
    let (from, to) = (period.from.to_string(), period.to.to_string());
    let mut stmt = conn.prepare(&format!(
        "{cohort} SELECT month, channel, min(start), count(*) FROM cohort GROUP BY ALL"
    ))?;
    let cohorts = stmt
        .query_map(
            named_params! { "site": site_id, "tz": period.tz, "from": from, "to": to },
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? as u64)),
        )?
        .collect::<Result<_, _>>()?;
    let mut stmt = conn.prepare(&format!(
        "{cohort}, paid AS (
            SELECT coalesce(p.visitor_id, sv.visitor_id) AS visitor_id, epoch_us(p.ts) AS us, {NET_USD} AS net
            FROM payments p {ATTRIBUTION_JOIN}
            LEFT JOIN touches sv ON sv.site_id = p.site_id AND sv.session_id = a.session_id
            WHERE p.site_id = $site AND p.status <> 'failed' AND epoch_us(p.ts) <= $now
        )
        SELECT c.month, c.channel, (p.us - c.us) // {WEEK_US} AS w, sum(p.net)
        FROM cohort c JOIN paid p USING (visitor_id)
        WHERE p.us >= c.us AND p.us - c.us < {WEEKS} * {WEEK_US} AND p.net IS NOT NULL
        GROUP BY ALL"
    ))?;
    let paid = stmt
        .query_map(
            named_params! {
                "site": site_id, "tz": period.tz, "from": from, "to": to, "now": micros_of(now),
            },
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get::<_, i64>(2)? as usize,
                    r.get(3)?,
                ))
            },
        )?
        .collect::<Result<_, _>>()?;
    Ok(build(cohorts, paid, now))
}
