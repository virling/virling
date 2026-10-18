use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use time::{Date, Duration, OffsetDateTime};
use url::Url;

use crate::app::AppState;
use crate::events::Event;
use crate::ua::{browser, device, is_bot, is_spam_referrer};
use crate::{channel, crawlers};

const SESSION_IDLE: Duration = Duration::minutes(30);
const MAX_BATCH: usize = 50;

#[derive(Debug, Deserialize)]
pub struct Incoming {
    /// "pageview" or "goal"
    n: String,
    /// full page URL
    u: String,
    /// document.referrer
    r: Option<String>,
    /// site domain from data-site
    s: String,
    /// visitor ID from the first-party cookie; absent in cookieless mode
    v: Option<String>,
    /// screen width in CSS px
    w: Option<u32>,
    /// goal name
    g: Option<String>,
    /// navigator.language
    l: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Batch {
    One(Incoming),
    Many(Vec<Incoming>),
}

#[derive(Default)]
pub struct Sessions {
    map: Mutex<HashMap<(String, String), (String, OffsetDateTime)>>,
}

impl Sessions {
    fn active(&self, site_id: &str, visitor_id: &str, now: OffsetDateTime) -> Option<String> {
        let mut map = self.map.lock().unwrap();
        let (id, last) = map.get_mut(&(site_id.to_string(), visitor_id.to_string()))?;
        (now - *last < SESSION_IDLE).then(|| {
            *last = now;
            id.clone()
        })
    }

    fn session_for(&self, site_id: &str, visitor_id: &str, now: OffsetDateTime) -> String {
        let mut map = self.map.lock().unwrap();
        if map.len() > 200_000 {
            map.retain(|_, (_, last)| now - *last < SESSION_IDLE);
        }
        let key = (site_id.to_string(), visitor_id.to_string());
        match map.get_mut(&key) {
            Some((id, last)) if now - *last < SESSION_IDLE => {
                *last = now;
                id.clone()
            }
            _ => {
                let id = uuid::Uuid::new_v4().to_string();
                map.insert(key, (id.clone(), now));
                id
            }
        }
    }
}

/// Daily-rotating salt for cookieless visitor IDs; yesterday's salt is dropped, never stored.
pub struct DailySalt {
    inner: Mutex<(Date, RandomState)>,
}

impl Default for DailySalt {
    fn default() -> Self {
        Self {
            inner: Mutex::new((OffsetDateTime::now_utc().date(), RandomState::new())),
        }
    }
}

impl DailySalt {
    fn visitor_id(&self, site_id: &str, ip: IpAddr, user_agent: &str) -> String {
        let mut inner = self.inner.lock().unwrap();
        let today = OffsetDateTime::now_utc().date();
        if inner.0 != today {
            *inner = (today, RandomState::new());
        }
        let mut h = inner.1.build_hasher();
        h.write(site_id.as_bytes());
        h.write(ip.to_string().as_bytes());
        h.write(user_agent.as_bytes());
        format!("c-{:016x}", h.finish())
    }
}

/// Per-site fixed one-second window.
#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<String, (i64, u32)>>,
}

impl RateLimiter {
    pub(crate) fn allow(&self, site_id: &str, limit: u32) -> bool {
        let second = OffsetDateTime::now_utc().unix_timestamp();
        let mut w = self.windows.lock().unwrap();
        let entry = w.entry(site_id.to_string()).or_insert((second, 0));
        if entry.0 != second {
            *entry = (second, 0);
        }
        entry.1 += 1;
        entry.1 <= limit
    }
}

/// Destination of an outbound click as origin and path, without query or fragment; None for same-site, same-visit or non-web links.
fn exit_link(raw: &str, domain: &str, same_visit: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    let host = url.host_str()?;
    if !matches!(url.scheme(), "http" | "https")
        || std::iter::once(domain)
            .chain(same_visit.split_whitespace())
            .any(|d| host_belongs(host, d))
    {
        return None;
    }
    Some(clip(
        format!("{}{}", host.to_ascii_lowercase(), url.path()),
        300,
    ))
}

/// Region and city appear only when the proxy sends them (Cloudflare visitor location headers, Vercel).
fn place(headers: &HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|h| {
        let raw = headers.get(*h)?.to_str().ok()?;
        // Vercel percent-encodes these; a bare value parses as one form key.
        let (v, _) = url::form_urlencoded::parse(raw.trim().as_bytes()).next()?;
        let v = clip(v.trim().to_string(), 80);
        (!v.is_empty()).then_some(v)
    })
}

fn country(headers: &HeaderMap) -> Option<String> {
    ["cf-ipcountry", "x-vercel-ip-country", "x-country-code"]
        .iter()
        .find_map(|h| {
            let v = headers.get(*h)?.to_str().ok()?.trim().to_ascii_uppercase();
            (v.len() == 2 && v != "XX" && v != "T1").then_some(v)
        })
}

/// "en" or "en-US" from a language tag such as "en-us" or "zh-Hans-CN"; `base` drops the region.
fn language(tag: &str, base: bool) -> Option<String> {
    let mut parts = tag.trim().split(['-', '_']);
    let lang = parts.next()?.to_ascii_lowercase();
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    let region = parts
        .find(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()))
        .filter(|_| !base);
    Some(match region {
        Some(r) => format!("{lang}-{}", r.to_ascii_uppercase()),
        None => lang,
    })
}

fn client_ip(headers: &HeaderMap, peer: SocketAddr, trust_proxy: bool) -> IpAddr {
    if trust_proxy {
        let forwarded = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .and_then(|v| v.trim().parse().ok());
        if let Some(ip) = forwarded {
            return ip;
        }
    }
    peer.ip()
}

/// Operating system from a user agent; iPadOS that reports as a Mac reads as macOS.
fn os(user_agent: &str) -> &'static str {
    if ["iPhone", "iPad", "iPod"]
        .iter()
        .any(|m| user_agent.contains(m))
    {
        "iOS"
    } else if user_agent.contains("Android") {
        "Android"
    } else if user_agent.contains("Windows") {
        "Windows"
    } else if user_agent.contains("CrOS") {
        "ChromeOS"
    } else if user_agent.contains("Mac OS X") || user_agent.contains("Macintosh") {
        "macOS"
    } else if user_agent.contains("Linux") {
        "Linux"
    } else {
        "Other"
    }
}

pub(crate) fn host_belongs(host: &str, domain: &str) -> bool {
    let host = host.strip_prefix("www.").unwrap_or(host);
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn valid_visitor_id(v: &str) -> bool {
    (8..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn clip(s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

pub async fn collect(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let parse = || -> Option<Vec<Incoming>> {
        Some(match serde_json::from_slice::<Batch>(&body).ok()? {
            Batch::One(e) => vec![e],
            Batch::Many(v) => v.into_iter().take(MAX_BATCH).collect(),
        })
    };
    if is_bot(user_agent) {
        // AI crawlers that run the script count as crawler hits, never as visitors.
        if let Some(bot) = crawlers::detect(user_agent) {
            for e in parse()
                .unwrap_or_default()
                .iter()
                .filter(|e| e.n == "pageview")
            {
                crawlers::record(&state, bot, &e.s, &e.u).await;
            }
        }
        return accepted();
    }
    let Some(incoming) = parse() else {
        return (StatusCode::BAD_REQUEST, "invalid event").into_response();
    };
    let ip = client_ip(&headers, peer, state.config.trust_proxy);
    let country = country(&headers);
    let region = place(&headers, &["cf-region", "x-vercel-ip-country-region"]);
    let city = place(&headers, &["cf-ipcity", "x-vercel-ip-city"]);
    let browser = browser(user_agent);
    let os = os(user_agent);
    let accept_language = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split([',', ';']).next())
        .unwrap_or("")
        .to_string();
    let now = OffsetDateTime::now_utc();

    let mut events = Vec::with_capacity(incoming.len());
    for e in incoming {
        let Some(site) = state
            .sites
            .by_domain(&state.db, &e.s.to_ascii_lowercase())
            .await
        else {
            continue;
        };
        if !state
            .limiter
            .allow(&site.id, state.config.events_per_second)
        {
            continue;
        }
        let Ok(page) = Url::parse(&e.u) else { continue };
        let Some(page_host) = page.host_str() else {
            continue;
        };
        if !host_belongs(page_host, &site.domain) {
            continue;
        }
        let kind = match e.n.as_str() {
            "pageview" => "pageview",
            "goal" => "goal",
            "exit" => "exit",
            "leave" => "leave",
            _ => continue,
        };
        let name = match kind {
            "goal" => match e
                .g
                .map(|g| clip(g.trim().to_string(), 64))
                .filter(|g| !g.is_empty())
            {
                Some(g) => Some(g),
                None => continue,
            },
            "exit" => match e
                .g
                .as_deref()
                .and_then(|g| exit_link(g, &site.domain, &site.same_visit))
            {
                Some(link) => Some(link),
                None => continue,
            },
            _ => None,
        };

        let query: HashMap<String, String> = page.query_pairs().into_owned().collect();
        let utm = |k: &str| {
            query
                .get(k)
                .map(|v| clip(v.trim().to_string(), 200))
                .filter(|v| !v.is_empty())
        };
        let (utm_source, utm_medium) =
            (utm("utm_source").or_else(|| utm("ref")), utm("utm_medium"));
        let referrer_host =
            e.r.as_deref()
                .and_then(|r| Url::parse(r).ok())
                .and_then(|r| r.host_str().map(|h| h.to_ascii_lowercase()))
                .filter(|h| !host_belongs(h, &site.domain))
                .or_else(|| utm_source.as_deref().and_then(channel::source_host));
        if referrer_host.as_deref().is_some_and(is_spam_referrer) {
            continue;
        }
        let channel = channel::classify(
            utm_medium.as_deref(),
            utm_source.as_deref(),
            referrer_host.as_deref(),
        );

        let visitor_id = match e.v.filter(|v| !site.cookieless && valid_visitor_id(v)) {
            Some(v) => v,
            None => state.salt.visitor_id(&site.id, ip, user_agent),
        };
        let session_id = if matches!(kind, "exit" | "leave") {
            // Exits only extend a live session; a late one never starts a new visit.
            match state.sessions.active(&site.id, &visitor_id, now) {
                Some(id) => id,
                None => continue,
            }
        } else {
            state.sessions.session_for(&site.id, &visitor_id, now)
        };

        events.push(Event {
            site_id: site.id.clone(),
            ts: now,
            visitor_id,
            session_id,
            kind: kind.to_string(),
            name,
            path: clip(page.path().to_string(), 500),
            referrer_host,
            utm_source,
            utm_medium,
            utm_campaign: utm("utm_campaign"),
            utm_content: utm("utm_content"),
            utm_term: utm("utm_term"),
            channel: channel.as_str().to_string(),
            country: country.clone(),
            region: region.clone(),
            city: city.clone(),
            device: device(e.w).to_string(),
            browser: browser.to_string(),
            os: os.to_string(),
            host: Some(page_host.to_ascii_lowercase()),
            language: language(e.l.as_deref().unwrap_or(&accept_language), site.cookieless),
        });
    }

    if !events.is_empty() {
        let store = state.events.clone();
        let result = tokio::task::spawn_blocking(move || store.insert_events(&events)).await;
        if !matches!(result, Ok(Ok(()))) {
            tracing::error!(?result, "failed to store events");
            return (StatusCode::SERVICE_UNAVAILABLE, "try again").into_response();
        }
    }
    accepted()
}

fn accepted() -> Response {
    (
        StatusCode::ACCEPTED,
        [(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
        "",
    )
        .into_response()
}

pub async fn preflight() -> Response {
    (
        StatusCode::NO_CONTENT,
        [
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::ACCESS_CONTROL_ALLOW_METHODS, "POST"),
            (header::ACCESS_CONTROL_ALLOW_HEADERS, "content-type"),
            (header::ACCESS_CONTROL_MAX_AGE, "86400"),
        ],
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_region_and_city_from_proxy_headers() {
        let mut h = HeaderMap::new();
        assert_eq!(
            place(&h, &["cf-region", "x-vercel-ip-country-region"]),
            None
        );
        h.insert("x-vercel-ip-city", "S%C3%A3o%20Paulo".parse().unwrap());
        h.insert("cf-region", "Bavaria".parse().unwrap());
        assert_eq!(
            place(&h, &["cf-ipcity", "x-vercel-ip-city"]).as_deref(),
            Some("São Paulo")
        );
        assert_eq!(
            place(&h, &["cf-region", "x-vercel-ip-country-region"]).as_deref(),
            Some("Bavaria")
        );
    }

    #[test]
    fn reads_the_operating_system() {
        let cases = [
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)",
                "iOS",
            ),
            ("Mozilla/5.0 (Linux; Android 14; Pixel 8)", "Android"),
            ("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", "Windows"),
            ("Mozilla/5.0 (X11; CrOS x86_64 14541.0.0)", "ChromeOS"),
            ("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)", "macOS"),
            ("Mozilla/5.0 (X11; Linux x86_64)", "Linux"),
            ("curl/8", "Other"),
        ];
        for (ua, want) in cases {
            assert_eq!(os(ua), want, "{ua}");
        }
    }

    #[test]
    fn normalizes_browser_languages() {
        let cases = [
            ("en-US", false, Some("en-US")),
            ("en-us", false, Some("en-US")),
            ("de", false, Some("de")),
            ("zh-Hans-CN", false, Some("zh-CN")),
            ("pt_BR", false, Some("pt-BR")),
            ("es-419", false, Some("es")),
            ("en-GB", true, Some("en")),
            ("", false, None),
            ("*", false, None),
            ("english", false, None),
        ];
        for (tag, base, want) in cases {
            assert_eq!(language(tag, base).as_deref(), want, "{tag}");
        }
    }

    #[test]
    fn sessions_expire_after_idle() {
        let s = Sessions::default();
        let t = OffsetDateTime::now_utc();
        let a = s.session_for("site", "v1", t);
        assert_eq!(a, s.session_for("site", "v1", t + Duration::minutes(29)));
        assert_ne!(a, s.session_for("site", "v1", t + Duration::minutes(60)));
    }

    #[test]
    fn www_and_same_visit_domains_are_not_exits() {
        let exit = |url: &str| exit_link(url, "example.com", "app.example.net docs.example.org");
        assert_eq!(exit("https://example.com/pricing"), None);
        assert_eq!(exit("https://www.example.com/"), None);
        assert_eq!(exit("https://app.example.net/signup"), None);
        assert_eq!(exit("https://www.docs.example.org/a"), None);
        assert_eq!(
            exit("https://x.com/virling?a=1").as_deref(),
            Some("x.com/virling")
        );
        assert_eq!(
            exit("https://example.net/").as_deref(),
            Some("example.net/")
        );
    }

    #[test]
    fn subdomains_belong_to_site() {
        assert!(host_belongs("www.example.com", "example.com"));
        assert!(host_belongs("app.example.com", "example.com"));
        assert!(!host_belongs("example.com.evil.io", "example.com"));
        assert!(!host_belongs("notexample.com", "example.com"));
    }

    #[test]
    fn cookieless_ids_are_stable_within_a_day() {
        let salt = DailySalt::default();
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        assert_eq!(
            salt.visitor_id("s", ip, "ua"),
            salt.visitor_id("s", ip, "ua")
        );
        assert_ne!(
            salt.visitor_id("s", ip, "ua"),
            salt.visitor_id("s", ip, "other")
        );
    }
}
