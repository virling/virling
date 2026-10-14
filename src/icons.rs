//! Source logos, channel and provider glyphs, and flags, all shipped in the binary.
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::chart::esc;
use crate::flags;

const BRANDS: &[(&str, &str, &str)] = &[
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
        "instagram",
        "Instagram",
        include_str!("../assets/icons/instagram.svg"),
    ),
    (
        "lemonsqueezy",
        "Lemon Squeezy",
        include_str!("../assets/icons/lemonsqueezy.svg"),
    ),
    (
        "linkedin",
        "LinkedIn",
        include_str!("../assets/icons/linkedin.svg"),
    ),
    ("polar", "Polar", include_str!("../assets/icons/polar.svg")),
    (
        "producthunt",
        "Product Hunt",
        include_str!("../assets/icons/producthunt.svg"),
    ),
    (
        "reddit",
        "Reddit",
        include_str!("../assets/icons/reddit.svg"),
    ),
    (
        "stripe",
        "Stripe",
        include_str!("../assets/icons/stripe.svg"),
    ),
    ("x", "X", include_str!("../assets/icons/x.svg")),
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
        "x" | "x.com" | "t.co" | "twitter.com" => "x",
        "reddit" | "reddit.com" | "old.reddit.com" => "reddit",
        "hacker news" | "news.ycombinator.com" => "ycombinator",
        "linkedin" | "linkedin.com" | "lnkd.in" => "linkedin",
        "youtube" | "youtube.com" | "youtu.be" => "youtube",
        "github" | "github.com" => "github",
        "product hunt" | "producthunt.com" => "producthunt",
        "facebook" | "facebook.com" | "l.facebook.com" => "facebook",
        "instagram" | "instagram.com" | "l.instagram.com" => "instagram",
        "duckduckgo" | "duckduckgo.com" => "duckduckgo",
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
        "direct" => r#"<path d="M5 3l14 8-6 2-3 6z"/>"#,
        "email" => r#"<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 7l9 6 9-6"/>"#,
        "unattributed" => {
            r#"<circle cx="12" cy="12" r="9" stroke-dasharray="3 3"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .8-1 1.5v.2M12 17h.01"/>"#
        }
        "search" => r#"<circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/>"#,
        "link" => {
            r#"<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1"/><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>"#
        }
        "social" => {
            r#"<circle cx="6" cy="12" r="2.5"/><circle cx="18" cy="6" r="2.5"/><circle cx="18" cy="18" r="2.5"/><path d="M8.2 11l7.6-3.8M8.2 13l7.6 3.8"/>"#
        }
        "paid" => r#"<path d="M20 12l-8 8-9-9V3h8z"/><circle cx="7.5" cy="7.5" r="1.5"/>"#,
        "ai" => r#"<path d="M12 3l2 5 5 2-5 2-2 5-2-5-5-2 5-2z"/>"#,
        "card" => r#"<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M2 10h20M6 15h4"/>"#,
        _ => "",
    }
}

/// A stroked 24x24 glyph: direct, email, unattributed, search, link, social, paid, ai or card.
pub fn glyph(name: &str, size: u32, color: &str) -> String {
    format!(
        r#"<svg class="ico" width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="{color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{}</svg>"#,
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
        "Paid" => "paid",
        "AI" => "ai",
        "Unattributed" => "unattributed",
        _ => return None,
    })
}

/// Logo for a source name, referrer host or channel; a letter tile when there is none.
pub fn source(name: &str, size: u32) -> String {
    if let Some(slug) = brand_slug(name) {
        return brand(slug, size);
    }
    match name {
        "Unattributed" => return glyph("unattributed", size, "#B9BCC2"),
        "Newsletter" => return glyph("email", size, "#6B717C"),
        _ => {}
    }
    if let Some(g) = channel_glyph(name) {
        return glyph(g, size, "#6B717C");
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

pub fn provider(provider: &str, size: u32) -> String {
    match provider {
        "stripe" | "lemonsqueezy" | "dodo" | "polar" => brand(provider, size),
        _ => glyph("card", size, "#0E1116"),
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
        assert!(source("Direct", 14).contains("M5 3l14 8"));
        assert!(source("Unattributed", 14).contains("#B9BCC2"));
        assert!(source("<b>log", 14).contains(">&lt;</span>"));
        assert!(source("someblog.dev", 14).contains(">S</span>"));
        assert!(provider("stripe", 14).contains("Stripe"));
        assert!(provider("other", 14).contains("M2 10h20"));
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
