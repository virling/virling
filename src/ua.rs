//! User-agent and screen-width helpers shared by the server and the virling.com Worker.
pub fn is_bot(user_agent: &str) -> bool {
    let ua = user_agent.to_ascii_lowercase();
    ua.is_empty()
        || [
            "bot",
            "crawl",
            "spider",
            "slurp",
            "headless",
            "lighthouse",
            "preview",
            "curl/",
            "wget/",
            "python-",
            "go-http",
            "phantomjs",
            "facebookexternalhit",
            "playwright",
            "puppeteer",
            "selenium",
            "webdriver",
            "scrapy",
            "pingdom",
            "uptime",
            "gtmetrix",
            "pagespeed",
            "prerender",
            "okhttp",
            "axios/",
            "node-fetch",
            "java/",
            "postman",
            "chatgpt-user",
            "claude-user",
            "perplexity-user",
            "anthropic-ai",
            "googleother",
            "google-extended",
            "meta-externalagent",
            "meta-externalfetcher",
        ]
        .iter()
        .any(|m| ua.contains(m))
}

/// Known referrer-spam domains and their subdomains.
pub fn is_spam_referrer(host: &str) -> bool {
    [
        "semalt.com",
        "darodar.com",
        "buttons-for-website.com",
        "buttons-for-your-website.com",
        "free-share-buttons.com",
        "social-buttons.com",
        "best-seo-offer.com",
        "best-seo-solution.com",
        "ilovevitaly.com",
        "priceg.com",
        "hulfingtonpost.com",
        "o-o-6-o-o.com",
        "7makemoneyonline.com",
        "get-free-traffic-now.com",
        "trafficmonetize.com",
        "seo-platform.com",
    ]
    .iter()
    .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

pub fn browser(user_agent: &str) -> &'static str {
    if user_agent.contains("Edg/") {
        "Edge"
    } else if user_agent.contains("OPR/") || user_agent.contains("Opera") {
        "Opera"
    } else if user_agent.contains("SamsungBrowser") {
        "Samsung Internet"
    } else if user_agent.contains("Firefox/") || user_agent.contains("FxiOS") {
        "Firefox"
    } else if user_agent.contains("Chrome/") || user_agent.contains("CriOS") {
        "Chrome"
    } else if user_agent.contains("Safari/") {
        "Safari"
    } else {
        "Other"
    }
}

pub fn device(width: Option<u32>) -> &'static str {
    match width {
        Some(w) if w < 768 => "Mobile",
        Some(w) if w < 1024 => "Tablet",
        Some(_) => "Desktop",
        None => "Unknown",
    }
}

/// Chromium claiming macOS on a screen under 1024 device pixels: no Mac has one, headless browsers posing as Macs do.
pub fn impossible_mac(user_agent: &str, width: Option<u32>, ratio: Option<f64>) -> bool {
    user_agent.contains("Macintosh")
        && user_agent.contains("Chrome/")
        && width.is_some_and(|w| f64::from(w) * ratio.unwrap_or(1.0) < 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bots_are_filtered() {
        assert!(is_bot("Mozilla/5.0 (compatible; Googlebot/2.1)"));
        assert!(is_bot("Mozilla/5.0 HeadlessChrome/120.0"));
        assert!(is_bot(""));
        assert!(is_bot("Mozilla/5.0 Chrome/120 Playwright/1.45"));
        assert!(is_bot("UptimeRobot/2.0"));
        assert!(is_bot("axios/1.7.2"));
        assert!(!is_bot(
            "Mozilla/5.0 (Macintosh) AppleWebKit/605 Version/17.0 Safari/605.1"
        ));
    }

    #[test]
    fn referrer_spam_is_matched_by_domain() {
        assert!(is_spam_referrer("semalt.com"));
        assert!(is_spam_referrer("forum.semalt.com"));
        assert!(!is_spam_referrer("notsemalt.com"));
        assert!(!is_spam_referrer("google.com"));
    }

    #[test]
    fn browsers_and_devices() {
        assert_eq!(browser("Mozilla/5.0 Chrome/120 Safari/537 Edg/120"), "Edge");
        assert_eq!(browser("Mozilla/5.0 Chrome/120 Safari/537"), "Chrome");
        assert_eq!(browser("Mozilla/5.0 Version/17 Safari/605"), "Safari");
        assert_eq!(device(Some(390)), "Mobile");
        assert_eq!(device(Some(800)), "Tablet");
        assert_eq!(device(Some(1440)), "Desktop");
    }

    #[test]
    fn macs_have_desktop_screens() {
        let mac = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
        let ipad = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
        let win = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
        assert!(impossible_mac(mac, Some(800), None));
        assert!(impossible_mac(mac, Some(800), Some(1.0)));
        assert!(!impossible_mac(mac, Some(960), Some(1.5)), "zoomed in");
        assert!(!impossible_mac(mac, Some(1440), None));
        assert!(!impossible_mac(mac, None, None));
        assert!(!impossible_mac(ipad, Some(820), Some(2.0)));
        assert!(!impossible_mac(ipad, Some(820), None));
        assert!(!impossible_mac(win, Some(800), None));
    }
}
