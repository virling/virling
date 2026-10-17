//! Per-site in-memory session index, so dashboard aggregates read compact arrays instead of scanning events.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use duckdb::{Connection, params};

use crate::channel::source_name;
use crate::events::Event;
use crate::stats::{CUSTOMER, Filter, NET_USD, Period};

/// No session, visitor or pageview.
const NONE: u32 = u32::MAX;
/// String id of NULL.
const NULL: u32 = 0;
/// Idle gap that ends a visit at ingest; sessions with a longer gap are not indexed.
const IDLE_US: i64 = 30 * 60 * 1_000_000;
pub const DAY_US: i64 = 86_400 * 1_000_000;
pub const HOUR_US: i64 = 3_600 * 1_000_000;

/// First-pageview columns kept per session, in this order.
pub const COLS: [&str; 11] = [
    "channel",
    "referrer_host",
    "utm_campaign",
    "utm_term",
    "path",
    "country",
    "city",
    "device",
    "browser",
    "os",
    "language",
];
pub const CHANNEL: usize = 0;
pub const REFERRER: usize = 1;
pub const PATH: usize = 4;

fn col(name: &str) -> Option<usize> {
    COLS.iter().position(|c| *c == name)
}

/// 64-bit hash of an ID string, so maps hold no copies of IDs.
fn key(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[derive(Default)]
struct Strings {
    ids: HashMap<Box<str>, u32>,
    names: Vec<Box<str>>,
}

impl Strings {
    fn new() -> Self {
        Strings {
            ids: HashMap::new(),
            names: vec!["".into()],
        }
    }

    fn id(&mut self, s: Option<&str>) -> u32 {
        let Some(s) = s else { return NULL };
        if let Some(id) = self.ids.get(s) {
            return *id;
        }
        let id = self.names.len() as u32;
        self.names.push(s.into());
        self.ids.insert(s.into(), id);
        id
    }

    fn get(&self, s: &str) -> Option<u32> {
        self.ids.get(s).copied()
    }

    fn name(&self, id: u32) -> Option<&str> {
        (id != NULL).then(|| &*self.names[id as usize])
    }
}

#[derive(Clone)]
struct Session {
    visitor: u32,
    first: i64,
    last: i64,
    /// The visitor's session before this one, or NONE.
    prev: u32,
    /// Time of the first pageview, or i64::MAX without one.
    touch: i64,
    cols: [u32; 15],
    /// Latest added pageview of this session, or NONE.
    pv: u32,
}

#[derive(Clone, Copy)]
struct Pageview {
    ts: i64,
    session: u32,
    path: u32,
    channel: u32,
    referrer: u32,
    /// Previous pageview added for the same session, or NONE.
    prev: u32,
}

#[derive(Clone, Copy)]
struct Goal {
    ts: i64,
    session: u32,
    name: u32,
}

#[derive(Clone, Copy)]
struct Visitor {
    last_session: u32,
    /// Session holding the visitor's first pageview, or NONE.
    first_touch: u32,
}

#[derive(Clone)]
struct Pay {
    ts: i64,
    net: Option<f64>,
    customer: u32,
    renewal: bool,
    /// Key of the payment's own visitor_id and of its credited session.
    visitor: Option<u64>,
    session: Option<u64>,
    credited: bool,
    channel: u32,
    provider: u32,
    /// Start of the credited visit.
    touch_ts: Option<i64>,
}

/// Index of one site's sessions, pageviews, goals and payments.
pub struct SiteIndex {
    strings: Strings,
    session_ids: HashMap<u64, u32>,
    visitor_ids: HashMap<u64, u32>,
    sessions: Vec<Session>,
    visitors: Vec<Visitor>,
    pageviews: Vec<Pageview>,
    goals: Vec<Goal>,
    /// Sessions by first event, pageviews and goals by time, sessions with a pageview by first pageview.
    by_first: Vec<u32>,
    by_touch: Vec<u32>,
    pv_by_ts: Vec<u32>,
    goal_by_ts: Vec<u32>,
    customers: Strings,
    payments: Vec<Pay>,
    /// Longest session, to find sessions overlapping a range from their start.
    longest: i64,
    /// Set when an append breaks an invariant; the index is rebuilt before its next use.
    broken: bool,
    /// Intermediate results shared by the reads of one dashboard, cleared by every append.
    memo: Mutex<Vec<(String, Arc<dyn std::any::Any + Send + Sync>)>>,
    /// Earliest payment credited to a visit, read with the payments.
    pub first_match: Option<crate::stats::PaymentRow>,
}

/// Inserts `item` into `order`, kept sorted by `at`, searching from the end since items mostly arrive in order.
fn insert_sorted(order: &mut Vec<u32>, item: u32, at: impl Fn(u32) -> i64) {
    let t = at(item);
    let mut i = order.len();
    while i > 0 && at(order[i - 1]) > t {
        i -= 1;
    }
    order.insert(i, item);
}

impl SiteIndex {
    /// Reads a site from one snapshot; None when its data breaks an index invariant.
    pub fn load(conn: &Connection, site_id: &str) -> anyhow::Result<Option<SiteIndex>> {
        conn.execute_batch("BEGIN TRANSACTION")?;
        let result = Self::read(conn, site_id);
        conn.execute_batch("COMMIT")?;
        result
    }

    fn read(conn: &Connection, site_id: &str) -> anyhow::Result<Option<SiteIndex>> {
        let mut ix = SiteIndex {
            strings: Strings::new(),
            session_ids: HashMap::new(),
            visitor_ids: HashMap::new(),
            sessions: Vec::new(),
            visitors: Vec::new(),
            pageviews: Vec::new(),
            goals: Vec::new(),
            by_first: Vec::new(),
            by_touch: Vec::new(),
            pv_by_ts: Vec::new(),
            goal_by_ts: Vec::new(),
            customers: Strings::new(),
            payments: Vec::new(),
            longest: 0,
            broken: false,
            memo: Mutex::default(),
            first_match: None,
        };
        let mut stmt = conn.prepare(
            "SELECT session_id, min(visitor_id), max(visitor_id), min(us), max(us), max(gap)
            FROM (
                SELECT session_id, visitor_id, epoch_us(ts) AS us,
                    epoch_us(ts) - epoch_us(lag(ts) OVER (PARTITION BY session_id ORDER BY ts)) AS gap
                FROM events WHERE site_id = ?
            )
            GROUP BY session_id ORDER BY min(us), session_id",
        )?;
        let mut rows = stmt.query(params![site_id])?;
        while let Some(r) = rows.next()? {
            let (sid, v1, v2): (String, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
            let gap: Option<i64> = r.get(5)?;
            if v1 != v2 || gap.is_some_and(|g| g >= IDLE_US) {
                tracing::warn!(session = sid, "session breaks the index invariants");
                return Ok(None);
            }
            let v = ix.visitor(&v1);
            let s = ix.sessions.len() as u32;
            let (first, last): (i64, i64) = (r.get(3)?, r.get(4)?);
            ix.sessions.push(Session {
                visitor: v,
                first,
                last,
                prev: ix.visitors[v as usize].last_session,
                touch: i64::MAX,
                cols: [NULL; 15],
                pv: NONE,
            });
            ix.visitors[v as usize].last_session = s;
            ix.session_ids.insert(key(&sid), s);
            ix.by_first.push(s);
            ix.longest = ix.longest.max(last - first);
        }
        let mut stmt = conn.prepare(&format!(
            "SELECT session_id, epoch_us(ts), {} FROM touches WHERE site_id = ? ORDER BY ts, path",
            COLS.join(", ")
        ))?;
        let mut rows = stmt.query(params![site_id])?;
        while let Some(r) = rows.next()? {
            let sid: String = r.get(0)?;
            let Some(&s) = ix.session_ids.get(&key(&sid)) else {
                tracing::warn!(session = sid, "touch without events");
                return Ok(None);
            };
            let mut cols = [NULL; 15];
            for (i, c) in cols.iter_mut().take(COLS.len()).enumerate() {
                *c = ix.strings.id(r.get::<_, Option<String>>(i + 2)?.as_deref());
            }
            ix.combine(&mut cols);
            let sess = &mut ix.sessions[s as usize];
            sess.touch = r.get(1)?;
            sess.cols = cols;
            ix.by_touch.push(s);
            let v = sess.visitor as usize;
            if ix.visitors[v].first_touch == NONE {
                ix.visitors[v].first_touch = s;
            }
        }
        let mut stmt = conn.prepare(
            "SELECT session_id, epoch_us(ts), kind, name, path, channel, referrer_host
            FROM events WHERE site_id = ? AND kind IN ('pageview', 'goal') ORDER BY ts",
        )?;
        let mut rows = stmt.query(params![site_id])?;
        while let Some(r) = rows.next()? {
            let sid: String = r.get(0)?;
            let s = ix.session_ids[&key(&sid)];
            let ts: i64 = r.get(1)?;
            let kind: String = r.get(2)?;
            if kind == "goal" {
                let name = ix.strings.id(r.get::<_, Option<String>>(3)?.as_deref());
                ix.goal_by_ts.push(ix.goals.len() as u32);
                ix.goals.push(Goal {
                    ts,
                    session: s,
                    name,
                });
            } else {
                let name = ix.strings.id(Some(&r.get::<_, String>(4)?));
                let channel = ix.strings.id(Some(&r.get::<_, String>(5)?));
                let referrer = ix.strings.id(r.get::<_, Option<String>>(6)?.as_deref());
                let p = ix.pageviews.len() as u32;
                ix.pageviews.push(Pageview {
                    ts,
                    session: s,
                    path: name,
                    channel,
                    referrer,
                    prev: ix.sessions[s as usize].pv,
                });
                ix.sessions[s as usize].pv = p;
                ix.pv_by_ts.push(p);
            }
        }
        ix.load_payments(conn, site_id)?;
        ix.shrink();
        Ok(Some(ix))
    }

    fn visitor(&mut self, id: &str) -> u32 {
        let next = self.visitors.len() as u32;
        let v = *self.visitor_ids.entry(key(id)).or_insert(next);
        if v == next {
            self.visitors.push(Visitor {
                last_session: NONE,
                first_touch: NONE,
            });
        }
        v
    }

    /// Replaces the payments and their credited sessions.
    pub fn load_payments(&mut self, conn: &Connection, site_id: &str) -> anyhow::Result<()> {
        let mut stmt = conn.prepare(&format!(
            "SELECT epoch_us(p.ts), {NET_USD}, {CUSTOMER}, p.is_renewal, p.visitor_id, a.session_id, a.channel,
                p.provider, epoch_us(a.touch_ts)
            FROM payments p
            LEFT JOIN attribution a ON a.site_id = p.site_id AND a.provider = p.provider
                AND a.provider_payment_id = p.provider_payment_id
            WHERE p.site_id = ? AND p.status <> 'failed'
            ORDER BY p.ts"
        ))?;
        let mut rows = stmt.query(params![site_id])?;
        let mut customers = Strings::new();
        let mut payments = Vec::new();
        while let Some(r) = rows.next()? {
            let visitor: Option<String> = r.get(4)?;
            let session: Option<String> = r.get(5)?;
            payments.push(Pay {
                ts: r.get(0)?,
                net: r.get(1)?,
                customer: customers.id(Some(&r.get::<_, String>(2)?)),
                renewal: r.get(3)?,
                visitor: visitor.map(|v| key(&v)),
                credited: session.is_some(),
                session: session.map(|s| key(&s)),
                channel: self.strings.id(r.get::<_, Option<String>>(6)?.as_deref()),
                provider: self.strings.id(Some(&r.get::<_, String>(7)?)),
                touch_ts: r.get(8)?,
            });
        }
        self.customers = customers;
        self.payments = payments;
        self.first_match = crate::stats::first_match(conn, site_id)?;
        self.memo.get_mut().unwrap().clear();
        Ok(())
    }

    /// Adds newly stored events; idempotent, so events already read from the snapshot may be applied again.
    pub fn apply(&mut self, events: &[Event]) {
        self.memo.get_mut().unwrap().clear();
        for e in events {
            let ts = (e.ts.unix_timestamp_nanos() / 1000) as i64;
            let v = self.visitor(&e.visitor_id);
            let next = self.sessions.len() as u32;
            let s = *self.session_ids.entry(key(&e.session_id)).or_insert(next);
            if s == next {
                self.sessions.push(Session {
                    visitor: v,
                    first: ts,
                    last: ts,
                    prev: self.visitors[v as usize].last_session,
                    touch: i64::MAX,
                    cols: [NULL; 15],
                    pv: NONE,
                });
                self.visitors[v as usize].last_session = s;
                let sessions = &self.sessions;
                insert_sorted(&mut self.by_first, s, |i| sessions[i as usize].first);
            } else {
                let sess = &mut self.sessions[s as usize];
                if sess.visitor != v || sess.first - ts >= IDLE_US || ts - sess.last >= IDLE_US {
                    self.broken = true;
                    return;
                }
                sess.last = sess.last.max(ts);
                if ts < sess.first {
                    sess.first = ts;
                    let sessions = &self.sessions;
                    let at = self.by_first.iter().rposition(|&x| x == s).unwrap();
                    self.by_first.remove(at);
                    insert_sorted(&mut self.by_first, s, |i| sessions[i as usize].first);
                }
                let sess = &self.sessions[s as usize];
                self.longest = self.longest.max(sess.last - sess.first);
            }
            match e.kind.as_str() {
                "pageview" => {
                    let path = self.strings.id(Some(&e.path));
                    let p = self.pageviews.len() as u32;
                    self.pageviews.push(Pageview {
                        ts,
                        session: s,
                        path,
                        channel: self.strings.id(Some(&e.channel)),
                        referrer: self.strings.id(e.referrer_host.as_deref()),
                        prev: self.sessions[s as usize].pv,
                    });
                    self.sessions[s as usize].pv = p;
                    let pageviews = &self.pageviews;
                    insert_sorted(&mut self.pv_by_ts, p, |i| pageviews[i as usize].ts);
                    let sess = &self.sessions[s as usize];
                    let earlier = sess.touch == i64::MAX
                        || (ts, &*e.path)
                            < (sess.touch, self.strings.name(sess.cols[PATH]).unwrap_or(""));
                    if earlier {
                        if sess.touch != i64::MAX {
                            let at = self.by_touch.iter().rposition(|&x| x == s).unwrap();
                            self.by_touch.remove(at);
                        }
                        let values = [
                            Some(e.channel.as_str()),
                            e.referrer_host.as_deref(),
                            e.utm_campaign.as_deref(),
                            e.utm_term.as_deref(),
                            Some(e.path.as_str()),
                            e.country.as_deref(),
                            e.city.as_deref(),
                            Some(e.device.as_str()),
                            Some(e.browser.as_str()),
                            Some(e.os.as_str()),
                            e.language.as_deref(),
                        ];
                        let mut cols = [NULL; 15];
                        for (c, v) in cols.iter_mut().zip(values) {
                            *c = self.strings.id(v);
                        }
                        self.combine(&mut cols);
                        let sess = &mut self.sessions[s as usize];
                        sess.touch = ts;
                        sess.cols = cols;
                        let sessions = &self.sessions;
                        insert_sorted(&mut self.by_touch, s, |i| sessions[i as usize].touch);
                        let first = self.visitors[v as usize].first_touch;
                        let before = first == NONE || {
                            let f = &self.sessions[first as usize];
                            (ts, &*e.path)
                                < (f.touch, self.strings.name(f.cols[PATH]).unwrap_or(""))
                        };
                        if before {
                            self.visitors[v as usize].first_touch = s;
                        }
                    }
                }
                "goal" => {
                    let g = self.goals.len() as u32;
                    self.goals.push(Goal {
                        ts,
                        session: s,
                        name: self.strings.id(e.name.as_deref()),
                    });
                    let goals = &self.goals;
                    insert_sorted(&mut self.goal_by_ts, g, |i| goals[i as usize].ts);
                }
                _ => {}
            }
        }
    }

    /// Rough heap size in bytes.
    pub fn bytes(&self) -> usize {
        self.sessions.capacity() * std::mem::size_of::<Session>()
            + self.pageviews.capacity() * std::mem::size_of::<Pageview>()
            + self.goals.capacity() * std::mem::size_of::<Goal>()
            + self.visitors.capacity() * std::mem::size_of::<Visitor>()
            + (self.by_first.capacity()
                + self.by_touch.capacity()
                + self.pv_by_ts.capacity()
                + self.goal_by_ts.capacity())
                * 4
            + (self.session_ids.capacity() + self.visitor_ids.capacity()) * 24
            + self.payments.capacity() * std::mem::size_of::<Pay>()
            + self
                .strings
                .names
                .iter()
                .map(|n| n.len() * 2 + 48)
                .sum::<usize>()
    }

    /// `make()` once per key until the next append.
    fn memo<T: Send + Sync + 'static>(&self, key: String, make: impl FnOnce() -> T) -> Arc<T> {
        if let Some(v) = self
            .memo
            .lock()
            .unwrap()
            .iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, v)| v.clone().downcast::<T>().ok())
        {
            return v;
        }
        let v = Arc::new(make());
        let mut memo = self.memo.lock().unwrap();
        if memo.len() >= 64 {
            memo.remove(0);
        }
        memo.push((key, v.clone()));
        v
    }

    fn shrink(&mut self) {
        self.sessions.shrink_to_fit();
        self.pageviews.shrink_to_fit();
        self.goals.shrink_to_fit();
        self.visitors.shrink_to_fit();
        self.by_first.shrink_to_fit();
        self.by_touch.shrink_to_fit();
        self.pv_by_ts.shrink_to_fit();
        self.goal_by_ts.shrink_to_fit();
        self.session_ids.shrink_to_fit();
        self.visitor_ids.shrink_to_fit();
    }

    pub fn first_event(&self) -> Option<i64> {
        self.by_first
            .first()
            .map(|&s| self.sessions[s as usize].first)
    }

    pub fn has_goals(&self) -> bool {
        !self.goals.is_empty()
    }
}

/// Columns after COLS holding dimension keys built from them.
pub const CHANNEL_SOURCE: usize = 11;
pub const PAGE_SOURCE: usize = 12;
pub const COUNTRY_CITY: usize = 13;
pub const DEVICE_OS: usize = 14;

impl SiteIndex {
    /// Fills the combined keys as SQL builds them: `a || '|' || b` is NULL when a part is.
    fn combine(&mut self, cols: &mut [u32; 15]) {
        let name = |strings: &Strings, i: usize| strings.name(cols[i]).map(str::to_string);
        let s = &self.strings;
        let (ch, rf, pa, co, ci, de, os) = (
            name(s, 0),
            name(s, 1).unwrap_or_default(),
            name(s, 4),
            name(s, 5),
            name(s, 6),
            name(s, 7),
            name(s, 9),
        );
        let keys = [
            ch.as_ref().map(|c| format!("{c}|{rf}")),
            pa.as_ref()
                .zip(ch.as_ref())
                .map(|(p, c)| format!("{p}|{c}|{rf}")),
            co.zip(ci).map(|(a, b)| format!("{a}|{b}")),
            de.zip(os).map(|(a, b)| format!("{a}|{b}")),
        ];
        for (i, k) in keys.into_iter().enumerate() {
            cols[CHANNEL_SOURCE + i] = self.strings.id(k.as_deref());
        }
    }

    /// The payment's visitor, when it has events here.
    fn pay_visitor(&self, p: &Pay) -> Option<u32> {
        p.visitor.and_then(|v| self.visitor_ids.get(&v).copied())
    }

    /// The payment's credited session, when indexed.
    fn pay_session(&self, p: &Pay) -> Option<u32> {
        p.session.and_then(|s| self.session_ids.get(&s).copied())
    }
}

/// Sessions a filter keeps (None keeps every session), and the filter as a memo key.
pub struct Scope(Option<Vec<bool>>, String);

impl Scope {
    fn has(&self, s: u32) -> bool {
        self.0.as_ref().is_none_or(|m| m[s as usize])
    }

    fn all(&self) -> bool {
        self.0.is_none()
    }
}

/// Local time buckets of a range: (UTC start, UTC end, label), in time order.
pub type Pieces = Vec<(i64, i64, i64)>;

/// Time zone offsets as (UTC start, offset) segments per zone and year, and period bounds, read from DuckDB once.
#[derive(Default)]
pub struct Zones {
    years: Mutex<HashMap<(String, i32), Arc<Vec<(i64, i64)>>>>,
    bounds: Mutex<HashMap<(String, time::Date, time::Date), (i64, i64)>>,
}

impl Zones {
    fn year(&self, conn: &Connection, tz: &str, year: i32) -> anyhow::Result<Arc<Vec<(i64, i64)>>> {
        if let Some(z) = self.years.lock().unwrap().get(&(tz.to_string(), year)) {
            return Ok(z.clone());
        }
        let mut stmt = conn.prepare(
            "SELECT epoch_us(t), epoch_us(timezone(?, t)) - epoch_us(t)
            FROM generate_series(make_timestamptz(?, 1, 1, 0, 0, 0, 'UTC'),
                make_timestamptz(? + 1, 1, 1, 0, 0, 0, 'UTC') - INTERVAL 15 MINUTE, INTERVAL 15 MINUTE) s(t)
            ORDER BY 1",
        )?;
        let mut segs: Vec<(i64, i64)> = Vec::new();
        let mut rows = stmt.query(params![tz, year, year])?;
        while let Some(r) = rows.next()? {
            let (t, off): (i64, i64) = (r.get(0)?, r.get(1)?);
            if segs.last().is_none_or(|l| l.1 != off) {
                segs.push((t, off));
            }
        }
        let segs = Arc::new(segs);
        self.years
            .lock()
            .unwrap()
            .insert((tz.to_string(), year), segs.clone());
        Ok(segs)
    }

    /// Buckets of `unit` local time (a day or an hour) covering [lo, hi); labels are local unit numbers since the epoch.
    pub fn pieces(
        &self,
        conn: &Connection,
        tz: &str,
        lo: i64,
        hi: i64,
        unit: i64,
    ) -> anyhow::Result<Option<Pieces>> {
        let year = |us: i64| {
            time::OffsetDateTime::from_unix_timestamp(us.div_euclid(1_000_000))
                .map(|t| t.year())
                .unwrap_or(1970)
        };
        let mut segs: Vec<(i64, i64)> = Vec::new();
        for y in year(lo)..=year(hi) {
            for &(t, off) in self.year(conn, tz, y)?.iter() {
                if segs.last().is_none_or(|l| l.1 != off) {
                    segs.push((t, off));
                }
            }
        }
        let mut out: Pieces = Vec::new();
        for (i, &(start, off)) in segs.iter().enumerate() {
            let end = segs.get(i + 1).map_or(i64::MAX, |n| n.0);
            let (s, e) = (start.max(lo), end.min(hi));
            if s >= e {
                continue;
            }
            let mut t = s;
            while t < e {
                let label = (t + off).div_euclid(unit);
                let next = ((label + 1) * unit - off).min(e);
                if next - t < IDLE_US {
                    // A bucket shorter than the idle gap may sit inside a session; leave it to SQL.
                    return Ok(None);
                }
                out.push((t, next, label));
                t = next;
            }
        }
        Ok(Some(out))
    }
}

/// Counts per label of distinct values, fed in non-decreasing label order per value.
struct Marks {
    last: Vec<i64>,
}

impl Marks {
    fn new(n: usize) -> Self {
        Marks {
            last: vec![i64::MIN; n],
        }
    }

    /// True the first time `v` is seen with `label` or a later one.
    fn first(&mut self, v: u32, label: i64) -> bool {
        let l = &mut self.last[v as usize];
        if *l < label {
            *l = label;
            true
        } else {
            false
        }
    }
}

/// One payment-side row of a breakdown: (unattributed, key, payments, revenue, customers).
pub type RawRow = (bool, Option<String>, i64, i64, f64, i64);

impl SiteIndex {
    pub fn scope(&self, f: &Filter) -> Arc<Scope> {
        if f.is_empty() {
            return Arc::new(Scope(None, String::new()));
        }
        let key = format!("{f:?}");
        self.memo(format!("scope {key}"), || self.scope_of(f, key.clone()))
    }

    fn scope_of(&self, f: &Filter, key: String) -> Scope {
        let mut conds: Vec<(usize, Vec<u32>)> = Vec::new();
        let mut never = false;
        for (c, vals) in &f.first {
            let Some(i) = col(c) else {
                never = true;
                continue;
            };
            let ids: Vec<u32> = vals
                .iter()
                .filter_map(|v| match v {
                    Some(v) => self.strings.get(v),
                    None => Some(NULL),
                })
                .collect();
            conds.push((i, ids));
        }
        let mut keep: Vec<bool> = self
            .sessions
            .iter()
            .map(|s| {
                !never
                    && s.touch != i64::MAX
                    && conds.iter().all(|(i, ids)| ids.contains(&s.cols[*i]))
            })
            .collect();
        if let Some(path) = &f.viewed {
            let mut viewed = vec![false; self.sessions.len()];
            if let Some(id) = self.strings.get(path) {
                for p in &self.pageviews {
                    if p.path == id {
                        viewed[p.session as usize] = true;
                    }
                }
            }
            for (k, v) in keep.iter_mut().zip(viewed) {
                *k &= v;
            }
        }
        Scope(Some(keep), key)
    }

    /// Sessions with an event in [lo, hi), in order of their first event.
    fn overlapping(&self, lo: i64, hi: i64) -> impl Iterator<Item = &Session> + '_ {
        let from = self.by_first.partition_point(|&s| {
            self.sessions[s as usize].first < lo.saturating_sub(self.longest)
        });
        let to = self
            .by_first
            .partition_point(|&s| self.sessions[s as usize].first < hi);
        self.by_first[from..to]
            .iter()
            .map(move |&s| &self.sessions[s as usize])
            .filter(move |s| s.last >= lo)
    }

    fn overlapping_ids(&self, lo: i64, hi: i64) -> impl Iterator<Item = u32> + '_ {
        let from = self.by_first.partition_point(|&s| {
            self.sessions[s as usize].first < lo.saturating_sub(self.longest)
        });
        let to = self
            .by_first
            .partition_point(|&s| self.sessions[s as usize].first < hi);
        self.by_first[from..to]
            .iter()
            .copied()
            .filter(move |&s| self.sessions[s as usize].last >= lo)
    }

    /// Distinct visitors and sessions with an event in [lo, hi), plus which visitors those are.
    pub fn tracked(&self, lo: i64, hi: i64, scope: &Scope) -> (u64, u64, Vec<bool>) {
        let mut seen = vec![false; self.visitors.len()];
        let (mut visitors, mut sessions) = (0, 0);
        for s in self.overlapping_ids(lo, hi) {
            if !scope.has(s) {
                continue;
            }
            sessions += 1;
            let v = self.sessions[s as usize].visitor as usize;
            if !seen[v] {
                seen[v] = true;
                visitors += 1;
            }
        }
        (visitors, sessions, seen)
    }

    fn pays<'a>(
        &'a self,
        lo: i64,
        hi: i64,
        scope: &'a Scope,
    ) -> impl Iterator<Item = &'a Pay> + 'a {
        let from = self.payments.partition_point(|p| p.ts < lo);
        let to = self.payments.partition_point(|p| p.ts < hi);
        self.payments[from..to]
            .iter()
            .filter(move |p| scope.all() || self.pay_session(p).is_some_and(|s| scope.has(s)))
    }

    /// (payments, customers, new customers, revenue, converted, matched payments, matched revenue).
    pub fn payment_totals(
        &self,
        lo: i64,
        hi: i64,
        scope: &Scope,
        tracked: &[bool],
    ) -> (u64, u64, u64, f64, u64, u64, f64) {
        let mut seen: HashSet<u32> = HashSet::new();
        let mut new: HashSet<u32> = HashSet::new();
        let mut converted: HashSet<u32> = HashSet::new();
        let (mut n, mut revenue, mut matched, mut matched_usd) = (0, 0.0, 0, 0.0);
        for p in self.pays(lo, hi, scope) {
            n += 1;
            seen.insert(p.customer);
            if !p.renewal {
                new.insert(p.customer);
            }
            revenue += p.net.unwrap_or(0.0);
            if self.pay_visitor(p).is_some_and(|v| tracked[v as usize]) {
                converted.insert(p.customer);
                matched += 1;
                matched_usd += p.net.unwrap_or(0.0);
            }
        }
        (
            n,
            seen.len() as u64,
            new.len() as u64,
            revenue,
            converted.len() as u64,
            matched,
            matched_usd,
        )
    }

    /// Distinct visitors per bucket label with an event in [lo, hi).
    pub fn visitors_per(&self, pieces: &Pieces, scope: &Scope) -> HashMap<i64, u64> {
        let mut out: HashMap<i64, u64> = HashMap::new();
        let (Some(lo), Some(hi)) = (pieces.first().map(|p| p.0), pieces.last().map(|p| p.1)) else {
            return out;
        };
        let mut marks = Marks::new(self.visitors.len());
        for s in self.overlapping_ids(lo, hi) {
            if !scope.has(s) {
                continue;
            }
            let sess = &self.sessions[s as usize];
            let start = pieces.partition_point(|p| p.1 <= sess.first);
            for p in &pieces[start..] {
                if p.0 > sess.last {
                    break;
                }
                if marks.first(sess.visitor, p.2) {
                    *out.entry(p.2).or_default() += 1;
                }
            }
        }
        out
    }

    /// Whether visitor `v` has an event in [lo, hi).
    fn active(&self, v: u32, lo: i64, hi: i64) -> bool {
        let mut s = self.visitors[v as usize].last_session;
        while s != NONE {
            let sess = &self.sessions[s as usize];
            if sess.first < hi && sess.last >= lo {
                return true;
            }
            s = sess.prev;
        }
        false
    }

    /// Per label: (payments, customers, revenue, converted, matched revenue, revenue by channel).
    #[allow(clippy::type_complexity)]
    pub fn payments_per(
        &self,
        pieces: &Pieces,
        scope: &Scope,
        matched: bool,
    ) -> HashMap<i64, (u64, u64, f64, u64, f64, Vec<(Option<String>, f64)>)> {
        let mut out: HashMap<
            i64,
            (
                u64,
                HashSet<u32>,
                f64,
                HashSet<u32>,
                f64,
                Vec<(Option<String>, f64)>,
            ),
        > = HashMap::new();
        let (Some(lo), Some(hi)) = (pieces.first().map(|p| p.0), pieces.last().map(|p| p.1)) else {
            return HashMap::new();
        };
        for p in self.pays(lo, hi, scope) {
            let i = pieces.partition_point(|x| x.1 <= p.ts);
            let label = pieces[i].2;
            let e = out.entry(label).or_default();
            e.0 += 1;
            e.1.insert(p.customer);
            e.2 += p.net.unwrap_or(0.0);
            if matched {
                // Any event of the visitor on the same local day, inside the range.
                let (mut a, mut b) = (i, i + 1);
                while a > 0 && pieces[a - 1].2 == label {
                    a -= 1;
                }
                while b < pieces.len() && pieces[b].2 == label {
                    b += 1;
                }
                let day: Vec<(i64, i64)> = pieces[a..b].iter().map(|x| (x.0, x.1)).collect();
                if self
                    .pay_visitor(p)
                    .is_some_and(|v| day.iter().any(|(a, b)| self.active(v, *a, *b)))
                {
                    e.3.insert(p.customer);
                    e.4 += p.net.unwrap_or(0.0);
                }
            }
            let channel = if p.credited {
                self.strings.name(p.channel).map(str::to_string)
            } else {
                None
            };
            match e.5.iter_mut().find(|c| c.0 == channel) {
                Some(c) => c.1 += p.net.unwrap_or(0.0),
                None => e.5.push((channel, p.net.unwrap_or(0.0))),
            }
        }
        out.into_iter()
            .map(|(k, (n, c, r, cv, mr, ch))| (k, (n, c.len() as u64, r, cv.len() as u64, mr, ch)))
            .collect()
    }

    /// Sessions in scope whose first pageview is in [lo, hi), ordered by visitor.
    fn touched(&self, lo: i64, hi: i64, scope: &Scope) -> Arc<Vec<u32>> {
        self.memo(format!("touched {lo} {hi} {}", scope.1), || {
            let from = self
                .by_touch
                .partition_point(|&s| self.sessions[s as usize].touch < lo);
            let to = self
                .by_touch
                .partition_point(|&s| self.sessions[s as usize].touch < hi);
            let mut list: Vec<(u32, u32)> = self.by_touch[from..to]
                .iter()
                .filter(|&&s| scope.has(s))
                .map(|&s| (self.sessions[s as usize].visitor, s))
                .collect();
            list.sort_unstable();
            list.into_iter().map(|(_, s)| s).collect()
        })
    }

    /// Distinct visitors per value of column `k` of sessions whose first pageview is in [lo, hi).
    pub fn touch_visitors(
        &self,
        lo: i64,
        hi: i64,
        scope: &Scope,
        k: usize,
    ) -> Vec<(Option<String>, i64)> {
        let touched = self.touched(lo, hi, scope);
        let mut counts = vec![0i64; self.strings.names.len()];
        let mut run: Vec<u32> = Vec::new();
        let mut visitor = NONE;
        for &s in touched.iter() {
            let sess = &self.sessions[s as usize];
            if sess.visitor != visitor {
                visitor = sess.visitor;
                run.clear();
            }
            let id = sess.cols[k];
            if !run.contains(&id) {
                run.push(id);
                counts[id as usize] += 1;
            }
        }
        counts
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .map(|(id, n)| (self.strings.name(id as u32).map(str::to_string), *n))
            .collect()
    }

    /// Payment rows of a breakdown keyed by column `k` of the credited session's first pageview.
    pub fn touch_payments(&self, lo: i64, hi: i64, scope: &Scope, k: usize) -> Vec<RawRow> {
        let mut rows: HashMap<(bool, u32), (i64, f64, HashSet<u32>)> = HashMap::new();
        for p in self.pays(lo, hi, scope) {
            let key = self
                .pay_session(p)
                .map_or(NULL, |s| self.sessions[s as usize].cols[k]);
            let e = rows.entry((!p.credited, key)).or_default();
            e.0 += 1;
            e.1 += p.net.unwrap_or(0.0);
            e.2.insert(p.customer);
        }
        rows.into_iter()
            .map(|((u, key), (n, r, c))| {
                (
                    u,
                    self.strings.name(key).map(str::to_string),
                    0,
                    n,
                    r,
                    c.len() as i64,
                )
            })
            .collect()
    }

    /// Distinct visitors per path over pageviews in [lo, hi).
    pub fn page_visitors(&self, lo: i64, hi: i64, scope: &Scope) -> Arc<HashMap<u32, i64>> {
        self.memo(format!("pages {lo} {hi} {}", scope.1), || {
            self.count_pages(lo, hi, scope)
        })
    }

    /// Sessions in scope with an event in [lo, hi), ordered by visitor.
    fn overlapping_by_visitor(&self, lo: i64, hi: i64, scope: &Scope) -> Arc<Vec<u32>> {
        self.memo(format!("overlapping {lo} {hi} {}", scope.1), || {
            let mut list: Vec<(u32, u32)> = self
                .overlapping_ids(lo, hi)
                .filter(|&s| scope.has(s))
                .map(|s| (self.sessions[s as usize].visitor, s))
                .collect();
            list.sort_unstable();
            list.into_iter().map(|(_, s)| s).collect()
        })
    }

    fn count_pages(&self, lo: i64, hi: i64, scope: &Scope) -> HashMap<u32, i64> {
        let sessions = self.overlapping_by_visitor(lo, hi, scope);
        let mut stamp = vec![NONE; self.strings.names.len()];
        let mut counts = vec![0i64; self.strings.names.len()];
        let mut visitor = NONE;
        for &s in sessions.iter() {
            let sess = &self.sessions[s as usize];
            if sess.visitor != visitor {
                visitor = sess.visitor;
            }
            let mut i = sess.pv;
            while i != NONE {
                let pv = &self.pageviews[i as usize];
                if pv.ts >= lo && pv.ts < hi && stamp[pv.path as usize] != visitor {
                    stamp[pv.path as usize] = visitor;
                    counts[pv.path as usize] += 1;
                }
                i = pv.prev;
            }
        }
        counts
            .into_iter()
            .enumerate()
            .filter(|(_, n)| *n > 0)
            .map(|(id, n)| (id as u32, n))
            .collect()
    }

    pub fn path_name(&self, id: u32) -> String {
        self.strings.name(id).unwrap_or("").to_string()
    }

    /// Payment rows of the Page breakdown: one per distinct page of the credited session.
    pub fn page_payments(&self, lo: i64, hi: i64, scope: &Scope) -> Vec<RawRow> {
        let mut rows: HashMap<(bool, Option<u32>), (i64, f64, HashSet<u32>)> = HashMap::new();
        for p in self.pays(lo, hi, scope) {
            let mut paths: Vec<u32> = Vec::new();
            if let Some(s) = self.pay_session(p) {
                let mut i = self.sessions[s as usize].pv;
                while i != NONE {
                    let pv = &self.pageviews[i as usize];
                    if !paths.contains(&pv.path) {
                        paths.push(pv.path);
                    }
                    i = pv.prev;
                }
            }
            let keys: Vec<Option<u32>> = if paths.is_empty() {
                vec![None]
            } else {
                paths.into_iter().map(Some).collect()
            };
            for key in keys {
                let e = rows.entry((!p.credited, key)).or_default();
                e.0 += 1;
                e.1 += p.net.unwrap_or(0.0);
                e.2.insert(p.customer);
            }
        }
        rows.into_iter()
            .map(|((u, key), (n, r, c))| {
                (u, key.map(|k| self.path_name(k)), 0, n, r, c.len() as i64)
            })
            .collect()
    }

    /// The last pageview, by time then path, at or before `ts` of the visitor or of the credited session.
    fn last_page(&self, p: &Pay) -> Option<u32> {
        let mut best: Option<(i64, &str, u32)> = None;
        let mut consider = |pv: &Pageview| {
            if pv.ts > p.ts {
                return;
            }
            let path = self.strings.name(pv.path).unwrap_or("");
            if best.is_none_or(|b| (pv.ts, path) > (b.0, b.1)) {
                best = Some((pv.ts, path, pv.path));
            }
        };
        let walk = |s: u32, consider: &mut dyn FnMut(&Pageview)| {
            let mut i = self.sessions[s as usize].pv;
            while i != NONE {
                let pv = &self.pageviews[i as usize];
                consider(pv);
                i = pv.prev;
            }
        };
        let visitor = self.pay_visitor(p);
        if let Some(v) = visitor {
            let mut s = self.visitors[v as usize].last_session;
            while s != NONE {
                walk(s, &mut consider);
                s = self.sessions[s as usize].prev;
            }
        }
        if let Some(s) = self.pay_session(p)
            && visitor != Some(self.sessions[s as usize].visitor)
        {
            walk(s, &mut consider);
        }
        best.map(|b| b.2)
    }

    /// (path, visitors, payments, revenue, customers) of pages viewed last before paying.
    pub fn last_pages(&self, lo: i64, hi: i64, scope: &Scope) -> Vec<(String, u64, u64, f64, u64)> {
        let seen = self.page_visitors(lo, hi, scope);
        let mut rows: HashMap<u32, (u64, f64, HashSet<u32>)> = HashMap::new();
        for p in self.pays(lo, hi, scope) {
            if let Some(path) = self.last_page(p) {
                let e = rows.entry(path).or_default();
                e.0 += 1;
                e.1 += p.net.unwrap_or(0.0);
                e.2.insert(p.customer);
            }
        }
        rows.into_iter()
            .map(|(path, (n, r, c))| {
                (
                    self.path_name(path),
                    seen.get(&path).copied().unwrap_or(0) as u64,
                    n,
                    r,
                    c.len() as u64,
                )
            })
            .collect()
    }

    /// (channel, referrer host) of each session's first pageview in [lo, hi), by distinct visitors.
    pub fn channel_sources(&self, lo: i64, hi: i64, scope: &Scope) -> Vec<(String, String, u64)> {
        // A session's first pageview in range is its first pageview, unless the session started before the range.
        let mut late: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        for s in self.overlapping_ids(lo, hi) {
            let sess = &self.sessions[s as usize];
            if sess.touch >= lo || !scope.has(s) {
                continue;
            }
            let mut first: Option<&Pageview> = None;
            let mut i = sess.pv;
            while i != NONE {
                let pv = &self.pageviews[i as usize];
                let name = |id| self.strings.name(id).unwrap_or("");
                if pv.ts >= lo
                    && pv.ts < hi
                    && first.is_none_or(|f| (pv.ts, name(pv.path)) < (f.ts, name(f.path)))
                {
                    first = Some(pv);
                }
                i = pv.prev;
            }
            if let Some(f) = first.filter(|f| f.referrer != NULL) {
                late.entry(sess.visitor)
                    .or_default()
                    .push((f.channel, f.referrer));
            }
        }
        let mut counts: HashMap<(u32, u32), u64> = HashMap::new();
        let mut run: Vec<(u32, u32)> = Vec::new();
        let mut visitor = NONE;
        let mut flush =
            |v: u32, run: &mut Vec<(u32, u32)>, late: &mut HashMap<u32, Vec<(u32, u32)>>| {
                if let Some(extra) = late.remove(&v) {
                    for pair in extra {
                        if !run.contains(&pair) {
                            run.push(pair);
                        }
                    }
                }
                for pair in run.drain(..) {
                    *counts.entry(pair).or_default() += 1;
                }
            };
        for &s in self.touched(lo, hi, scope).iter() {
            let sess = &self.sessions[s as usize];
            if sess.visitor != visitor {
                if visitor != NONE {
                    flush(visitor, &mut run, &mut late);
                }
                visitor = sess.visitor;
            }
            let pair = (sess.cols[CHANNEL], sess.cols[REFERRER]);
            if pair.1 != NULL && !run.contains(&pair) {
                run.push(pair);
            }
        }
        if visitor != NONE {
            flush(visitor, &mut run, &mut late);
        }
        let rest: Vec<u32> = late.keys().copied().collect();
        for v in rest {
            flush(v, &mut run, &mut late);
        }
        counts
            .into_iter()
            .map(|((c, r), n)| (self.path_name(c), self.path_name(r), n))
            .collect()
    }

    /// First visits of visitors seen in [lo, hi) by (channel, referrer), and goals by distinct visitors.
    #[allow(clippy::type_complexity)]
    pub fn facets(
        &self,
        lo: i64,
        hi: i64,
    ) -> (Vec<(String, Option<String>, u64)>, Vec<(String, u64)>) {
        let (_, _, seen) = self.tracked(lo, hi, &Scope(None, String::new()));
        let mut sources: HashMap<(u32, u32), u64> = HashMap::new();
        for (v, on) in seen.iter().enumerate() {
            if !on {
                continue;
            }
            let s = self.visitors[v].first_touch;
            if s != NONE {
                let c = &self.sessions[s as usize].cols;
                *sources.entry((c[CHANNEL], c[REFERRER])).or_default() += 1;
            }
        }
        let from = self
            .goal_by_ts
            .partition_point(|&g| self.goals[g as usize].ts < lo);
        let to = self
            .goal_by_ts
            .partition_point(|&g| self.goals[g as usize].ts < hi);
        let mut pairs: Vec<(u32, u32)> = self.goal_by_ts[from..to]
            .iter()
            .map(|&g| &self.goals[g as usize])
            .map(|g| (g.name, self.sessions[g.session as usize].visitor))
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        let mut goals: HashMap<u32, u64> = HashMap::new();
        for (name, _) in pairs {
            *goals.entry(name).or_default() += 1;
        }
        let mut sources: Vec<(String, Option<String>, u64)> = sources
            .into_iter()
            .map(|((c, r), n)| {
                (
                    self.path_name(c),
                    self.strings.name(r).map(str::to_string),
                    n,
                )
            })
            .collect();
        sources.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| a.0.cmp(&b.0))
                .then_with(|| match (&a.1, &b.1) {
                    (Some(x), Some(y)) => x.cmp(y),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
        });
        let mut goals: Vec<(String, u64)> = goals
            .into_iter()
            .map(|(g, n)| (self.path_name(g), n))
            .collect();
        goals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        (sources, goals)
    }
}

/// Top source names per channel, as the report shows them.
pub fn top_sources(mut rows: Vec<(String, String, u64)>) -> Vec<(String, Vec<String>)> {
    rows.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.2.cmp(&a.2))
            .then_with(|| a.1.cmp(&b.1))
    });
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (channel, host, _) in rows {
        if out.last().is_none_or(|(c, _)| *c != channel) {
            out.push((channel, Vec::new()));
        }
        let names = &mut out.last_mut().unwrap().1;
        let name = source_name(&host);
        if names.len() < 3 && !names.contains(&name) {
            names.push(name);
        }
    }
    out
}

enum Slot {
    /// Events written while the index is read, applied once it is ready.
    Loading(Vec<Event>),
    Ready(Arc<RwLock<SiteIndex>>, usize),
    /// The site's data does not fit the index; reads use SQL.
    Unfit,
}

/// Site indexes kept in memory under a byte budget, least recently used dropped first.
pub struct Indexes {
    slots: Mutex<HashMap<String, (Slot, Instant)>>,
    cap: usize,
    /// Sites whose payments changed since the index read them.
    stale_payments: Mutex<HashSet<String>>,
}

impl Indexes {
    pub fn new(cap: usize) -> Self {
        Indexes {
            slots: Mutex::default(),
            cap,
            stale_payments: Mutex::default(),
        }
    }

    /// The site's index when loaded; otherwise starts loading it in the background and returns None.
    pub fn get(
        self: &Arc<Self>,
        site_id: &str,
        conn: impl FnOnce() -> anyhow::Result<Connection>,
    ) -> anyhow::Result<Option<Arc<RwLock<SiteIndex>>>> {
        if self.cap == 0 {
            return Ok(None);
        }
        let mut slots = self.slots.lock().unwrap();
        match slots.get_mut(site_id) {
            Some((Slot::Ready(ix, _), used)) => {
                *used = Instant::now();
                let ix = ix.clone();
                drop(slots);
                self.fresh(site_id, ix, conn)
            }
            Some(_) => Ok(None),
            None => {
                slots.insert(
                    site_id.to_string(),
                    (Slot::Loading(Vec::new()), Instant::now()),
                );
                drop(slots);
                let (this, site, conn) = (self.clone(), site_id.to_string(), conn()?);
                std::thread::spawn(move || {
                    if let Err(err) = this.load(&site, &conn) {
                        tracing::error!(%err, site_id = site, "loading the session index failed");
                        this.slots.lock().unwrap().remove(&site);
                    }
                });
                Ok(None)
            }
        }
    }

    /// Reads the site into memory now, replacing any slot it has.
    pub fn load(&self, site_id: &str, conn: &Connection) -> anyhow::Result<()> {
        let started = Instant::now();
        self.slots
            .lock()
            .unwrap()
            .entry(site_id.to_string())
            .or_insert((Slot::Loading(Vec::new()), Instant::now()));
        self.stale_payments.lock().unwrap().remove(site_id);
        let loaded = SiteIndex::load(conn, site_id)?;
        let mut slots = self.slots.lock().unwrap();
        let pending = match slots.remove(site_id) {
            Some((Slot::Loading(p), _)) => p,
            _ => Vec::new(),
        };
        let fit = loaded.map(|mut ix| {
            ix.apply(&pending);
            ix
        });
        let Some(ix) = fit.filter(|ix| !ix.broken && ix.bytes() <= self.cap) else {
            tracing::warn!(
                site_id,
                "site data does not fit the session index; reading from SQL"
            );
            slots.insert(site_id.to_string(), (Slot::Unfit, Instant::now()));
            return Ok(());
        };
        let bytes = ix.bytes();
        let mut used: usize = slots
            .values()
            .map(|(s, _)| if let Slot::Ready(_, b) = s { *b } else { 0 })
            .sum();
        while used + bytes > self.cap {
            let Some(oldest) = slots
                .iter()
                .filter(|(_, (s, _))| matches!(s, Slot::Ready(..)))
                .min_by_key(|(_, (_, t))| *t)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some((Slot::Ready(_, b), _)) = slots.remove(&oldest) {
                used -= b;
                tracing::info!(site_id = oldest, "evicted session index");
            }
        }
        tracing::info!(
            site_id,
            mb = bytes / 1_000_000,
            ms = started.elapsed().as_millis() as u64,
            "loaded session index"
        );
        slots.insert(
            site_id.to_string(),
            (
                Slot::Ready(Arc::new(RwLock::new(ix)), bytes),
                Instant::now(),
            ),
        );
        Ok(())
    }

    fn fresh(
        &self,
        site_id: &str,
        ix: Arc<RwLock<SiteIndex>>,
        conn: impl FnOnce() -> anyhow::Result<Connection>,
    ) -> anyhow::Result<Option<Arc<RwLock<SiteIndex>>>> {
        if ix.read().unwrap().broken {
            self.slots.lock().unwrap().remove(site_id);
            return Ok(None);
        }
        if self.stale_payments.lock().unwrap().remove(site_id) {
            let conn = conn()?;
            ix.write().unwrap().load_payments(&conn, site_id)?;
        }
        Ok(Some(ix))
    }

    /// Adds stored events to loaded or loading indexes.
    pub fn apply(&self, events: &[Event]) {
        let mut by_site: HashMap<&str, Vec<Event>> = HashMap::new();
        for e in events {
            by_site.entry(&e.site_id).or_default().push(e.clone());
        }
        let mut ready = Vec::new();
        {
            let mut slots = self.slots.lock().unwrap();
            for (site, list) in by_site {
                match slots.get_mut(site) {
                    Some((Slot::Loading(pending), _)) => pending.extend(list),
                    Some((Slot::Ready(ix, _), _)) => ready.push((ix.clone(), list)),
                    _ => {}
                }
            }
        }
        for (ix, list) in ready {
            ix.write().unwrap().apply(&list);
        }
    }

    pub fn payments_changed(&self, site_id: &str) {
        self.stale_payments
            .lock()
            .unwrap()
            .insert(site_id.to_string());
    }

    /// Bytes held by loaded indexes.
    pub fn bytes(&self) -> usize {
        self.slots
            .lock()
            .unwrap()
            .values()
            .map(|(s, _)| if let Slot::Ready(_, b) = s { *b } else { 0 })
            .sum()
    }
}

impl Zones {
    /// UTC micros of [lo, hi) for a period, as BOUNDS computes them.
    pub fn bounds(&self, conn: &Connection, p: &Period) -> anyhow::Result<(i64, i64)> {
        let key = (p.tz.clone(), p.from, p.to);
        if let Some(b) = self.bounds.lock().unwrap().get(&key) {
            return Ok(*b);
        }
        let b: (i64, i64) = conn.query_row(
            "SELECT epoch_us(timezone(?, CAST(? AS DATE)::TIMESTAMP)),
                epoch_us(timezone(?, (CAST(? AS DATE) + 1)::TIMESTAMP))",
            params![p.tz, p.from.to_string(), p.tz, p.to.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        self.bounds.lock().unwrap().insert(key, b);
        Ok(b)
    }
}

/// Reads the index in place of the SQL in stats.rs, with the same results.
pub mod read {
    use super::*;
    use crate::stats::{
        BreakdownRow, DayPoint, Dimension, Overview, Period, Totals, breakdown_rows, ratio,
        sort_rows,
    };
    use crate::visitors::Facets;

    pub fn key_of(d: Dimension) -> Option<usize> {
        Some(match d {
            Dimension::Channel => 0,
            Dimension::Referrer => 1,
            Dimension::Campaign => 2,
            Dimension::Keyword => 3,
            Dimension::LandingPage => 4,
            Dimension::Country => 5,
            Dimension::Device => 7,
            Dimension::Browser => 8,
            Dimension::Os => 9,
            Dimension::Language => 10,
            Dimension::ChannelSource => CHANNEL_SOURCE,
            Dimension::PageSource => PAGE_SOURCE,
            Dimension::CountryCity => COUNTRY_CITY,
            Dimension::DeviceOs => DEVICE_OS,
            Dimension::Page => return None,
        })
    }

    pub fn totals(ix: &SiteIndex, lo: i64, hi: i64, scope: &Scope) -> Totals {
        let (visitors, sessions, seen) = ix.tracked(lo, hi, scope);
        let (payments, customers, new_customers, revenue_usd, converted, matched, matched_usd) =
            ix.payment_totals(lo, hi, scope, &seen);
        let some = matched > 0;
        Totals {
            visitors,
            sessions,
            payments,
            customers,
            new_customers,
            revenue_usd,
            converted,
            matched_payments: matched,
            matched_revenue_usd: matched_usd,
            revenue_per_visitor: ratio(matched_usd, visitors).filter(|_| some),
            conversion: ratio(converted as f64, visitors).filter(|_| some),
        }
    }

    pub fn overview(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Option<Overview>> {
        let scope = ix.scope(f);
        let (lo, hi) = z.bounds(conn, period)?;
        let (plo, phi) = z.bounds(conn, &period.previous())?;
        Ok(Some(Overview {
            current: totals(ix, lo, hi, &scope),
            previous: totals(ix, plo, phi, &scope),
        }))
    }

    fn epoch_day(d: time::Date) -> i64 {
        (d - time::Date::from_calendar_date(1970, time::Month::January, 1).unwrap()).whole_days()
    }

    pub fn timeseries(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Option<Vec<DayPoint>>> {
        let scope = ix.scope(f);
        let (lo, hi) = z.bounds(conn, period)?;
        let Some(pieces) = z.pieces(conn, &period.tz, lo, hi, DAY_US)? else {
            return Ok(None);
        };
        let visitors = ix.visitors_per(&pieces, &scope);
        let mut pays = ix.payments_per(&pieces, &scope, true);
        let mut out = Vec::new();
        let mut d = period.from;
        while d <= period.to {
            let label = epoch_day(d);
            let v = visitors.get(&label).copied().unwrap_or(0);
            let (payments, customers, revenue_usd, converted, matched_usd, mut by_channel) =
                pays.remove(&label).unwrap_or_default();
            by_channel.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            out.push(DayPoint {
                date: d,
                visitors: v,
                payments,
                customers,
                revenue_usd,
                converted,
                matched_revenue_usd: matched_usd,
                conversion: ratio(converted as f64, v),
                revenue_per_visitor: ratio(matched_usd, v),
                hour: None,
                by_channel,
            });
            d = d.next_day().unwrap();
        }
        Ok(Some(out))
    }

    pub fn hourly(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Option<Vec<DayPoint>>> {
        let scope = ix.scope(f);
        let day = Period {
            from: period.from,
            to: period.from,
            tz: period.tz.clone(),
        };
        let (lo, hi) = z.bounds(conn, &day)?;
        let Some(pieces) = z.pieces(conn, &period.tz, lo, hi, HOUR_US)? else {
            return Ok(None);
        };
        let hour = |label: i64| label.rem_euclid(24);
        let mut visitors = [0u64; 24];
        let mut marks = Marks::new(ix.visitors.len());
        // Labels are hours since the epoch; a repeated local hour shares one label.
        for s in ix.overlapping_ids(lo, hi) {
            if !scope.has(s) {
                continue;
            }
            let sess = &ix.sessions[s as usize];
            let start = pieces.partition_point(|p| p.1 <= sess.first);
            for p in &pieces[start..] {
                if p.0 > sess.last {
                    break;
                }
                if marks.first(sess.visitor, p.2) {
                    visitors[hour(p.2) as usize] += 1;
                }
            }
        }
        let mut per: [(u64, HashSet<u32>, f64, Vec<(Option<String>, f64)>); 24] =
            std::array::from_fn(|_| Default::default());
        for p in ix.pays(lo, hi, &scope) {
            let i = pieces.partition_point(|x| x.1 <= p.ts);
            let e = &mut per[hour(pieces[i].2) as usize];
            e.0 += 1;
            e.1.insert(p.customer);
            e.2 += p.net.unwrap_or(0.0);
            let channel = if p.credited {
                ix.strings.name(p.channel).map(str::to_string)
            } else {
                None
            };
            match e.3.iter_mut().find(|c| c.0 == channel) {
                Some(c) => c.1 += p.net.unwrap_or(0.0),
                None => e.3.push((channel, p.net.unwrap_or(0.0))),
            }
        }
        Ok(Some(
            per.into_iter()
                .enumerate()
                .map(|(h, (n, c, r, mut by_channel))| {
                    by_channel.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                    DayPoint {
                        date: period.from,
                        visitors: visitors[h],
                        payments: n,
                        customers: c.len() as u64,
                        revenue_usd: r,
                        converted: 0,
                        matched_revenue_usd: 0.0,
                        conversion: None,
                        revenue_per_visitor: None,
                        hour: Some(h as u8),
                        by_channel,
                    }
                })
                .collect(),
        ))
    }

    pub fn breakdown(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        d: Dimension,
        f: &Filter,
    ) -> anyhow::Result<Option<Vec<BreakdownRow>>> {
        let scope = ix.scope(f);
        let (lo, hi) = z.bounds(conn, period)?;
        let mut rows: Vec<RawRow> = Vec::new();
        match key_of(d) {
            Some(k) => {
                rows.extend(
                    ix.touch_visitors(lo, hi, &scope, k)
                        .into_iter()
                        .map(|(key, n)| (false, key, n, 0, 0.0, 0)),
                );
                rows.extend(ix.touch_payments(lo, hi, &scope, k));
            }
            None => {
                rows.extend(
                    ix.page_visitors(lo, hi, &scope)
                        .iter()
                        .map(|(&path, &n)| (false, Some(ix.path_name(path)), n, 0, 0.0, 0)),
                );
                rows.extend(ix.page_payments(lo, hi, &scope));
            }
        }
        Ok(Some(breakdown_rows(d, rows)))
    }

    pub fn last_pages(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Option<Vec<BreakdownRow>>> {
        let scope = ix.scope(f);
        let (lo, hi) = z.bounds(conn, period)?;
        let mut out: Vec<BreakdownRow> = ix
            .last_pages(lo, hi, &scope)
            .into_iter()
            .map(
                |(path, visitors, payments, revenue_usd, customers)| BreakdownRow {
                    key: Some(path.clone()),
                    label: path,
                    unattributed: false,
                    visitors,
                    payments,
                    customers,
                    revenue_usd,
                    revenue_per_visitor: ratio(revenue_usd, visitors),
                },
            )
            .collect();
        sort_rows(&mut out);
        Ok(Some(out))
    }

    pub fn channel_sources(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
        f: &Filter,
    ) -> anyhow::Result<Option<Vec<(String, Vec<String>)>>> {
        let scope = ix.scope(f);
        let (lo, hi) = z.bounds(conn, period)?;
        Ok(Some(top_sources(ix.channel_sources(lo, hi, &scope))))
    }

    pub fn setup(ix: &SiteIndex) -> crate::stats::Setup {
        crate::stats::Setup {
            first_event: ix.first_event().map(crate::stats::from_micros),
            has_goals: ix.has_goals(),
            payments: ix.payments.len() as u64,
            first_match: ix.first_match.clone(),
        }
    }

    pub fn payment_split(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
    ) -> anyhow::Result<Option<Vec<crate::stats::ProviderSplit>>> {
        let (lo, hi) = z.bounds(conn, period)?;
        let all = Scope(None, String::new());
        let mut out: Vec<crate::stats::ProviderSplit> = Vec::new();
        for p in ix.pays(lo, hi, &all) {
            let provider = ix.path_name(p.provider);
            let i = match out.iter().position(|x| x.provider == provider) {
                Some(i) => i,
                None => {
                    out.push(crate::stats::ProviderSplit {
                        provider,
                        first_payments: 0,
                        first_usd: 0.0,
                        renewals: 0,
                        renewal_usd: 0.0,
                        no_visitor: 0,
                        unseen: 0,
                        earlier_usd: 0.0,
                    });
                    out.len() - 1
                }
            };
            let x = &mut out[i];
            let net = p.net.unwrap_or(0.0);
            if p.renewal {
                x.renewals += 1;
                x.renewal_usd += net;
            } else {
                x.first_payments += 1;
                x.first_usd += net;
            }
            if !p.credited {
                if p.visitor.is_none() {
                    x.no_visitor += 1;
                } else {
                    x.unseen += 1;
                }
            }
            if p.touch_ts.is_some_and(|t| t < lo) {
                x.earlier_usd += net;
            }
        }
        out.sort_by(|a, b| a.provider.cmp(&b.provider));
        Ok(Some(out))
    }

    pub fn facets(
        ix: &SiteIndex,
        z: &Zones,
        conn: &Connection,
        period: &Period,
    ) -> anyhow::Result<Option<Facets>> {
        let (lo, hi) = z.bounds(conn, period)?;
        let (sources, goals) = ix.facets(lo, hi);
        Ok(Some(Facets { sources, goals }))
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde_json::Value;
    use time::{Date, Duration, OffsetDateTime};

    use super::*;
    use crate::events::{DuckStore, EventStore};
    use crate::stats::{self, Dimension, Period};
    use crate::visitors;

    const SITE: &str = "site";

    /// Equal JSON, with floats equal to 1e-6.
    fn same(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-6
            }
            (Value::Array(x), Value::Array(y)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
            }
            (Value::Object(x), Value::Object(y)) => {
                x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
            }
            _ => a == b,
        }
    }

    /// Rows keyed by label, since float sums can swap rows whose revenue ties.
    fn by_label(v: &Value) -> Option<Vec<(String, Value)>> {
        let rows = v.as_array()?;
        let mut keyed: Vec<(String, Value)> = rows
            .iter()
            .map(|r| {
                Some((
                    format!("{}{}", r.get("label")?, r.get("unattributed")?),
                    r.clone(),
                ))
            })
            .collect::<Option<_>>()?;
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        Some(keyed)
    }

    fn check<T: Serialize>(what: &str, index: Option<T>, sql: T) {
        let Some(index) = index else { return };
        let (a, b) = (
            serde_json::to_value(index).unwrap(),
            serde_json::to_value(sql).unwrap(),
        );
        if let (Some(x), Some(y)) = (by_label(&a), by_label(&b)) {
            assert_eq!(x.len(), y.len(), "{what}\nindex: {a}\nsql:   {b}");
            for ((k, p), (_, q)) in x.iter().zip(&y) {
                assert!(same(p, q), "{what} {k}\nindex: {p}\nsql:   {q}");
            }
            return;
        }
        assert!(same(&a, &b), "{what}\nindex: {a}\nsql:   {b}");
    }

    /// The demo seed, half read by the index from the snapshot and half appended through insert_events.
    fn seeded() -> (tempfile::TempDir, DuckStore, Date) {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        let now = OffsetDateTime::now_utc();
        let (mut events, payments, subscriptions) = crate::demo::seed(SITE, now);
        events.sort_by_key(|e| e.ts);
        let later = events.split_off(events.len() / 2);
        store.insert_events(&events).unwrap();
        store.upsert_payments(&payments).unwrap();
        store
            .replace_subscriptions(SITE, "stripe", &subscriptions)
            .unwrap();
        store
            .rebuild_attribution(SITE, "last_non_direct", 90)
            .unwrap();
        store.load_index(SITE);
        for chunk in later.chunks(997) {
            store.insert_events(chunk).unwrap();
        }
        (dir, store, now.date())
    }

    fn filters() -> Vec<Filter> {
        let one = |col: &'static str, v: Option<&str>| Filter {
            first: vec![(col, vec![v.map(str::to_string)])],
            viewed: None,
        };
        vec![
            Filter::default(),
            one("channel", Some("Organic search")),
            one("referrer_host", None),
            Filter {
                first: vec![
                    ("country", vec![Some("US".into()), Some("DE".into())]),
                    ("device", vec![Some("Desktop".into())]),
                ],
                viewed: None,
            },
            Filter {
                first: Vec::new(),
                viewed: Some("/pricing".into()),
            },
            one("channel", Some("no such channel")),
        ]
    }

    #[test]
    fn index_reads_match_sql() {
        let (_dir, store, today) = seeded();
        let ix = store.index_for_test(SITE);
        let ix = ix.read().unwrap();
        let conn = store.conn_for_test();
        let z = Zones::default();
        for tz in ["UTC", "America/New_York", "Asia/Kolkata"] {
            for (from, to) in [
                (today, today),
                (today - Duration::days(6), today),
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
                let all = filters();
                let few = if (to - from).whole_days() == 29 || from == to {
                    &all[..]
                } else {
                    &all[..1]
                };
                for f in few {
                    let f = f.clone();
                    let at = format!("{tz} {from}..{to} {f:?}");
                    check(
                        &format!("overview {at}"),
                        read::overview(&ix, &z, &conn, &p, &f).unwrap(),
                        stats::overview(&conn, SITE, &p, &f).unwrap(),
                    );
                    check(
                        &format!("timeseries {at}"),
                        read::timeseries(&ix, &z, &conn, &p, &f).unwrap(),
                        stats::timeseries(&conn, SITE, &p, &f).unwrap(),
                    );
                    check(
                        &format!("hourly {at}"),
                        read::hourly(&ix, &z, &conn, &p, &f).unwrap(),
                        stats::hourly(&conn, SITE, &p, &f).unwrap(),
                    );
                    for d in [
                        Dimension::Channel,
                        Dimension::Referrer,
                        Dimension::LandingPage,
                        Dimension::Country,
                        Dimension::Campaign,
                        Dimension::Keyword,
                        Dimension::ChannelSource,
                        Dimension::Page,
                        Dimension::Browser,
                        Dimension::Os,
                        Dimension::Device,
                        Dimension::PageSource,
                        Dimension::CountryCity,
                        Dimension::DeviceOs,
                        Dimension::Language,
                    ] {
                        check(
                            &format!("breakdown {d:?} {at}"),
                            read::breakdown(&ix, &z, &conn, &p, d, &f).unwrap(),
                            stats::breakdown(&conn, SITE, &p, d, &f).unwrap(),
                        );
                    }
                    check(
                        &format!("last_pages {at}"),
                        read::last_pages(&ix, &z, &conn, &p, &f).unwrap(),
                        stats::last_pages(&conn, SITE, &p, &f).unwrap(),
                    );
                    check(
                        &format!("channel_sources {at}"),
                        read::channel_sources(&ix, &z, &conn, &p, &f).unwrap(),
                        stats::channel_sources(&conn, SITE, &p, &f).unwrap(),
                    );
                }
                check(
                    &format!("payment_split {tz} {from}..{to}"),
                    read::payment_split(&ix, &z, &conn, &p).unwrap(),
                    stats::payment_split(&conn, SITE, &p).unwrap(),
                );
                check(
                    &format!("facets {tz} {from}..{to}"),
                    read::facets(&ix, &z, &conn, &p).unwrap(),
                    visitors::facets(&conn, SITE, &p).unwrap(),
                );
            }
        }
    }

    #[test]
    fn setup_matches_sql() {
        let (_dir, store, _) = seeded();
        let ix = store.index_for_test(SITE);
        let conn = store.conn_for_test();
        assert_eq!(
            read::setup(&ix.read().unwrap()),
            stats::setup(&conn, SITE).unwrap()
        );
    }

    #[test]
    fn store_reads_use_the_index_and_follow_new_events() {
        let (_dir, store, today) = seeded();
        let p = Period {
            from: today - Duration::days(29),
            to: today,
            tz: "UTC".into(),
        };
        let before = store.overview("site", &p).unwrap();
        let mut e = crate::demo::seed(SITE, OffsetDateTime::now_utc())
            .0
            .remove(0);
        e.ts = OffsetDateTime::now_utc();
        e.visitor_id = "brand-new-visitor".into();
        e.session_id = "brand-new-session".into();
        e.kind = "pageview".into();
        store.insert_events(&[e]).unwrap();
        let after = store.overview("site", &p).unwrap();
        assert_eq!(after.current.visitors, before.current.visitors + 1);
        let conn = store.conn_for_test();
        check(
            "overview",
            Some(after),
            stats::overview(&conn, SITE, &p, &Filter::default()).unwrap(),
        );
    }
}
