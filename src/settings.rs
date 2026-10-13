use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::app::AppState;
use crate::auth::Account;
use crate::dashboard::{Opt, Shell, ago, load, provider_name};
use crate::onboarding::render;
use crate::sync;

const WINDOWS: [i64; 4] = [30, 60, 90, 365];
const MODELS: [&str; 2] = ["last_non_direct", "first_touch"];

type IntegrationRow = (String, Option<i64>, Option<String>);

pub struct Connection {
    provider: String,
    name: &'static str,
    status: &'static str,
    status_class: &'static str,
    error: String,
    synced: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    shell: Shell,
    title: &'static str,
    script_host: String,
    cookieless: bool,
    model: String,
    window_days: i64,
    windows: Vec<Opt>,
    connections: Vec<Connection>,
    saved: bool,
}

#[derive(Deserialize)]
pub struct SavedQuery {
    saved: Option<String>,
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(site_id): Path<String>,
    Query(q): Query<SavedQuery>,
) -> Response {
    let ctx = match load(&state, &account, &site_id, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let row: Result<(String, i64), _> =
        sqlx::query_as("SELECT attribution_model, window_days FROM sites WHERE id = ?")
            .bind(&ctx.site.id)
            .fetch_one(&state.db)
            .await;
    let integrations: Result<Vec<IntegrationRow>, _> = sqlx::query_as(
        "SELECT provider, last_synced_at, last_error FROM integrations WHERE site_id = ? ORDER BY created_at",
    )
    .bind(&ctx.site.id)
    .fetch_all(&state.db)
    .await;
    let (Ok((model, window_days)), Ok(integrations)) = (row, integrations) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let now = OffsetDateTime::now_utc();
    render(SettingsPage {
        title: "Site settings",
        script_host: state.config.base_url.clone(),
        cookieless: ctx.site.cookieless,
        windows: WINDOWS
            .iter()
            .map(|d| Opt {
                label: format!("{d} days"),
                href: d.to_string(),
                on: *d == window_days,
                count: String::new(),
            })
            .collect(),
        model,
        window_days,
        connections: integrations
            .into_iter()
            .map(|(provider, synced_at, error)| {
                let synced_at = synced_at.and_then(|t| OffsetDateTime::from_unix_timestamp(t).ok());
                let (status, status_class) = match (&error, synced_at) {
                    (Some(_), _) => ("Error", "bad"),
                    (None, None) => ("Syncing", "warn"),
                    (None, Some(_)) => ("Connected", "ok"),
                };
                Connection {
                    name: provider_name(&provider),
                    provider,
                    status,
                    status_class,
                    error: error.unwrap_or_default(),
                    synced: synced_at.map_or_else(
                        || "Importing the last 90 days".into(),
                        |t| format!("Last synced {}", ago(t, now)),
                    ),
                }
            })
            .collect(),
        saved: q.saved.is_some(),
        shell: ctx.shell,
    })
}

#[derive(Deserialize)]
pub struct SettingsInput {
    model: String,
    window_days: i64,
    cookieless: Option<String>,
}

pub async fn save(
    State(state): State<AppState>,
    account: Account,
    Path(site_id): Path<String>,
    Form(input): Form<SettingsInput>,
) -> Response {
    let ctx = match load(&state, &account, &site_id, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if !MODELS.contains(&input.model.as_str()) || !WINDOWS.contains(&input.window_days) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let updated = sqlx::query(
        "UPDATE sites SET attribution_model = ?, window_days = ?, cookieless = ? WHERE id = ?",
    )
    .bind(&input.model)
    .bind(input.window_days)
    .bind(input.cookieless.is_some())
    .bind(&ctx.site.id)
    .execute(&state.db)
    .await;
    if let Err(err) = updated {
        tracing::error!(%err, "saving site settings failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    state.sites.invalidate(&ctx.site.domain);
    sync::rebuild_attribution(&state, Some(&ctx.site.id)).await;
    Redirect::to(&format!("/app/sites/{}/settings?saved=1", ctx.site.id)).into_response()
}

#[derive(Deserialize)]
pub struct DisconnectInput {
    provider: String,
}

pub async fn disconnect(
    State(state): State<AppState>,
    account: Account,
    Path(site_id): Path<String>,
    Form(input): Form<DisconnectInput>,
) -> Response {
    let ctx = match load(&state, &account, &site_id, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if let Err(err) = sqlx::query("DELETE FROM integrations WHERE site_id = ? AND provider = ?")
        .bind(&ctx.site.id)
        .bind(&input.provider)
        .execute(&state.db)
        .await
    {
        tracing::error!(%err, "disconnecting integration failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("/app/sites/{}/settings", ctx.site.id)).into_response()
}
