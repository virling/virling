//! Links that make long ranges cheap: each session and pageview knows its visitor's previous one with the same value.

use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

/// Last page not computed yet.
const UNKNOWN: u32 = u32::MAX - 1;
/// Ranges whose counts are kept across appends.
const RANGES_KEPT: usize = 32;

/// Payment rows of a breakdown per column, keyed by (unattributed, value).
pub(super) type ColumnPays = Vec<HashMap<(bool, u32), Agg>>;

/// Counts of long ranges kept across appends, updated in place when an append only adds to them.
#[derive(Default)]
pub(super) struct Kept {
    /// Per (lo, hi): distinct visitors at `value * 15 + column` over first pageviews in range.
    counts: HashMap<(i64, i64), Vec<u32>>,
    /// Per (lo, hi): distinct visitors per path id.
    pages: HashMap<(i64, i64), Vec<i64>>,
    /// Per (lo, hi): distinct visitors per (channel, referrer) of first pageviews in range.
    sources: HashMap<(i64, i64), HashMap<(u32, u32), u64>>,
    /// Per (lo, hi): visitors and sessions with an event in range, and which visitors.
    tracked: HashMap<(i64, i64), (u64, u64, Vec<bool>)>,
    /// Per (lo, hi): payment rows of every column, until a credited session or the payments change.
    pub(super) tpay: HashMap<(i64, i64), Arc<ColumnPays>>,
}

/// Whole seconds plus one, so 0 means none; `same <= lo / 1e6` holds exactly when the earlier one is before `lo`.
fn secs(us: i64) -> u32 {
    (us.div_euclid(1_000_000) + 1).clamp(1, u32::MAX as i64) as u32
}

/// `lo` in whole seconds, when the links can answer for it.
fn whole(lo: i64) -> Option<u32> {
    (lo >= 0 && lo % 1_000_000 == 0 && lo / 1_000_000 < u32::MAX as i64)
        .then_some((lo / 1_000_000) as u32)
}

impl SiteIndex {
    /// Recomputes `same` of visitor `v`'s touched sessions; true when `s` is the latest, so no other one changed.
    pub(super) fn relink_sessions(&mut self, v: u32, s: u32) -> bool {
        let latest = s;
        let mut list: Vec<(i64, u32)> = Vec::new();
        let mut s = self.visitors[v as usize].last_session;
        while s != NONE {
            let sess = &self.sessions[s as usize];
            if sess.touch != i64::MAX {
                list.push((sess.touch, s));
            }
            s = sess.prev;
        }
        list.sort_unstable();
        let last = list.last().is_some_and(|l| l.1 == latest);
        for (i, &(_, s)) in list.iter().enumerate() {
            let cols = self.sessions[s as usize].cols;
            let mut same = [0u32; 15];
            for (c, slot) in same.iter_mut().enumerate() {
                if let Some(&(t, _)) = list[..i]
                    .iter()
                    .rev()
                    .find(|&&(_, o)| self.sessions[o as usize].cols[c] == cols[c])
                {
                    *slot = secs(t);
                }
            }
            self.sessions[s as usize].same = same;
        }
        last
    }

    fn visitor_pageviews(&self, v: u32, path: Option<u32>) -> Vec<(u32, i64, u32)> {
        let mut list = Vec::new();
        let mut s = self.visitors[v as usize].last_session;
        while s != NONE {
            let mut i = self.sessions[s as usize].pv;
            while i != NONE {
                let pv = &self.pageviews[i as usize];
                if path.is_none_or(|p| p == pv.path) {
                    list.push((pv.path, pv.ts, i));
                }
                i = pv.prev;
            }
            s = self.sessions[s as usize].prev;
        }
        list.sort_unstable();
        list
    }

    fn link_pages(&mut self, list: &[(u32, i64, u32)]) {
        for (i, &(path, _, p)) in list.iter().enumerate() {
            self.pageviews[p as usize].same = match i.checked_sub(1).map(|j| list[j]) {
                Some((q, t, _)) if q == path => secs(t),
                _ => 0,
            };
        }
    }

    pub(super) fn relink_all_pages(&mut self, v: u32) {
        let list = self.visitor_pageviews(v, None);
        self.link_pages(&list);
    }

    /// True when the newest pageview is the visitor's latest of `path`, so no other link changed.
    pub(super) fn relink_pages(&mut self, v: u32, path: u32) -> bool {
        let list = self.visitor_pageviews(v, Some(path));
        self.link_pages(&list);
        list.last()
            .is_some_and(|l| l.2 as usize + 1 == self.pageviews.len())
    }

    /// Adds touched session `s` to kept counts, or drops them when `add` is false.
    pub(super) fn kept_touch(&mut self, s: u32, add: bool) {
        let kept = self.kept.get_mut().unwrap();
        if self
            .pays_by_session
            .contains_key(&self.sessions[s as usize].id.key())
        {
            kept.tpay.clear();
        }
        if !add {
            kept.counts.clear();
            kept.sources.clear();
            return;
        }
        let sess = &self.sessions[s as usize];
        let n = self.strings.names.len() * 15;
        for (&(lo, hi), counts) in kept.counts.iter_mut() {
            if sess.touch < lo || sess.touch >= hi {
                continue;
            }
            let lo_s = (lo / 1_000_000) as u32;
            counts.resize(n, 0);
            for c in 0..15 {
                if sess.same[c] <= lo_s {
                    counts[sess.cols[c] as usize * 15 + c] += 1;
                }
            }
        }
        for (&(lo, hi), counts) in kept.sources.iter_mut() {
            if sess.touch >= lo
                && sess.touch < hi
                && sess.cols[REFERRER] != NULL
                && sess.same[CHANNEL_SOURCE] <= (lo / 1_000_000) as u32
            {
                *counts
                    .entry((sess.cols[CHANNEL], sess.cols[REFERRER]))
                    .or_default() += 1;
            }
        }
    }

    /// Follows session `s` into kept tracked counts; `before` is its span before the event, None when new.
    pub(super) fn kept_span(&mut self, s: u32, before: Option<(i64, i64)>) {
        let kept = self.kept.get_mut().unwrap();
        let sess = &self.sessions[s as usize];
        let n = self.visitors.len();
        for (&(lo, hi), (visitors, sessions, seen)) in kept.tracked.iter_mut() {
            let overlaps = |(f, l): (i64, i64)| f < hi && l >= lo;
            if !overlaps((sess.first, sess.last)) || before.is_some_and(overlaps) {
                continue;
            }
            *sessions += 1;
            seen.resize(n, false);
            if !seen[sess.visitor as usize] {
                seen[sess.visitor as usize] = true;
                *visitors += 1;
            }
        }
    }

    /// `tracked` over every session, kept across appends.
    pub(super) fn kept_tracked(&self, lo: i64, hi: i64) -> (u64, u64, Vec<bool>) {
        let mut kept = self.kept.lock().unwrap();
        if let Some(t) = kept.tracked.get(&(lo, hi)) {
            let mut t = t.clone();
            t.2.resize(self.visitors.len(), false);
            return t;
        }
        let t = self.count_tracked(lo, hi, &Scope(None, String::new()));
        if kept.tracked.len() >= RANGES_KEPT {
            kept.tracked.clear();
        }
        kept.tracked.insert((lo, hi), t.clone());
        t
    }

    /// Adds the newest pageview to kept page counts, or drops them when `add` is false.
    pub(super) fn kept_page(&mut self, add: bool) {
        let kept = self.kept.get_mut().unwrap();
        if !add {
            kept.pages.clear();
            return;
        }
        let pv = self.pageviews[self.pageviews.len() - 1];
        let n = self.strings.names.len();
        for (&(lo, hi), counts) in kept.pages.iter_mut() {
            if pv.ts >= lo && pv.ts < hi && pv.same <= (lo / 1_000_000) as u32 {
                counts.resize(n, 0);
                counts[pv.path as usize] += 1;
            }
        }
    }

    /// Forgets the last page of payments a new pageview of visitor `v` in session `s` may change.
    pub(super) fn page_viewed(&mut self, v: u32, s: u32) {
        let keys = [
            self.pays_by_visitor
                .get(&self.visitors[v as usize].id.key()),
            self.pays_by_session
                .get(&self.sessions[s as usize].id.key()),
        ];
        for i in keys.into_iter().flatten().flatten() {
            self.last_page_of[*i as usize].store(UNKNOWN, Ordering::Relaxed);
        }
    }

    pub(super) fn index_payments(&mut self) {
        self.kept.get_mut().unwrap().tpay.clear();
        self.last_page_of = (0..self.payments.len())
            .map(|_| AtomicU32::new(UNKNOWN))
            .collect();
        self.pays_by_visitor.clear();
        self.pays_by_session.clear();
        for (i, p) in self.payments.iter().enumerate() {
            if let Some(v) = p.visitor {
                self.pays_by_visitor.entry(v).or_default().push(i as u32);
            }
            if let Some(s) = p.session {
                self.pays_by_session.entry(s).or_default().push(i as u32);
            }
        }
    }

    /// `last_page` of payment `i`, kept until a pageview may change it.
    pub(super) fn cached_last_page(&self, i: usize) -> Option<u32> {
        let slot = &self.last_page_of[i];
        let known = slot.load(Ordering::Relaxed);
        if known != UNKNOWN {
            return (known != NONE).then_some(known);
        }
        let page = self.last_page(&self.payments[i]);
        slot.store(page.unwrap_or(NONE), Ordering::Relaxed);
        page
    }

    /// `touch_counts` from the links: a session counts for a value when the visitor's previous one with it is before `lo`.
    pub(super) fn linked_counts(
        &self,
        lo: i64,
        hi: i64,
        scope: &Scope,
    ) -> Option<Vec<Vec<(u32, i64)>>> {
        let lo_s = whole(lo).filter(|_| scope.all())?;
        let mut kept = self.kept.lock().unwrap();
        if !kept.counts.contains_key(&(lo, hi)) {
            let mut counts = vec![0u32; self.strings.names.len() * 15];
            let from = self
                .by_touch
                .partition_point(|&s| self.sessions[s as usize].touch < lo);
            let to = self
                .by_touch
                .partition_point(|&s| self.sessions[s as usize].touch < hi);
            for &s in &self.by_touch[from..to] {
                let sess = &self.sessions[s as usize];
                for c in 0..15 {
                    if sess.same[c] <= lo_s {
                        counts[sess.cols[c] as usize * 15 + c] += 1;
                    }
                }
            }
            if kept.counts.len() >= RANGES_KEPT {
                kept.counts.clear();
            }
            kept.counts.insert((lo, hi), counts);
        }
        let counts = &kept.counts[&(lo, hi)];
        let mut out: Vec<Vec<(u32, i64)>> = vec![Vec::new(); 15];
        for (i, &k) in counts.iter().enumerate() {
            if k > 0 {
                out[i % 15].push(((i / 15) as u32, k as i64));
            }
        }
        Some(out)
    }

    /// `count_pages` from the links.
    pub(super) fn linked_pages(
        &self,
        lo: i64,
        hi: i64,
        scope: &Scope,
    ) -> Option<HashMap<u32, i64>> {
        let lo_s = whole(lo).filter(|_| scope.all())?;
        let mut kept = self.kept.lock().unwrap();
        if !kept.pages.contains_key(&(lo, hi)) {
            let mut counts = vec![0i64; self.strings.names.len()];
            let from = self
                .pv_by_ts
                .partition_point(|&p| self.pageviews[p as usize].ts < lo);
            let to = self
                .pv_by_ts
                .partition_point(|&p| self.pageviews[p as usize].ts < hi);
            for &p in &self.pv_by_ts[from..to] {
                let pv = &self.pageviews[p as usize];
                if pv.same <= lo_s {
                    counts[pv.path as usize] += 1;
                }
            }
            if kept.pages.len() >= RANGES_KEPT {
                kept.pages.clear();
            }
            kept.pages.insert((lo, hi), counts);
        }
        Some(
            kept.pages[&(lo, hi)]
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, n)| *n > 0)
                .map(|(id, n)| (id as u32, n))
                .collect(),
        )
    }

    /// `channel_sources` from the links; the combined key stands for the pair only while no referrer is empty.
    pub(super) fn linked_sources(
        &self,
        lo: i64,
        hi: i64,
        scope: &Scope,
    ) -> Option<Vec<(String, String, u64)>> {
        let lo_s = whole(lo).filter(|_| scope.all() && self.strings.get("").is_none())?;
        let mut kept = self.kept.lock().unwrap();
        if !kept.sources.contains_key(&(lo, hi)) {
            let mut counts: HashMap<(u32, u32), u64> = HashMap::new();
            let from = self
                .by_touch
                .partition_point(|&s| self.sessions[s as usize].touch < lo);
            let to = self
                .by_touch
                .partition_point(|&s| self.sessions[s as usize].touch < hi);
            for &s in &self.by_touch[from..to] {
                let sess = &self.sessions[s as usize];
                if sess.cols[REFERRER] != NULL && sess.same[CHANNEL_SOURCE] <= lo_s {
                    *counts
                        .entry((sess.cols[CHANNEL], sess.cols[REFERRER]))
                        .or_default() += 1;
                }
            }
            if kept.sources.len() >= RANGES_KEPT {
                kept.sources.clear();
            }
            kept.sources.insert((lo, hi), counts);
        }
        let mut counts = kept.sources[&(lo, hi)].clone();
        drop(kept);
        // Sessions begun before the range count by their first pageview in it, once per visitor and pair.
        let mut late: HashSet<(u32, u32, u32)> = HashSet::new();
        let start = self.by_first.partition_point(|&s| {
            self.sessions[s as usize].first < lo.saturating_sub(self.longest)
        });
        let end = self
            .by_first
            .partition_point(|&s| self.sessions[s as usize].first < lo);
        for &s in &self.by_first[start..end] {
            let sess = &self.sessions[s as usize];
            if sess.last < lo || sess.touch >= lo {
                continue;
            }
            let name = |id| self.strings.name(id).unwrap_or("");
            let mut first: Option<&Pageview> = None;
            let mut i = sess.pv;
            while i != NONE {
                let pv = &self.pageviews[i as usize];
                if pv.ts >= lo
                    && pv.ts < hi
                    && first.is_none_or(|f| (pv.ts, name(pv.path)) < (f.ts, name(f.path)))
                {
                    first = Some(pv);
                }
                i = pv.prev;
            }
            if let Some(f) = first.filter(|f| f.referrer != NULL) {
                late.insert((sess.visitor, f.channel, f.referrer));
            }
        }
        for (v, c, r) in late {
            let mut s = self.visitors[v as usize].last_session;
            let mut seen = false;
            while s != NONE && !seen {
                let o = &self.sessions[s as usize];
                seen =
                    o.touch >= lo && o.touch < hi && o.cols[CHANNEL] == c && o.cols[REFERRER] == r;
                s = o.prev;
            }
            if !seen {
                *counts.entry((c, r)).or_default() += 1;
            }
        }
        Some(
            counts
                .into_iter()
                .map(|((c, r), n)| (self.path_name(c), self.path_name(r), n))
                .collect(),
        )
    }
}
