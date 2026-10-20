//! Fetches site and referrer favicons from public homepages and caches them in SQLite.
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::{Client, Url, redirect};
use sqlx::SqlitePool;
use time::OffsetDateTime;
use tokio::sync::mpsc;

use crate::app::AppState;

const MAX_BYTES: usize = 256 * 1024;
const REFRESH_SECS: i64 = 86_400;
/// A failed fetch is tried again sooner, so a site that was briefly down gets its icon.
const RETRY_SECS: i64 = 600;
const SOURCE_REFRESH_SECS: i64 = 30 * 86_400;
const MAX_QUEUED: usize = 1000;
const TYPES: [&str; 5] = [
    "image/svg+xml",
    "image/png",
    "image/x-icon",
    "image/vnd.microsoft.icon",
    "image/webp",
];

/// True for addresses on the public internet; private, loopback, link-local and similar are refused.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || a == 0
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19))
                || a >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || first == 0x2001 && v6.segments()[1] == 0x0db8)
        }
    }
}

/// Resolves names with the system resolver and keeps only public addresses.
pub(crate) struct PublicOnly;

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 443))
                .await?
                .filter(|a| is_public(a.ip()))
                .collect();
            if addrs.is_empty() {
                return Err(format!("{host} has no public address").into());
            }
            let addrs: Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}

fn literal_ip_is_public(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => is_public(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => is_public(IpAddr::V6(ip)),
        Some(url::Host::Domain(_)) => true,
        None => false,
    }
}

pub fn client() -> Client {
    Client::builder()
        .user_agent(concat!("virling/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(5))
        .https_only(true)
        .no_proxy()
        .dns_resolver(Arc::new(PublicOnly))
        .redirect(redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 || !literal_ip_is_public(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .expect("favicon client")
}

/// Icon URLs declared in `html`, in page order, then `/favicon.ico`; https only.
pub fn candidates(html: &str, base: &Url) -> Vec<Url> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = lower[from..].find("<link") {
        let start = from + i;
        let end = lower[start..].find('>').map_or(lower.len(), |e| start + e);
        let tag = &html[start..end];
        from = end;
        let rel = attr(tag, "rel").unwrap_or_default().to_ascii_lowercase();
        let is_icon = rel
            .split_ascii_whitespace()
            .any(|r| r == "icon" || r == "apple-touch-icon");
        if is_icon
            && let Some(href) = attr(tag, "href")
            && let Ok(url) = base.join(href.trim())
            && url.scheme() == base.scheme()
            && !out.contains(&url)
        {
            out.push(url);
        }
    }
    if let Ok(url) = base.join("/favicon.ico")
        && !out.contains(&url)
    {
        out.push(url);
    }
    out
}

/// Value of attribute `name` in a single tag, quoted or not.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let before = lower[..at].chars().last();
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let rest = lower[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let offset = tag.len() - rest.trim_start().len();
        let value = &tag[offset..];
        return Some(match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or_default(),
            _ => value.split_whitespace().next().unwrap_or_default(),
        });
    }
    None
}

/// Reads at most `MAX_BYTES` of a response body.
async fn body(mut res: reqwest::Response) -> anyhow::Result<Vec<u8>> {
    if res.content_length().is_some_and(|n| n > MAX_BYTES as u64) {
        anyhow::bail!("too large");
    }
    let mut out = Vec::new();
    while let Some(chunk) = res.chunk().await? {
        if out.len() + chunk.len() > MAX_BYTES {
            anyhow::bail!("too large");
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// The icon of `domain` as (content type, bytes).
pub async fn fetch(client: &Client, domain: &str) -> anyhow::Result<(String, Vec<u8>)> {
    fetch_from(client, Url::parse(&format!("https://{domain}/"))?).await
}

async fn fetch_from(client: &Client, base: Url) -> anyhow::Result<(String, Vec<u8>)> {
    if !literal_ip_is_public(&base) {
        anyhow::bail!("not a public host");
    }
    let page = client.get(base.clone()).send().await?;
    let base = page.url().clone();
    let html = body(page).await.unwrap_or_default();
    for url in candidates(&String::from_utf8_lossy(&html), &base) {
        if !literal_ip_is_public(&url) {
            continue;
        }
        let Ok(res) = client.get(url).send().await else {
            continue;
        };
        if !res.status().is_success() {
            continue;
        }
        let kind = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(|v| v.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if !TYPES.contains(&kind.as_str()) {
            continue;
        }
        if let Ok(bytes) = body(res).await
            && !bytes.is_empty()
        {
            return Ok((kind, bytes));
        }
    }
    anyhow::bail!("no usable icon")
}

static RUNNING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Fetches the icon in the background unless one was fetched in the last day or tried in the last ten minutes.
pub fn refresh(state: &AppState, site_id: &str, domain: &str, fetched: Option<(i64, bool)>) {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    // Tests never reach the network.
    if cfg!(test)
        || fetched.is_some_and(|(t, ok)| now - t < if ok { REFRESH_SECS } else { RETRY_SECS })
    {
        return;
    }
    if !RUNNING
        .lock()
        .unwrap()
        .get_or_insert_default()
        .insert(site_id.to_string())
    {
        return;
    }
    let (state, site_id, domain) = (state.clone(), site_id.to_string(), domain.to_string());
    tokio::spawn(async move {
        let fetched = fetch(&state.favicons, &domain).await;
        if let Err(err) = &fetched {
            tracing::debug!(%err, %domain, "favicon fetch failed");
        }
        let (kind, bytes) = fetched.map_or((None, None), |(k, b)| (Some(k), Some(b)));
        let saved = sqlx::query(
            "INSERT INTO site_icons (site_id, content_type, body, fetched_at) VALUES (?, ?, ?, ?)
             ON CONFLICT (site_id) DO UPDATE SET
                content_type = coalesce(excluded.content_type, site_icons.content_type),
                body = coalesce(excluded.body, site_icons.body),
                fetched_at = excluded.fetched_at",
        )
        .bind(&site_id)
        .bind(kind)
        .bind(bytes)
        .bind(now)
        .execute(&state.db)
        .await;
        if let Err(err) = saved {
            tracing::error!(%err, "saving favicon failed");
        }
        if let Some(set) = RUNNING.lock().unwrap().as_mut() {
            set.remove(&site_id);
        }
    });
}

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// Referrer hosts by (last fetch, has an icon), and hosts waiting for the worker.
#[derive(Default)]
struct Sources {
    known: HashMap<String, (i64, bool)>,
    queued: HashSet<String>,
}

static SOURCES: Mutex<Option<Sources>> = Mutex::new(None);
static QUEUE: OnceLock<mpsc::UnboundedSender<String>> = OnceLock::new();

/// `name` as a referrer host worth an icon: a plain domain name, not an address or a label.
pub fn source_host(name: &str) -> Option<String> {
    let host = crate::sites::normalize_domain(name)?;
    let plain = name
        .trim()
        .trim_start_matches("www.")
        .eq_ignore_ascii_case(&host);
    let tld = host.rsplit('.').next().unwrap_or_default();
    (plain && tld.bytes().all(|b| b.is_ascii_alphabetic())).then_some(host)
}

/// Version of the cached icon of `host`; queues a fetch when it is missing or 30 days old.
pub fn source_icon(host: &str) -> Option<i64> {
    let mut guard = SOURCES.lock().unwrap();
    let sources = guard.get_or_insert_default();
    let known = sources.known.get(host).copied();
    if known.is_none_or(|(t, _)| now() - t >= SOURCE_REFRESH_SECS)
        && sources.queued.len() < MAX_QUEUED
        && let Some(queue) = QUEUE.get()
        && sources.queued.insert(host.to_string())
    {
        queue.send(host.to_string()).ok();
    }
    known.filter(|k| k.1).map(|k| k.0)
}

fn remember(host: &str, fetched_at: i64, has_icon: bool) {
    let mut guard = SOURCES.lock().unwrap();
    let sources = guard.get_or_insert_default();
    sources
        .known
        .insert(host.to_string(), (fetched_at, has_icon));
    sources.queued.remove(host);
}

/// Starts the worker that fetches referrer icons one at a time.
pub fn spawn_sources(state: AppState) {
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    if QUEUE.set(tx).is_err() {
        return;
    }
    tokio::spawn(async move {
        let rows: Vec<(String, i64, bool)> =
            sqlx::query_as("SELECT host, fetched_at, body IS NOT NULL FROM source_icons")
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();
        for (host, fetched_at, has_icon) in rows {
            remember(&host, fetched_at, has_icon);
        }
        while let Some(host) = rx.recv().await {
            let known = SOURCES
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|s| s.known.get(&host).copied());
            match (known, Url::parse(&format!("https://{host}/"))) {
                (Some((t, has)), _) if now() - t < SOURCE_REFRESH_SECS => remember(&host, t, has),
                (_, Ok(base)) => save_source(&state.db, &state.favicons, &host, base).await,
                (_, Err(_)) => remember(&host, now(), false),
            }
        }
    });
}

/// Fetches the icon of `host` from `base` and caches it, keeping an older icon when that fails.
pub(crate) async fn save_source(db: &SqlitePool, client: &Client, host: &str, base: Url) {
    let fetched = fetch_from(client, base).await;
    if let Err(err) = &fetched {
        tracing::debug!(%err, %host, "source icon fetch failed");
    }
    let (kind, bytes) = fetched.map_or((None, None), |(k, b)| (Some(k), Some(b)));
    let at = now();
    let saved: Result<bool, _> = sqlx::query_scalar(
        "INSERT INTO source_icons (host, content_type, body, fetched_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (host) DO UPDATE SET
            content_type = coalesce(excluded.content_type, source_icons.content_type),
            body = coalesce(excluded.body, source_icons.body),
            fetched_at = excluded.fetched_at
         RETURNING body IS NOT NULL",
    )
    .bind(host)
    .bind(kind)
    .bind(bytes)
    .bind(at)
    .fetch_one(db)
    .await;
    if let Err(err) = &saved {
        tracing::error!(%err, "saving source icon failed");
    }
    remember(host, at, saved.unwrap_or(false));
}

/// The cached icon of a referrer host as (content type, bytes).
pub async fn source_body(db: &SqlitePool, host: &str) -> Option<(String, Vec<u8>)> {
    sqlx::query_as(
        "SELECT content_type, body FROM source_icons WHERE host = ? AND body IS NOT NULL",
    )
    .bind(host)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_links_resolve_against_the_page() {
        let base = Url::parse("https://shop.io/blog/").unwrap();
        let html = r#"<head>
            <LINK rel="stylesheet" href="/a.css">
            <link rel="shortcut icon" href="/img/fav.png">
            <link href='icon.svg' rel='icon' type="image/svg+xml">
            <link rel=apple-touch-icon href=https://cdn.shop.io/t.png>
            <link rel="icon" href="http://shop.io/plain.ico">
        </head>"#;
        let urls: Vec<String> = candidates(html, &base).iter().map(Url::to_string).collect();
        assert_eq!(
            urls,
            [
                "https://shop.io/img/fav.png",
                "https://shop.io/blog/icon.svg",
                "https://cdn.shop.io/t.png",
                "https://shop.io/favicon.ico",
            ]
        );
        assert_eq!(
            candidates("", &base)[0].as_str(),
            "https://shop.io/favicon.ico"
        );
    }

    #[test]
    fn private_addresses_are_refused() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["93.184.216.34", "2606:4700::6810:85e5"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn only_plain_domains_get_icons() {
        assert_eq!(source_host("someblog.dev").as_deref(), Some("someblog.dev"));
        assert_eq!(source_host("www.Blog.io").as_deref(), Some("blog.io"));
        for name in [
            "Hacker News",
            "partner-site",
            "93.184.216.34",
            "https://blog.io/a",
            "blog.io/a",
            "",
        ] {
            assert_eq!(source_host(name), None, "{name}");
        }
    }

    #[tokio::test]
    async fn source_icons_are_fetched_and_cached() {
        use axum::http::{StatusCode, header};
        use axum::routing::get;
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = fail.clone();
        let stub = axum::Router::new()
            .route(
                "/",
                get(|| async { r#"<html><head><link rel="icon" href="/i.png"></head></html>"# }),
            )
            .route(
                "/i.png",
                get(move || async move {
                    if flag.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(StatusCode::NOT_FOUND);
                    }
                    Ok(([(header::CONTENT_TYPE, "image/png")], b"PNG".to_vec()))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, stub).await.unwrap() });
        let client = Client::builder()
            .resolve("stub.example", addr)
            .resolve("gone.example", "127.0.0.1:1".parse().unwrap())
            .build()
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db = crate::open_sqlite(dir.path()).await.unwrap();
        let base = Url::parse(&format!("http://stub.example:{}/", addr.port())).unwrap();
        assert!(crate::icons::source("stub.example", 14).contains(">S</span>"));

        save_source(&db, &client, "stub.example", base.clone()).await;
        let body = source_body(&db, "stub.example").await;
        assert_eq!(body, Some(("image/png".into(), b"PNG".to_vec())));
        let v = source_icon("stub.example").expect("cached");
        assert!(crate::icons::source("stub.example", 14).contains(&format!(
            r#"<img class="ico" src="/source-icons/stub.example?v={v}""#
        )));

        fail.store(true, std::sync::atomic::Ordering::Relaxed);
        save_source(&db, &client, "stub.example", base).await;
        assert_eq!(source_body(&db, "stub.example").await.unwrap().1, b"PNG");
        assert!(
            source_icon("stub.example").is_some(),
            "a failed refresh keeps the icon"
        );

        save_source(
            &db,
            &client,
            "gone.example",
            "http://gone.example:1/".parse().unwrap(),
        )
        .await;
        assert_eq!(source_body(&db, "gone.example").await, None);
        assert_eq!(source_icon("gone.example"), None);
        assert!(crate::icons::source("gone.example", 14).contains(">G</span>"));
    }

    #[tokio::test]
    async fn local_hosts_are_never_fetched() {
        let client = client();
        for domain in ["localhost", "127.0.0.1", "[::1]", "169.254.169.254"] {
            assert!(fetch(&client, domain).await.is_err(), "{domain}");
        }
    }
}
