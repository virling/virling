use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::OffsetDateTime;
use url::Url;

use crate::app::AppState;
use crate::auth::{Account, random_token};
use crate::dashboard::{Opt, Shell, blocking, group, load, money, period, range, range_opts};
use crate::onboarding::render;
use crate::sites::app_path;
use crate::stats::Dimension;
use crate::ua::is_bot;

/// (value, label, utm_medium) for the channel a link is shared on.
const CHANNELS: [(&str, &str, &str); 4] = [
    ("social", "Social", "social"),
    ("email", "Email", "email"),
    ("paid", "Paid", "cpc"),
    ("referral", "Referral", "referral"),
];

#[derive(sqlx::FromRow)]
struct LinkRow {
    id: String,
    slug: String,
    name: String,
    destination: String,
    source: String,
    channel: String,
    clicks: i64,
}

pub struct LinkView {
    id: String,
    name: String,
    short_url: String,
    destination: String,
    icon: String,
    source: String,
    channel: &'static str,
    clicks: String,
    visitors: String,
    revenue: String,
    per_visitor: String,
}

#[derive(Template)]
#[template(path = "links.html")]
struct LinksPage {
    shell: Shell,
    title: &'static str,
    range_label: &'static str,
    ranges: Vec<Opt>,
    links: Vec<LinkView>,
    channels: Vec<(&'static str, &'static str)>,
    home: String,
    error: &'static str,
}

#[derive(Deserialize, Default)]
pub struct LinksQuery {
    range: Option<String>,
    error: Option<String>,
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<LinksQuery>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "links").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let rows: Vec<LinkRow> = match sqlx::query_as(
        "SELECT id, slug, name, destination, source, channel, clicks FROM short_links
         WHERE site_id = ? ORDER BY created_at DESC",
    )
    .bind(&ctx.site.id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!(%err, "loading short links failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let (rkey, rlabel, days, rdefault) = range(&ctx, q.range.as_deref());
    let p = period(&ctx, days);
    let (store, id) = (state.events.clone(), ctx.site.id.clone());
    let campaigns = match blocking(move || store.breakdown(&id, &p, Dimension::Campaign)).await {
        Ok(rows) => rows,
        Err(code) => return code.into_response(),
    };
    let base = app_path(&ctx.site.domain, "links");
    let links = rows
        .into_iter()
        .map(|l| {
            let stats = campaigns
                .iter()
                .find(|c| c.key.as_deref() == Some(l.slug.as_str()));
            let visitors = stats.map_or(0, |c| c.visitors);
            let revenue = stats.map_or(0.0, |c| c.revenue_usd);
            LinkView {
                short_url: format!("{}/l/{}", state.config.base_url, l.slug),
                icon: crate::icons::source(&l.source, 16),
                channel: CHANNELS
                    .iter()
                    .find(|c| c.0 == l.channel)
                    .map_or("Referral", |c| c.1),
                clicks: group(l.clicks.max(0) as u64),
                visitors: group(visitors),
                revenue: money(revenue),
                per_visitor: if visitors > 0 {
                    money(revenue / visitors as f64)
                } else {
                    "n/a".into()
                },
                id: l.id,
                name: l.name,
                destination: l.destination,
                source: l.source,
            }
        })
        .collect();
    render(LinksPage {
        title: "Links",
        range_label: rlabel,
        ranges: range_opts(&base, rkey, rdefault, &[]),
        links,
        channels: CHANNELS.iter().map(|c| (c.0, c.1)).collect(),
        home: format!("https://{}/", ctx.site.domain),
        error: match q.error.as_deref() {
            Some("destination") => "The destination must be a page on this site.",
            Some("fields") => "Give the link a name and a source.",
            _ => "",
        },
        shell: ctx.shell,
    })
}

#[derive(Deserialize)]
pub struct LinkInput {
    name: String,
    destination: String,
    source: String,
    channel: String,
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
    let base = app_path(&ctx.site.domain, "links");
    let (name, source) = (input.name.trim(), input.source.trim().to_ascii_lowercase());
    if name.is_empty() || source.is_empty() {
        return Redirect::to(&format!("{base}?error=fields")).into_response();
    }
    let Some(destination) = valid_destination(&input.destination, &ctx.site.domain) else {
        return Redirect::to(&format!("{base}?error=destination")).into_response();
    };
    let channel = CHANNELS
        .iter()
        .find(|c| c.0 == input.channel)
        .map_or("referral", |c| c.0);
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
            Ok(_) => return Redirect::to(&base).into_response(),
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
    Redirect::to(&app_path(&ctx.site.domain, "links")).into_response()
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
