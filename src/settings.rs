use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::app::AppState;
use crate::auth::Account;
use crate::dashboard::{Opt, Shell, ago, load, provider_name};
use crate::onboarding::render;
use crate::sites::app_path;
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
    brand: Brand,
    brand_saved: bool,
    brand_error: &'static str,
}

#[derive(Deserialize)]
pub struct SavedQuery {
    saved: Option<String>,
    error: Option<String>,
}

/// Agency branding on a site's client report; empty fields are left off.
#[derive(Default, sqlx::FromRow)]
pub struct Brand {
    pub agency_name: String,
    pub logo_url: String,
    pub accent: String,
    pub footer: String,
}

pub async fn brand(db: &SqlitePool, site_id: &str) -> sqlx::Result<Brand> {
    Ok(sqlx::query_as(
        "SELECT agency_name, logo_url, accent, footer FROM report_brands WHERE site_id = ?",
    )
    .bind(site_id)
    .fetch_optional(db)
    .await?
    .unwrap_or_default())
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<SavedQuery>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
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
    let brand = brand(&state.db, &ctx.site.id).await;
    let (Ok((model, window_days)), Ok(integrations), Ok(brand)) = (row, integrations, brand) else {
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
        saved: q.saved.as_deref() == Some("1"),
        brand,
        brand_saved: q.saved.as_deref() == Some("report"),
        brand_error: match q.error.as_deref() {
            Some("logo") => "The logo URL must start with https://.",
            Some("accent") => "The accent color must be a hex color like #2F5BEA.",
            _ => "",
        },
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
    Path(domain): Path<String>,
    Form(input): Form<SettingsInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
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
    Redirect::to(&format!(
        "{}?saved=1",
        app_path(&ctx.site.domain, "settings")
    ))
    .into_response()
}

#[derive(Deserialize)]
pub struct DisconnectInput {
    provider: String,
    back: Option<String>,
}

pub async fn disconnect(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<DisconnectInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
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
    let back = match input.back.as_deref() {
        Some("setup") => "setup/payments",
        _ => "settings",
    };
    Redirect::to(&app_path(&ctx.site.domain, back)).into_response()
}

#[derive(Deserialize)]
pub struct BrandInput {
    agency_name: String,
    logo_url: String,
    accent: String,
    footer: String,
}

/// Checks and tidies branding input; the error names the bad field.
fn clean_brand(input: BrandInput) -> Result<Brand, &'static str> {
    let cut = |s: &str, n: usize| s.trim().chars().take(n).collect::<String>();
    let logo_url = input.logo_url.trim().to_string();
    let logo_ok = logo_url.is_empty()
        || (logo_url.len() <= 500
            && url::Url::parse(&logo_url).is_ok_and(|u| u.scheme() == "https"));
    if !logo_ok {
        return Err("logo");
    }
    let accent = input.accent.trim().to_ascii_lowercase();
    let accent_ok = accent.is_empty()
        || (accent.len() == 7
            && accent.starts_with('#')
            && accent[1..].bytes().all(|b| b.is_ascii_hexdigit()));
    if !accent_ok {
        return Err("accent");
    }
    Ok(Brand {
        agency_name: cut(&input.agency_name, 80),
        logo_url,
        accent,
        footer: cut(&input.footer, 200),
    })
}

pub async fn save_brand(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<BrandInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let settings = app_path(&ctx.site.domain, "settings");
    let b = match clean_brand(input) {
        Ok(b) => b,
        Err(field) => {
            return Redirect::to(&format!("{settings}?error={field}#report")).into_response();
        }
    };
    let saved = sqlx::query(
        "INSERT INTO report_brands (site_id, agency_name, logo_url, accent, footer) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(site_id) DO UPDATE SET agency_name = excluded.agency_name, logo_url = excluded.logo_url, accent = excluded.accent, footer = excluded.footer",
    )
    .bind(&ctx.site.id)
    .bind(&b.agency_name)
    .bind(&b.logo_url)
    .bind(&b.accent)
    .bind(&b.footer)
    .execute(&state.db)
    .await;
    if let Err(err) = saved {
        tracing::error!(%err, "saving report branding failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("{settings}?saved=report#report")).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(logo_url: &str, accent: &str) -> BrandInput {
        BrandInput {
            agency_name: "  Kettle and Co.  ".into(),
            logo_url: logo_url.into(),
            accent: accent.into(),
            footer: String::new(),
        }
    }

    #[test]
    fn brand_input_is_checked() {
        let b = clean_brand(input("https://kettle.co/logo.png", "#2F5BEA")).unwrap();
        assert_eq!(
            (b.agency_name.as_str(), b.accent.as_str()),
            ("Kettle and Co.", "#2f5bea")
        );
        assert!(clean_brand(input("", "")).is_ok());
        assert_eq!(
            clean_brand(input("javascript:alert(1)", "")).err(),
            Some("logo")
        );
        assert_eq!(
            clean_brand(input("http://kettle.co/a.png", "")).err(),
            Some("logo")
        );
        assert_eq!(clean_brand(input("", "red; x")).err(), Some("accent"));
        assert_eq!(clean_brand(input("", "#12345g")).err(), Some("accent"));
    }
}
