//! Pure first-party analytics for virling.com: event validation and conversion kinds.
use serde::Deserialize;
use worker::Url;

use crate::channel;
use crate::ua::{browser, device};

pub const SITE: &str = "virling.com";
pub const MAX_BATCH: usize = 50;

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
}
