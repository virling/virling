use axum::Form;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::OffsetDateTime;
use url::Url;

use crate::app::AppState;
use crate::auth::{Account, random_token};
use crate::channel::Channel;
use crate::dashboard::load;
use crate::sites::app_path;
use crate::ua::is_bot;

/// (value, label, utm_medium) for the channel a link is shared on.
const CHANNELS: [(&str, &str, &str); 4] = [
    ("social", "Social", "social"),
    ("email", "Email", "email"),
    ("paid", "Paid", "cpc"),
    ("referral", "Referral", "referral"),
];

/// Places a link is shared, as (form value, label, utm_source); "other" takes a typed source.
pub const PLACES: [(&str, &str, &str); 10] = [
    ("x", "X", "x.com"),
    ("linkedin", "LinkedIn", "linkedin.com"),
    ("newsletter", "Newsletter", "newsletter"),
    ("youtube", "YouTube", "youtube.com"),
    ("reddit", "Reddit", "reddit.com"),
    ("discord", "Discord", "discord.com"),
    ("producthunt", "Product Hunt", "producthunt.com"),
    ("podcast", "Podcast", "podcast"),
    ("qr", "QR code", "qr"),
    ("other", "Other", ""),
];

/// A site's tracked link, for the dashboard's Campaign tab.
#[derive(sqlx::FromRow)]
pub struct SiteLink {
    pub slug: String,
    pub name: String,
    pub source: String,
    pub clicks: i64,
}

pub async fn site_links(db: &sqlx::SqlitePool, site_id: &str) -> sqlx::Result<Vec<SiteLink>> {
    sqlx::query_as("SELECT slug, name, source, clicks FROM short_links WHERE site_id = ?")
        .bind(site_id)
        .fetch_all(db)
        .await
}

/// Links live in the dashboard's Campaign tab now.
pub async fn page(Path(domain): Path<String>) -> Redirect {
    Redirect::to(&campaigns(&domain, ""))
}

/// The dashboard's Campaign tab, optionally with `extra` params such as the new link.
fn campaigns(domain: &str, extra: &str) -> String {
    format!("{}?channels=campaign{extra}#channels", app_path(domain, ""))
}

#[derive(Deserialize)]
pub struct LinkInput {
    place: String,
    #[serde(default)]
    other: String,
    destination: String,
    #[serde(default)]
    name: String,
}

/// The destination when it is an http(s) page on the site's domain or a subdomain.
fn valid_destination(raw: &str, domain: &str) -> Option<String> {
    let url = Url::parse(raw.trim()).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let ok = matches!(url.scheme(), "http" | "https")
        && (host == domain || host.ends_with(&format!(".{domain}")));
    ok.then(|| url.to_string())
}

pub async fn create(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<LinkInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "links").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let fail = |e: &str| Redirect::to(&campaigns(&ctx.site.domain, &format!("&link_error={e}")));
    let Some(&(_, label, known)) = PLACES.iter().find(|p| p.0 == input.place) else {
        return fail("place").into_response();
    };
    let source = if known.is_empty() {
        input.other.trim().to_ascii_lowercase()
    } else {
        known.to_string()
    };
    if source.is_empty() {
        return fail("place").into_response();
    }
    let raw = input.destination.trim();
    let raw = if raw.starts_with('/') {
        format!("https://{}{raw}", ctx.site.domain)
    } else {
        raw.to_string()
    };
    let Some(destination) = valid_destination(&raw, &ctx.site.domain) else {
        return fail("destination").into_response();
    };
    let channel = match crate::channel::classify(None, Some(&source), None) {
        Channel::Social => "social",
        Channel::Email => "email",
        Channel::Paid => "paid",
        _ => "referral",
    };
    let name = match input.name.trim() {
        "" => format!(
            "{} link",
            if known.is_empty() {
                source.as_str()
            } else {
                label
            }
        ),
        n => n.to_string(),
    };
    let now = OffsetDateTime::now_utc().unix_timestamp();
    for _ in 0..3 {
        let slug: String = random_token().chars().take(7).collect();
        let res = sqlx::query(
            "INSERT INTO short_links (id, site_id, slug, name, destination, source, channel, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&ctx.site.id)
        .bind(&slug)
        .bind(name.chars().take(80).collect::<String>())
        .bind(&destination)
        .bind(source.chars().take(80).collect::<String>())
        .bind(channel)
        .bind(now)
        .execute(&state.db)
        .await;
        match res {
            Ok(_) => {
                return Redirect::to(&campaigns(&ctx.site.domain, &format!("&new_link={slug}")))
                    .into_response();
            }
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => continue,
            Err(err) => {
                tracing::error!(%err, "creating a short link failed");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        }
    }
    StatusCode::SERVICE_UNAVAILABLE.into_response()
}

#[derive(Deserialize)]
pub struct DeleteInput {
    id: String,
}

pub async fn delete(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<DeleteInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "links").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if let Err(err) = sqlx::query("DELETE FROM short_links WHERE id = ? AND site_id = ?")
        .bind(&input.id)
        .bind(&ctx.site.id)
        .execute(&state.db)
        .await
    {
        tracing::error!(%err, "deleting a short link failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&campaigns(&ctx.site.domain, "")).into_response()
}

/// `destination` with the link's UTM tags added, keeping any the destination already has.
fn tagged(destination: &str, slug: &str, source: &str, channel: &str) -> String {
    let Ok(mut url) = Url::parse(destination) else {
        return destination.to_string();
    };
    let medium = CHANNELS
        .iter()
        .find(|c| c.0 == channel)
        .map_or("referral", |c| c.2);
    let existing: Vec<String> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in [
            ("utm_source", source),
            ("utm_medium", medium),
            ("utm_campaign", slug),
        ] {
            if !existing.iter().any(|e| e == k) {
                q.append_pair(k, v);
            }
        }
    }
    url.to_string()
}

pub async fn follow(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let row: Option<(String, String, String)> =
        match sqlx::query_as("SELECT destination, source, channel FROM short_links WHERE slug = ?")
            .bind(&slug)
            .fetch_optional(&state.db)
            .await
        {
            Ok(row) => row,
            Err(err) => {
                tracing::error!(%err, "loading a short link failed");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        };
    let Some((destination, source, channel)) = row else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !is_bot(ua) {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        if let Err(err) = sqlx::query(
            "UPDATE short_links SET clicks = clicks + 1, last_click_at = ? WHERE slug = ?",
        )
        .bind(now)
        .bind(&slug)
        .execute(&state.db)
        .await
        {
            tracing::warn!(%err, "counting a short link click failed");
        }
    }
    (
        StatusCode::FOUND,
        [
            (
                header::LOCATION,
                tagged(&destination, &slug, &source, &channel),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destinations_must_be_on_the_site() {
        assert!(valid_destination("https://example.com/pricing", "example.com").is_some());
        assert!(valid_destination("https://www.example.com/", "example.com").is_some());
        assert!(valid_destination("https://docs.example.com/a", "example.com").is_some());
        assert!(valid_destination("https://evil.com/", "example.com").is_none());
        assert!(valid_destination("https://notexample.com/", "example.com").is_none());
        assert!(valid_destination("javascript:alert(1)", "example.com").is_none());
    }

    #[test]
    fn tags_keep_existing_utm_values() {
        let url = tagged(
            "https://example.com/p?utm_source=keep",
            "abc1234",
            "x.com",
            "social",
        );
        assert_eq!(
            url,
            "https://example.com/p?utm_source=keep&utm_medium=social&utm_campaign=abc1234"
        );
        let url = tagged("https://example.com/", "abc1234", "newsletter", "email");
        assert_eq!(
            url,
            "https://example.com/?utm_source=newsletter&utm_medium=email&utm_campaign=abc1234"
        );
    }
}
