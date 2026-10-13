use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::{Form, Json};
use serde::Deserialize;
use serde_json::json;

use crate::app::AppState;
use crate::auth::Account;
use crate::sites::{self, Site};

pub const TIMEZONES: &[&str] = &[
    "UTC",
    "Europe/London",
    "Europe/Berlin",
    "Europe/Paris",
    "Europe/Amsterdam",
    "Europe/Madrid",
    "Europe/Stockholm",
    "Europe/Warsaw",
    "Europe/Istanbul",
    "Asia/Dubai",
    "Asia/Kolkata",
    "Asia/Singapore",
    "Asia/Shanghai",
    "Asia/Tokyo",
    "Australia/Sydney",
    "Pacific/Auckland",
    "America/New_York",
    "America/Toronto",
    "America/Chicago",
    "America/Denver",
    "America/Los_Angeles",
    "America/Mexico_City",
    "America/Sao_Paulo",
    "America/Buenos_Aires",
    "Africa/Lagos",
    "Africa/Johannesburg",
];

#[derive(Template)]
#[template(path = "setup.html")]
struct SetupPage {
    step: u8,
    domain: String,
    timezone: String,
    timezones: Vec<(&'static str, bool)>,
    error: Option<&'static str>,
    site_id: String,
    script_host: String,
}

fn tz_options(selected: &str) -> Vec<(&'static str, bool)> {
    TIMEZONES.iter().map(|tz| (*tz, *tz == selected)).collect()
}

fn render(t: impl Template) -> Response {
    match t.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!(%err, "template render failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong").into_response()
        }
    }
}

pub async fn home(State(state): State<AppState>, account: Account) -> Response {
    match sites::for_account(&state.db, &account.id)
        .await
        .ok()
        .and_then(|s| s.into_iter().next())
    {
        Some(site) => Redirect::to(&format!("/app/sites/{}/setup/script", site.id)).into_response(),
        None => Redirect::to("/app/setup").into_response(),
    }
}

pub async fn site_form(_account: Account) -> Response {
    render(SetupPage {
        step: 1,
        domain: String::new(),
        timezone: "UTC".into(),
        timezones: tz_options("UTC"),
        error: None,
        site_id: String::new(),
        script_host: String::new(),
    })
}

#[derive(Deserialize)]
pub struct SiteInput {
    domain: String,
    timezone: String,
}

pub async fn create_site(
    State(state): State<AppState>,
    account: Account,
    Form(input): Form<SiteInput>,
) -> Response {
    let timezone = if TIMEZONES.contains(&input.timezone.as_str()) {
        input.timezone
    } else {
        "UTC".into()
    };
    let retry = |error| {
        render(SetupPage {
            step: 1,
            domain: input.domain.clone(),
            timezone: timezone.clone(),
            timezones: tz_options(&timezone),
            error: Some(error),
            site_id: String::new(),
            script_host: String::new(),
        })
    };
    let Some(domain) = sites::normalize_domain(&input.domain) else {
        return retry("Enter a domain like yourproduct.com.");
    };
    match sites::create_site(&state.db, &account.id, &domain, &timezone).await {
        Ok(id) => {
            state.sites.invalidate(&domain);
            Redirect::to(&format!("/app/sites/{id}/setup/script")).into_response()
        }
        Err(err) if err.to_string().contains("UNIQUE") => {
            retry("This domain is already on Virling. Ask its owner to invite you.")
        }
        Err(err) => {
            tracing::error!(%err, "creating site failed");
            retry("Something went wrong. Try again.")
        }
    }
}

async fn owned_site(state: &AppState, account: &Account, site_id: &str) -> Option<Site> {
    sites::for_account(&state.db, &account.id)
        .await
        .ok()?
        .into_iter()
        .find(|s| s.id == site_id)
}

pub async fn script_step(
    State(state): State<AppState>,
    account: Account,
    Path(site_id): Path<String>,
) -> Response {
    let Some(site) = owned_site(&state, &account, &site_id).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    render(SetupPage {
        step: 2,
        domain: site.domain,
        timezone: site.timezone,
        timezones: Vec::new(),
        error: None,
        site_id: site.id,
        script_host: state.config.base_url.clone(),
    })
}

pub async fn latest_event(
    State(state): State<AppState>,
    account: Account,
    Path(site_id): Path<String>,
) -> Response {
    if owned_site(&state, &account, &site_id).await.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let store = state.events.clone();
    match tokio::task::spawn_blocking(move || store.latest_pageview(&site_id)).await {
        Ok(Ok(Some(e))) => Json(json!({
            "path": e.path,
            "country": e.country,
            "device": e.device,
            "referrer": e.referrer_host,
        }))
        .into_response(),
        Ok(Ok(None)) => Json(json!(null)).into_response(),
        other => {
            tracing::error!(?other, "latest event lookup failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
