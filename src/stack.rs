//! Reads a site's live homepage to guess its framework and payment provider and to confirm the script is installed.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::{Client, Url, redirect};

use crate::favicon::{PublicOnly, is_public};
use crate::providers::Provider;

const MAX_BYTES: usize = 2 * 1024 * 1024;
const PER_MINUTE: usize = 10;

/// What the homepage shows: the install guide tab to open, payment providers seen, and whether the script is there.
#[derive(Debug, Default, PartialEq)]
pub struct Stack {
    pub platform: Option<&'static str>,
    pub providers: Vec<Provider>,
    pub installed: bool,
}

/// Matches the tab ids on the Script step.
const PLATFORMS: [(&str, &[&str]); 5] = [
    (
        "p-next",
        &["/_next/static/", "__NEXT_DATA__", "next-route-announcer"],
    ),
    ("p-astro", &["astro-island", "/_astro/", "content=\"Astro"]),
    (
        "p-wp",
        &["/wp-content/", "/wp-includes/", "content=\"WordPress"],
    ),
    (
        "p-nocode",
        &[
            "framerusercontent.com",
            "content=\"Framer",
            "data-wf-site",
            "website-files.com",
            "webflow.com",
        ],
    ),
    ("p-shopify", &["cdn.shopify.com", "Shopify.theme"]),
];

const PROVIDERS: [(Provider, &[&str]); 5] = [
    (
        Provider::Stripe,
        &["js.stripe.com", "buy.stripe.com", "checkout.stripe.com"],
    ),
    (Provider::Dodo, &["dodopayments.com"]),
    (
        Provider::Polar,
        &["polar.sh/checkout", "buy.polar.sh", "@polar-sh/checkout"],
    ),
    (Provider::LemonSqueezy, &["lemonsqueezy.com", "lemon.js"]),
    (
        Provider::Paddle,
        &["cdn.paddle.com/paddle", "@paddle/paddle-js"],
    ),
];

pub fn detect(html: &str, script_host: &str) -> Stack {
    let platform = PLATFORMS
        .iter()
        .find(|(_, marks)| marks.iter().any(|m| html.contains(m)))
        .map(|(id, _)| *id);
    let providers = PROVIDERS
        .iter()
        .filter(|(_, marks)| marks.iter().any(|m| html.contains(m)))
        .map(|(p, _)| *p)
        .collect();
    let host = script_host.trim_end_matches('/');
    let installed = html.contains(&format!("{host}/v.js")) || html.contains("virling.com/v.js");
    Stack {
        platform,
        providers,
        installed,
    }
}

/// True when `url` is https on the default port and, if it names an IP, that IP is public.
fn allowed(url: &Url) -> bool {
    if url.scheme() != "https" || url.port().is_some_and(|p| p != 443) {
        return false;
    }
    match url.host() {
        Some(url::Host::Ipv4(ip)) => is_public(ip.into()),
        Some(url::Host::Ipv6(ip)) => is_public(ip.into()),
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
            if attempt.previous().len() >= 3 || !allowed(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .expect("stack client")
}

/// The homepage of `domain`, at most 2 MB.
pub async fn homepage(client: &Client, domain: &str) -> anyhow::Result<String> {
    let url = Url::parse(&format!("https://{domain}/"))?;
    if !allowed(&url) {
        anyhow::bail!("not a public https host");
    }
    let mut res = client.get(url).send().await?;
    if !allowed(res.url()) {
        anyhow::bail!("redirected off a public https host");
    }
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
    Ok(String::from_utf8_lossy(&out).into_owned())
}

static RECENT: Mutex<Option<HashMap<String, Vec<Instant>>>> = Mutex::new(None);

/// Takes one of the account's homepage checks for this minute; false when it has used them all.
pub fn take(account_id: &str) -> bool {
    let mut guard = RECENT.lock().unwrap();
    let all = guard.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    let seen = all.entry(account_id.to_string()).or_default();
    seen.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
    if seen.len() >= PER_MINUTE {
        return false;
    }
    seen.push(now);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sites_are_detected() {
        let next = r#"<script src="/_next/static/chunks/main.js"></script><script src="https://js.stripe.com/v3"></script>"#;
        let s = detect(next, "https://app.virling.com");
        assert_eq!(s.platform, Some("p-next"));
        assert_eq!(s.providers, [Provider::Stripe]);
        assert!(!s.installed);
        let wp = r#"<link href="/wp-content/themes/x.css"><script defer src="https://app.virling.com/v.js" data-site="a.com"></script>"#;
        let s = detect(wp, "https://app.virling.com/");
        assert_eq!(s.platform, Some("p-wp"));
        assert!(s.installed);
        let framer = r#"<meta name="generator" content="Framer 2a"><a href="https://app.lemonsqueezy.com/checkout">"#;
        let s = detect(framer, "https://app.virling.com");
        assert_eq!(s.platform, Some("p-nocode"));
        assert_eq!(s.providers, [Provider::LemonSqueezy]);
        assert_eq!(detect("<html></html>", "https://x"), Stack::default());
    }

    #[test]
    fn private_and_odd_urls_are_refused() {
        for bad in [
            "https://127.0.0.1/",
            "https://169.254.169.254/latest/meta-data",
            "https://10.0.0.5/",
            "https://[::1]/",
            "http://example.com/",
            "https://example.com:8443/",
        ] {
            assert!(!allowed(&Url::parse(bad).unwrap()), "{bad}");
        }
        assert!(allowed(&Url::parse("https://example.com/").unwrap()));
        assert!(allowed(&Url::parse("https://93.184.216.34/").unwrap()));
    }

    #[tokio::test]
    async fn private_names_never_connect() {
        let c = client();
        for domain in ["127.0.0.1", "169.254.169.254", "10.1.2.3", "localhost"] {
            assert!(homepage(&c, domain).await.is_err(), "{domain}");
        }
    }

    #[test]
    fn checks_are_rate_limited() {
        for _ in 0..PER_MINUTE {
            assert!(take("acct-rate"));
        }
        assert!(!take("acct-rate"));
        assert!(take("acct-other"));
    }
}
