//! The goal table and funnels read from the index.

use duckdb::Connection;

use super::*;
use crate::stats::{GoalStat, Period, Step, StepCount};

impl SiteIndex {
    pub fn goal_table(&self, lo: i64, hi: i64, scope: &Scope, pages: &[String]) -> Vec<GoalStat> {
        // Path ID -> index in `pages`; earlier pages win.
        let mut page_of: FastMap<u32, u32> = FastMap::default();
        for (i, p) in pages.iter().enumerate() {
            for id in self.strings.matching(p) {
                page_of.entry(id).or_insert(i as u32);
            }
        }
        // (is goal, name or index in `pages`) -> (completions, visitors, visitors by channel)
        type Done = (u64, FastSet<u32>, FastMap<u32, FastSet<u32>>);
        let mut done: FastMap<(bool, u32), Done> = FastMap::default();
        let mut add = |k: (bool, u32), s: u32, channel: u32| {
            let sess = &self.sessions[s as usize];
            let channel = if sess.touch != i64::MAX {
                sess.cols[CHANNEL]
            } else {
                channel
            };
            let e = done.entry(k).or_default();
            e.0 += 1;
            e.1.insert(sess.visitor);
            e.2.entry(channel).or_default().insert(sess.visitor);
        };
        if !page_of.is_empty() {
            let from = self
                .pv_by_ts
                .partition_point(|&p| self.pageviews[p as usize].ts < lo);
            let to = self
                .pv_by_ts
                .partition_point(|&p| self.pageviews[p as usize].ts < hi);
            for &p in &self.pv_by_ts[from..to] {
                let pv = &self.pageviews[p as usize];
                if let Some(&i) = page_of.get(&pv.path)
                    && scope.has(pv.session)
                {
                    add((false, i), pv.session, pv.channel);
                }
            }
        }
        for g in self.goals_in(lo, hi) {
            if g.name != NULL && scope.has(g.session) {
                add((true, g.name), g.session, g.channel);
            }
        }
        let mut out: Vec<GoalStat> = done
            .into_iter()
            .map(|((goal, id), (completions, visitors, channels))| {
                let mut channels: Vec<(String, u64)> = channels
                    .into_iter()
                    .map(|(c, v)| (self.path_name(c), v.len() as u64))
                    .collect();
                channels.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                GoalStat {
                    key: if goal {
                        format!("goal:{}", self.path_name(id))
                    } else {
                        format!("page:{}", pages[id as usize])
                    },
                    completions,
                    visitors: visitors.len() as u64,
                    channels,
                }
            })
            .collect();
        out.sort_by(|a, b| b.visitors.cmp(&a.visitors).then_with(|| a.key.cmp(&b.key)));
        out
    }

    /// Earliest time in [from, hi) visitor `v` reached `step`, with `paths` the IDs of a page step.
    fn reached(&self, v: u32, step: &Step, paths: &[u32], from: i64, hi: i64) -> Option<i64> {
        let mut best: Option<i64> = None;
        let mut see = |t: i64| {
            if t >= from && t < hi && best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        };
        match step {
            Step::Paid => {
                for &t in self.paid_at.get(&key(&self.visitors[v as usize].id))? {
                    see(t);
                }
            }
            Step::Visit | Step::Page(_) => {
                let page = matches!(step, Step::Page(_));
                if page && paths.is_empty() {
                    return None;
                }
                for s in self.sessions_of(v) {
                    let sess = &self.sessions[s as usize];
                    if sess.last < from || sess.first >= hi {
                        continue;
                    }
                    for pv in self.pageviews_of(s) {
                        if !page || paths.contains(&pv.path) {
                            see(pv.ts);
                        }
                    }
                }
            }
            Step::Goal(name) => {
                let name = self.strings.get(name)?;
                for s in self.sessions_of(v) {
                    let sess = &self.sessions[s as usize];
                    if sess.last < from || sess.first >= hi {
                        continue;
                    }
                    for m in self.marks_of(s) {
                        if m.kind == GOAL && m.name == name {
                            see(m.ts);
                        }
                    }
                }
            }
        }
        best
    }

    pub fn funnel(&self, lo: i64, hi: i64, scope: &Scope, steps: &[Step]) -> Vec<StepCount> {
        let mut at: FastMap<u32, i64> = FastMap::default();
        let from = self
            .pv_by_ts
            .partition_point(|&p| self.pageviews[p as usize].ts < lo);
        let to = self
            .pv_by_ts
            .partition_point(|&p| self.pageviews[p as usize].ts < hi);
        for &p in &self.pv_by_ts[from..to] {
            let pv = &self.pageviews[p as usize];
            if scope.has(pv.session) {
                at.entry(self.sessions[pv.session as usize].visitor)
                    .or_insert(pv.ts);
            }
        }
        let mut out = vec![StepCount {
            visitors: at.len() as u64,
            median_secs: None,
        }];
        for step in steps.iter().skip(1) {
            let paths = match step {
                Step::Page(p) => self.strings.matching(p),
                _ => Vec::new(),
            };
            let mut next: FastMap<u32, i64> = FastMap::default();
            let mut gaps: Vec<i64> = Vec::new();
            for (&v, &t) in &at {
                if let Some(r) = self.reached(v, step, &paths, t, hi) {
                    next.insert(v, r);
                    gaps.push(r - t);
                }
            }
            gaps.sort_unstable();
            let n = gaps.len();
            let median = (n > 0).then(|| {
                let mid = if n % 2 == 1 {
                    gaps[n / 2] as f64
                } else {
                    (gaps[n / 2 - 1] as f64 + gaps[n / 2] as f64) / 2.0
                };
                mid / 1e6
            });
            out.push(StepCount {
                visitors: next.len() as u64,
                median_secs: median,
            });
            at = next;
        }
        out
    }
}

pub mod read {
    use super::*;

    pub fn goal_table(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        pages: &[String],
        f: &Filter,
    ) -> anyhow::Result<Option<Vec<GoalStat>>> {
        let (lo, hi) = z.bounds(conn, period)?;
        Ok(Some(ix.goal_table(lo, hi, &ix.scope(f), pages)))
    }

    pub fn funnel(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        steps: &[Step],
        f: &Filter,
    ) -> anyhow::Result<Option<Vec<StepCount>>> {
        let (lo, hi) = z.bounds(conn, period)?;
        Ok(Some(ix.funnel(lo, hi, &ix.scope(f), steps)))
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;
    use crate::index::tests::{SITE, check, filters, seeded};
    use crate::stats;

    #[test]
    fn goals_and_funnels_match_sql() {
        let (_dir, store, today) = seeded();
        let ix = store.index_for_test(SITE);
        let ix = ix.read().unwrap();
        let conn = store.conn_for_test();
        let z = Zones::default();
        let mut checked = 0;
        for tz in ["UTC", "America/New_York"] {
            for (from, to) in [
                (today, today),
                (today - Duration::days(6), today),
                (today - Duration::days(29), today),
                (today - Duration::days(400), today),
                (today - Duration::days(45), today - Duration::days(40)),
            ] {
                let p = Period {
                    from,
                    to,
                    tz: tz.into(),
                };
                let at = format!("{tz} {from}..{to}");
                let names: Vec<String> = stats::goal_counts(&conn, SITE, &p)
                    .unwrap()
                    .into_iter()
                    .map(|g| g.name)
                    .collect();
                let pages = [
                    "/pricing*".to_string(),
                    "/pricing".to_string(),
                    "/".to_string(),
                    "/nope".to_string(),
                ];
                let mut funnels = vec![
                    vec![Step::Visit, Step::Paid],
                    vec![Step::Visit, Step::Visit],
                ];
                for g in names.iter().take(3) {
                    funnels.push(vec![
                        Step::Visit,
                        Step::Page("/pricing".into()),
                        Step::Goal(g.clone()),
                        Step::Paid,
                    ]);
                    funnels.push(vec![
                        Step::Visit,
                        Step::Goal(g.clone()),
                        Step::Page("/".into()),
                    ]);
                    funnels.push(vec![Step::Visit, Step::Page("/p*".into()), Step::Paid]);
                }
                funnels.push(vec![
                    Step::Visit,
                    Step::Goal("no such goal".into()),
                    Step::Paid,
                ]);
                for f in filters() {
                    for pages in [&pages[..1], &pages[..], &[]] {
                        let sql = stats::goal_table(&conn, SITE, &p, pages, &f).unwrap();
                        checked += sql.len();
                        check(
                            &format!("goal_table {at} {f:?} {pages:?}"),
                            read::goal_table(&ix, &z, &conn, &p, pages, &f).unwrap(),
                            sql,
                        );
                    }
                    for steps in &funnels {
                        check(
                            &format!("funnel {at} {f:?} {steps:?}"),
                            read::funnel(&ix, &z, &conn, &p, steps, &f).unwrap(),
                            stats::funnel(&conn, SITE, &p, steps, &f).unwrap(),
                        );
                    }
                }
            }
        }
        assert!(checked > 50, "{checked}");
    }
}
