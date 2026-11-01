//! Partner links for creators, affiliates and sponsors, ranked by the paying customers they sent.

use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::{Duration, OffsetDateTime};

use crate::app::AppState;
use crate::auth::{Account, random_token};
use crate::dashboard::{Opt, RangeArgs, RangeMenu, Shell, blocking, group, load_ranged, money};
use crate::onboarding::render;
use crate::sites::app_path;
use crate::stats::{BreakdownRow, Dimension, Period};

/// A partner's figures for a range, from the Campaign breakdown row of its link.
#[derive(Default)]
pub struct Figures {
    visitors: String,
    customers: String,
    revenue: String,
    refund_rate: String,
}

fn figures(row: Option<&BreakdownRow>) -> Figures {
    let Some(r) = row else {
        return Figures {
            visitors: "0".into(),
            customers: "0".into(),
            revenue: money(0.0),
            refund_rate: "n/a".into(),
        };
    };
    Figures {
        visitors: group(r.visitors),
        customers: group(r.customers),
        revenue: money(r.revenue_usd),
        refund_rate: if r.payments > 0 {
            format!("{:.0}%", r.refunded as f64 / r.payments as f64 * 100.0)
        } else {
            "n/a".into()
        },
    }
}

pub struct Row {
    id: String,
    name: String,
    link: String,
    share: String,
    figures: Figures,
}

#[derive(Template)]
#[template(path = "partners.html")]
struct PartnersPage {
    shell: Shell,
    title: &'static str,
    range: RangeMenu,
    rows: Vec<Row>,
    error: &'static str,
}

#[derive(Deserialize, Default)]
pub struct PartnersQuery {
    range: Option<String>,
    from: Option<String>,
    to: Option<String>,
    error: Option<String>,
}

/// (id, slug, name, share token) of the site's partner links.
async fn partner_links(
    db: &sqlx::SqlitePool,
    site_id: &str,
) -> sqlx::Result<Vec<(String, String, String, String)>> {
    sqlx::query_as(
        "SELECT id, slug, name, share_token FROM short_links
         WHERE site_id = ? AND share_token IS NOT NULL ORDER BY created_at",
    )
    .bind(site_id)
    .fetch_all(db)
    .await
}

/// Campaign rows of a site for a period, one read for every partner.
async fn campaigns(
    state: &AppState,
    site_id: &str,
    p: Period,
) -> Result<Vec<BreakdownRow>, StatusCode> {
    let (store, id) = (state.events.clone(), site_id.to_string());
    blocking(move || store.breakdown(&id, &p, Dimension::Campaign)).await
}

fn find<'a>(rows: &'a [BreakdownRow], slug: &str) -> Option<&'a BreakdownRow> {
    rows.iter()
        .find(|r| !r.unattributed && r.key.as_deref() == Some(slug))
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<PartnersQuery>,
) -> Response {
    let args = RangeArgs {
        range: q.range.clone(),
        from: q.from.clone(),
        to: q.to.clone(),
        compare: None,
    };
    let (ctx, picked) = match load_ranged(&state, &account, &domain, "partners", args).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let links = match partner_links(&state.db, &ctx.site.id).await {
        Ok(l) => l,
        Err(err) => {
            tracing::error!(%err, "reading partner links failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let rows = match campaigns(&state, &ctx.site.id, picked.period(&ctx.site.timezone)).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let base = &state.config.base_url;
    let mut out: Vec<(f64, Row)> = links
        .into_iter()
        .map(|(id, slug, name, token)| {
            let row = find(&rows, &slug);
            (
                row.map_or(0.0, |r| r.revenue_usd),
                Row {
                    id,
                    name,
                    link: format!("{base}/l/{slug}"),
                    share: format!("{base}/partner/{token}"),
                    figures: figures(row),
                },
            )
        })
        .collect();
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    let path = app_path(&ctx.site.domain, "partners");
    let mut range = picked.menu(ctx.today, ctx.first_day, &path, &[]);
    range.vs = false;
    render(PartnersPage {
        title: "Partners",
        range,
        rows: out.into_iter().map(|r| r.1).collect(),
        error: match q.error.as_deref() {
            Some("name") => "Give the partner a name of up to 80 characters.",
            Some("destination") => "That page is not on this site. Use a path like /pricing.",
            _ => "",
        },
        shell: ctx.shell,
    })
}

#[derive(Deserialize)]
pub struct PartnerInput {
    name: String,
    #[serde(default)]
    destination: String,
}

/// utm_source of a partner: its name in lowercase words joined by dashes.
fn source_of(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub async fn create(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<PartnerInput>,
) -> Response {
    let ctx = match crate::dashboard::load(&state, &account, &domain, "partners").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let path = app_path(&ctx.site.domain, "partners");
    let name = input.name.trim();
    let source = source_of(name);
    if source.is_empty() || name.chars().count() > 80 {
        return Redirect::to(&format!("{path}?error=name")).into_response();
    }
    let Some(destination) = crate::links::destination(&input.destination, &ctx.site.domain) else {
        return Redirect::to(&format!("{path}?error=destination")).into_response();
    };
    let now = OffsetDateTime::now_utc().unix_timestamp();
    for _ in 0..3 {
        let slug: String = random_token().chars().take(7).collect();
        let res = sqlx::query(
            "INSERT INTO short_links (id, site_id, slug, name, destination, source, channel, created_at, share_token)
             VALUES (?, ?, ?, ?, ?, ?, 'referral', ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&ctx.site.id)
        .bind(&slug)
        .bind(name)
        .bind(&destination)
        .bind(source.chars().take(80).collect::<String>())
        .bind(now)
        .bind(random_token())
        .execute(&state.db)
        .await;
        match res {
            Ok(_) => return Redirect::to(&path).into_response(),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => continue,
            Err(err) => {
                tracing::error!(%err, "creating a partner link failed");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        }
    }
    StatusCode::SERVICE_UNAVAILABLE.into_response()
}

#[derive(Deserialize)]
pub struct RemoveInput {
    id: String,
}

pub async fn remove(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<RemoveInput>,
) -> Response {
    let ctx = match crate::dashboard::load(&state, &account, &domain, "partners").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if let Err(err) = sqlx::query(
        "DELETE FROM short_links WHERE id = ? AND site_id = ? AND share_token IS NOT NULL",
    )
    .bind(&input.id)
    .bind(&ctx.site.id)
    .execute(&state.db)
    .await
    {
        tracing::error!(%err, "removing a partner link failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&app_path(&ctx.site.domain, "partners")).into_response()
}

#[derive(Template)]
#[template(path = "partner_shared.html")]
struct SharedPage {
    name: String,
    domain: String,
    link: String,
    tabs: Vec<Opt>,
    figures: Figures,
}

/// (slug, name, site id, domain, timezone) of a shared partner link.
type SharedLink = (String, String, String, String, String);

#[derive(Deserialize, Default)]
pub struct SharedQuery {
    days: Option<i64>,
}

/// One partner's figures behind its read-only link, without sign-in.
pub async fn shared(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<SharedQuery>,
) -> Response {
    let found: sqlx::Result<Option<SharedLink>> = sqlx::query_as(
        "SELECT l.slug, l.name, s.id, s.domain, s.timezone FROM short_links l
         JOIN sites s ON s.id = l.site_id WHERE l.share_token = ? AND s.sample_data = 0",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await;
    let (slug, name, site_id, domain, tz) = match found {
        Ok(Some(f)) => f,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let days = [7, 30, 90]
        .into_iter()
        .find(|d| Some(*d) == q.days)
        .unwrap_or(30);
    let store = state.events.clone();
    let zone = tz.clone();
    let offset = match blocking(move || store.utc_offset(&zone)).await {
        Ok(o) => o,
        Err(code) => return code.into_response(),
    };
    let today = OffsetDateTime::now_utc().to_offset(offset).date();
    let p = Period {
        from: today - Duration::days(days - 1),
        to: today,
        tz,
    };
    let rows = match campaigns(&state, &site_id, p).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    render(SharedPage {
        name,
        link: format!("{}/l/{slug}", state.config.base_url),
        domain,
        tabs: [7, 30, 90]
            .into_iter()
            .map(|d| Opt {
                label: format!("Last {d} days"),
                href: format!("/partner/{token}?days={d}"),
                on: d == days,
                count: String::new(),
            })
            .collect(),
        figures: figures(find(&rows, &slug)),
    })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::api::tests::fixture;
    use crate::auth::hash_token;

    #[tokio::test]
    async fn ranks_partners_and_shares_one_read_only() {
        let f = fixture().await;
        let session = random_token();
        sqlx::query("INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, 0, ?)")
            .bind(hash_token(&session))
            .bind(&f.account)
            .bind(i64::MAX)
            .execute(&f.state.db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO short_links (id, site_id, slug, name, destination, source, channel, created_at, share_token)
             SELECT 'p1', id, 'launch', 'Jane Podcast', 'https://example.com/', 'jane-podcast', 'referral', 0, 'tok'
             FROM sites WHERE domain = 'example.com'",
        )
        .execute(&f.state.db)
        .await
        .unwrap();
        let get = |uri: &str, cookie: bool| {
            let mut req = Request::get(uri);
            if cookie {
                req = req.header("cookie", format!("virling_session={session}"));
            }
            let app = crate::app::router(f.state.clone());
            let req = req.body(Body::empty()).unwrap();
            async move {
                let res = app.oneshot(req).await.unwrap();
                let status = res.status();
                let bytes = axum::body::to_bytes(res.into_body(), 1 << 22)
                    .await
                    .unwrap();
                (status, String::from_utf8(bytes.to_vec()).unwrap())
            }
        };
        let (code, html) = get("/app/example.com/partners", true).await;
        assert_eq!(code, StatusCode::OK);
        assert!(html.contains("Jane Podcast") && html.contains("/partner/tok"));
        let (code, html) = get("/partner/tok?days=7", false).await;
        assert_eq!(code, StatusCode::OK);
        assert!(html.contains("Jane Podcast") && html.contains("Last 7 days"));
        assert_eq!(get("/partner/nope", false).await.0, StatusCode::NOT_FOUND);
    }

    #[test]
    fn sources_are_dashed_words() {
        assert_eq!(source_of("Jane Doe's Podcast"), "jane-doe-s-podcast");
        assert_eq!(source_of("  !! "), "");
    }
}
