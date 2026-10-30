//! Source logos, channel and provider glyphs, and flags, all shipped in the binary.
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::chart::esc;
use crate::flags;

const BRANDS: &[(&str, &str, &str)] = &[
    (
        "amazon",
        "Amazon",
        include_str!("../assets/icons/amazon.svg"),
    ),
    (
        "anthropic",
        "Anthropic",
        include_str!("../assets/icons/anthropic.svg"),
    ),
    ("apple", "Apple", include_str!("../assets/icons/apple.svg")),
    ("baidu", "Baidu", include_str!("../assets/icons/baidu.svg")),
    ("bing", "Bing", include_str!("../assets/icons/bing.svg")),
    (
        "bluesky",
        "Bluesky",
        include_str!("../assets/icons/bluesky.svg"),
    ),
    ("brave", "Brave", include_str!("../assets/icons/brave.svg")),
    (
        "bytedance",
        "ByteDance",
        include_str!("../assets/icons/bytedance.svg"),
    ),
    (
        "chatgpt",
        "ChatGPT",
        include_str!("../assets/icons/chatgpt.svg"),
    ),
    (
        "claude",
        "Claude",
        include_str!("../assets/icons/claude.svg"),
    ),
    (
        "commoncrawl",
        "Common Crawl",
        include_str!("../assets/icons/commoncrawl.svg"),
    ),
    (
        "copilot",
        "Copilot",
        include_str!("../assets/icons/copilot.svg"),
    ),
    ("creem", "Creem", include_str!("../assets/icons/creem.svg")),
    (
        "deepseek",
        "DeepSeek",
        include_str!("../assets/icons/deepseek.svg"),
    ),
    (
        "devdotto",
        "DEV",
        include_str!("../assets/icons/devdotto.svg"),
    ),
    (
        "discord",
        "Discord",
        include_str!("../assets/icons/discord.svg"),
    ),
    (
        "dodo",
        "Dodo Payments",
        include_str!("../assets/icons/dodo.svg"),
    ),
    (
        "duckduckgo",
        "DuckDuckGo",
        include_str!("../assets/icons/duckduckgo.svg"),
    ),
    (
        "ecosia",
        "Ecosia",
        include_str!("../assets/icons/ecosia.svg"),
    ),
    (
        "facebook",
        "Facebook",
        include_str!("../assets/icons/facebook.svg"),
    ),
    (
        "github",
        "GitHub",
        include_str!("../assets/icons/github.svg"),
    ),
    (
        "google",
        "Google",
        include_str!("../assets/icons/google.svg"),
    ),
    (
        "googlegemini",
        "Gemini",
        include_str!("../assets/icons/googlegemini.svg"),
    ),
    (
        "hashnode",
        "Hashnode",
        include_str!("../assets/icons/hashnode.svg"),
    ),
    (
        "indiehackers",
        "Indie Hackers",
        include_str!("../assets/icons/indiehackers.svg"),
    ),
    (
        "instagram",
        "Instagram",
        include_str!("../assets/icons/instagram.svg"),
    ),
    ("kagi", "Kagi", include_str!("../assets/icons/kagi.svg")),
    ("meta", "Meta", include_str!("../assets/icons/meta.svg")),
    (
        "lemonsqueezy",
        "Lemon Squeezy",
        include_str!("../assets/icons/lemonsqueezy.svg"),
    ),
    (
        "lobsters",
        "Lobsters",
        include_str!("../assets/icons/lobsters.svg"),
    ),
    (
        "linkedin",
        "LinkedIn",
        include_str!("../assets/icons/linkedin.svg"),
    ),
    (
        "mastodon",
        "Mastodon",
        include_str!("../assets/icons/mastodon.svg"),
    ),
    (
        "medium",
        "Medium",
        include_str!("../assets/icons/medium.svg"),
    ),
    (
        "paddle",
        "Paddle",
        include_str!("../assets/icons/paddle.svg"),
    ),
    (
        "perplexity",
        "Perplexity",
        include_str!("../assets/icons/perplexity.svg"),
    ),
    (
        "pinterest",
        "Pinterest",
        include_str!("../assets/icons/pinterest.svg"),
    ),
    ("polar", "Polar", include_str!("../assets/icons/polar.svg")),
    (
        "producthunt",
        "Product Hunt",
        include_str!("../assets/icons/producthunt.svg"),
    ),
    ("qwant", "Qwant", include_str!("../assets/icons/qwant.svg")),
    (
        "reddit",
        "Reddit",
        include_str!("../assets/icons/reddit.svg"),
    ),
    ("slack", "Slack", include_str!("../assets/icons/slack.svg")),
    (
        "startpage",
        "Startpage",
        include_str!("../assets/icons/startpage.svg"),
    ),
    (
        "stripe",
        "Stripe",
        include_str!("../assets/icons/stripe.svg"),
    ),
    (
        "substack",
        "Substack",
        include_str!("../assets/icons/substack.svg"),
    ),
    (
        "telegram",
        "Telegram",
        include_str!("../assets/icons/telegram.svg"),
    ),
    (
        "threads",
        "Threads",
        include_str!("../assets/icons/threads.svg"),
    ),
    (
        "tiktok",
        "TikTok",
        include_str!("../assets/icons/tiktok.svg"),
    ),
    (
        "whatsapp",
        "WhatsApp",
        include_str!("../assets/icons/whatsapp.svg"),
    ),
    ("x", "X", include_str!("../assets/icons/x.svg")),
    ("yahoo", "Yahoo", include_str!("../assets/icons/yahoo.svg")),
    (
        "yandex",
        "Yandex",
        include_str!("../assets/icons/yandex.svg"),
    ),
    (
        "ycombinator",
        "Hacker News",
        include_str!("../assets/icons/ycombinator.svg"),
    ),
    (
        "youtube",
        "YouTube",
        include_str!("../assets/icons/youtube.svg"),
    ),
];

/// Simple Icons slug for a referrer host or a source name.
pub fn brand_slug(source: &str) -> Option<&'static str> {
    let s = source.trim().to_ascii_lowercase();
    let h = s.strip_prefix("www.").unwrap_or(&s);
    let h = h.strip_prefix("m.").unwrap_or(h);
    Some(match h {
        "google" => "google",
        _ if h.starts_with("google.") || h.contains(".google.") => "google",
        "x" | "twitter" | "x.com" | "t.co" | "twitter.com" => "x",
        "reddit" | "reddit.com" | "old.reddit.com" => "reddit",
        "hacker news" | "hackernews" | "hn" | "ycombinator" | "news.ycombinator.com" => {
            "ycombinator"
        }
        "lobsters" | "lobste.rs" => "lobsters",
        "linkedin" | "linkedin.com" | "lnkd.in" => "linkedin",
        "youtube" | "youtube.com" | "youtu.be" => "youtube",
        "github" | "github.com" => "github",
        "product hunt" | "producthunt" | "producthunt.com" => "producthunt",
        "facebook" | "fb" | "facebook.com" | "l.facebook.com" => "facebook",
        "instagram" | "ig" | "instagram.com" | "l.instagram.com" => "instagram",
        "duckduckgo" | "duckduckgo.com" => "duckduckgo",
        "claude" | "claude.ai" => "claude",
        "chatgpt" | "openai" | "chatgpt.com" | "chat.openai.com" | "openai.com" => "chatgpt",
        "copilot" | "copilot.microsoft.com" => "copilot",
        "deepseek" | "deepseek.com" | "chat.deepseek.com" => "deepseek",
        "perplexity" | "perplexity.ai" => "perplexity",
        "gemini" | "gemini.google.com" => "googlegemini",
        "bluesky" | "bsky.app" | "go.bsky.app" => "bluesky",
        "threads" | "threads.com" | "threads.net" | "l.threads.com" => "threads",
        "mastodon" | "mastodon.social" | "mastodon.online" | "mstdn.social" => "mastodon",
        "tiktok" | "tiktok.com" => "tiktok",
        "pinterest" | "pinterest.com" | "pin.it" => "pinterest",
        "discord" | "discord.com" | "discordapp.com" | "discord.gg" => "discord",
        "slack" | "slack.com" | "app.slack.com" => "slack",
        "telegram" | "t.me" | "telegram.org" | "web.telegram.org" => "telegram",
        "whatsapp" | "whatsapp.com" | "web.whatsapp.com" | "wa.me" => "whatsapp",
        "substack" | "substack.com" => "substack",
        _ if h.ends_with(".substack.com") => "substack",
        "medium" | "medium.com" => "medium",
        "dev" | "devto" | "dev.to" => "devdotto",
        "hashnode" | "hashnode.com" | "hashnode.dev" => "hashnode",
        _ if h.ends_with(".hashnode.dev") => "hashnode",
        "indie hackers" | "indiehackers" | "indiehackers.com" => "indiehackers",
        "brave" | "search.brave.com" => "brave",
        "ecosia" | "ecosia.org" => "ecosia",
        "baidu" | "baidu.com" => "baidu",
        "bing" | "bing.com" => "bing",
        _ if h.ends_with(".bing.com") => "bing",
        "yahoo" => "yahoo",
        _ if h.starts_with("yahoo.") || h.contains(".yahoo.") => "yahoo",
        "yandex" => "yandex",
        _ if h.starts_with("yandex.") || h.contains(".yandex.") => "yandex",
        "kagi" | "kagi.com" => "kagi",
        "anthropic" | "anthropic.com" => "anthropic",
        "meta" | "meta.com" => "meta",
        "amazon" | "amazon.com" => "amazon",
        "apple" | "apple.com" => "apple",
        "bytedance" | "bytedance.com" => "bytedance",
        "common crawl" | "commoncrawl" | "commoncrawl.org" => "commoncrawl",
        "qwant" | "qwant.com" => "qwant",
        "startpage" | "startpage.com" => "startpage",
        _ => return None,
    })
}

pub fn brand(slug: &str, size: u32) -> String {
    let Some((_, name, svg)) = BRANDS.iter().find(|b| b.0 == slug) else {
        return String::new();
    };
    static IDS: AtomicUsize = AtomicUsize::new(0);
    let svg = if svg.contains("mask0_") {
        let n = IDS.fetch_add(1, Ordering::Relaxed);
        svg.replace("mask0_", &format!("{slug}{n}_"))
    } else {
        svg.to_string()
    };
    svg.trim_end().replacen(
        "<svg ",
        &format!(
            r#"<svg class="ico" width="{size}" height="{size}" role="img" aria-label="{name}" "#
        ),
        1,
    )
}

fn glyph_path(name: &str) -> &'static str {
    match name {
        "direct" => r#"<circle cx="12" cy="12" r="8"/><path d="M6.3 17.7L17.7 6.3"/>"#,
        "email" => {
            r#"<rect x="3" y="5.5" width="18" height="13" rx="2.5"/><path d="M4 7l8 6 8-6"/>"#
        }
        "unattributed" => {
            r#"<circle cx="12" cy="12" r="9" stroke-dasharray="3 3"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .8-1 1.5v.2M12 17h.01"/>"#
        }
        "search" => r#"<circle cx="10.5" cy="10.5" r="6"/><path d="M15 15l5 5"/>"#,
        "link" => {
            r#"<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>"#
        }
        "social" => {
            r#"<circle cx="6" cy="12" r="2.5"/><circle cx="18" cy="6" r="2.5"/><circle cx="18" cy="18" r="2.5"/><path d="M8.2 11l7.6-3.8M8.2 13l7.6 3.8"/>"#
        }
        "community" => {
            r#"<path d="M14 10a6 6 0 0 1-8.7 5.4L2 16.5l1-3.2A6 6 0 1 1 14 10z"/><path d="M16.5 8.6A6 6 0 0 1 21 18.3l1 3.2-3.3-1.1A6 6 0 0 1 10.6 18"/>"#
        }
        "paid" => r#"<path d="M20 12l-8 8-9-9V3h8z"/><circle cx="7.5" cy="7.5" r="1.5"/>"#,
        "ai" => {
            r#"<path d="M12 3l2.2 5.6L20 11l-5.8 2.4L12 19l-2.2-5.6L4 11l5.8-2.4zM19 17v4M17 19h4"/>"#
        }
        "card" => {
            r#"<rect x="3" y="5.5" width="18" height="13" rx="2.5"/><path d="M3 10h18M7 14.5h3"/>"#
        }
        "desktop" => {
            r#"<rect x="3" y="4.5" width="18" height="12" rx="2"/><path d="M9 20.5h6M12 16.5v4"/>"#
        }
        "mobile" => r#"<rect x="7" y="3" width="10" height="18" rx="2.5"/><path d="M11 17.5h2"/>"#,
        "tablet" => r#"<rect x="4" y="2" width="16" height="20" rx="2"/><path d="M11 18h2"/>"#,
        "windows" => r#"<path d="M3 5l8-1v7H3zM13 4l8-1v8h-8zM3 13h8v7l-8-1zM13 13h8v8l-8-1z"/>"#,
        "page" => {
            r#"<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>"#
        }
        "pin" => {
            r#"<path d="M12 21s-7-6.2-7-11a7 7 0 0 1 14 0c0 4.8-7 11-7 11z"/><circle cx="12" cy="10" r="2.5"/>"#
        }
        "language" => {
            r#"<path d="M4 5h9M8.5 3v2M6 5c0 4 3 7 6 8M11 5c0 3-2.5 7-6 9"/><path d="M13 21l4-9 4 9M14.5 18h5"/>"#
        }
        "calendar" => {
            r#"<rect x="4" y="5" width="16" height="15" rx="2.5"/><path d="M4 10h16M8.5 3v4M15.5 3v4"/>"#
        }
        "globe" => {
            r#"<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.4 2.4 3.4 5.2 3.4 8.5s-1 6.1-3.4 8.5c-2.4-2.4-3.4-5.2-3.4-8.5s1-6.1 3.4-8.5z"/>"#
        }
        _ => "",
    }
}

/// A stroked 24x24 glyph: direct, email, unattributed, search, link, social, community, paid, ai, card, desktop, mobile, tablet, windows, page, pin, language or globe.
pub fn glyph(name: &str, size: u32, color: &str) -> String {
    format!(
        r#"<svg class="ico" width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" style="color:{color}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{}</svg>"#,
        glyph_path(name)
    )
}

pub fn channel_glyph(channel: &str) -> Option<&'static str> {
    Some(match channel {
        "Direct" => "direct",
        "Email" => "email",
        "Organic search" => "search",
        "Referral" => "link",
        "Social" => "social",
        "Communities" => "community",
        "Paid" => "paid",
        "AI" => "ai",
        "Unattributed" => "unattributed",
        _ => return None,
    })
}

/// Logo for a source name, referrer host or channel, else the host's cached favicon, else a letter tile.
pub fn source(name: &str, size: u32) -> String {
    if let Some(slug) = brand_slug(name) {
        return brand(slug, size);
    }
    match name {
        "Unattributed" => return glyph("unattributed", size, "var(--ink-4)"),
        _ if name.eq_ignore_ascii_case("newsletter") => {
            return glyph("email", size, "var(--ink-2)");
        }
        _ => {}
    }
    if let Some(g) = channel_glyph(name) {
        return glyph(g, size, "var(--ink-2)");
    }
    if let Some(host) = crate::favicon::source_host(name)
        && let Some(v) = crate::favicon::source_icon(&host)
    {
        return format!(
            r#"<img class="ico" src="/source-icons/{}?v={v}" width="{size}" height="{size}" alt="">"#,
            esc(&host)
        );
    }
    let letter = name
        .trim_start_matches("www.")
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    format!(
        r#"<span class="letter" style="width:{size}px;height:{size}px;font-size:{}px" aria-hidden="true">{}</span>"#,
        size.saturating_sub(6).max(9),
        esc(&letter)
    )
}

/// Path of an image file for `source`'s logo, for email where SVG can't be inlined; empty when there is none.
pub fn source_src(name: &str) -> String {
    if let Some(slug) = brand_slug(name) {
        return format!("/assets/icons/{slug}.svg");
    }
    let g = match name {
        "Unattributed" => Some("unattributed"),
        "Newsletter" => Some("email"),
        _ => channel_glyph(name),
    };
    if let Some(g) = g {
        return format!("/assets/icons/glyph-{g}.svg");
    }
    match crate::favicon::source_host(name) {
        Some(host) => crate::favicon::source_icon(&host)
            .map(|v| format!("/source-icons/{host}?v={v}"))
            .unwrap_or_default(),
        None => String::new(),
    }
}

/// The file behind /assets/icons/{name}.svg: a browser or OS logo, a source logo, or "glyph-{name}".
pub fn file(name: &str) -> Option<String> {
    if let Some((_, svg)) = UA_ICONS.iter().find(|(s, _)| *s == name) {
        return Some(svg.to_string());
    }
    if let Some((_, _, svg)) = BRANDS.iter().find(|b| b.0 == name) {
        return Some(svg.to_string());
    }
    let path = glyph_path(name.strip_prefix("glyph-")?);
    (!path.is_empty()).then(|| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#4B515C" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{path}</svg>"##
        )
    })
}

/// Browser and OS logos, served as files since a list repeats them on every row.
pub const UA_ICONS: [(&str, &str); 7] = [
    (
        "googlechrome",
        include_str!("../assets/icons/googlechrome.svg"),
    ),
    ("safari", include_str!("../assets/icons/safari.svg")),
    (
        "firefoxbrowser",
        include_str!("../assets/icons/firefoxbrowser.svg"),
    ),
    ("opera", include_str!("../assets/icons/opera.svg")),
    ("apple", include_str!("../assets/icons/apple.svg")),
    ("android", include_str!("../assets/icons/android.svg")),
    ("linux", include_str!("../assets/icons/linux.svg")),
];

/// Logo for a browser or operating system name; a plain glyph for the rest.
pub fn ua(name: &str) -> String {
    let slug = match name {
        "Chrome" | "ChromeOS" => "googlechrome",
        "Safari" => "safari",
        "Firefox" => "firefoxbrowser",
        "Opera" => "opera",
        "macOS" | "iOS" => "apple",
        "Android" => "android",
        "Linux" => "linux",
        "Windows" => return glyph("windows", 12, "var(--ink-2)"),
        _ => return glyph("globe", 12, "var(--ink-2)"),
    };
    format!(r#"<img class="ico" src="/assets/icons/{slug}.svg" width="12" height="12" alt="">"#)
}

/// Glyph for Desktop, Mobile or Tablet; nothing when unknown.
pub fn device(name: &str) -> String {
    match name {
        "Desktop" | "Mobile" | "Tablet" => glyph(&name.to_ascii_lowercase(), 12, "var(--ink-2)"),
        _ => String::new(),
    }
}

pub fn provider(provider: &str, size: u32) -> String {
    match provider {
        "stripe" | "lemonsqueezy" | "dodo" | "polar" | "creem" | "paddle" => brand(provider, size),
        _ => glyph("card", size, "var(--ink)"),
    }
}

/// A flag image for an uppercase ISO code, or nothing when the code is unknown.
pub fn flag(code: &str) -> String {
    match flags::lookup(code) {
        Some((name, _)) => format!(
            r#"<img class="flag" src="/assets/flags/{}.svg" width="16" height="12" alt="{} flag">"#,
            code.to_ascii_lowercase(),
            esc(name)
        ),
        None => String::new(),
    }
}

/// English name for an uppercase ISO code, else the code itself.
pub fn country(code: &str) -> String {
    flags::lookup(code).map_or_else(|| code.to_string(), |(name, _)| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_source_has_a_bundled_logo() {
        for s in crate::channel::known_sources() {
            let icon = source(&s, 16);
            assert!(icon.contains(r#"role="img""#), "{s}: {icon}");
        }
    }

    #[test]
    fn hosts_and_names_map_to_logos() {
        for (input, slug) in [
            ("news.ycombinator.com", "ycombinator"),
            ("Hacker News", "ycombinator"),
            ("t.co", "x"),
            ("X", "x"),
            ("www.google.de", "google"),
            ("Google", "google"),
            ("m.facebook.com", "facebook"),
            ("lnkd.in", "linkedin"),
            ("Product Hunt", "producthunt"),
            ("claude.ai", "claude"),
            ("Gemini", "googlegemini"),
            ("go.bsky.app", "bluesky"),
            ("someone.substack.com", "substack"),
            ("dev.to", "devdotto"),
            ("search.brave.com", "brave"),
        ] {
            assert_eq!(brand_slug(input), Some(slug), "{input}");
        }
        assert_eq!(brand_slug("someblog.dev"), None);
        for (slug, ..) in BRANDS {
            assert!(brand(slug, 16).starts_with(r#"<svg class="ico" width="16""#));
        }
    }

    #[test]
    fn provider_logos_get_unique_mask_ids() {
        let (a, b) = (provider("dodo", 14), provider("dodo", 14));
        assert!(a.contains(r#"aria-label="Dodo Payments""#));
        assert!(provider("polar", 14).contains(r#"aria-label="Polar""#));
        assert!(provider("creem", 14).contains(r#"aria-label="Creem""#));
        assert!(provider("paddle", 14).contains(r#"aria-label="Paddle""#));
        let id = |s: &str| {
            s.split("mask id=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_string()
        };
        assert_ne!(id(&a), id(&b));
        assert!(a.contains(&format!("url(#{})", id(&a))));
    }

    #[test]
    fn sources_fall_back_to_glyphs_and_letters() {
        assert!(source("Hacker News", 14).contains(r#"aria-label="Hacker News""#));
        assert!(source("Direct", 14).contains("M6.3 17.7L17.7 6.3"));
        assert!(source("Unattributed", 14).contains("var(--ink-4)"));
        assert!(source("<b>log", 14).contains(">&lt;</span>"));
        assert!(source("someblog.dev", 14).contains(">S</span>"));
        assert!(provider("stripe", 14).contains("Stripe"));
        assert!(provider("other", 14).contains("M3 10h18"));
    }

    #[test]
    fn flags_cover_iso_codes() {
        assert!(flags::FLAGS.windows(2).all(|w| w[0].0 < w[1].0), "sorted");
        assert_eq!(country("US"), "United States");
        assert_eq!(country("DE"), "Germany");
        assert_eq!(country("ZZ"), "ZZ");
        assert!(flag("IN").contains(r#"src="/assets/flags/in.svg""#));
        assert_eq!(flag("ZZ"), "");
        assert!(flags::lookup("GB").unwrap().1.starts_with("<svg"));
    }
}
