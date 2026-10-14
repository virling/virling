//! Fetches a site's own favicon from its public homepage and caches it in SQLite.
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::{Client, Url, redirect};
use time::OffsetDateTime;

use crate::app::AppState;

const MAX_BYTES: usize = 256 * 1024;
const REFRESH_SECS: i64 = 86_400;
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
struct PublicOnly;

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
            && url.scheme() == "https"
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
    let base = Url::parse(&format!("https://{domain}/"))?;
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

/// Fetches the icon in the background unless one was tried in the last day.
pub fn refresh(state: &AppState, site_id: &str, domain: &str, fetched_at: Option<i64>) {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    // Tests never reach the network.
    if cfg!(test) || fetched_at.is_some_and(|t| now - t < REFRESH_SECS) {
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

    #[tokio::test]
    async fn local_hosts_are_never_fetched() {
        let client = client();
        for domain in ["localhost", "127.0.0.1", "[::1]", "169.254.169.254"] {
            assert!(fetch(&client, domain).await.is_err(), "{domain}");
        }
    }
}
