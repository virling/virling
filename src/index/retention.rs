//! Retention and MRR read from the index's payments and subscriptions.

use std::collections::HashMap;

use duckdb::Connection;
use time::{Duration, OffsetDateTime};

use super::*;
use crate::retention::{Customer, Mrr, Retention, StatusSource, summarize};
use crate::stats::{from_micros, micros_of};

/// A customer's first payment time, latest end of a live subscription (`i64::MIN` with none live), and lifetime.
pub(crate) type Cohort = (i64, Option<i64>, Customer);

/// Keys, paying-candidate subscriptions as (key, end, usd), and each payment's key.
pub(crate) type MrrKeys = (usize, Vec<(u32, i64, f64)>, Vec<u32>);

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
        let mut stmt = conn.prepare(&format!(
            "WITH {} SELECT provider, customer_id, status, live, epoch_us(ends_at), mrr_usd FROM charged",
            crate::retention::SUBSCRIPTIONS
        ))?;
        let mut rows = stmt.query(duckdb::named_params! { "site": site_id })?;
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

    /// Customers whose first payment is no renewal, by first payment, from payments up to `now`, with the latest end of a live subscription.
    fn cohorts_until(&self, now: i64) -> Vec<Cohort> {
        struct Acc<'a> {
            first: &'a Pay,
            net: f64,
            refunded_usd: f64,
            any_refund: bool,
            kept: Vec<(i64, &'a str)>,
            count: u64,
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
                count: 0,
            });
            a.count += 1;
            if id(p) < id(a.first) {
                a.first = p;
            }
            a.net += p.net.unwrap_or(0.0);
            if let (Some(usd), Some(net)) = (row.amount_usd, p.net) {
                a.refunded_usd += usd - net;
            }
            a.any_refund |= row.refunded > 0;
            if row.refunded < row.amount {
                a.kept.push(id(p));
            }
        }
        let mut ends: HashMap<(u32, &str), i64> = HashMap::new();
        for s in &self.subs {
            let end = ends.entry((s.provider, &s.customer)).or_insert(i64::MIN);
            if s.live {
                *end = (*end).max(s.ends_at.unwrap_or(i64::MAX));
            }
        }
        let mut out = Vec::new();
        for mut a in by.into_values() {
            let (f, row) = (a.first, &a.first.row);
            if row.is_renewal {
                continue;
            }
            a.kept.sort_unstable();
            let n = a.kept.len();
            let touch = row.touch.as_ref();
            let first_source = self
                .pay_visitor(f)
                .or_else(|| {
                    self.pay_session(f)
                        .map(|s| self.sessions[s as usize].visitor)
                })
                .map(|v| self.visitors[v as usize].first_touch)
                .filter(|&s| s != NONE && self.sessions[s as usize].touch <= f.ts)
                .map(|s| {
                    let sess = &self.sessions[s as usize];
                    crate::stats::source(
                        self.strings.name(sess.cols[REFERRER]),
                        &self.path_name(sess.cols[CHANNEL]),
                    )
                });
            let end = row
                .customer_id
                .as_deref()
                .and_then(|c| ends.get(&(f.provider, c)).copied());
            out.push((
                f.ts,
                end,
                Customer {
                    first: from_micros(f.ts),
                    first_payment_usd: row.amount_usd.unwrap_or(0.0),
                    net_usd: a.net,
                    refunded_usd: a.refunded_usd,
                    any_refund: a.any_refund,
                    last_kept: a.kept.last().map(|k| from_micros(k.0)),
                    gap: (n >= 2)
                        .then(|| Duration::microseconds(a.kept[n - 1].0 - a.kept[n - 2].0)),
                    subscribed: None,
                    unattributed: touch.is_none(),
                    channel: touch.map(|t| t.channel.clone()),
                    referrer_host: touch.and_then(|t| t.referrer_host.clone()),
                    campaign: touch.and_then(|t| t.campaign.clone()),
                    in_range: false,
                    payments: a.count,
                    first_source,
                },
            ));
        }
        out.sort_by_key(|c| c.0);
        out
    }

    fn customers(&self, lo: i64, hi: i64, now: i64) -> Vec<Customer> {
        let until;
        // With no payment after `now`, every read shares one list built from all payments.
        let all = if self.payments.last().is_none_or(|p| p.ts <= now) {
            self.cohorts.get_or_init(|| self.cohorts_until(i64::MAX))
        } else {
            until = self.cohorts_until(now);
            &until
        };
        let at = |t: i64| all.partition_point(|c| c.0 < t);
        let (a0, a1, b0) = (at(lo), at(hi).max(at(lo)), at(now - 60 * DAY_US + 1));
        let spans = if b0 <= a1 {
            [a0.min(b0)..all.len(), 0..0]
        } else {
            [a0..a1, b0..all.len()]
        };
        spans
            .into_iter()
            .flat_map(|r| &all[r])
            .map(|(ts, end, c)| Customer {
                in_range: *ts >= lo && *ts < hi,
                subscribed: end.map(|e| e > now),
                ..c.clone()
            })
            .collect()
    }

    pub fn retention(&self, lo: i64, hi: i64, now: OffsetDateTime) -> Retention {
        let list = self.customers(lo, hi, micros_of(now));
        let days_of_data = self
            .payments
            .first()
            .map_or(0, |p| (now - from_micros(p.ts)).whole_days());
        summarize(&list, now, days_of_data, self.status_sources.clone())
    }

    /// Paying-candidate subscriptions as (key, end, usd) and each payment's key, NONE without one.
    fn mrr_keys(&self) -> MrrKeys {
        let mut keys: HashMap<(u32, &str), u32> = HashMap::new();
        let mut subs = Vec::new();
        for s in &self.subs {
            if !s.live || matches!(&*s.status, "trialing" | "on_trial") {
                continue;
            }
            if let Some(usd) = s.mrr_usd {
                let n = keys.len() as u32;
                let k = *keys.entry((s.provider, &s.customer)).or_insert(n);
                subs.push((k, s.ends_at.unwrap_or(i64::MAX), usd));
            }
        }
        let pays = self
            .payments
            .iter()
            .map(|p| {
                p.row
                    .customer_id
                    .as_deref()
                    .filter(|_| !p.renewal)
                    .and_then(|c| keys.get(&(p.provider, c)).copied())
                    .unwrap_or(NONE)
            })
            .collect();
        (keys.len(), subs, pays)
    }

    pub fn mrr(&self, lo: i64, hi: i64, now: OffsetDateTime) -> Option<Mrr> {
        let now = micros_of(now);
        if !self.subs.iter().any(|s| s.mrr_usd.is_some()) {
            return None;
        }
        let (n, subs, pays) = self.mrr_keys.get_or_init(|| self.mrr_keys());
        let mut usd: Vec<Option<f64>> = vec![None; *n];
        for &(k, end, u) in subs {
            if end > now {
                *usd[k as usize].get_or_insert(0.0) += u;
            }
        }
        let mut cohort = vec![false; *n];
        let from = self.payments.partition_point(|p| p.ts < lo);
        let to = self.payments.partition_point(|p| p.ts < hi).max(from);
        for &k in &pays[from..to] {
            if k != NONE {
                cohort[k as usize] = true;
            }
        }
        let mut m = Mrr::default();
        for (k, usd) in usd.into_iter().enumerate() {
            let Some(usd) = usd else { continue };
            m.usd += usd;
            m.customers += 1;
            if cohort[k] {
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
