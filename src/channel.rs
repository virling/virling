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
    "threads.com",
    "bsky.app",
    "mastodon.social",
    "mastodon.online",
    "mstdn.social",
    "pinterest.com",
    "pin.it",
    "tiktok.com",
    "producthunt.com",
    "indiehackers.com",
    "discord.com",
    "discordapp.com",
    "discord.gg",
    "slack.com",
    "t.me",
    "telegram.org",
    "whatsapp.com",
    "wa.me",
    "substack.com",
    "medium.com",
    "dev.to",
    "hashnode.com",
    "hashnode.dev",
];

const SOCIAL_SOURCES: &[&str] = &[
    "twitter",
    "x",
    "facebook",
    "fb",
    "instagram",
    "ig",
    "linkedin",
    "reddit",
    "hackernews",
    "hn",
    "youtube",
    "threads",
    "bluesky",
    "mastodon",
    "tiktok",
    "pinterest",
    "producthunt",
    "discord",
    "slack",
    "telegram",
    "whatsapp",
    "substack",
    "medium",
    "devto",
    "hashnode",
    "indiehackers",
];

const SEARCH_SOURCES: &[&str] = &[
    "google",
    "bing",
    "duckduckgo",
    "yahoo",
    "ecosia",
    "brave",
    "yandex",
    "baidu",
];
const AI_SOURCES: &[&str] = &[
    "chatgpt",
    "openai",
    "perplexity",
    "claude",
    "gemini",
    "copilot",
];

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

/// A utm_source that names a known source host ("chatgpt.com"), to stand in for a missing referrer.
pub fn source_host(utm_source: &str) -> Option<String> {
    let s = utm_source.to_ascii_lowercase();
    [AI, SEARCH, SOCIAL]
        .iter()
        .any(|list| host_matches(&s, list))
        .then_some(s)
}

/// Applies the PRD channel rules in order. `referrer_host` must already exclude self-referrals.
pub fn classify(
    utm_medium: Option<&str>,
    utm_source: Option<&str>,
    referrer_host: Option<&str>,
) -> Channel {
    let medium = utm_medium.map(str::to_ascii_lowercase);
    if let Some(m) = medium.as_deref() {
        if matches!(
            m,
            "cpc" | "ppc" | "paid" | "paidsearch" | "paid_social" | "paidsocial"
        ) {
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
        let s = source.as_str();
        if AI_SOURCES.contains(&s) || host_matches(s, AI) {
            return Channel::Ai;
        }
        if SEARCH_SOURCES.contains(&s) || host_matches(s, SEARCH) {
            return Channel::OrganicSearch;
        }
        if SOCIAL_SOURCES.contains(&s) || host_matches(s, SOCIAL) {
            return Channel::Social;
        }
        return Channel::Referral;
    }
    if medium.is_some() {
        return Channel::Referral;
    }
    Channel::Direct
}

/// Canonical domain for a referrer host, so aliases share one row ("t.co" -> "x.com").
pub fn source_domain(host: &str) -> String {
    let h = host.strip_prefix("www.").unwrap_or(host);
    let alias = [
        ("t.co", "x.com"),
        ("twitter.com", "x.com"),
        ("lnkd.in", "linkedin.com"),
        ("old.reddit.com", "reddit.com"),
        ("chat.openai.com", "chatgpt.com"),
        ("l.facebook.com", "facebook.com"),
        ("threads.net", "threads.com"),
        ("l.threads.com", "threads.com"),
        ("go.bsky.app", "bsky.app"),
        ("pin.it", "pinterest.com"),
        ("discordapp.com", "discord.com"),
        ("discord.gg", "discord.com"),
        ("app.slack.com", "slack.com"),
        ("web.telegram.org", "t.me"),
        ("web.whatsapp.com", "whatsapp.com"),
        ("wa.me", "whatsapp.com"),
    ];
    if let Some((_, domain)) = alias.iter().find(|(k, _)| *k == h) {
        return (*domain).to_string();
    }
    if h != "gemini.google.com" && (h.starts_with("google.") || h.contains(".google.")) {
        return "google.com".to_string();
    }
    h.to_string()
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
        ("copilot.microsoft.com", "Copilot"),
        ("chat.deepseek.com", "DeepSeek"),
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
        ("bsky.app", "Bluesky"),
        ("go.bsky.app", "Bluesky"),
        ("threads.com", "Threads"),
        ("threads.net", "Threads"),
        ("mastodon.social", "Mastodon"),
        ("tiktok.com", "TikTok"),
        ("pinterest.com", "Pinterest"),
        ("discord.com", "Discord"),
        ("slack.com", "Slack"),
        ("app.slack.com", "Slack"),
        ("t.me", "Telegram"),
        ("web.telegram.org", "Telegram"),
        ("whatsapp.com", "WhatsApp"),
        ("web.whatsapp.com", "WhatsApp"),
        ("substack.com", "Substack"),
        ("medium.com", "Medium"),
        ("dev.to", "DEV"),
        ("hashnode.com", "Hashnode"),
        ("search.brave.com", "Brave"),
        ("ecosia.org", "Ecosia"),
        ("baidu.com", "Baidu"),
    ];
    if let Some((_, name)) = known.iter().find(|(k, _)| *k == h) {
        return (*name).to_string();
    }
    if h.starts_with("google.") || h.contains(".google.") {
        return "Google".to_string();
    }
    if h.starts_with("yandex.") {
        return "Yandex".to_string();
    }
    h.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_apply_in_order() {
        assert_eq!(
            classify(Some("cpc"), Some("google"), Some("google.com")),
            Channel::Paid
        );
        assert_eq!(classify(Some("Newsletter"), None, None), Channel::Email);
        assert_eq!(classify(None, None, Some("chatgpt.com")), Channel::Ai);
        assert_eq!(
            classify(None, None, Some("www.google.co.in")),
            Channel::OrganicSearch
        );
        assert_eq!(
            classify(None, None, Some("news.ycombinator.com")),
            Channel::Social
        );
        assert_eq!(
            classify(None, None, Some("old.reddit.com")),
            Channel::Social
        );
        assert_eq!(
            classify(None, None, Some("someblog.dev")),
            Channel::Referral
        );
        assert_eq!(classify(None, None, None), Channel::Direct);
    }

    #[test]
    fn utm_source_used_without_referrer() {
        assert_eq!(classify(None, Some("twitter"), None), Channel::Social);
        assert_eq!(
            classify(Some("social"), Some("partner-site"), None),
            Channel::Referral
        );
    }

    #[test]
    fn ai_sources_get_their_own_channel() {
        for host in [
            "chatgpt.com",
            "chat.openai.com",
            "www.perplexity.ai",
            "claude.ai",
            "gemini.google.com",
            "copilot.microsoft.com",
        ] {
            assert_eq!(classify(None, None, Some(host)), Channel::Ai, "{host}");
        }
        for source in ["chatgpt.com", "ChatGPT", "perplexity", "claude.ai"] {
            assert_eq!(classify(None, Some(source), None), Channel::Ai, "{source}");
        }
        assert_eq!(
            classify(Some("cpc"), Some("chatgpt.com"), None),
            Channel::Paid
        );
    }

    #[test]
    fn social_chat_and_writing_hosts_are_social() {
        for host in [
            "bsky.app",
            "go.bsky.app",
            "www.threads.com",
            "threads.net",
            "mastodon.social",
            "www.tiktok.com",
            "pinterest.com",
            "discord.com",
            "app.slack.com",
            "t.me",
            "web.telegram.org",
            "web.whatsapp.com",
            "someone.substack.com",
            "medium.com",
            "dev.to",
            "blog.hashnode.dev",
            "www.indiehackers.com",
        ] {
            assert_eq!(classify(None, None, Some(host)), Channel::Social, "{host}");
        }
        for source in ["bluesky", "discord", "substack", "dev.to"] {
            assert_eq!(
                classify(None, Some(source), None),
                Channel::Social,
                "{source}"
            );
        }
    }

    #[test]
    fn more_search_engines() {
        for host in [
            "search.brave.com",
            "yandex.ru",
            "www.ecosia.org",
            "www.baidu.com",
        ] {
            assert_eq!(
                classify(None, None, Some(host)),
                Channel::OrganicSearch,
                "{host}"
            );
        }
        assert_eq!(classify(None, Some("yandex"), None), Channel::OrganicSearch);
    }

    #[test]
    fn utm_sources_naming_known_hosts() {
        assert_eq!(source_host("ChatGPT.com").as_deref(), Some("chatgpt.com"));
        assert_eq!(
            source_host("perplexity.ai").as_deref(),
            Some("perplexity.ai")
        );
        assert_eq!(source_host("chatgpt"), None);
        assert_eq!(source_host("newsletter"), None);
        assert_eq!(source_host("partner.io"), None);
    }

    #[test]
    fn hosts_do_not_match_by_substring() {
        assert_eq!(classify(None, None, Some("notx.com")), Channel::Referral);
        assert_eq!(
            classify(None, None, Some("mychatgpt.com.evil.io")),
            Channel::Referral
        );
    }

    #[test]
    fn source_names() {
        assert_eq!(source_name("news.ycombinator.com"), "Hacker News");
        assert_eq!(source_name("www.google.de"), "Google");
        assert_eq!(source_name("someblog.dev"), "someblog.dev");
        assert_eq!(source_name("copilot.microsoft.com"), "Copilot");
        assert_eq!(source_name("bsky.app"), "Bluesky");
        assert_eq!(source_name("yandex.ru"), "Yandex");
        assert_eq!(source_name("dev.to"), "DEV");
    }

    #[test]
    fn source_domains() {
        assert_eq!(source_domain("t.co"), "x.com");
        assert_eq!(source_domain("twitter.com"), "x.com");
        assert_eq!(source_domain("www.x.com"), "x.com");
        assert_eq!(source_domain("lnkd.in"), "linkedin.com");
        assert_eq!(source_domain("old.reddit.com"), "reddit.com");
        assert_eq!(source_domain("chat.openai.com"), "chatgpt.com");
        assert_eq!(source_domain("l.facebook.com"), "facebook.com");
        assert_eq!(source_domain("threads.net"), "threads.com");
        assert_eq!(source_domain("discord.gg"), "discord.com");
        assert_eq!(source_domain("wa.me"), "whatsapp.com");
        assert_eq!(source_domain("www.google.co.in"), "google.com");
        assert_eq!(source_domain("news.google.com"), "google.com");
        assert_eq!(source_domain("gemini.google.com"), "gemini.google.com");
        assert_eq!(
            source_domain("news.ycombinator.com"),
            "news.ycombinator.com"
        );
        assert_eq!(source_domain("someblog.dev"), "someblog.dev");
    }
}
