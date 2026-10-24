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

/// Host of the app whose in-app browser or messenger sent this user agent, for visits with no referrer.
pub fn in_app_source(user_agent: &str) -> Option<&'static str> {
    let ua = user_agent.to_ascii_lowercase();
    [
        (&["instagram"][..], "instagram.com"),
        (&["fban", "fbav", "fb_iab"], "facebook.com"),
        (&["musical_ly", "bytedancewebview", "tiktok"], "tiktok.com"),
        (&["linkedinapp"], "linkedin.com"),
        (&["telegram"], "t.me"),
        (&["whatsapp"], "whatsapp.com"),
        (&["slack_ssb", "slack/"], "slack.com"),
        (&["discord"], "discord.com"),
    ]
    .iter()
    .find(|(marks, _)| marks.iter().any(|m| ua.contains(m)))
    .map(|(_, host)| *host)
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
    fn in_app_browsers_are_named() {
        let ios = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/21F90";
        let android = "Mozilla/5.0 (Linux; Android 14; Pixel 8 Build/AP1A.240505.005; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/125.0.6422.52 Mobile Safari/537.36";
        let cases = [
            (format!("{ios} Instagram 336.0.3.30.109 (iPhone14,5; iOS 17_5; en_US; en-US; scale=3.00; 1170x2532; 612401234)"), "instagram.com"),
            (format!("{ios} [FBAN/FBIOS;FBAV/470.0.0.40.108;FBBV/600000000;FBDV/iPhone14,5;FBMD/iPhone;FBSN/iOS;FBSV/17.5;FBSS/3;FBID/phone;FBLC/en_US;FBOP/5]"), "facebook.com"),
            (format!("{ios} [FBAN/MessengerForiOS;FBAV/468.0.0.40.108;FBBV/590000000;FBDV/iPhone14,5;FBMD/iPhone;FBSN/iOS;FBSV/17.5]"), "facebook.com"),
            (format!("{android} [FB_IAB/FB4A;FBAV/470.0.0.50.109;]"), "facebook.com"),
            (format!("{ios} musical_ly_34.5.0 JsSdk/2.0 NetType/WIFI Channel/App Store ByteLocale/en Region/US"), "tiktok.com"),
            (format!("{android} trill_2023405040 JsSdk/1.0 NetType/WIFI Channel/googleplay AppName/musical_ly app_version/34.5.4"), "tiktok.com"),
            (format!("{ios} [LinkedInApp]/9.30.1234"), "linkedin.com"),
            (format!("{android} Telegram-Android/10.14.0 (Google Pixel 8; Android 14; SDK 34; HIGH)"), "t.me"),
            (format!("{android} WhatsApp/2.24.10.80"), "whatsapp.com"),
            (format!("{ios} Slack/24.05.10 (iPhone; iOS 17.5)"), "slack.com"),
            ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Slack_SSB/4.38.121 Chrome/124.0.6367.243 Electron/30.0.3 Safari/537.36".to_string(), "slack.com"),
            (format!("{ios} Discord/241.0 (iPhone; iOS 17.5)"), "discord.com"),
        ];
        for (ua, host) in cases {
            assert_eq!(in_app_source(&ua), Some(host), "{ua}");
        }
        assert_eq!(
            in_app_source(&format!("{ios} Version/17.5 Safari/604.1")),
            None
        );
        assert_eq!(
            in_app_source(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/125.0.0.0 Safari/537.36"
            ),
            None
        );
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
}
