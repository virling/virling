//! Pure first-party analytics for virling.com: event validation, conversion kinds and stats SQL.
use serde::{Deserialize, Serialize};
use worker::Url;

use crate::channel;
use crate::ua::{browser, device, is_spam_referrer};

pub const SITE: &str = "virling.com";
pub const MAX_BATCH: usize = 50;
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;
pub const ATTRIBUTION_MS: i64 = 90 * DAY_MS;

#[derive(Debug, Deserialize)]
pub struct Incoming {
    n: String,
    u: String,
    r: Option<String>,
    s: String,
    v: Option<String>,
    w: Option<u32>,
    g: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Batch {
    One(Incoming),
    Many(Vec<Incoming>),
}

pub fn parse_batch(body: &str) -> Option<Vec<Incoming>> {
    match serde_json::from_str::<Batch>(body).ok()? {
        Batch::One(e) => Some(vec![e]),
        Batch::Many(v) => Some(v.into_iter().take(MAX_BATCH).collect()),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub ts: i64,
    pub visitor_id: String,
    pub kind: &'static str,
    pub name: Option<String>,
    pub path: String,
    pub referrer_host: Option<String>,
    pub utm_source: Option<String>,
    pub utm_medium: Option<String>,
    pub utm_campaign: Option<String>,
    pub utm_content: Option<String>,
    pub utm_term: Option<String>,
    pub channel: &'static str,
    pub country: Option<String>,
    pub device: &'static str,
    pub browser: &'static str,
}

pub const INSERT_EVENT: &str = "INSERT INTO events (ts, visitor_id, kind, name, path, referrer_host, utm_source, utm_medium, utm_campaign, utm_content, utm_term, channel, country, device, browser) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)";

pub const INSERT_CONVERSION: &str = "INSERT OR IGNORE INTO conversions (ts, kind, visitor_id, amount_cents, ref) VALUES (?1, ?2, ?3, ?4, ?5)";

fn own_host(host: &str) -> bool {
    host == SITE || host == "www.virling.com"
}

pub fn valid_visitor_id(v: &str) -> bool {
    (8..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn clip(s: &str, max: usize) -> Option<String> {
    let s: String = s.trim().chars().take(max).collect();
    (!s.is_empty()).then_some(s)
}

/// Destination of an outbound click as host and path; None for our own pages or non-web links.
fn exit_link(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    if !matches!(url.scheme(), "http" | "https") || own_host(&host) {
        return None;
    }
    clip(&format!("{host}{}", url.path()), 300)
}

pub fn country(header: Option<&str>) -> Option<String> {
    let v = header?.trim().to_ascii_uppercase();
    (v.len() == 2 && v != "XX" && v != "T1").then_some(v)
}

/// Validates one tracker event; returns None when it is not ours or is malformed.
pub fn to_row(e: Incoming, user_agent: &str, country: Option<&str>, now_ms: i64) -> Option<Row> {
    if !e.s.eq_ignore_ascii_case(SITE) {
        return None;
    }
    let page = Url::parse(&e.u).ok()?;
    if !own_host(page.host_str()?) {
        return None;
    }
    let (kind, name) = match e.n.as_str() {
        "pageview" => ("pageview", None),
        "goal" => ("goal", Some(clip(e.g.as_deref()?, 64)?)),
        "exit" => ("exit", Some(exit_link(e.g.as_deref()?)?)),
        "leave" => ("leave", None),
        _ => return None,
    };
    let visitor_id = e.v.filter(|v| valid_visitor_id(v))?;
    let utm = |k: &str| {
        page.query_pairs()
            .find(|(key, _)| key == k)
            .and_then(|(_, v)| clip(&v, 200))
    };
    let referrer_host =
        e.r.as_deref()
            .and_then(|r| Url::parse(r).ok())
            .and_then(|r| r.host_str().map(str::to_ascii_lowercase))
            .filter(|h| !own_host(h));
    if referrer_host.as_deref().is_some_and(is_spam_referrer) {
        return None;
    }
    let (utm_source, utm_medium) = (utm("utm_source").or_else(|| utm("ref")), utm("utm_medium"));
    let channel = channel::classify(
        utm_medium.as_deref(),
        utm_source.as_deref(),
        referrer_host.as_deref(),
    );
    Some(Row {
        ts: now_ms,
        visitor_id,
        kind,
        name,
        path: page.path().chars().take(500).collect(),
        referrer_host,
        utm_source,
        utm_medium,
        utm_campaign: utm("utm_campaign"),
        utm_content: utm("utm_content"),
        utm_term: utm("utm_term"),
        channel: channel.as_str(),
        country: country.map(str::to_string),
        device: device(e.w),
        browser: browser(user_agent),
    })
}

pub fn waitlist_kind(wedge: &str) -> Option<&'static str> {
    match wedge {
        "selfhost" => Some("waitlist_selfhost"),
        "agency" => Some("waitlist_agency"),
        _ => None,
    }
}

/// Stats queries. ?1 is the window start, ?2 the attribution lookback start, both unix ms.
pub const STATS_TOTALS: &str = "SELECT COUNT(DISTINCT visitor_id) AS visitors, COUNT(*) AS pageviews FROM events WHERE kind = 'pageview' AND ts >= ?1";

pub const STATS_SESSIONS: &str = "WITH e AS (SELECT ts, LAG(ts) OVER (PARTITION BY visitor_id ORDER BY ts, id) AS prev FROM events WHERE kind IN ('pageview', 'goal') AND ts >= ?1) SELECT COUNT(*) AS sessions FROM e WHERE prev IS NULL OR ts - prev > 1800000";

pub const STATS_CHANNELS: &str = "WITH first AS (SELECT channel, ROW_NUMBER() OVER (PARTITION BY visitor_id ORDER BY ts, id) AS rn FROM events WHERE kind IN ('pageview', 'goal') AND ts >= ?1) SELECT channel AS label, COUNT(*) AS visitors FROM first WHERE rn = 1 GROUP BY channel ORDER BY visitors DESC, label";

pub const STATS_CONVERSIONS: &str = "SELECT kind, COUNT(*) AS count, COALESCE(SUM(amount_cents), 0) AS amount_cents FROM conversions WHERE ts >= ?1 GROUP BY kind ORDER BY kind";

pub const STATS_CONVERSIONS_BY_CHANNEL: &str = "SELECT COALESCE((SELECT e.channel FROM events e WHERE e.visitor_id = c.visitor_id AND e.channel <> 'Direct' AND e.ts <= c.ts AND e.ts >= c.ts - 7776000000 ORDER BY e.ts DESC, e.id DESC LIMIT 1), 'Direct') AS label, c.kind, COUNT(*) AS count FROM conversions c WHERE c.ts >= ?1 GROUP BY label, c.kind ORDER BY label, c.kind";

pub const STATS_LANDING: &str = "WITH first AS (SELECT path, ROW_NUMBER() OVER (PARTITION BY visitor_id ORDER BY ts, id) AS rn FROM events WHERE kind = 'pageview' AND ts >= ?1) SELECT path AS label, COUNT(*) AS visitors FROM first WHERE rn = 1 GROUP BY path ORDER BY visitors DESC, label";

pub const STATS_EXIT_PAGES: &str = "WITH last AS (SELECT path, ROW_NUMBER() OVER (PARTITION BY visitor_id ORDER BY ts DESC, id DESC) AS rn FROM events WHERE kind = 'pageview' AND ts >= ?1) SELECT path AS label, COUNT(*) AS visitors FROM last WHERE rn = 1 GROUP BY path ORDER BY visitors DESC, label LIMIT 20";

pub const STATS_EXIT_LINKS: &str = "SELECT name AS label, COUNT(DISTINCT visitor_id) AS visitors FROM events WHERE kind = 'exit' AND ts >= ?1 GROUP BY name ORDER BY visitors DESC, label LIMIT 20";

pub const STATS_CONVERSIONS_BY_LANDING: &str = "WITH first AS (SELECT visitor_id, path, ROW_NUMBER() OVER (PARTITION BY visitor_id ORDER BY ts, id) AS rn FROM events WHERE kind = 'pageview' AND ts >= ?2) SELECT COALESCE(f.path, '(unknown)') AS label, c.kind, COUNT(*) AS count FROM conversions c LEFT JOIN first f ON f.visitor_id = c.visitor_id AND f.rn = 1 WHERE c.ts >= ?1 GROUP BY label, c.kind ORDER BY label, c.kind";

#[derive(Deserialize, Serialize)]
pub struct LabelCount {
    pub label: String,
    pub visitors: i64,
}

#[derive(Deserialize)]
pub struct LabelConversion {
    pub label: String,
    pub kind: String,
    pub count: i64,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct StatsRow {
    pub label: String,
    pub visitors: i64,
    pub waitlist_selfhost: i64,
    pub waitlist_agency: i64,
    pub preorder: i64,
}

/// Joins visitor counts and conversion counts on label; labels with only conversions are kept.
pub fn pivot(visitors: Vec<LabelCount>, conversions: Vec<LabelConversion>) -> Vec<StatsRow> {
    let mut rows: Vec<StatsRow> = visitors
        .into_iter()
        .map(|v| StatsRow {
            label: v.label,
            visitors: v.visitors,
            ..StatsRow::default()
        })
        .collect();
    for c in conversions {
        let i = match rows.iter().position(|r| r.label == c.label) {
            Some(i) => i,
            None => {
                rows.push(StatsRow {
                    label: c.label,
                    ..StatsRow::default()
                });
                rows.len() - 1
            }
        };
        match c.kind.as_str() {
            "waitlist_selfhost" => rows[i].waitlist_selfhost += c.count,
            "waitlist_agency" => rows[i].waitlist_agency += c.count,
            "preorder" | "preorder_personal" => rows[i].preorder += c.count,
            _ => {}
        }
    }
    rows
}

/// Constant-time token comparison for the stats endpoint.
pub fn token_matches(given: &str, expected: &str) -> bool {
    !expected.is_empty()
        && given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const UA: &str = "Mozilla/5.0 (Macintosh) AppleWebKit/605 Version/17.0 Safari/605.1";
    const VID: &str = "0b9f6c3e-1d2a-4b5c-8d7e-9f0a1b2c3d4e";

    fn one(json: serde_json::Value) -> Option<Row> {
        let mut batch = parse_batch(&json.to_string()).unwrap();
        to_row(batch.remove(0), UA, Some("IN"), 1_700_000_000_000)
    }

    fn pageview(url: &str, referrer: Option<&str>) -> serde_json::Value {
        serde_json::json!({ "n": "pageview", "u": url, "r": referrer, "s": "virling.com", "v": VID, "w": 1440 })
    }

    #[test]
    fn parses_single_and_caps_batches() {
        assert_eq!(
            parse_batch(&pageview("https://virling.com/", None).to_string())
                .unwrap()
                .len(),
            1
        );
        let many: Vec<_> = (0..80)
            .map(|_| pageview("https://virling.com/", None))
            .collect();
        assert_eq!(
            parse_batch(&serde_json::Value::from(many).to_string())
                .unwrap()
                .len(),
            MAX_BATCH
        );
        assert!(parse_batch("nope").is_none());
        assert!(parse_batch(r#"{"n":"pageview"}"#).is_none());
    }

    #[test]
    fn builds_row_with_channel_and_utms() {
        let row = one(pageview(
            "https://www.virling.com/agency?utm_source=newsletter&utm_medium=email&utm_campaign=launch",
            Some("https://mail.google.com/"),
        ))
        .unwrap();
        assert_eq!(row.path, "/agency");
        assert_eq!(row.channel, "Email");
        assert_eq!(row.utm_campaign.as_deref(), Some("launch"));
        assert_eq!(row.referrer_host.as_deref(), Some("mail.google.com"));
        assert_eq!(row.country.as_deref(), Some("IN"));
        assert_eq!(
            (row.device, row.browser, row.kind),
            ("Desktop", "Safari", "pageview")
        );
    }

    #[test]
    fn drops_self_referrals() {
        let row = one(pageview(
            "https://virling.com/agency",
            Some("https://virling.com/"),
        ))
        .unwrap();
        assert_eq!(row.referrer_host, None);
        assert_eq!(row.channel, "Direct");
        let row = one(pageview("https://virling.com/", Some("https://t.co/abc"))).unwrap();
        assert_eq!(row.channel, "Social");
    }

    #[test]
    fn rejects_foreign_sites_hosts_and_bad_visitors() {
        let mut e = pageview("https://virling.com/", None);
        e["s"] = "example.com".into();
        assert!(one(e).is_none());
        assert!(one(pageview("https://virling.com.evil.io/", None)).is_none());
        assert!(one(pageview("https://app.virling.com/", None)).is_none());
        let mut e = pageview("https://virling.com/", None);
        e["v"] = serde_json::Value::Null;
        assert!(one(e).is_none());
        let mut e = pageview("https://virling.com/", None);
        e["v"] = "short".into();
        assert!(one(e).is_none());
        let mut e = pageview("https://virling.com/", None);
        e["n"] = "click".into();
        assert!(one(e).is_none());
    }

    #[test]
    fn exits_keep_host_and_path_of_outside_links() {
        let mut e = pageview("https://virling.com/", None);
        e["n"] = "exit".into();
        e["g"] = "https://github.com/virling/virling?tab=readme#top".into();
        let row = one(e.clone()).unwrap();
        assert_eq!(
            (row.kind, row.name.as_deref()),
            ("exit", Some("github.com/virling/virling"))
        );
        for g in [
            "https://www.virling.com/agency",
            "mailto:hi@virling.com",
            "",
        ] {
            e["g"] = g.into();
            assert!(one(e.clone()).is_none(), "{g}");
        }
        e["n"] = "leave".into();
        assert_eq!(one(e).unwrap().kind, "leave");
    }

    #[test]
    fn goals_need_a_name() {
        let mut e = pageview("https://virling.com/agency", None);
        e["n"] = "goal".into();
        assert!(one(e.clone()).is_none());
        e["g"] = "  ".into();
        assert!(one(e.clone()).is_none());
        e["g"] = "preorder_agency".into();
        let row = one(e).unwrap();
        assert_eq!(
            (row.kind, row.name.as_deref()),
            ("goal", Some("preorder_agency"))
        );
    }

    #[test]
    fn country_header_is_cleaned() {
        assert_eq!(country(Some("de")), Some("DE".into()));
        assert_eq!(country(Some("XX")), None);
        assert_eq!(country(Some("T1")), None);
        assert_eq!(country(None), None);
    }

    #[test]
    fn waitlist_kinds() {
        assert_eq!(waitlist_kind("selfhost"), Some("waitlist_selfhost"));
        assert_eq!(waitlist_kind("agency"), Some("waitlist_agency"));
        assert_eq!(waitlist_kind("other"), None);
    }

    #[test]
    fn attribution_window_constants_match_sql() {
        assert!(STATS_SESSIONS.contains(&(30 * 60 * 1000).to_string()));
        assert!(STATS_CONVERSIONS_BY_CHANNEL.contains(&ATTRIBUTION_MS.to_string()));
    }

    #[test]
    fn pivot_merges_visitors_and_conversions() {
        let visitors = vec![
            LabelCount {
                label: "Social".into(),
                visitors: 10,
            },
            LabelCount {
                label: "Direct".into(),
                visitors: 4,
            },
        ];
        let conversions = vec![
            LabelConversion {
                label: "Social".into(),
                kind: "waitlist_agency".into(),
                count: 2,
            },
            LabelConversion {
                label: "Social".into(),
                kind: "preorder".into(),
                count: 1,
            },
            LabelConversion {
                label: "Social".into(),
                kind: "preorder_personal".into(),
                count: 1,
            },
            LabelConversion {
                label: "Email".into(),
                kind: "waitlist_selfhost".into(),
                count: 3,
            },
        ];
        let rows = pivot(visitors, conversions);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            StatsRow {
                label: "Social".into(),
                visitors: 10,
                waitlist_selfhost: 0,
                waitlist_agency: 2,
                preorder: 2
            }
        );
        assert_eq!(rows[1].visitors, 4);
        assert_eq!(
            (
                rows[2].label.as_str(),
                rows[2].visitors,
                rows[2].waitlist_selfhost
            ),
            ("Email", 0, 3)
        );
    }

    #[test]
    fn token_check() {
        assert!(token_matches("abc123", "abc123"));
        assert!(!token_matches("abc124", "abc123"));
        assert!(!token_matches("abc", "abc123"));
        assert!(!token_matches("", ""));
    }
}
