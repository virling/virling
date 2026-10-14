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
        ]
        .iter()
        .any(|m| ua.contains(m))
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
        assert!(!is_bot(
            "Mozilla/5.0 (Macintosh) AppleWebKit/605 Version/17.0 Safari/605.1"
        ));
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
