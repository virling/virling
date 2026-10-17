//! Retention and MRR read from the index's payments and subscriptions.

use std::collections::HashMap;

use duckdb::Connection;
use time::{Duration, OffsetDateTime};

use super::*;
use crate::retention::{Customer, Mrr, Retention, StatusSource, summarize};
use crate::stats::{from_micros, micros_of};

/// A subscription as stored.
pub(crate) struct Sub {
    provider: u32,
    customer: Box<str>,
    status: Box<str>,
    live: bool,
    ends_at: Option<i64>,
    mrr_usd: Option<f64>,
}

impl SiteIndex {
    pub(super) fn load_subscriptions(
        &mut self,
        conn: &Connection,
        site_id: &str,
    ) -> anyhow::Result<()> {
        let mut stmt = conn.prepare(
            "SELECT provider, customer_id, status, live, epoch_us(ends_at), mrr_usd
            FROM subscriptions WHERE site_id = ?",
        )?;
        let mut rows = stmt.query(params![site_id])?;
        let mut subs = Vec::new();
        while let Some(r) = rows.next()? {
            subs.push(Sub {
                provider: self.strings.id(Some(&r.get::<_, String>(0)?)),
                customer: r.get::<_, String>(1)?.into(),
                status: r.get::<_, String>(2)?.into(),
                live: r.get(3)?,
                ends_at: r.get(4)?,
                mrr_usd: r.get(5)?,
            });
        }
        let mut stmt = conn.prepare(
            "SELECT p.provider, bool_or(s.provider IS NOT NULL)
            FROM (SELECT DISTINCT provider FROM payments WHERE site_id = $site) p
            LEFT JOIN (SELECT DISTINCT provider FROM subscriptions WHERE site_id = $site) s USING (provider)
            GROUP BY ALL ORDER BY 1",
        )?;
        self.status_sources = stmt
            .query_map(duckdb::named_params! { "site": site_id }, |r| {
                Ok(StatusSource {
                    provider: r.get(0)?,
                    subscription_status: r.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        self.subs = subs;
        Ok(())
    }

    fn customers(&self, lo: i64, hi: i64, now: i64) -> Vec<Customer> {
        struct Acc<'a> {
            first: &'a Pay,
            net: f64,
            refunded_usd: f64,
            any_refund: bool,
            kept: Vec<(i64, &'a str)>,
        }
        fn id(p: &Pay) -> (i64, &str) {
            (p.ts, &p.row.provider_payment_id)
        }
        let mut by: HashMap<u32, Acc> = HashMap::new();
        for p in self.payments.iter().take_while(|p| p.ts <= now) {
            let row = &p.row;
            let a = by.entry(p.customer).or_insert(Acc {
                first: p,
                net: 0.0,
                refunded_usd: 0.0,
                any_refund: false,
                kept: Vec::new(),
            });
            if id(p) < id(a.first) {
                a.first = p;
            }
            a.net += p.net.unwrap_or(0.0);
            if let Some(usd) = row.amount_usd.filter(|_| row.amount != 0) {
                a.refunded_usd += usd * row.refunded as f64 / row.amount as f64;
            }
            a.any_refund |= row.refunded > 0;
            if row.refunded < row.amount {
                a.kept.push(id(p));
            }
        }
        let mut live: HashMap<(u32, &str), bool> = HashMap::new();
        for s in &self.subs {
            let on = s.live && s.ends_at.is_none_or(|t| t > now);
            *live.entry((s.provider, &s.customer)).or_default() |= on;
        }
        let since = now - 60 * DAY_US;
        let mut out = Vec::new();
        for mut a in by.into_values() {
            let (f, row) = (a.first, &a.first.row);
            let in_range = f.ts >= lo && f.ts < hi;
            if row.is_renewal || !(in_range || f.ts > since) {
                continue;
            }
            a.kept.sort_unstable();
            let n = a.kept.len();
            let touch = row.touch.as_ref();
            out.push(Customer {
                first: from_micros(f.ts),
                first_payment_usd: row.amount_usd.unwrap_or(0.0),
                net_usd: a.net,
                refunded_usd: a.refunded_usd,
                any_refund: a.any_refund,
                last_kept: a.kept.last().map(|k| from_micros(k.0)),
                gap: (n >= 2).then(|| Duration::microseconds(a.kept[n - 1].0 - a.kept[n - 2].0)),
                subscribed: row
                    .customer_id
                    .as_deref()
                    .and_then(|c| live.get(&(f.provider, c)).copied()),
                unattributed: touch.is_none(),
                channel: touch.map(|t| t.channel.clone()),
                referrer_host: touch.and_then(|t| t.referrer_host.clone()),
                campaign: touch.and_then(|t| t.campaign.clone()),
                in_range,
            });
        }
        out.sort_by_key(|c| c.first);
        out
    }

    pub fn retention(&self, lo: i64, hi: i64, now: OffsetDateTime) -> Retention {
        let list = self.customers(lo, hi, micros_of(now));
        let days_of_data = self
            .payments
            .first()
            .map_or(0, |p| (now - from_micros(p.ts)).whole_days());
        summarize(&list, now, days_of_data, self.status_sources.clone())
    }

    pub fn mrr(&self, lo: i64, hi: i64, now: OffsetDateTime) -> Option<Mrr> {
        let now = micros_of(now);
        let priced = self.subs.iter().filter(|s| s.mrr_usd.is_some()).count();
        if priced == 0 {
            return None;
        }
        let mut subs: HashMap<(u32, &str), f64> = HashMap::new();
        for s in &self.subs {
            let paying = s.live
                && !matches!(&*s.status, "trialing" | "on_trial")
                && s.ends_at.is_none_or(|t| t > now);
            if let Some(usd) = s.mrr_usd.filter(|_| paying) {
                *subs.entry((s.provider, &s.customer)).or_default() += usd;
            }
        }
        let all = Scope(None, String::new());
        let cohort: HashSet<(u32, &str)> = self
            .pays(lo, hi, &all)
            .filter(|p| !p.renewal)
            .filter_map(|p| Some((p.provider, p.row.customer_id.as_deref()?)))
            .collect();
        let mut m = Mrr::default();
        for (k, usd) in subs {
            m.usd += usd;
            m.customers += 1;
            if cohort.contains(&k) {
                m.cohort_usd += usd;
                m.cohort_customers += 1;
            }
        }
        Some(m)
    }
}

pub mod read {
    use super::*;
    use crate::stats::Period;

    pub fn retention(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Option<Retention>> {
        let (lo, hi) = z.bounds(conn, period)?;
        Ok(Some(ix.retention(lo, hi, now)))
    }

    pub fn mrr(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        now: OffsetDateTime,
    ) -> anyhow::Result<Option<Option<Mrr>>> {
        let (lo, hi) = z.bounds(conn, period)?;
        Ok(Some(ix.mrr(lo, hi, now)))
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;
    use crate::index::tests::{SITE, check, seeded};
    use crate::retention;
    use crate::stats::Period;

    #[test]
    fn retention_and_mrr_match_sql() {
        let (_dir, store, today) = seeded();
        let ix = store.index_for_test(SITE);
        let ix = ix.read().unwrap();
        let conn = store.conn_for_test();
        let z = Zones::default();
        let now = OffsetDateTime::now_utc();
        for tz in ["UTC", "America/New_York"] {
            for (from, to) in [
                (today, today),
                (today - Duration::days(29), today),
                (today - Duration::days(89), today),
                (today - Duration::days(400), today),
                (today - Duration::days(45), today - Duration::days(40)),
            ] {
                let p = Period {
                    from,
                    to,
                    tz: tz.into(),
                };
                for at in [
                    now,
                    now - Duration::days(20),
                    now - Duration::days(200),
                    now + Duration::days(40),
                ] {
                    let sql = retention::retention(&conn, SITE, &p, at).unwrap();
                    assert!(sql.total.customers > 0 || from == to || at < now);
                    assert!(retention::mrr(&conn, SITE, &p, at).unwrap().is_some());
                    check(
                        &format!("retention {tz} {from}..{to} {at}"),
                        read::retention(&ix, &z, &conn, &p, at).unwrap(),
                        sql,
                    );
                    check(
                        &format!("mrr {tz} {from}..{to} {at}"),
                        read::mrr(&ix, &z, &conn, &p, at).unwrap(),
                        retention::mrr(&conn, SITE, &p, at).unwrap(),
                    );
                }
            }
        }
    }
}
