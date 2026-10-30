use duckdb::{Connection, ToSql, named_params};
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::stats::{
    ATTRIBUTION_JOIN, BOUNDS, FIRST_TOUCH_JOIN, NET_USD, PAYMENT_COLUMNS, PaymentRow, Period,
    from_micros, payment_row, source,
};

/// Idle gap that ends a visit, as at ingest.
const IDLE: Duration = Duration::minutes(30);

/// Non-failed payments per visitor: their own plus every payment of a customer they paid as.
pub(crate) const OWNED: &str = "owned AS (
    SELECT visitor_id, provider, provider_payment_id FROM payments
    WHERE site_id = $site AND visitor_id IS NOT NULL AND status <> 'failed'
    UNION
    SELECT c.visitor_id, p.provider, p.provider_payment_id
    FROM payments p
    JOIN (SELECT DISTINCT visitor_id, provider, customer_id FROM payments
          WHERE site_id = $site AND visitor_id IS NOT NULL AND customer_id IS NOT NULL) c
        ON c.provider = p.provider AND c.customer_id = p.customer_id
    WHERE p.site_id = $site AND p.status <> 'failed'
)";

#[derive(Debug, Clone, Default)]
pub struct VisitorFilter {
    pub channel: Option<String>,
    /// Referrer hosts of the first visit, e.g. every host of one source.
    pub hosts: Option<Vec<String>>,
    pub goal: Option<String>,
    /// Some(true) clicked out to another site, Some(false) left from a page.
    pub clicked_out: Option<bool>,
    pub paid: Option<bool>,
    /// Lowercase text found in the landing or exit page, the first referrer or the channel.
    pub search: Option<String>,
    /// "visits", "pages" or "revenue", most first; otherwise most recently seen first.
    pub sort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VisitorRow {
    pub visitor_id: String,
    /// Channel and referrer of the visitor's first ever pageview.
    pub channel: String,
    pub referrer_host: Option<String>,
    pub source: String,
    pub visits: u64,
    pub pageviews: u64,
    pub last_seen: OffsetDateTime,
    /// First and last page seen in the range.
    pub landing_page: Option<String>,
    pub exit_page: Option<String>,
    /// Outbound link clicked after that page, as host and path.
    pub exit_link: Option<String>,
    pub goals: Vec<String>,
    /// Viewed a /pricing page, or clicked out to a payment checkout, in the range.
    pub pricing: bool,
    pub checkout: bool,
    /// Lifetime payments and net USD.
    pub payments: u64,
    pub revenue_usd: f64,
    /// Key of the generated name: the first payment's payer, as on payment rows, else the visitor.
    pub who: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VisitorPage {
    pub total: u64,
    pub rows: Vec<VisitorRow>,
}

/// What the filters can offer for a range.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Facets {
    /// (channel, referrer host, visitors) of first visits.
    pub sources: Vec<(String, Option<String>, u64)>,
    /// (goal, visitors), most visitors first.
    pub goals: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Step {
    Page {
        #[serde(with = "time::serde::rfc3339")]
        ts: OffsetDateTime,
        path: String,
        /// Until the next pageview, else until the page was hidden or left; None when unknown.
        secs: Option<i64>,
    },
    Goal {
        #[serde(with = "time::serde::rfc3339")]
        ts: OffsetDateTime,
        name: String,
    },
    /// A click out to another site in the middle of the visit.
    Left {
        #[serde(with = "time::serde::rfc3339")]
        ts: OffsetDateTime,
        domain: String,
    },
    /// The visitor came back to the visit after leaving the site.
    Back {
        #[serde(with = "time::serde::rfc3339")]
        ts: OffsetDateTime,
    },
    /// How the visit ended: the last page and the link clicked out, if any.
    Exit {
        #[serde(with = "time::serde::rfc3339")]
        ts: OffsetDateTime,
        page: String,
        link: Option<String>,
    },
    Payment(Box<PaymentRow>),
}

impl Step {
    pub fn ts(&self) -> OffsetDateTime {
        match self {
            Step::Page { ts, .. }
            | Step::Goal { ts, .. }
            | Step::Left { ts, .. }
            | Step::Back { ts }
            | Step::Exit { ts, .. } => *ts,
            Step::Payment(p) => p.ts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Visit {
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
    pub channel: String,
    pub referrer_host: Option<String>,
    pub source: String,
    pub campaign: Option<String>,
    pub pageviews: u64,
    pub steps: Vec<Step>,
}

impl Visit {
    /// A plain verdict on the visit (looks human, unusual or suspect) with the reasons for it.
    pub fn verdict(&self) -> (&'static str, Vec<String>) {
        let read: i64 = self
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::Page { secs, .. } => *secs,
                _ => None,
            })
            .sum();
        let acted = self.steps.iter().any(|s| {
            matches!(
                s,
                Step::Goal { .. } | Step::Payment(_) | Step::Left { .. } | Step::Back { .. }
            )
        });
        let span = (self.end - self.start).whole_seconds();
        let mut good = Vec::new();
        if self.pageviews > 1 {
            good.push(format!("viewed {} pages", self.pageviews));
        }
        if read >= 10 {
            good.push(format!(
                "stayed {} on its pages",
                crate::visitor_pages::stay(read)
            ));
        }
        if acted {
            good.push("clicked, reached a goal or paid".to_string());
        }
        if !good.is_empty() {
            return ("Looks human", good);
        }
        if self.pageviews <= 1 && span < 2 && read < 2 {
            return (
                "Suspect",
                vec!["one page, gone within 2 seconds with no reading".into()],
            );
        }
        (
            "Unusual",
            vec!["one page and under 10 seconds on it, no clicks".into()],
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Entry {
    Visit(Visit),
    /// A payment outside any visit, such as a renewal.
    Payment(Box<PaymentRow>),
}

impl Entry {
    fn ts(&self) -> OffsetDateTime {
        match self {
            Entry::Visit(v) => v.start,
            Entry::Payment(p) => p.ts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Timeline {
    pub visitor_id: String,
    /// Oldest first.
    pub entries: Vec<Entry>,
    /// Every payment, oldest first.
    pub payments: Vec<PaymentRow>,
    /// From the last pageview.
    pub country: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub device: Option<String>,
}

impl Timeline {
    pub fn visits(&self) -> impl Iterator<Item = &Visit> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Visit(v) => Some(v),
            Entry::Payment(_) => None,
        })
    }
}

/// True when an exit link, stored as host and path, opens a payment checkout.
pub(crate) fn checkout_link(link: &str) -> bool {
    let (host, path) = link.split_once('/').unwrap_or((link, ""));
    crate::ingest::checkout(host, &format!("/{path}"))
}

pub(crate) fn visitor_list(
    conn: &Connection,
    site_id: &str,
    period: &Period,
    filter: &VisitorFilter,
    limit: u32,
    offset: u32,
) -> anyhow::Result<VisitorPage> {
    let mut wheres = vec!["true"];
    if filter.channel.is_some() {
        wheres.push("f.channel = $channel");
    }
    if filter.hosts.is_some() {
        wheres.push("list_contains(string_split($hosts, ' '), f.referrer_host)");
    }
    if filter.goal.is_some() {
        wheres.push("list_contains(per.goals, $goal)");
    }
    match filter.clicked_out {
        Some(true) => wheres.push("o.exit_link IS NOT NULL"),
        Some(false) => wheres.push("o.exit_link IS NULL"),
        None => {}
    }
    match filter.paid {
        Some(true) => wheres.push("coalesce(pd.n, 0) > 0"),
        Some(false) => wheres.push("coalesce(pd.n, 0) = 0"),
        None => {}
    }
    if filter.search.is_some() {
        wheres.push(
            "(contains(lower(coalesce(per.landing_page, '')), $q) OR contains(lower(coalesce(per.exit_page, '')), $q)
             OR contains(lower(coalesce(f.referrer_host, '')), $q) OR contains(lower(coalesce(f.channel, 'Direct')), $q))",
        );
    }
    let order = match filter.sort.as_deref() {
        Some("visits") => "visits DESC, ",
        Some("pages") => "pageviews DESC, ",
        Some("revenue") => "round(revenue, 2) DESC, ",
        _ => "",
    };
    let with = format!(
        "WITH {BOUNDS},
        ev AS (
            SELECT e.* FROM events e, bounds WHERE e.site_id = $site AND e.ts >= lo AND e.ts < hi
        ),
        per AS (
            SELECT visitor_id, count(DISTINCT session_id) AS visits,
                count(*) FILTER (WHERE kind = 'pageview') AS pageviews,
                max(ts) AS last_seen,
                max(ts) FILTER (WHERE kind = 'pageview') AS last_pv,
                arg_max(path, ts) FILTER (WHERE kind = 'pageview') AS exit_page,
                first(path ORDER BY ts, path) FILTER (WHERE kind = 'pageview') AS landing_page,
                coalesce(bool_or(kind = 'pageview' AND starts_with(path, '/pricing')), false) AS pricing,
                to_json(coalesce(list(name) FILTER (WHERE kind = 'exit'), []))::VARCHAR AS exits,
                list_sort(list_distinct(list(name) FILTER (WHERE kind = 'goal'))) AS goals
            FROM ev GROUP BY visitor_id
        ),
        outs AS (
            SELECT e.visitor_id, arg_max(e.name, e.ts) AS exit_link
            FROM ev e JOIN per ON per.visitor_id = e.visitor_id
            WHERE e.kind = 'exit' AND (per.last_pv IS NULL OR e.ts >= per.last_pv)
            GROUP BY 1
        ),
        firsts AS (
            SELECT visitor_id, channel, referrer_host FROM first_touches
            WHERE site_id = $site AND visitor_id IN (SELECT visitor_id FROM per)
        ),
        {OWNED},
        pd AS (
            SELECT o.visitor_id, count(*) AS n, coalesce(sum({NET_USD}), 0) AS revenue,
                arg_min(coalesce('c:' || p.provider || ':' || p.customer_id, 'v:' || p.visitor_id), p.ts) AS payer
            FROM owned o
            JOIN payments p ON p.site_id = $site AND p.provider = o.provider
                AND p.provider_payment_id = o.provider_payment_id
            WHERE o.visitor_id IN (SELECT visitor_id FROM per)
            GROUP BY 1
        ),
        vis AS (
            SELECT per.visitor_id, f.channel, f.referrer_host, per.visits, per.pageviews,
                epoch_us(per.last_seen) AS last_us, per.exit_page, o.exit_link,
                to_json(coalesce(per.goals, []))::VARCHAR AS goals,
                coalesce(pd.n, 0) AS n, coalesce(pd.revenue, 0) AS revenue,
                coalesce(pd.payer, 'v:' || per.visitor_id) AS who,
                per.landing_page, per.pricing, per.exits
            FROM per
            LEFT JOIN firsts f USING (visitor_id)
            LEFT JOIN outs o USING (visitor_id)
            LEFT JOIN pd USING (visitor_id)
            WHERE {}
        )",
        wheres.join(" AND ")
    );
    let (from, to) = (period.from.to_string(), period.to.to_string());
    let hosts = filter.hosts.as_ref().map(|h| h.join(" "));
    let (limit, offset) = (limit as i64, offset as i64);
    let mut params: Vec<(&str, &dyn ToSql)> = vec![
        ("site", &site_id),
        ("tz", &period.tz),
        ("from", &from),
        ("to", &to),
    ];
    if let Some(c) = &filter.channel {
        params.push(("channel", c));
    }
    if let Some(h) = &hosts {
        params.push(("hosts", h));
    }
    if let Some(g) = &filter.goal {
        params.push(("goal", g));
    }
    if let Some(q) = &filter.search {
        params.push(("q", q));
    }
    let total: i64 = conn.query_row(
        &format!("{with} SELECT count(*) FROM vis"),
        params.as_slice(),
        |r| r.get(0),
    )?;
    params.push(("limit", &limit));
    params.push(("offset", &offset));
    let mut stmt = conn.prepare(&format!(
        "{with} SELECT * FROM vis ORDER BY {order}last_us DESC, visitor_id LIMIT $limit OFFSET $offset"
    ))?;
    let rows = stmt.query_map(params.as_slice(), |r| {
        let channel: Option<String> = r.get(1)?;
        let channel = channel.unwrap_or_else(|| "Direct".into());
        let referrer_host: Option<String> = r.get(2)?;
        let goals: String = r.get(8)?;
        let exits: String = r.get(14)?;
        let exits: Vec<String> = serde_json::from_str(&exits).unwrap_or_default();
        Ok(VisitorRow {
            visitor_id: r.get(0)?,
            source: source(referrer_host.as_deref(), &channel),
            channel,
            referrer_host,
            visits: r.get::<_, i64>(3)? as u64,
            pageviews: r.get::<_, i64>(4)? as u64,
            last_seen: from_micros(r.get(5)?),
            landing_page: r.get(12)?,
            exit_page: r.get(6)?,
            exit_link: r.get(7)?,
            goals: serde_json::from_str(&goals).unwrap_or_default(),
            pricing: r.get(13)?,
            checkout: exits.iter().any(|l| checkout_link(l)),
            payments: r.get::<_, i64>(9)? as u64,
            revenue_usd: r.get(10)?,
            who: r.get(11)?,
        })
    })?;
    Ok(VisitorPage {
        total: total as u64,
        rows: rows.collect::<Result<_, _>>()?,
    })
}

pub(crate) fn facets(conn: &Connection, site_id: &str, period: &Period) -> anyhow::Result<Facets> {
    let params = named_params! {
        "site": site_id,
        "tz": period.tz,
        "from": period.from.to_string(),
        "to": period.to.to_string(),
    };
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS},
        seen AS (
            SELECT DISTINCT visitor_id FROM events, bounds
            WHERE site_id = $site AND ts >= lo AND ts < hi
        ),
        firsts AS (
            SELECT visitor_id, channel, referrer_host FROM first_touches
            WHERE site_id = $site AND visitor_id IN (SELECT visitor_id FROM seen)
        )
        SELECT channel, referrer_host, count(*) AS n FROM firsts
        GROUP BY ALL ORDER BY n DESC, channel, referrer_host"
    ))?;
    let sources = stmt
        .query_map(params, |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64))
        })?
        .collect::<Result<_, _>>()?;
    let mut stmt = conn.prepare(&format!(
        "WITH {BOUNDS}
        SELECT name, count(DISTINCT visitor_id) AS n FROM events, bounds
        WHERE site_id = $site AND kind = 'goal' AND ts >= lo AND ts < hi
        GROUP BY name ORDER BY n DESC, name"
    ))?;
    let goals = stmt
        .query_map(params, |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
        .collect::<Result<_, _>>()?;
    Ok(Facets { sources, goals })
}

pub(crate) struct Ev {
    pub ts: OffsetDateTime,
    pub session: String,
    pub kind: String,
    pub name: Option<String>,
    pub path: String,
    pub channel: String,
    pub referrer_host: Option<String>,
    pub campaign: Option<String>,
}

/// Steps of one visit's events, oldest first, ending with how the visit ended.
fn steps(events: &[Ev]) -> Vec<Step> {
    let pages: Vec<usize> = (0..events.len())
        .filter(|&i| events[i].kind == "pageview")
        .collect();
    let mut out = Vec::new();
    for (n, &i) in pages.iter().enumerate() {
        let e = &events[i];
        let end = match pages.get(n + 1) {
            Some(&j) => events[i..j]
                .iter()
                .find(|x| x.kind == "leave")
                .map_or(Some(events[j].ts), |x| Some(x.ts)),
            None => events[i..]
                .iter()
                .filter(|x| matches!(x.kind.as_str(), "leave" | "exit"))
                .map(|x| x.ts)
                .max(),
        };
        out.push(Step::Page {
            ts: e.ts,
            path: e.path.clone(),
            secs: end.map(|t| (t - e.ts).whole_seconds()),
        });
    }
    let last_page = pages.last().copied().unwrap_or(0);
    for (i, e) in events.iter().enumerate() {
        match e.kind.as_str() {
            "goal" => out.push(Step::Goal {
                ts: e.ts,
                name: e.name.clone().unwrap_or_default(),
            }),
            "exit" if i < last_page => out.push(Step::Left {
                ts: e.ts,
                domain: e
                    .name
                    .as_deref()
                    .and_then(|l| l.split('/').next())
                    .unwrap_or_default()
                    .to_string(),
            }),
            "back" => out.push(Step::Back { ts: e.ts }),
            _ => {}
        }
    }
    if let Some(&last) = pages.last() {
        let after = &events[last..];
        let link = after
            .iter()
            .rev()
            .find(|e| e.kind == "exit")
            .and_then(|e| e.name.clone());
        out.push(Step::Exit {
            ts: after.iter().map(|e| e.ts).max().unwrap_or(events[last].ts),
            page: events[last].path.clone(),
            link,
        });
    }
    out.sort_by_key(|s| s.ts());
    out
}

pub(crate) fn timeline(
    conn: &Connection,
    site_id: &str,
    visitor_id: &str,
) -> anyhow::Result<Option<Timeline>> {
    let params = named_params! { "site": site_id, "visitor": visitor_id };
    let mut stmt = conn.prepare(
        "SELECT epoch_us(ts), kind, name, path, channel, referrer_host, utm_campaign, session_id
        FROM events WHERE site_id = $site AND visitor_id = $visitor
        ORDER BY ts, CASE kind WHEN 'pageview' THEN 0 WHEN 'goal' THEN 1 WHEN 'exit' THEN 2 ELSE 3 END, path",
    )?;
    let events = stmt
        .query_map(params, |r| {
            Ok(Ev {
                ts: from_micros(r.get(0)?),
                kind: r.get(1)?,
                name: r.get(2)?,
                path: r.get(3)?,
                channel: r.get(4)?,
                referrer_host: r.get(5)?,
                campaign: r.get(6)?,
                session: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare(&format!(
        "WITH {OWNED}
        SELECT {PAYMENT_COLUMNS} FROM payments p {ATTRIBUTION_JOIN} {FIRST_TOUCH_JOIN}
        WHERE p.site_id = $site AND (p.provider, p.provider_payment_id) IN
            (SELECT provider, provider_payment_id FROM owned WHERE visitor_id = $visitor)
        ORDER BY p.ts, p.provider, p.provider_payment_id"
    ))?;
    let payments: Vec<PaymentRow> = stmt
        .query_map(params, payment_row)?
        .collect::<Result<_, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT country, browser, device, os FROM events
        WHERE site_id = $site AND visitor_id = $visitor AND kind = 'pageview'
        ORDER BY ts DESC LIMIT 1",
    )?;
    let last = stmt
        .query_map(params, |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .next()
        .transpose()?;
    Ok(assemble(visitor_id, events, payments, last))
}

/// Country, browser, device and OS of the last pageview.
pub(crate) type LastSeen = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// A visitor's events (ordered as the timeline query orders them) and payments as visits.
pub(crate) fn assemble(
    visitor_id: &str,
    events: Vec<Ev>,
    payments: Vec<PaymentRow>,
    last: Option<LastSeen>,
) -> Option<Timeline> {
    if events.is_empty() && payments.is_empty() {
        return None;
    }
    let mut groups: Vec<&[Ev]> = Vec::new();
    let mut start = 0;
    for i in 1..=events.len() {
        if i == events.len() || events[i].session != events[i - 1].session {
            groups.push(&events[start..i]);
            start = i;
        }
    }
    let mut entries: Vec<Entry> = groups
        .into_iter()
        .filter(|g| !g.is_empty())
        .map(|g| {
            let first = g.iter().find(|e| e.kind == "pageview").unwrap_or(&g[0]);
            Visit {
                start: g[0].ts,
                end: g[g.len() - 1].ts,
                source: source(first.referrer_host.as_deref(), &first.channel),
                channel: first.channel.clone(),
                referrer_host: first.referrer_host.clone(),
                campaign: first.campaign.clone(),
                pageviews: g.iter().filter(|e| e.kind == "pageview").count() as u64,
                steps: steps(g),
            }
        })
        .map(Entry::Visit)
        .collect();
    for p in &payments {
        let during = entries.iter_mut().find_map(|e| match e {
            Entry::Visit(v) if v.start <= p.ts && p.ts <= v.end + IDLE => Some(v),
            _ => None,
        });
        match during {
            Some(v) => {
                v.steps.push(Step::Payment(Box::new(p.clone())));
                v.steps.sort_by_key(|s| s.ts());
            }
            None => entries.push(Entry::Payment(Box::new(p.clone()))),
        }
    }
    entries.sort_by_key(|e| e.ts());
    let (country, browser, device, os) = last.unwrap_or_default();
    Some(Timeline {
        visitor_id: visitor_id.to_string(),
        entries,
        payments,
        country,
        browser,
        os,
        device,
    })
}

#[cfg(test)]
mod tests {
    use time::macros::{date, datetime};

    use super::*;
    use crate::events::{DuckStore, Event, EventStore, Payment};

    const T: OffsetDateTime = datetime!(2026-10-01 12:00 UTC);

    #[test]
    fn each_visit_gets_a_verdict_with_reasons() {
        let page = |secs| Step::Page {
            ts: T,
            path: "/".into(),
            secs,
        };
        let visit = |pageviews, secs: u64, steps| Visit {
            start: T,
            end: T + time::Duration::seconds(secs as i64),
            channel: "Direct".into(),
            referrer_host: None,
            source: "Direct".into(),
            campaign: None,
            pageviews,
            steps,
        };
        let (v, why) = visit(3, 120, vec![page(Some(40)), page(Some(60))]).verdict();
        assert_eq!(v, "Looks human");
        assert_eq!(why, ["viewed 3 pages", "stayed 1m 40s on its pages"]);
        assert_eq!(visit(1, 0, vec![page(None)]).verdict().0, "Suspect");
        assert_eq!(visit(1, 5, vec![page(Some(5))]).verdict().0, "Unusual");
    }

    fn store() -> (tempfile::TempDir, DuckStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        (dir, store)
    }

    fn ev(visitor: &str, session: &str, mins: i64, kind: &str, path: &str) -> Event {
        let (name, referrer_host, channel) = match kind {
            "exit" => (Some(path.to_string()), None, "Direct"),
            "goal" => (Some(path.to_string()), None, "Direct"),
            _ if session == "s1" => (None, Some("news.ycombinator.com".into()), "Social"),
            _ => (None, None, "Direct"),
        };
        Event {
            site_id: "s".into(),
            ts: T + Duration::minutes(mins),
            visitor_id: visitor.into(),
            session_id: session.into(),
            kind: kind.into(),
            name,
            path: if matches!(kind, "exit" | "goal") {
                "/pricing".into()
            } else {
                path.into()
            },
            referrer_host,
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            channel: channel.into(),
            country: Some("DE".into()),
            device: "Desktop".into(),
            browser: "Firefox".into(),
            os: "macOS".into(),
            region: None,
            city: None,
            host: None,
            language: None,
            direct: None,
        }
    }

    fn pay(id: &str, mins: i64, visitor: Option<&str>, renewal: bool) -> Payment {
        Payment {
            site_id: "s".into(),
            provider: "stripe".into(),
            provider_payment_id: id.into(),
            ts: T + Duration::minutes(mins),
            amount: 2900,
            currency: "usd".into(),
            amount_usd: Some(29.0),
            refunded: 0,
            status: "paid".into(),
            visitor_id: visitor.map(Into::into),
            customer_id: Some("cus_1".into()),
            is_renewal: renewal,
        }
    }

    fn period() -> Period {
        Period {
            from: date!(2026 - 09 - 01),
            to: date!(2026 - 12 - 31),
            tz: "UTC".into(),
        }
    }

    /// v1 lands from HN, reads two pages, clicks out to GitHub; v2 leaves from /docs.
    fn seed(store: &DuckStore) {
        store
            .insert_events(&[
                ev("v1", "s1", 0, "pageview", "/"),
                ev("v1", "s1", 2, "pageview", "/pricing"),
                ev("v1", "s1", 3, "goal", "signup"),
                ev("v1", "s1", 5, "exit", "github.com/virling/virling"),
                ev("v2", "s9", 10, "pageview", "/docs"),
                ev("v2", "s9", 14, "leave", "/docs"),
            ])
            .unwrap();
    }

    fn list(store: &DuckStore, filter: VisitorFilter) -> Vec<VisitorRow> {
        store
            .visitor_list("s", &period(), &filter, 50, 0)
            .unwrap()
            .rows
    }

    #[test]
    fn landing_intent_search_and_sort() {
        let (_dir, store) = store();
        seed(&store);
        store
            .insert_events(&[
                ev("v3", "s3", 20, "pageview", "/blog/x"),
                ev("v3", "s3", 21, "pageview", "/blog/y"),
                ev("v3", "s3", 22, "exit", "buy.stripe.com/abc"),
            ])
            .unwrap();
        let rows = list(&store, VisitorFilter::default());
        let v1 = rows.iter().find(|r| r.visitor_id == "v1").unwrap();
        assert_eq!(v1.landing_page.as_deref(), Some("/"));
        assert!(v1.pricing && !v1.checkout);
        let v3 = rows.iter().find(|r| r.visitor_id == "v3").unwrap();
        assert!(v3.checkout && !v3.pricing);
        let find = |q: &str| {
            list(
                &store,
                VisitorFilter {
                    search: Some(q.into()),
                    ..Default::default()
                },
            )
            .into_iter()
            .map(|r| r.visitor_id)
            .collect::<Vec<_>>()
        };
        assert_eq!(find("/docs"), ["v2"]);
        assert_eq!(find("ycombinator"), ["v1"]);
        let by_pages = list(
            &store,
            VisitorFilter {
                sort: Some("pages".into()),
                ..Default::default()
            },
        );
        assert_eq!(by_pages[0].pageviews, 2);
        assert_eq!(by_pages[2].visitor_id, "v2");
    }

    #[test]
    fn exit_page_and_link_per_visitor() {
        let (_dir, store) = store();
        seed(&store);
        let rows = list(&store, VisitorFilter::default());
        assert_eq!(rows.len(), 2);
        let (v2, v1) = (&rows[0], &rows[1]);
        assert_eq!(v2.visitor_id, "v2", "latest seen first");
        assert_eq!(v2.exit_page.as_deref(), Some("/docs"));
        assert_eq!(v2.exit_link, None);
        assert_eq!(v1.exit_page.as_deref(), Some("/pricing"));
        assert_eq!(v1.exit_link.as_deref(), Some("github.com/virling/virling"));
        assert_eq!((v1.visits, v1.pageviews), (1, 2));
        assert_eq!(
            (v1.source.as_str(), v1.goals.clone()),
            ("Hacker News", vec!["signup".to_string()])
        );
        let out = list(
            &store,
            VisitorFilter {
                clicked_out: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].visitor_id, "v1");
        let left = list(
            &store,
            VisitorFilter {
                clicked_out: Some(false),
                ..Default::default()
            },
        );
        assert_eq!(left[0].visitor_id, "v2");
        let hn = list(
            &store,
            VisitorFilter {
                hosts: Some(vec!["news.ycombinator.com".into()]),
                ..Default::default()
            },
        );
        assert_eq!(hn.len(), 1);
        let direct = list(
            &store,
            VisitorFilter {
                channel: Some("Direct".into()),
                goal: None,
                ..Default::default()
            },
        );
        assert_eq!(direct[0].visitor_id, "v2");
        let goal = list(
            &store,
            VisitorFilter {
                goal: Some("signup".into()),
                ..Default::default()
            },
        );
        assert_eq!(goal[0].visitor_id, "v1");
    }

    #[test]
    fn a_later_page_cancels_an_earlier_exit_click() {
        let (_dir, store) = store();
        store
            .insert_events(&[
                ev("v1", "s1", 0, "pageview", "/"),
                ev("v1", "s1", 1, "exit", "github.com/virling"),
                ev("v1", "s1", 2, "pageview", "/blog"),
            ])
            .unwrap();
        let rows = list(&store, VisitorFilter::default());
        assert_eq!(rows[0].exit_page.as_deref(), Some("/blog"));
        assert_eq!(rows[0].exit_link, None);
    }

    #[test]
    fn paid_filter_and_lifetime_revenue_include_renewals() {
        let (_dir, store) = store();
        seed(&store);
        store
            .upsert_payments(&[
                pay("p1", 20, Some("v1"), false),
                pay("p2", 60 * 24 * 30, None, true),
            ])
            .unwrap();
        let paid = list(
            &store,
            VisitorFilter {
                paid: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(paid.len(), 1);
        assert_eq!((paid[0].payments, paid[0].revenue_usd), (2, 58.0));
        assert_eq!(paid[0].who, "c:stripe:cus_1", "named like the payment rows");
        let unpaid = list(
            &store,
            VisitorFilter {
                paid: Some(false),
                ..Default::default()
            },
        );
        assert_eq!(
            (unpaid[0].visitor_id.as_str(), unpaid[0].who.as_str()),
            ("v2", "v:v2")
        );
    }

    #[test]
    fn time_on_page_from_next_page_or_leave() {
        let (_dir, store) = store();
        seed(&store);
        let t = store.timeline("s", "v2").unwrap().unwrap();
        let v = t.visits().next().unwrap();
        assert!(matches!(&v.steps[0], Step::Page { path, secs: Some(240), .. } if path == "/docs"));
        assert!(
            matches!(&v.steps[1], Step::Exit { page, link: None, .. } if page == "/docs"),
            "left from /docs"
        );
        let t = store.timeline("s", "v1").unwrap().unwrap();
        let v = t.visits().next().unwrap();
        let secs: Vec<Option<i64>> = v
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::Page { secs, .. } => Some(*secs),
                _ => None,
            })
            .collect();
        assert_eq!(secs, vec![Some(120), Some(180)]);
        assert!(
            matches!(v.steps.last(), Some(Step::Exit { link: Some(l), .. }) if l == "github.com/virling/virling")
        );
        assert!(
            v.steps
                .iter()
                .any(|s| matches!(s, Step::Goal { name, .. } if name == "signup"))
        );
        assert_eq!(v.source, "Hacker News");
    }

    #[test]
    fn page_without_leave_has_unknown_time() {
        let (_dir, store) = store();
        store
            .insert_events(&[ev("v1", "s1", 0, "pageview", "/")])
            .unwrap();
        let t = store.timeline("s", "v1").unwrap().unwrap();
        let v = t.visits().next().unwrap();
        assert!(matches!(v.steps[0], Step::Page { secs: None, .. }));
    }

    #[test]
    fn visits_split_after_thirty_idle_minutes() {
        let (_dir, store) = store();
        store
            .insert_events(&[
                ev("v1", "s1", 0, "pageview", "/"),
                ev("v1", "s1", 29, "pageview", "/a"),
                ev("v1", "s2", 60, "pageview", "/b"),
                ev("v1", "s2", 61, "leave", "/b"),
            ])
            .unwrap();
        let t = store.timeline("s", "v1").unwrap().unwrap();
        let visits: Vec<_> = t.visits().collect();
        assert_eq!(visits.len(), 2);
        assert_eq!(visits[0].pageviews, 2);
        assert_eq!(visits[1].source, "Direct");
        assert!(
            matches!(&visits[0].steps[1], Step::Page { secs: None, .. }),
            "the next visit does not count as time on page"
        );
        assert!(store.timeline("s", "nobody").unwrap().is_none());
    }

    #[test]
    fn payments_sit_in_place_on_the_timeline() {
        let (_dir, store) = store();
        seed(&store);
        store
            .insert_events(&[ev("v1", "s3", 60 * 24 * 3, "pageview", "/changelog")])
            .unwrap();
        let mut refunded = pay("p1", 8, Some("v1"), false);
        refunded.refunded = 2900;
        refunded.status = "refunded".into();
        let mut other = pay("other", 9, Some("v2"), false);
        other.customer_id = Some("cus_2".into());
        store
            .upsert_payments(&[refunded, pay("p2", 60 * 24 * 30, None, true), other])
            .unwrap();
        let t = store.timeline("s", "v1").unwrap().unwrap();
        assert_eq!(t.payments.len(), 2);
        let kinds: Vec<&str> = t
            .entries
            .iter()
            .map(|e| match e {
                Entry::Visit(_) => "visit",
                Entry::Payment(_) => "payment",
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["visit", "visit", "payment"],
            "renewal after the later visit"
        );
        let Entry::Visit(first) = &t.entries[0] else {
            unreachable!()
        };
        let order: Vec<&str> = first
            .steps
            .iter()
            .map(|s| match s {
                Step::Page { .. } => "page",
                Step::Goal { .. } => "goal",
                Step::Left { .. } => "left",
                Step::Back { .. } => "back",
                Step::Exit { .. } => "exit",
                Step::Payment(_) => "payment",
            })
            .collect();
        assert_eq!(order, vec!["page", "page", "goal", "exit", "payment"]);
        assert!(matches!(&first.steps[4], Step::Payment(p) if p.refunded == 2900));
        let Entry::Payment(renewal) = &t.entries[2] else {
            unreachable!()
        };
        assert!(renewal.is_renewal);
        assert_eq!(
            renewal.visitor.as_deref(),
            Some("v1"),
            "linked back by customer"
        );
    }
}
