//! The launch ledger: posts and launch days a site logs by hand, with the visits and revenue of that day and the next.

use askama::Template;
use axum::Form;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};
use url::Url;

use crate::app::AppState;
use crate::auth::Account;
use crate::dashboard::{Shell, blocking, load, money};
use crate::onboarding::render;
use crate::sites::app_path;
use crate::stats::Period;

/// Channels a launch may be filed under.
const CHANNELS: [&str; 8] = [
    "Paid",
    "Email",
    "AI",
    "Organic search",
    "Communities",
    "Social",
    "Referral",
    "Direct",
];

/// Launches shown, newest day first.
const MAX_SHOWN: i64 = 100;

pub struct Row {
    id: i64,
    name: String,
    url: String,
    channel: String,
    day: String,
    visitors: String,
    customers: String,
    revenue: String,
}

#[derive(Template)]
#[template(path = "launches.html")]
struct LaunchesPage {
    shell: Shell,
    title: &'static str,
    rows: Vec<Row>,
    today: String,
    channels: &'static [&'static str],
    error: &'static str,
}

fn parse_day(raw: &str) -> Option<Date> {
    Date::parse(raw.trim(), format_description!("[year]-[month]-[day]")).ok()
}

fn http_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Some(String::new());
    }
    let url = Url::parse(raw).ok()?;
    (matches!(url.scheme(), "http" | "https") && raw.len() <= 300).then(|| url.to_string())
}

async fn show(state: &AppState, account: &Account, domain: &str, error: &'static str) -> Response {
    let ctx = match load(state, account, domain, "launches").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let found: Vec<(i64, String, String, String, String)> = match sqlx::query_as(
        "SELECT id, name, url, channel, day FROM launches WHERE site_id = ? ORDER BY day DESC, id DESC LIMIT ?",
    )
    .bind(&ctx.site.id)
    .bind(MAX_SHOWN)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!(%err, "reading launches failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let (store, id, tz) = (
        state.events.clone(),
        ctx.site.id.clone(),
        ctx.site.timezone.clone(),
    );
    let days: Vec<String> = found.iter().map(|r| r.4.clone()).collect();
    let totals = match blocking(move || {
        days.iter()
            .map(|d| {
                let Some(day) = parse_day(d) else {
                    return Ok((0, 0, 0.0));
                };
                let p = Period {
                    from: day,
                    to: day + Duration::days(1),
                    tz: tz.clone(),
                };
                let t = store.overview(&id, &p)?.current;
                Ok((t.visitors, t.new_customers, t.revenue_usd))
            })
            .collect::<anyhow::Result<Vec<_>>>()
    })
    .await
    {
        Ok(t) => t,
        Err(code) => return code.into_response(),
    };
    let rows = found
        .into_iter()
        .zip(totals)
        .map(|((id, name, url, channel, day), (v, c, r))| Row {
            id,
            name,
            url,
            channel,
            day,
            visitors: v.to_string(),
            customers: c.to_string(),
            revenue: money(r),
        })
        .collect();
    render(LaunchesPage {
        shell: ctx.shell,
        title: "Launches",
        rows,
        today: ctx.today.to_string(),
        channels: &CHANNELS,
        error,
    })
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    show(&state, &account, &domain, "").await
}

#[derive(Deserialize)]
pub struct LaunchInput {
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    channel: String,
    day: String,
}

pub async fn create(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<LaunchInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "launches").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return show(
            &state,
            &account,
            &domain,
            "Give the launch a name of up to 80 characters.",
        )
        .await;
    }
    let Some(url) = http_url(&input.url) else {
        return show(
            &state,
            &account,
            &domain,
            "The link must be a web address starting with http or https.",
        )
        .await;
    };
    let Some(day) = parse_day(&input.day) else {
        return show(&state, &account, &domain, "Pick the day of the launch.").await;
    };
    let channel = input.channel.trim();
    if !channel.is_empty() && !CHANNELS.contains(&channel) {
        return show(
            &state,
            &account,
            &domain,
            "Pick one of the listed channels.",
        )
        .await;
    }
    let inserted = sqlx::query(
        "INSERT INTO launches (site_id, name, url, channel, day, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&ctx.site.id)
    .bind(name)
    .bind(url)
    .bind(channel)
    .bind(day.to_string())
    .bind(OffsetDateTime::now_utc().unix_timestamp())
    .execute(&state.db)
    .await;
    if let Err(err) = inserted {
        tracing::error!(%err, "saving a launch failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&app_path(&ctx.site.domain, "launches")).into_response()
}

#[derive(Deserialize)]
pub struct RemoveInput {
    id: i64,
}

pub async fn remove(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<RemoveInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "launches").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let removed = sqlx::query("DELETE FROM launches WHERE id = ? AND site_id = ?")
        .bind(input.id)
        .bind(&ctx.site.id)
        .execute(&state.db)
        .await;
    if let Err(err) = removed {
        tracing::error!(%err, "removing a launch failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&app_path(&ctx.site.domain, "launches")).into_response()
}
