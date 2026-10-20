use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
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
    /// Navigation type of a page load (navigate, reload or back_forward); absent on route changes.
    k: Option<String>,
    /// Set by the script when the browser is automated or has no languages.
    b: Option<bool>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Batch {
    One(Incoming),
    Many(Vec<Incoming>),
}

/// Away at least this long before a page load counts as coming back.
const BACK_AFTER: Duration = Duration::seconds(10);

/// A visitor's current visit: its ID, last event, when the visitor last left, its UTM tags and last page.
struct Live {
    id: String,
    last: OffsetDateTime,
    left: Option<OffsetDateTime>,
    utm: Option<String>,
    page: String,
}

/// How an event arrives, for deciding whether it starts a new visit.
#[derive(Default)]
pub struct Arrival {
    /// A page load with a referrer outside the site, or none; route changes and same-site links are not.
    pub outside: bool,
    pub reload: bool,
    /// From a checkout, sign-in or same-visit domain: never a new visit while the visit is live.
    pub keep: bool,
    /// UTM tags of the landing URL.
    pub utm: Option<String>,
    /// Path of a pageview.
    pub page: Option<String>,
    /// Back or forward navigation: a return only to the visit's last page, else a move within the site.
    pub back_forward: bool,
    /// Away at least this long counts as a new visit.
    pub threshold: Duration,
}

#[derive(Default)]
pub struct Sessions {
    map: Mutex<HashMap<(String, String), Live>>,
}

impl Sessions {
    /// The live visit an exit or leave belongs to; a leave marks the visitor as gone.
    fn active(
        &self,
        site_id: &str,
        visitor_id: &str,
        now: OffsetDateTime,
        leave: bool,
    ) -> Option<String> {
        let mut map = self.map.lock().unwrap();
        let s = map.get_mut(&(site_id.to_string(), visitor_id.to_string()))?;
        (now - s.last < SESSION_IDLE).then(|| {
            s.last = now;
            if leave {
                s.left = Some(now);
            }
            s.id.clone()
        })
    }

    /// The visit of an event, and whether the visitor came back to it after leaving.
    fn session_for(
        &self,
        site_id: &str,
        visitor_id: &str,
        now: OffsetDateTime,
        a: &Arrival,
    ) -> (String, bool) {
        let mut map = self.map.lock().unwrap();
        if map.len() > 200_000 {
            map.retain(|_, s| now - s.last < SESSION_IDLE);
        }
        let key = (site_id.to_string(), visitor_id.to_string());
        if let Some(s) = map.get_mut(&key)
            && now - s.last < SESSION_IDLE
            && (a.utm.is_none() || a.utm == s.utm)
        {
            let away = s.left.map(|t| now - t).filter(|_| {
                a.outside && !a.reload && (!a.back_forward || a.page.as_ref() == Some(&s.page))
            });
            if a.keep || !away.is_some_and(|d| d >= a.threshold) {
                s.last = now;
                s.left = None;
                if let Some(p) = &a.page {
                    s.page.clone_from(p);
                }
                return (s.id.clone(), away.is_some_and(|d| d >= BACK_AFTER));
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        map.insert(
            key,
            Live {
                id: id.clone(),
                last: now,
                left: None,
                utm: a.utm.clone(),
                page: a.page.clone().unwrap_or_default(),
            },
        );
        (id, false)
    }
}

/// Payment provider checkout pages.
pub fn checkout(host: &str, path: &str) -> bool {
    matches!(
        host,
        "checkout.stripe.com"
            | "buy.stripe.com"
            | "checkout.dodopayments.com"
            | "test.checkout.dodopayments.com"
            | "buy.polar.sh"
            | "sandbox.polar.sh"
            | "checkout.paddle.com"
            | "buy.paddle.com"
            | "sandbox-checkout.paddle.com"
    ) || host.ends_with(".lemonsqueezy.com")
        || (host == "polar.sh" && path.starts_with("/checkout"))
}

/// Checkout and sign-in pages a visitor returns from in the middle of a visit.
fn checkout_or_login(host: &str, path: &str) -> bool {
    checkout(host, path)
        || matches!(host, "accounts.google.com" | "appleid.apple.com")
        || (host == "github.com" && (path.starts_with("/login") || path.starts_with("/session")))
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

fn country(headers: &HeaderMap, skip: usize) -> Option<String> {
    ["cf-ipcountry", "x-vercel-ip-country", "x-country-code"][skip..]
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

/// Cloudflare's published ranges (cloudflare.com/ips) as address and prefix length.
const CLOUDFLARE: [(IpAddr, u32); 22] = {
    const fn v4(a: u8, b: u8, c: u8, d: u8, bits: u32) -> (IpAddr, u32) {
        (IpAddr::V4(Ipv4Addr::new(a, b, c, d)), bits)
    }
    const fn v6(a: u16, b: u16, bits: u32) -> (IpAddr, u32) {
        (IpAddr::V6(Ipv6Addr::new(a, b, 0, 0, 0, 0, 0, 0)), bits)
    }
    [
        v4(173, 245, 48, 0, 20),
        v4(103, 21, 244, 0, 22),
        v4(103, 22, 200, 0, 22),
        v4(103, 31, 4, 0, 22),
        v4(141, 101, 64, 0, 18),
        v4(108, 162, 192, 0, 18),
        v4(190, 93, 240, 0, 20),
        v4(188, 114, 96, 0, 20),
        v4(197, 234, 240, 0, 22),
        v4(198, 41, 128, 0, 17),
        v4(162, 158, 0, 0, 15),
        v4(104, 16, 0, 0, 13),
        v4(104, 24, 0, 0, 14),
        v4(172, 64, 0, 0, 13),
        v4(131, 0, 72, 0, 22),
        v6(0x2400, 0xcb00, 32),
        v6(0x2606, 0x4700, 32),
        v6(0x2803, 0xf800, 32),
        v6(0x2405, 0xb500, 32),
        v6(0x2405, 0x8100, 32),
        v6(0x2a06, 0x98c0, 29),
        v6(0x2c0f, 0xf248, 32),
    ]
};

/// Whether cf-* headers are Cloudflare's: the peer is Cloudflare, or with trust_proxy a proxy on this host or network, such as cloudflared.
fn via_cloudflare(peer: IpAddr, trust_proxy: bool) -> bool {
    let peer = peer.to_canonical();
    let local = match peer {
        IpAddr::V4(a) => a.is_loopback() || a.is_private(),
        IpAddr::V6(a) => a.is_loopback() || a.is_unique_local(),
    };
    (trust_proxy && local)
        || CLOUDFLARE.iter().any(|&(net, bits)| match (peer, net) {
            (IpAddr::V4(a), IpAddr::V4(n)) => {
                u32::from(a) >> (32 - bits) == u32::from(n) >> (32 - bits)
            }
            (IpAddr::V6(a), IpAddr::V6(n)) => {
                u128::from(a) >> (128 - bits) == u128::from(n) >> (128 - bits)
            }
            _ => false,
        })
}

fn client_ip(headers: &HeaderMap, peer: SocketAddr, trust_proxy: bool, cf: bool) -> IpAddr {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .and_then(|v| v.trim().parse().ok())
    };
    if cf && let Some(ip) = header("cf-connecting-ip") {
        return ip;
    }
    if trust_proxy && let Some(ip) = header("x-forwarded-for") {
        return ip;
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
    let cf = via_cloudflare(peer.ip(), state.config.trust_proxy);
    let ip = client_ip(&headers, peer, state.config.trust_proxy, cf);
    // Each list starts with its cf-* header, skipped unless the request came through Cloudflare.
    let skip = usize::from(!cf);
    let country = country(&headers, skip);
    let region = place(
        &headers,
        &["cf-region", "x-vercel-ip-country-region"][skip..],
    );
    let city = place(&headers, &["cf-ipcity", "x-vercel-ip-city"][skip..]);
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
    let mut bots = Vec::new();
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
            "ping" => "ping",
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
        let referrer = e.r.as_deref().and_then(|r| Url::parse(r).ok());
        let from = referrer
            .as_ref()
            .and_then(|r| Some((r.host_str()?.to_ascii_lowercase(), r.path())));
        let same_site = from
            .as_ref()
            .is_some_and(|(h, _)| host_belongs(h, &site.domain));
        // Returns from checkout, sign-in and same-visit domains continue the visit and are never its source.
        let keep = from.as_ref().is_some_and(|(h, path)| {
            checkout_or_login(h, path)
                || site
                    .same_visit
                    .split_whitespace()
                    .any(|d| host_belongs(h, d))
        });
        let referrer_host = from
            .map(|(h, _)| h)
            .filter(|_| !same_site && !keep)
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
        let path = clip(page.path().to_string(), 500);
        let bot = if e.b == Some(true) {
            Some("automated")
        } else if e.w == Some(0) {
            Some("no screen")
        } else {
            None
        };
        if bot.is_none() && kind == "leave" {
            state.presence.gone(&site.id, &visitor_id);
        } else if bot.is_none() {
            state.presence.seen(&site.id, &visitor_id);
        }
        if kind == "ping" {
            continue;
        }
        let (session_id, back) = if bot.is_some() {
            (String::new(), false)
        } else if matches!(kind, "exit" | "leave") {
            // Exits only extend a live session; a late one never starts a new visit.
            match state
                .sessions
                .active(&site.id, &visitor_id, now, kind == "leave")
            {
                Some(id) => (id, false),
                None => continue,
            }
        } else {
            let load = kind == "pageview" && e.k.is_some();
            let arrival = Arrival {
                outside: load && !same_site,
                reload: e.k.as_deref() == Some("reload"),
                back_forward: e.k.as_deref() == Some("back_forward"),
                page: (kind == "pageview").then(|| path.clone()),
                keep,
                utm: (kind == "pageview")
                    .then(|| {
                        ["utm_source", "utm_medium", "utm_campaign"]
                            .map(|k| utm(k).unwrap_or_default())
                            .join("|")
                    })
                    .filter(|u| u != "||"),
                threshold: Duration::minutes(site.return_minutes),
            };
            state
                .sessions
                .session_for(&site.id, &visitor_id, now, &arrival)
        };

        let event = Event {
            site_id: site.id.clone(),
            ts: now,
            visitor_id,
            session_id,
            kind: kind.to_string(),
            name,
            path,
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
        };
        if let Some(reason) = bot {
            bots.push((event, reason));
            continue;
        }
        if back {
            // A "Came back" row just before the page the visitor returned to.
            events.push(Event {
                ts: now - Duration::milliseconds(1),
                kind: "back".into(),
                name: None,
                ..event.clone()
            });
        }
        events.push(event);
    }

    if !events.is_empty() || !bots.is_empty() {
        let store = state.events.clone();
        let result = tokio::task::spawn_blocking(move || {
            store.insert_bots(&bots)?;
            if events.is_empty() {
                return Ok(());
            }
            store.insert_events(&events)
        })
        .await;
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
    fn trusts_cloudflare_headers_only_from_cloudflare() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert!(via_cloudflare(ip("162.158.12.1"), false));
        assert!(via_cloudflare(ip("::ffff:104.16.0.9"), false));
        assert!(via_cloudflare(ip("2606:4700:10::1"), false));
        assert!(!via_cloudflare(ip("203.0.113.7"), false));
        assert!(!via_cloudflare(ip("127.0.0.1"), false));
        assert!(via_cloudflare(ip("127.0.0.1"), true));
        assert!(via_cloudflare(ip("172.17.0.1"), true));
        assert!(!via_cloudflare(ip("203.0.113.7"), true));
        let mut h = HeaderMap::new();
        h.insert("cf-ipcountry", "de".parse().unwrap());
        h.insert("cf-connecting-ip", "198.51.100.4".parse().unwrap());
        h.insert("x-forwarded-for", "192.0.2.1".parse().unwrap());
        let peer = SocketAddr::from(([127, 0, 0, 1], 9));
        assert_eq!(country(&h, 0).as_deref(), Some("DE"));
        assert_eq!(country(&h, 1), None);
        assert_eq!(client_ip(&h, peer, true, true), ip("198.51.100.4"));
        assert_eq!(client_ip(&h, peer, true, false), ip("192.0.2.1"));
        assert_eq!(client_ip(&h, peer, false, false), ip("127.0.0.1"));
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
        let a = Arrival::default();
        let (id, _) = s.session_for("site", "v1", t, &a);
        assert_eq!(
            id,
            s.session_for("site", "v1", t + Duration::minutes(29), &a).0
        );
        assert_ne!(
            id,
            s.session_for("site", "v1", t + Duration::minutes(60), &a).0
        );
    }

    #[test]
    fn returns_follow_the_threshold() {
        let s = Sessions::default();
        let t = OffsetDateTime::now_utc();
        let m = Duration::minutes;
        let load = |page: &str| Arrival {
            outside: true,
            page: Some(page.into()),
            threshold: m(5),
            ..Default::default()
        };
        let (a, _) = s.session_for("site", "v1", t, &load("/"));
        s.active("site", "v1", t + m(1), true);
        assert_eq!(
            s.session_for("site", "v1", t + m(3), &load("/")),
            (a.clone(), true)
        );
        s.active("site", "v1", t + m(4), true);
        let reload = Arrival {
            reload: true,
            ..load("/")
        };
        assert_eq!(s.session_for("site", "v1", t + m(12), &reload).0, a);
        s.active("site", "v1", t + m(13), true);
        let checkout = Arrival {
            keep: true,
            ..load("/welcome")
        };
        assert_eq!(s.session_for("site", "v1", t + m(23), &checkout).0, a);
        s.active("site", "v1", t + m(24), true);
        let other_page = Arrival {
            back_forward: true,
            ..load("/pricing")
        };
        assert_eq!(
            s.session_for("site", "v1", t + m(31), &other_page),
            (a.clone(), false)
        );
        s.active("site", "v1", t + m(32), true);
        let (b, back) = s.session_for("site", "v1", t + m(38), &load("/"));
        assert!(b != a && !back, "back after 6 minutes away is a new visit");
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
