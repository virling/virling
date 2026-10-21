//! AI crawler hits: counted per site, bot, page and hour, never as visitors.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use sqlx::SqlitePool;
use time::OffsetDateTime;
use url::Url;

use crate::app::AppState;
use crate::ingest::host_belongs;

/// What a crawler fetches pages for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Collects pages to train models.
    Training,
    /// Builds an AI search index.
    Search,
    /// Fetches a page live because someone asked an assistant.
    Answer,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Training => "Training",
            Kind::Search => "Search",
            Kind::Answer => "Answer",
        }
    }
}

pub struct Crawler {
    /// Lowercase user-agent token.
    token: &'static str,
    pub name: &'static str,
    pub company: &'static str,
    /// Name `icons::source` knows the company's logo by.
    pub logo: &'static str,
    pub kind: Kind,
}

const fn bot(
    token: &'static str,
    name: &'static str,
    company: &'static str,
    logo: &'static str,
    kind: Kind,
) -> Crawler {
    Crawler {
        token,
        name,
        company,
        logo,
        kind,
    }
}

pub const CRAWLERS: &[Crawler] = &[
    bot("gptbot", "GPTBot", "OpenAI", "openai", Kind::Training),
    bot(
        "oai-searchbot",
        "OAI-SearchBot",
        "OpenAI",
        "openai",
        Kind::Search,
    ),
    bot(
        "chatgpt-user",
        "ChatGPT-User",
        "OpenAI",
        "openai",
        Kind::Answer,
    ),
    bot(
        "claudebot",
        "ClaudeBot",
        "Anthropic",
        "anthropic",
        Kind::Training,
    ),
    bot(
        "claude-searchbot",
        "Claude-SearchBot",
        "Anthropic",
        "anthropic",
        Kind::Search,
    ),
    bot(
        "claude-user",
        "Claude-User",
        "Anthropic",
        "anthropic",
        Kind::Answer,
    ),
    bot(
        "anthropic-ai",
        "anthropic-ai",
        "Anthropic",
        "anthropic",
        Kind::Training,
    ),
    bot(
        "perplexitybot",
        "PerplexityBot",
        "Perplexity",
        "perplexity",
        Kind::Search,
    ),
    bot(
        "perplexity-user",
        "Perplexity-User",
        "Perplexity",
        "perplexity",
        Kind::Answer,
    ),
    bot(
        "google-extended",
        "Google-Extended",
        "Google",
        "google",
        Kind::Training,
    ),
    bot(
        "googleother",
        "GoogleOther",
        "Google",
        "google",
        Kind::Training,
    ),
    bot(
        "applebot-extended",
        "Applebot-Extended",
        "Apple",
        "apple",
        Kind::Training,
    ),
    bot("amazonbot", "Amazonbot", "Amazon", "amazon", Kind::Search),
    bot(
        "meta-externalagent",
        "meta-externalagent",
        "Meta",
        "meta",
        Kind::Training,
    ),
    bot(
        "meta-externalfetcher",
        "meta-externalfetcher",
        "Meta",
        "meta",
        Kind::Answer,
    ),
    bot(
        "ccbot",
        "CCBot",
        "Common Crawl",
        "commoncrawl",
        Kind::Training,
    ),
    bot(
        "bytespider",
        "Bytespider",
        "ByteDance",
        "bytedance",
        Kind::Training,
    ),
    bot(
        "duckassistbot",
        "DuckAssistBot",
        "DuckDuckGo",
        "duckduckgo",
        Kind::Answer,
    ),
];

/// The AI crawler a user agent belongs to, if any.
pub fn detect(user_agent: &str) -> Option<&'static Crawler> {
    let ua = user_agent.to_ascii_lowercase();
    CRAWLERS.iter().find(|c| ua.contains(c.token))
}

pub fn by_name(name: &str) -> Option<&'static Crawler> {
    CRAWLERS.iter().find(|c| c.name == name)
}

/// Counts one fetch of `page` by `bot` when the page belongs to the site at `domain`.
pub async fn record(state: &AppState, bot: &Crawler, domain: &str, page: &str) {
    let Some(site) = state
        .sites
        .by_domain(&state.db, &domain.to_ascii_lowercase())
        .await
    else {
        return;
    };
    let Some(url) = Url::parse(page).ok() else {
        return;
    };
    if !url
        .host_str()
        .is_some_and(|h| host_belongs(h, &site.domain))
        || !state.limiter.allow(
            &format!("crawl:{}", site.id),
            state.config.events_per_second,
        )
    {
        return;
    }
    let path: String = url.path().chars().take(500).collect();
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if let Err(err) = count(&state.db, &site.id, bot.name, &path, now).await {
        tracing::warn!(%err, "counting a crawler hit failed");
    }
}

pub(crate) async fn count(
    db: &SqlitePool,
    site_id: &str,
    bot: &str,
    path: &str,
    now: i64,
) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO crawler_days (site_id, day, bot, hits, last_seen) VALUES (?, ?, ?, 1, ?)
         ON CONFLICT (site_id, day, bot) DO UPDATE SET hits = hits + 1, last_seen = excluded.last_seen",
    )
    .bind(site_id)
    .bind(now.div_euclid(86400))
    .bind(bot)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO crawler_pages (site_id, bot, path, hits, last_seen) VALUES (?, ?, ?, 1, ?)
         ON CONFLICT (site_id, bot, path) DO UPDATE SET hits = hits + 1, last_seen = excluded.last_seen",
    )
    .bind(site_id)
    .bind(bot)
    .bind(path)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

#[derive(Deserialize)]
pub struct Hit {
    /// Site domain.
    s: String,
    /// Full page URL the crawler requested.
    u: String,
    /// The crawler's user agent.
    ua: String,
}

/// POST /api/crawl from a site's server or edge, for crawlers that never run the script.
pub async fn collect(State(state): State<AppState>, Json(hit): Json<Hit>) -> Response {
    if let Some(bot) = detect(&hit.ua) {
        record(&state, bot, &hit.s, &hit.u).await;
    }
    StatusCode::ACCEPTED.into_response()
}

/// Counts a crawler that fetched the script, from the page in its Referer.
pub fn script_fetch(state: &AppState, headers: &HeaderMap) {
    let get = |h| headers.get(h).and_then(|v| v.to_str().ok());
    let Some(bot) = get(header::USER_AGENT).and_then(detect) else {
        return;
    };
    let Some(page) = get(header::REFERER).map(str::to_string) else {
        return;
    };
    let Some(host) = Url::parse(&page).ok().and_then(|u| {
        u.host_str()
            .map(|h| h.trim_start_matches("www.").to_string())
    }) else {
        return;
    };
    let state = state.clone();
    tokio::spawn(async move { record(&state, bot, &host, &page).await });
}

/// One bot's hits in a range.
pub struct BotHits {
    pub bot: String,
    pub hits: i64,
    pub pages: i64,
    pub top_page: String,
    pub last_seen: i64,
}

/// Each bot's hits and last visit between two unix times, most hits first, with the pages it last read
/// in that time and the most read of them; days are UTC.
pub async fn hits(
    db: &SqlitePool,
    site_id: &str,
    from: i64,
    to: i64,
) -> sqlx::Result<Vec<BotHits>> {
    let days: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT bot, SUM(hits), MAX(last_seen) FROM crawler_days
         WHERE site_id = ? AND day BETWEEN ? AND ? GROUP BY bot ORDER BY 2 DESC, bot",
    )
    .bind(site_id)
    .bind(from.div_euclid(86400))
    .bind(to.div_euclid(86400))
    .fetch_all(db)
    .await?;
    let pages: Vec<(String, i64, String, i64)> = sqlx::query_as(
        "SELECT bot, COUNT(*), path, MAX(hits) FROM crawler_pages
         WHERE site_id = ? AND last_seen BETWEEN ? AND ? GROUP BY bot",
    )
    .bind(site_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?;
    Ok(days
        .into_iter()
        .map(|(bot, hits, last_seen)| {
            let p = pages.iter().find(|p| p.0 == bot);
            BotHits {
                pages: p.map_or(0, |p| p.1),
                top_page: p.map(|p| p.2.clone()).unwrap_or_default(),
                bot,
                hits,
                last_seen,
            }
        })
        .collect())
}

/// Pages each answer-time fetcher, such as ChatGPT-User, read between two unix times, most read first.
pub async fn answer_pages(
    db: &SqlitePool,
    site_id: &str,
    from: i64,
    to: i64,
) -> sqlx::Result<Vec<(String, String, i64)>> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT bot, path, hits FROM crawler_pages WHERE site_id = ? AND last_seen BETWEEN ? AND ?
         ORDER BY hits DESC, path",
    )
    .bind(site_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter(|r| by_name(&r.0).is_some_and(|c| c.kind == Kind::Answer))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crawlers_are_named_by_their_user_agent() {
        let name = |ua: &str| detect(ua).map(|c| (c.name, c.kind));
        assert_eq!(
            name(
                "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; GPTBot/1.2; +https://openai.com/gptbot)"
            ),
            Some(("GPTBot", Kind::Training))
        );
        assert_eq!(
            name("Mozilla/5.0 (compatible; Claude-User/1.0; +Claude-User@anthropic.com)"),
            Some(("Claude-User", Kind::Answer))
        );
        assert_eq!(
            name("Mozilla/5.0 (compatible; PerplexityBot/1.0)"),
            Some(("PerplexityBot", Kind::Search))
        );
        assert_eq!(
            name(
                "meta-externalagent/1.1 (+https://developers.facebook.com/docs/sharing/webmasters/crawler)"
            ),
            Some(("meta-externalagent", Kind::Training))
        );
        assert_eq!(name("Mozilla/5.0 (compatible; Googlebot/2.1)"), None);
        assert_eq!(name("Mozilla/5.0 (Macintosh) Safari/605.1"), None);
    }

    #[tokio::test]
    async fn hits_add_up_per_bot_and_range() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::open_sqlite(dir.path()).await.unwrap();
        sqlx::query("INSERT INTO accounts (id, email, created_at) VALUES ('a', 'a@x.com', '0')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO sites (id, account_id, domain, created_at) VALUES ('s', 'a', 'x.com', '0')")
            .execute(&db)
            .await
            .unwrap();
        let day = 20_000 * 86400;
        for (bot, path, at) in [
            ("GPTBot", "/pricing", day + 10),
            ("GPTBot", "/pricing", day + 20),
            ("GPTBot", "/", day + 30),
            ("ClaudeBot", "/docs", day + 40),
            ("GPTBot", "/old", day - 86400 * 40),
        ] {
            count(&db, "s", bot, path, at).await.unwrap();
        }
        let rows = hits(&db, "s", day - 86400 * 29, day + 86399).await.unwrap();
        let got: Vec<_> = rows
            .iter()
            .map(|r| {
                (
                    r.bot.as_str(),
                    r.hits,
                    r.pages,
                    r.top_page.as_str(),
                    r.last_seen,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("GPTBot", 3, 2, "/pricing", day + 30),
                ("ClaudeBot", 1, 1, "/docs", day + 40)
            ]
        );
    }

    #[test]
    fn every_crawler_is_a_bot_with_a_real_logo() {
        for c in CRAWLERS {
            assert!(
                crate::ua::is_bot(&format!("Mozilla/5.0 (compatible; {}/1.0)", c.name)),
                "{}",
                c.name
            );
            assert!(
                crate::icons::source(c.logo, 16).contains(r#"role="img""#),
                "{}",
                c.logo
            );
        }
    }
}
