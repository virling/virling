use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Channel {
    Paid,
    Email,
    #[serde(rename = "AI")]
    Ai,
    #[serde(rename = "Organic search")]
    OrganicSearch,
    Social,
    Referral,
    Direct,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Paid => "Paid",
            Channel::Email => "Email",
            Channel::Ai => "AI",
            Channel::OrganicSearch => "Organic search",
            Channel::Social => "Social",
            Channel::Referral => "Referral",
            Channel::Direct => "Direct",
        }
    }
}

const AI: &[&str] = &[
    "chatgpt.com",
    "chat.openai.com",
    "perplexity.ai",
    "claude.ai",
    "gemini.google.com",
    "copilot.microsoft.com",
    "chat.deepseek.com",
];

const SEARCH: &[&str] = &[
    "google.",
    "bing.com",
    "duckduckgo.com",
    "search.yahoo.",
    "yahoo.",
    "ecosia.org",
    "yandex.",
    "baidu.com",
    "search.brave.com",
    "startpage.com",
    "kagi.com",
    "qwant.com",
];

const SOCIAL: &[&str] = &[
    "t.co",
    "x.com",
    "twitter.com",
    "facebook.com",
    "instagram.com",
    "linkedin.com",
    "lnkd.in",
    "reddit.com",
    "news.ycombinator.com",
    "youtube.com",
    "youtu.be",
    "threads.net",
    "bsky.app",
    "mastodon.social",
    "pinterest.com",
    "tiktok.com",
    "producthunt.com",
    "indiehackers.com",
];

const SOCIAL_SOURCES: &[&str] = &[
    "twitter", "x", "facebook", "fb", "instagram", "ig", "linkedin", "reddit", "hackernews", "hn",
    "youtube", "threads", "bluesky", "tiktok", "producthunt",
];

const SEARCH_SOURCES: &[&str] = &["google", "bing", "duckduckgo", "yahoo", "ecosia", "brave"];
const AI_SOURCES: &[&str] = &["chatgpt", "openai", "perplexity", "claude", "gemini", "copilot"];

fn host_matches(host: &str, list: &[&str]) -> bool {
    let host = host.strip_prefix("www.").unwrap_or(host);
    list.iter().any(|entry| {
        if entry.ends_with('.') {
            host.starts_with(entry) || host.contains(&format!(".{entry}"))
        } else {
            host == *entry || host.ends_with(&format!(".{entry}"))
        }
    })
}

/// Applies the PRD channel rules in order. `referrer_host` must already exclude self-referrals.
pub fn classify(utm_medium: Option<&str>, utm_source: Option<&str>, referrer_host: Option<&str>) -> Channel {
    let medium = utm_medium.map(str::to_ascii_lowercase);
    if let Some(m) = medium.as_deref() {
        if matches!(m, "cpc" | "ppc" | "paid" | "paidsearch" | "paid_social" | "paidsocial") {
            return Channel::Paid;
        }
        if matches!(m, "email" | "newsletter" | "e-mail") {
            return Channel::Email;
        }
    }
    if let Some(host) = referrer_host {
        if host_matches(host, AI) {
            return Channel::Ai;
        }
        if host_matches(host, SEARCH) {
            return Channel::OrganicSearch;
        }
        if host_matches(host, SOCIAL) {
            return Channel::Social;
        }
        return Channel::Referral;
    }
    if let Some(source) = utm_source.map(str::to_ascii_lowercase) {
        if AI_SOURCES.contains(&source.as_str()) {
            return Channel::Ai;
        }
        if SEARCH_SOURCES.contains(&source.as_str()) {
            return Channel::OrganicSearch;
        }
        if SOCIAL_SOURCES.contains(&source.as_str()) {
            return Channel::Social;
        }
        return Channel::Referral;
    }
    if medium.is_some() {
        return Channel::Referral;
    }
    Channel::Direct
}

/// Human name for a referrer host, used in breakdowns ("news.ycombinator.com" -> "Hacker News").
pub fn source_name(host: &str) -> String {
    let h = host.strip_prefix("www.").unwrap_or(host);
    let known = [
        ("news.ycombinator.com", "Hacker News"),
        ("t.co", "X"),
        ("x.com", "X"),
        ("twitter.com", "X"),
        ("chatgpt.com", "ChatGPT"),
        ("chat.openai.com", "ChatGPT"),
        ("perplexity.ai", "Perplexity"),
        ("claude.ai", "Claude"),
        ("gemini.google.com", "Gemini"),
        ("reddit.com", "Reddit"),
        ("old.reddit.com", "Reddit"),
        ("linkedin.com", "LinkedIn"),
        ("lnkd.in", "LinkedIn"),
        ("facebook.com", "Facebook"),
        ("l.facebook.com", "Facebook"),
        ("instagram.com", "Instagram"),
        ("youtube.com", "YouTube"),
        ("bing.com", "Bing"),
        ("duckduckgo.com", "DuckDuckGo"),
        ("producthunt.com", "Product Hunt"),
        ("indiehackers.com", "Indie Hackers"),
        ("github.com", "GitHub"),
    ];
    if let Some((_, name)) = known.iter().find(|(k, _)| *k == h) {
        return (*name).to_string();
    }
    if h.starts_with("google.") || h.contains(".google.") {
        return "Google".to_string();
    }
    h.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_apply_in_order() {
        assert_eq!(classify(Some("cpc"), Some("google"), Some("google.com")), Channel::Paid);
        assert_eq!(classify(Some("Newsletter"), None, None), Channel::Email);
        assert_eq!(classify(None, None, Some("chatgpt.com")), Channel::Ai);
        assert_eq!(classify(None, None, Some("www.google.co.in")), Channel::OrganicSearch);
        assert_eq!(classify(None, None, Some("news.ycombinator.com")), Channel::Social);
        assert_eq!(classify(None, None, Some("old.reddit.com")), Channel::Social);
        assert_eq!(classify(None, None, Some("someblog.dev")), Channel::Referral);
        assert_eq!(classify(None, None, None), Channel::Direct);
    }

    #[test]
    fn utm_source_used_without_referrer() {
        assert_eq!(classify(None, Some("twitter"), None), Channel::Social);
        assert_eq!(classify(Some("social"), Some("partner-site"), None), Channel::Referral);
    }

    #[test]
    fn hosts_do_not_match_by_substring() {
        assert_eq!(classify(None, None, Some("notx.com")), Channel::Referral);
        assert_eq!(classify(None, None, Some("mychatgpt.com.evil.io")), Channel::Referral);
    }

    #[test]
    fn source_names() {
        assert_eq!(source_name("news.ycombinator.com"), "Hacker News");
        assert_eq!(source_name("www.google.de"), "Google");
        assert_eq!(source_name("someblog.dev"), "someblog.dev");
    }
}
