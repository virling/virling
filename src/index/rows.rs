//! Payment rows, journeys, activity, goal lists, visitor lists and timelines read from the index.

use std::collections::{HashMap, HashSet};

use duckdb::Connection;

use super::*;
use crate::stats::{
    ACTIVITY_DAYS, Activity, FirstTouch, GoalCount, GoalRow, Journey, JourneySession,
    PaymentFilter, PaymentPage, PaymentRow, PaymentSummary, Period, Who, from_micros, micros_of,
    source,
};
use crate::visitors::{self, Ev, Timeline, VisitorFilter, VisitorPage, VisitorRow};

impl Zones {
    /// UTC offset in micros of `tz` at `us`.
    fn offset_at(&self, conn: &Connection, tz: &str, us: i64) -> anyhow::Result<i64> {
        let year = time::OffsetDateTime::from_unix_timestamp(us.div_euclid(1_000_000))
            .map(|t| t.year())
            .unwrap_or(1970);
        let segs = self.year(conn, tz, year)?;
        let i = segs.partition_point(|s| s.0 <= us);
        Ok(segs.get(i.saturating_sub(1)).map_or(0, |s| s.1))
    }
}

impl SiteIndex {
    fn name_of(&self, id: u32) -> Option<String> {
        self.strings.name(id).map(str::to_string)
    }

    fn attr(&self, attrs: u32, i: usize) -> Option<String> {
        self.name_of(self.attrs[attrs as usize][i])
    }

    /// Sessions of visitor `v`, newest first.
    pub(super) fn sessions_of(&self, v: u32) -> impl Iterator<Item = u32> + '_ {
        let mut s = self.visitors[v as usize].last_session;
        std::iter::from_fn(move || {
            (s != NONE).then(|| {
                let at = s;
                s = self.sessions[s as usize].prev;
                at
            })
        })
    }

    pub(super) fn pageviews_of(&self, s: u32) -> impl Iterator<Item = &Pageview> + '_ {
        let mut i = self.sessions[s as usize].pv;
        std::iter::from_fn(move || {
            (i != NONE).then(|| {
                let pv = &self.pageviews[i as usize];
                i = pv.prev;
                pv
            })
        })
    }

    pub(super) fn marks_of(&self, s: u32) -> impl Iterator<Item = &Mark> + '_ {
        let mut i = self.sessions[s as usize].mark;
        std::iter::from_fn(move || {
            (i != NONE).then(|| {
                let m = &self.marks[i as usize];
                i = m.prev;
                m
            })
        })
    }

    fn visitor_of(&self, id: Option<&str>) -> Option<u32> {
        id.and_then(|v| self.visitor_ids.get(&key(v)).copied())
    }

    fn session_of(&self, id: Option<&str>) -> Option<u32> {
        id.and_then(|s| self.session_ids.get(&key(s)).copied())
    }

    /// The payment as stored, with its visitor and first touch as of now.
    fn row(&self, p: &Pay) -> PaymentRow {
        let mut row = (*p.row).clone();
        let sv = self
            .pay_session(p)
            .filter(|&s| self.sessions[s as usize].touch != i64::MAX)
            .map(|s| self.sessions[s as usize].visitor);
        row.visitor = row
            .visitor_id
            .clone()
            .or_else(|| sv.map(|v| self.visitors[v as usize].id.to_string()))
            .or_else(|| p.customer_visitor.as_deref().map(str::to_string));
        let payer = if row.visitor_id.is_some() {
            self.pay_visitor(p)
        } else {
            sv
        };
        row.first_touch = payer
            .map(|v| self.visitors[v as usize].first_touch)
            .filter(|&s| s != NONE && self.sessions[s as usize].touch <= p.ts)
            .map(|s| {
                let sess = &self.sessions[s as usize];
                let channel = self.path_name(sess.cols[CHANNEL]);
                let referrer_host = self.name_of(sess.cols[REFERRER]);
                FirstTouch {
                    ts: from_micros(sess.touch),
                    source: source(referrer_host.as_deref(), &channel),
                    channel,
                    referrer_host,
                }
            });
        row
    }

    pub fn first_match(&self) -> Option<PaymentRow> {
        self.payments
            .iter()
            .find(|p| p.credited)
            .map(|p| self.row(p))
    }

    pub fn payment_list(
        &self,
        lo: i64,
        hi: i64,
        scope: &Scope,
        filter: PaymentFilter,
        limit: u32,
        offset: u32,
    ) -> PaymentPage {
        let keep = |p: &Pay| match filter {
            PaymentFilter::All => true,
            PaymentFilter::Attributed => p.credited,
            PaymentFilter::Unattributed => !p.credited,
            PaymentFilter::Refunded => p.row.refunded > 0,
        };
        let list: Vec<&Pay> = self.pays(lo, hi, scope).filter(|p| keep(p)).collect();
        let (skip, want) = (offset as usize, offset as usize + limit as usize);
        let mut rows = Vec::new();
        let mut end = list.len();
        // Stored by time, provider and ID; shown newest first, ties by provider and ID.
        while end > 0 && rows.len() < want {
            let mut start = end - 1;
            while start > 0 && list[start - 1].ts == list[end - 1].ts {
                start -= 1;
            }
            rows.extend_from_slice(&list[start..end]);
            end = start;
        }
        PaymentPage {
            total: list.len() as u64,
            rows: rows
                .into_iter()
                .take(want)
                .skip(skip)
                .map(|p| self.row(p))
                .collect(),
        }
    }

    pub fn payment_summary(&self, lo: i64, hi: i64) -> PaymentSummary {
        let all = Scope(None, String::new());
        let mut s = PaymentSummary::default();
        let mut customers = HashSet::new();
        for p in self.pays(lo, hi, &all) {
            s.payments += 1;
            if p.credited {
                s.attributed += 1;
            } else {
                s.unattributed += 1;
            }
            if p.row.refunded > 0 {
                s.refunded += 1;
            }
            customers.insert(p.customer);
            s.net_usd += p.net.unwrap_or(0.0);
            if let (Some(usd), Some(net)) = (p.row.amount_usd, p.net) {
                s.refunded_usd += usd - net;
            }
        }
        s.customers = customers.len() as u64;
        s
    }

    /// The last pageview, by time then path, at or before `ts` of visitor `v` or session `s`.
    fn last_seen(&self, v: Option<u32>, s: Option<u32>, ts: i64) -> Option<&Pageview> {
        let mut best: Option<&Pageview> = None;
        let own = s.filter(|&s| Some(self.sessions[s as usize].visitor) != v);
        for s in v.into_iter().flat_map(|v| self.sessions_of(v)).chain(own) {
            if self.sessions[s as usize].first > ts {
                continue;
            }
            for pv in self.pageviews_of(s) {
                let later = |b: &Pageview| {
                    (pv.ts, self.strings.name(pv.path)) > (b.ts, self.strings.name(b.path))
                };
                if pv.ts <= ts && best.is_none_or(later) {
                    best = Some(pv);
                }
            }
        }
        best
    }

    /// None when the payment is not indexed, such as a failed one.
    pub fn journey(&self, provider: &str, id: &str, window_days: i64) -> Option<Journey> {
        let &i = self.pay_ids.get(&key(&format!("{provider}:{id}")))?;
        let p = &self.payments[i as usize];
        let payment = self.row(p);
        let credited = self.pay_session(p);
        let visitor = self.pay_visitor(p);
        let from = p.ts - window_days * DAY_US;
        let mut picked: Vec<u32> = visitor
            .into_iter()
            .flat_map(|v| self.sessions_of(v))
            .filter(|&s| {
                let t = self.sessions[s as usize].touch;
                t != i64::MAX && (t <= p.ts && t >= from || Some(s) == credited)
            })
            .collect();
        if let Some(c) = credited
            && !picked.contains(&c)
            && self.sessions[c as usize].touch != i64::MAX
        {
            picked.push(c);
        }
        picked.sort_by(|&a, &b| {
            let (a, b) = (&self.sessions[a as usize], &self.sessions[b as usize]);
            (a.touch, &a.id).cmp(&(b.touch, &b.id))
        });
        let sessions = picked
            .into_iter()
            .map(|s| {
                let sess = &self.sessions[s as usize];
                let mut paths: Vec<(i64, &str)> = self
                    .pageviews_of(s)
                    .map(|pv| (pv.ts, self.strings.name(pv.path).unwrap_or("")))
                    .collect();
                paths.sort_unstable();
                let mut goals: Vec<(i64, &str)> = self
                    .marks_of(s)
                    .filter(|m| m.kind == GOAL)
                    .map(|m| (m.ts, self.strings.name(m.name).unwrap_or("")))
                    .collect();
                goals.sort_unstable();
                let channel = self.path_name(sess.cols[CHANNEL]);
                let referrer_host = self.name_of(sess.cols[REFERRER]);
                JourneySession {
                    session_id: sess.id.to_string(),
                    ts: from_micros(sess.touch),
                    source: source(referrer_host.as_deref(), &channel),
                    channel,
                    referrer_host,
                    campaign: self.name_of(sess.cols[2]),
                    paths: paths.into_iter().map(|p| p.1.to_string()).collect(),
                    goals: goals.into_iter().map(|g| g.1.to_string()).collect(),
                    credited: Some(s) == credited,
                }
            })
            .collect();
        let seen = self.last_seen(visitor, credited, p.ts);
        Some(Journey {
            payment,
            sessions,
            country: seen.and_then(|pv| self.attr(pv.attrs, COUNTRY)),
            browser: seen.and_then(|pv| self.attr(pv.attrs, BROWSER)),
            device: seen.and_then(|pv| self.attr(pv.attrs, DEVICE)),
            last_page: seen.map(|pv| self.path_name(pv.path)),
        })
    }

    pub fn activity(
        &self,
        local_day: &mut dyn FnMut(i64) -> anyhow::Result<i64>,
        who: &[Who],
    ) -> anyhow::Result<Vec<Activity>> {
        let mut out = Vec::with_capacity(who.len());
        for w in who {
            let v = self.visitor_of(w.visitor_id.as_deref());
            let s = self.session_of(w.session_id.as_deref());
            let moment = micros_of(w.ts);
            let mut a = Activity {
                days: vec![0; ACTIVITY_DAYS],
                ..Default::default()
            };
            if let Some(pv) = self.last_seen(v, s, moment) {
                a.country = self.attr(pv.attrs, COUNTRY);
                a.device = self.attr(pv.attrs, DEVICE);
                a.os = self.attr(pv.attrs, OS);
                a.browser = self.attr(pv.attrs, BROWSER);
                a.last_page = Some(self.path_name(pv.path));
            }
            let today = local_day(moment)?;
            let since = moment - 15 * DAY_US;
            let own = s.filter(|&s| Some(self.sessions[s as usize].visitor) != v);
            let mut counted: HashSet<(u32, i64)> = HashSet::new();
            for s in v.into_iter().flat_map(|v| self.sessions_of(v)).chain(own) {
                let sess = &self.sessions[s as usize];
                if sess.first > moment || sess.last <= since {
                    continue;
                }
                for pv in self.pageviews_of(s) {
                    if pv.ts > moment || pv.ts <= since {
                        continue;
                    }
                    let ago = today - local_day(pv.ts)?;
                    if (0..ACTIVITY_DAYS as i64).contains(&ago) && counted.insert((s, ago)) {
                        a.days[ACTIVITY_DAYS - 1 - ago as usize] += 1;
                    }
                }
            }
            out.push(a);
        }
        Ok(out)
    }

    pub(super) fn goals_in(&self, lo: i64, hi: i64) -> impl Iterator<Item = &Mark> + '_ {
        let from = self
            .goal_by_ts
            .partition_point(|&g| self.marks[g as usize].ts < lo);
        let to = self
            .goal_by_ts
            .partition_point(|&g| self.marks[g as usize].ts < hi);
        self.goal_by_ts[from..to]
            .iter()
            .map(|&g| &self.marks[g as usize])
    }

    pub fn goal_counts(&self, lo: i64, hi: i64) -> Vec<GoalCount> {
        let mut counts: HashMap<u32, u64> = HashMap::new();
        for g in self.goals_in(lo, hi) {
            *counts.entry(g.name).or_default() += 1;
        }
        let mut out: Vec<GoalCount> = counts
            .into_iter()
            .map(|(name, completions)| GoalCount {
                name: self.path_name(name),
                completions,
            })
            .collect();
        out.sort_by(|a, b| {
            b.completions
                .cmp(&a.completions)
                .then_with(|| a.name.cmp(&b.name))
        });
        out
    }

    pub fn goal_list(
        &self,
        lo: i64,
        hi: i64,
        goal: &str,
        limit: u32,
        offset: u32,
    ) -> (u64, Vec<GoalRow>) {
        let Some(name) = self.strings.get(goal) else {
            return (0, Vec::new());
        };
        let mut done: Vec<&Mark> = self.goals_in(lo, hi).filter(|g| g.name == name).collect();
        done.sort_by(|a, b| {
            b.ts.cmp(&a.ts).then_with(|| {
                self.sessions[a.session as usize]
                    .id
                    .cmp(&self.sessions[b.session as usize].id)
            })
        });
        let rows = done
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|g| {
                let sess = &self.sessions[g.session as usize];
                let (channel, referrer) = if sess.touch != i64::MAX {
                    (sess.cols[CHANNEL], sess.cols[REFERRER])
                } else {
                    (g.channel, g.referrer)
                };
                let visitor = &self.visitors[sess.visitor as usize];
                let channel = self.path_name(channel);
                let referrer_host = self.name_of(referrer);
                let vk = visitor.id.key();
                GoalRow {
                    visitor_id: visitor.id.to_string(),
                    session_id: sess.id.to_string(),
                    ts: from_micros(g.ts),
                    source: source(referrer_host.as_deref(), &channel),
                    channel,
                    referrer_host,
                    first_ts: (visitor.first_touch != NONE)
                        .then(|| from_micros(self.sessions[visitor.first_touch as usize].touch)),
                    spent_usd: self.spent.get(&vk).copied().unwrap_or(0.0),
                    customer: self.first_pay.get(&vk).map(|c| c.to_string()),
                }
            })
            .collect();
        (done.len() as u64, rows)
    }

    /// Latest event of session `s` before `hi`.
    fn last_before(&self, s: u32, hi: i64) -> Option<i64> {
        let sess = &self.sessions[s as usize];
        if sess.last < hi {
            return Some(sess.last);
        }
        let pvs = self.pageviews_of(s).map(|pv| pv.ts);
        pvs.chain(self.marks_of(s).map(|m| m.ts))
            .filter(|&t| t < hi)
            .max()
    }

    /// A visitor's row over events in [lo, hi), and whether they clicked out.
    fn visitor_row(&self, v: u32, lo: i64, hi: i64, last_seen: i64) -> VisitorRow {
        let visitor = &self.visitors[v as usize];
        let (mut visits, mut pageviews) = (0, 0);
        let mut last_pv: Option<&Pageview> = None;
        let mut first_pv: Option<&Pageview> = None;
        let (mut pricing, mut checkout) = (false, false);
        let mut goals: Vec<&str> = Vec::new();
        let mut exits: Vec<(i64, u32)> = Vec::new();
        for s in self.sessions_of(v) {
            let sess = &self.sessions[s as usize];
            if sess.first >= hi || sess.last < lo {
                continue;
            }
            let mut any = false;
            for pv in self.pageviews_of(s).filter(|pv| pv.ts >= lo && pv.ts < hi) {
                any = true;
                pageviews += 1;
                let later = |b: &Pageview| {
                    (pv.ts, self.strings.name(pv.path)) > (b.ts, self.strings.name(b.path))
                };
                if last_pv.is_none_or(later) {
                    last_pv = Some(pv);
                }
                let earlier = |b: &Pageview| {
                    (pv.ts, self.strings.name(pv.path)) < (b.ts, self.strings.name(b.path))
                };
                if first_pv.is_none_or(earlier) {
                    first_pv = Some(pv);
                }
                pricing |= self
                    .strings
                    .name(pv.path)
                    .is_some_and(|p| p.starts_with("/pricing"));
            }
            for m in self.marks_of(s).filter(|m| m.ts >= lo && m.ts < hi) {
                any = true;
                match m.kind {
                    GOAL => goals.push(self.strings.name(m.name).unwrap_or("")),
                    EXIT => {
                        checkout |= self
                            .strings
                            .name(m.name)
                            .is_some_and(crate::visitors::checkout_link);
                        exits.push((m.ts, m.name));
                    }
                    _ => {}
                }
            }
            visits += any as u64;
        }
        goals.sort_unstable();
        goals.dedup();
        let exit_link = exits
            .into_iter()
            .filter(|(t, n)| *n != NULL && last_pv.is_none_or(|pv| *t >= pv.ts))
            .max_by_key(|(t, _)| *t)
            .map(|(_, n)| self.path_name(n));
        let first = (visitor.first_touch != NONE)
            .then(|| &self.sessions[visitor.first_touch as usize].cols);
        let channel = first.map_or_else(|| "Direct".into(), |c| self.path_name(c[CHANNEL]));
        let referrer_host = first.and_then(|c| self.name_of(c[REFERRER]));
        let owned = self.owned.get(&visitor.id.key());
        let pays = owned.map_or(&[][..], |o| &o[..]);
        let payer = pays.first().map(|&i| {
            let p = &self.payments[i as usize];
            self.customers.name(p.customer).unwrap_or("").to_string()
        });
        VisitorRow {
            visitor_id: visitor.id.to_string(),
            source: source(referrer_host.as_deref(), &channel),
            channel,
            referrer_host,
            visits,
            pageviews,
            last_seen: from_micros(last_seen),
            landing_page: first_pv.map(|pv| self.path_name(pv.path)),
            exit_page: last_pv.map(|pv| self.path_name(pv.path)),
            exit_link,
            goals: goals.into_iter().map(str::to_string).collect(),
            pricing,
            checkout,
            payments: pays.len() as u64,
            revenue_usd: pays
                .iter()
                .map(|&i| self.payments[i as usize].net.unwrap_or(0.0))
                .sum(),
            who: payer.unwrap_or_else(|| format!("v:{}", visitor.id)),
        }
    }

    /// The visits, pages or revenue `visitor_row` gives `v`, to sort `visitor_list` by.
    fn sort_key(&self, v: u32, lo: i64, hi: i64, sort: Option<&str>) -> i64 {
        match sort {
            Some("visits" | "pages") => {
                let (mut visits, mut pages) = (0, 0);
                for s in self.sessions_of(v) {
                    let sess = &self.sessions[s as usize];
                    if sess.first >= hi || sess.last < lo {
                        continue;
                    }
                    let n = self
                        .pageviews_of(s)
                        .filter(|pv| pv.ts >= lo && pv.ts < hi)
                        .count() as i64;
                    pages += n;
                    visits += (n > 0 || self.marks_of(s).any(|m| m.ts >= lo && m.ts < hi)) as i64;
                }
                if sort == Some("visits") {
                    visits
                } else {
                    pages
                }
            }
            Some("revenue") => {
                let owned = self.owned.get(&self.visitors[v as usize].id.key());
                let revenue: f64 = owned
                    .map_or(&[][..], |o| &o[..])
                    .iter()
                    .map(|&i| self.payments[i as usize].net.unwrap_or(0.0))
                    .sum();
                (revenue * 100.0).round() as i64
            }
            _ => 0,
        }
    }

    pub fn visitor_list(
        &self,
        lo: i64,
        hi: i64,
        filter: &VisitorFilter,
        limit: u32,
        offset: u32,
    ) -> VisitorPage {
        let mut last: FastMap<u32, i64> = FastMap::default();
        for s in self.overlapping_ids(lo, hi) {
            if let Some(t) = self.last_before(s, hi).filter(|&t| t >= lo) {
                let e = last.entry(self.sessions[s as usize].visitor).or_insert(t);
                *e = (*e).max(t);
            }
        }
        let channel = filter.channel.as_deref().map(|c| self.strings.get(c));
        let hosts: Option<Vec<u32>> = filter
            .hosts
            .as_ref()
            .map(|h| h.iter().filter_map(|h| self.strings.get(h)).collect());
        let detailed =
            filter.goal.is_some() || filter.clicked_out.is_some() || filter.search.is_some();
        let sort = filter.sort.as_deref();
        let mut list: Vec<(i64, i64, u32, Option<Box<VisitorRow>>)> = Vec::new();
        for (v, t) in last {
            let ft = self.visitors[v as usize].first_touch;
            let cols = (ft != NONE).then(|| &self.sessions[ft as usize].cols);
            if let Some(c) = channel
                && cols.is_none_or(|cols| Some(cols[CHANNEL]) != c)
            {
                continue;
            }
            if let Some(h) = &hosts
                && cols.is_none_or(|cols| cols[REFERRER] == NULL || !h.contains(&cols[REFERRER]))
            {
                continue;
            }
            if let Some(paid) = filter.paid {
                let n = self
                    .owned
                    .get(&self.visitors[v as usize].id.key())
                    .map_or(0, Vec::len);
                if (n > 0) != paid {
                    continue;
                }
            }
            let row = detailed.then(|| Box::new(self.visitor_row(v, lo, hi, t)));
            if let Some(r) = &row {
                if filter.goal.as_ref().is_some_and(|g| !r.goals.contains(g)) {
                    continue;
                }
                if filter
                    .clicked_out
                    .is_some_and(|out| r.exit_link.is_some() != out)
                {
                    continue;
                }
                if let Some(q) = &filter.search {
                    let has = |s: Option<&str>| s.is_some_and(|s| s.to_lowercase().contains(q));
                    if !(has(r.landing_page.as_deref())
                        || has(r.exit_page.as_deref())
                        || has(r.referrer_host.as_deref())
                        || has(Some(&r.channel)))
                    {
                        continue;
                    }
                }
            }
            list.push((self.sort_key(v, lo, hi, sort), t, v, row));
        }
        let order = |a: &(i64, i64, u32, Option<Box<VisitorRow>>),
                     b: &(i64, i64, u32, Option<Box<VisitorRow>>)| {
            b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then_with(|| {
                self.visitors[a.2 as usize]
                    .id
                    .cmp(&self.visitors[b.2 as usize].id)
            })
        };
        let total = list.len() as u64;
        // Only the rows up to the end of the page need their order.
        let end = (offset as usize).saturating_add(limit as usize);
        if end < list.len() {
            list.select_nth_unstable_by(end, order);
            list.truncate(end);
        }
        list.sort_by(order);
        let rows = list
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|(_, t, v, row)| row.map_or_else(|| self.visitor_row(v, lo, hi, t), |r| *r))
            .collect();
        VisitorPage { total, rows }
    }

    pub fn timeline(&self, visitor_id: &str) -> Option<Timeline> {
        let v = self.visitor_of(Some(visitor_id));
        let mut events: Vec<(i64, u8, &str, Ev)> = Vec::new();
        let mut last: Option<&Pageview> = None;
        let ev =
            |s: u32, ts: i64, kind: &str, name: u32, path: u32, channel: u32, referrer, attrs| Ev {
                ts: from_micros(ts),
                session: self.sessions[s as usize].id.to_string(),
                kind: kind.to_string(),
                name: self.name_of(name),
                path: self.path_name(path),
                channel: self.path_name(channel),
                referrer_host: self.name_of(referrer),
                campaign: self.attr(attrs, CAMPAIGN),
            };
        for s in v.into_iter().flat_map(|v| self.sessions_of(v)) {
            for pv in self.pageviews_of(s) {
                if last.is_none_or(|b| {
                    (pv.ts, self.strings.name(pv.path)) > (b.ts, self.strings.name(b.path))
                }) {
                    last = Some(pv);
                }
                let path = self.strings.name(pv.path).unwrap_or("");
                let e = ev(
                    s,
                    pv.ts,
                    "pageview",
                    NULL,
                    pv.path,
                    pv.channel,
                    pv.referrer,
                    pv.attrs,
                );
                events.push((pv.ts, 0, path, e));
            }
            for m in self.marks_of(s) {
                let kind = ["", "goal", "exit", "leave", "back"][m.kind as usize];
                let path = self.strings.name(m.path).unwrap_or("");
                let e = ev(
                    s, m.ts, kind, m.name, m.path, m.channel, m.referrer, m.attrs,
                );
                events.push((m.ts, m.kind.min(3), path, e));
            }
        }
        events.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
        let payments = self.owned.get(&key(visitor_id)).map_or(Vec::new(), |o| {
            o.iter()
                .map(|&i| self.row(&self.payments[i as usize]))
                .collect()
        });
        let last = last.map(|pv| {
            (
                self.attr(pv.attrs, COUNTRY),
                self.attr(pv.attrs, BROWSER),
                self.attr(pv.attrs, DEVICE),
                self.attr(pv.attrs, OS),
            )
        });
        visitors::assemble(
            visitor_id,
            events.into_iter().map(|e| e.3).collect(),
            payments,
            last,
        )
    }
}

/// Reads with SQL's arguments; None leaves the question to SQL.
pub mod read {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    pub fn payment_list(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        filter: PaymentFilter,
        limit: u32,
        offset: u32,
        f: &Filter,
    ) -> anyhow::Result<Option<PaymentPage>> {
        let (lo, hi) = z.bounds(conn, period)?;
        let scope = ix.scope(f);
        Ok(Some(ix.payment_list(lo, hi, &scope, filter, limit, offset)))
    }

    pub fn payment_summary(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
    ) -> anyhow::Result<Option<PaymentSummary>> {
        let (lo, hi) = z.bounds(conn, period)?;
        Ok(Some(ix.payment_summary(lo, hi)))
    }

    pub fn activity(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        tz: &str,
        who: &[Who],
    ) -> anyhow::Result<Option<Vec<Activity>>> {
        let mut day = |us: i64| -> anyhow::Result<i64> {
            Ok((us + z.offset_at(conn, tz, us)?).div_euclid(DAY_US))
        };
        Ok(Some(ix.activity(&mut day, who)?))
    }

    pub fn goal_counts(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
    ) -> anyhow::Result<Option<Vec<GoalCount>>> {
        let (lo, hi) = z.bounds(conn, period)?;
        Ok(Some(ix.goal_counts(lo, hi)))
    }

    pub fn goal_list(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        goal: &str,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Option<(u64, Vec<GoalRow>)>> {
        let (lo, hi) = z.bounds(conn, period)?;
        Ok(Some(ix.goal_list(lo, hi, goal, limit, offset)))
    }

    pub fn visitor_list(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        filter: &VisitorFilter,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Option<VisitorPage>> {
        let (lo, hi) = z.bounds(conn, period)?;
        Ok(Some(ix.visitor_list(lo, hi, filter, limit, offset)))
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;
    use crate::index::tests::{SITE, check, filters, seeded};
    use crate::{stats, visitors};

    #[test]
    fn rows_match_sql() {
        let (_dir, store, today) = seeded();
        let ix = store.index_for_test(SITE);
        let ix = ix.read().unwrap();
        let conn = store.conn_for_test();
        let z = Zones::default();
        let all = Period {
            from: today - Duration::days(4000),
            to: today + Duration::days(1),
            tz: "UTC".into(),
        };
        let mut people: Vec<String> = Vec::new();
        for tz in ["UTC", "America/New_York"] {
            for (from, to) in [
                (today, today),
                (today - Duration::days(1), today),
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
                for f in filters() {
                    for filter in [
                        PaymentFilter::All,
                        PaymentFilter::Attributed,
                        PaymentFilter::Unattributed,
                        PaymentFilter::Refunded,
                    ] {
                        for (limit, offset) in [(10, 0), (7, 3), (100_000, 0)] {
                            let what =
                                format!("payment_list {at} {filter:?} {f:?} {limit} {offset}");
                            let sql =
                                stats::payment_list(&conn, SITE, &p, filter, limit, offset, &f)
                                    .unwrap();
                            let index =
                                read::payment_list(&ix, &z, &conn, &p, filter, limit, offset, &f)
                                    .unwrap();
                            check(&what, index, sql);
                        }
                    }
                }
                check(
                    &format!("payment_summary {at}"),
                    read::payment_summary(&ix, &z, &conn, &p).unwrap(),
                    stats::payment_summary(&conn, SITE, &p).unwrap(),
                );
                let goals = stats::goal_counts(&conn, SITE, &p).unwrap();
                check(
                    &format!("goal_counts {at}"),
                    read::goal_counts(&ix, &z, &conn, &p).unwrap(),
                    goals.clone(),
                );
                for g in &goals {
                    for (limit, offset) in [(10, 0), (100_000, 0)] {
                        let sql =
                            stats::goal_list(&conn, SITE, &p, &g.name, limit, offset).unwrap();
                        let who: Vec<Who> = sql
                            .1
                            .iter()
                            .map(|r| Who {
                                visitor_id: Some(r.visitor_id.clone()),
                                session_id: Some(r.session_id.clone()),
                                ts: r.ts,
                            })
                            .collect();
                        check(
                            &format!("goal_list {at} {}", g.name),
                            read::goal_list(&ix, &z, &conn, &p, &g.name, limit, offset).unwrap(),
                            sql,
                        );
                        check(
                            &format!("goal activity {at}"),
                            read::activity(&ix, &z, &conn, tz, &who).unwrap(),
                            stats::activity(&conn, SITE, tz, &who).unwrap(),
                        );
                    }
                }
                let channel = stats::breakdown(
                    &conn,
                    SITE,
                    &p,
                    stats::Dimension::Channel,
                    &Filter::default(),
                )
                .unwrap()
                .into_iter()
                .find_map(|r| r.key);
                let goal = goals.first().map(|g| g.name.clone());
                let hosts: Vec<String> = visitors::facets(&conn, SITE, &p)
                    .unwrap()
                    .sources
                    .into_iter()
                    .filter_map(|s| s.1)
                    .take(3)
                    .collect();
                for vf in [
                    VisitorFilter::default(),
                    VisitorFilter {
                        sort: Some("pages".into()),
                        ..Default::default()
                    },
                    VisitorFilter {
                        sort: Some("revenue".into()),
                        ..Default::default()
                    },
                    VisitorFilter {
                        search: Some("pricing".into()),
                        sort: Some("visits".into()),
                        ..Default::default()
                    },
                    VisitorFilter {
                        paid: Some(true),
                        ..Default::default()
                    },
                    VisitorFilter {
                        paid: Some(false),
                        ..Default::default()
                    },
                    VisitorFilter {
                        clicked_out: Some(true),
                        ..Default::default()
                    },
                    VisitorFilter {
                        clicked_out: Some(false),
                        ..Default::default()
                    },
                    VisitorFilter {
                        goal: goal.clone(),
                        ..Default::default()
                    },
                    VisitorFilter {
                        channel: channel.clone(),
                        ..Default::default()
                    },
                    VisitorFilter {
                        hosts: Some(hosts.clone()),
                        ..Default::default()
                    },
                ] {
                    for (limit, offset) in [(50, 0), (25, 25), (100_000, 0)] {
                        let sql =
                            visitors::visitor_list(&conn, SITE, &p, &vf, limit, offset).unwrap();
                        let who: Vec<Who> = sql
                            .rows
                            .iter()
                            .map(|r| Who {
                                visitor_id: Some(r.visitor_id.clone()),
                                session_id: None,
                                ts: r.last_seen,
                            })
                            .collect();
                        if limit == 50 {
                            people.extend(sql.rows.iter().map(|r| r.visitor_id.clone()));
                            check(
                                &format!("visitor activity {at}"),
                                read::activity(&ix, &z, &conn, tz, &who).unwrap(),
                                stats::activity(&conn, SITE, tz, &who).unwrap(),
                            );
                        }
                        check(
                            &format!("visitor_list {at} {vf:?} {limit} {offset}"),
                            read::visitor_list(&ix, &z, &conn, &p, &vf, limit, offset).unwrap(),
                            sql,
                        );
                    }
                }
            }
        }
        let pays = stats::payment_list(
            &conn,
            SITE,
            &all,
            PaymentFilter::All,
            100_000,
            0,
            &Filter::default(),
        )
        .unwrap();
        let who: Vec<Who> = pays.rows.iter().map(Who::of).collect();
        check(
            "payment activity",
            read::activity(&ix, &z, &conn, "Asia/Kolkata", &who).unwrap(),
            stats::activity(&conn, SITE, "Asia/Kolkata", &who).unwrap(),
        );
        for r in &pays.rows {
            for window in [1, 30, 90] {
                check(
                    &format!("journey {} {window}", r.provider_payment_id),
                    Some(
                        ix.journey(&r.provider, &r.provider_payment_id, window)
                            .unwrap(),
                    ),
                    stats::journey(&conn, SITE, &r.provider, &r.provider_payment_id, window)
                        .unwrap()
                        .unwrap(),
                );
            }
            people.extend(r.visitor.clone());
        }
        people.sort();
        people.dedup();
        assert!(
            pays.rows.len() > 50 && people.len() > 100,
            "{} {}",
            pays.rows.len(),
            people.len()
        );
        eprintln!(
            "checked {} payments and {} visitors",
            pays.rows.len(),
            people.len()
        );
        people.push("no-such-visitor".into());
        for v in &people {
            check(
                &format!("timeline {v}"),
                Some(ix.timeline(v)),
                visitors::timeline(&conn, SITE, v).unwrap(),
            );
        }
    }

    #[test]
    fn timelines_with_returns_match_sql() {
        use crate::events::{DuckStore, Event, EventStore};
        use crate::visitors::{Entry, Step};
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        let t0 = time::OffsetDateTime::now_utc() - Duration::days(1);
        let ev = |v: &str, s: &str, secs: i64, kind: &str, path: &str, name: Option<&str>| Event {
            site_id: SITE.into(),
            ts: t0 + Duration::seconds(secs),
            visitor_id: v.into(),
            session_id: s.into(),
            kind: kind.into(),
            name: name.map(Into::into),
            path: path.into(),
            referrer_host: (secs == 0).then(|| "news.ycombinator.com".into()),
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            channel: "Social".into(),
            country: Some("DE".into()),
            device: "Desktop".into(),
            browser: "Firefox".into(),
            os: "macOS".into(),
            region: None,
            city: None,
            host: None,
            language: None,
        };
        // r1 clicks out to x.com and comes back by Back, then returns 15 minutes later as a new visit.
        let r1 = |s, secs, kind, path, name| ev("r1", s, secs, kind, path, name);
        store
            .insert_events(&[
                r1("a", 0, "pageview", "/", None),
                r1("a", 60, "exit", "/", Some("x.com/virling")),
                r1("a", 60, "leave", "/", None),
                r1("a", 179, "back", "/", None),
                r1("a", 180, "pageview", "/", None),
                r1("a", 240, "pageview", "/pricing", None),
                r1("a", 300, "leave", "/pricing", None),
                r1("b", 1200, "pageview", "/", None),
            ])
            .unwrap();
        store.load_index(SITE);
        // r2 spends 10 minutes on checkout and comes back to the same visit.
        let r2 = |secs, kind, path| ev("r2", "c", secs, kind, path, None);
        store
            .insert_events(&[
                r2(0, "pageview", "/pricing"),
                r2(30, "leave", "/pricing"),
                r2(629, "back", "/welcome"),
                r2(630, "pageview", "/welcome"),
            ])
            .unwrap();
        let ix = store.index_for_test(SITE);
        let ix = ix.read().unwrap();
        let conn = store.conn_for_test();
        for v in ["r1", "r2"] {
            check(
                &format!("timeline {v}"),
                Some(ix.timeline(v)),
                visitors::timeline(&conn, SITE, v).unwrap(),
            );
        }
        let kinds = |v: &str| -> Vec<Vec<&'static str>> {
            let t = ix.timeline(v).unwrap();
            t.entries
                .iter()
                .filter_map(|e| match e {
                    Entry::Visit(v) => Some(
                        v.steps
                            .iter()
                            .map(|s| match s {
                                Step::Page { .. } => "page",
                                Step::Left { domain, .. } if domain == "x.com" => "left x.com",
                                Step::Back { .. } => "back",
                                Step::Exit { .. } => "exit",
                                _ => "other",
                            })
                            .collect(),
                    ),
                    Entry::Payment(_) => None,
                })
                .collect()
        };
        assert_eq!(
            kinds("r1"),
            vec![
                vec!["page", "left x.com", "back", "page", "page", "exit"],
                vec!["page", "exit"],
            ]
        );
        assert_eq!(kinds("r2"), vec![vec!["page", "back", "page", "exit"]]);
        let t = ix.timeline("r1").unwrap();
        let Entry::Visit(first) = &t.entries[0] else {
            unreachable!()
        };
        assert!(
            matches!(first.steps[0], Step::Page { secs: Some(60), .. }),
            "time away is not time on page"
        );
    }
}
