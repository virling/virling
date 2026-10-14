use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::{Form, Json};
use serde::Deserialize;
use serde_json::json;

use crate::app::AppState;
use crate::auth::Account;
use crate::dashboard::{blocking, group, money};
use crate::providers::Provider;
use crate::sites::{self, Site};
use crate::stats::{PaymentFilter, Period};
use crate::sync;

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
    script_host: String,
}

fn tz_options(selected: &str) -> Vec<(&'static str, bool)> {
    TIMEZONES.iter().map(|tz| (*tz, *tz == selected)).collect()
}

pub fn render(t: impl Template) -> Response {
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
        Some(site) => {
            let store = state.events.clone();
            let id = site.id.clone();
            let has_events = tokio::task::spawn_blocking(move || store.latest_pageview(&id))
                .await
                .is_ok_and(|r| r.is_ok_and(|e| e.is_some()));
            let has_integration =
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM integrations WHERE site_id = ?")
                    .bind(&site.id)
                    .fetch_one(&state.db)
                    .await
                    .is_ok_and(|n| n > 0);
            let next = if has_events || has_integration {
                "dashboard"
            } else {
                "setup/script"
            };
            Redirect::to(&sites::app_path(&site.domain, next)).into_response()
        }
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
            script_host: String::new(),
        })
    };
    let Some(domain) = sites::normalize_domain(&input.domain) else {
        return retry("Enter a domain like yourproduct.com.");
    };
    match sites::create_site(&state.db, &account.id, &domain, &timezone).await {
        Ok(_) => {
            state.sites.invalidate(&domain);
            Redirect::to(&sites::app_path(&domain, "setup/script")).into_response()
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

async fn owned_site(state: &AppState, account: &Account, domain: &str) -> Option<Site> {
    sites::for_account(&state.db, &account.id)
        .await
        .ok()?
        .into_iter()
        .find(|s| s.domain == domain)
}

/// Permanently redirects an old `/app/sites/{id}/...` URL to the site's domain URL.
pub async fn legacy(
    State(state): State<AppState>,
    account: Account,
    Path(path): Path<String>,
    uri: Uri,
) -> Response {
    let (id, page) = path.split_once('/').unwrap_or((&path, ""));
    let site = sites::for_account(&state.db, &account.id)
        .await
        .ok()
        .and_then(|all| all.into_iter().find(|s| s.id == id));
    let Some(site) = site else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    Redirect::permanent(&format!("{}{query}", sites::app_path(&site.domain, page))).into_response()
}

pub async fn script_step(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let Some(site) = owned_site(&state, &account, &domain).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    render(SetupPage {
        step: 2,
        domain: site.domain,
        timezone: site.timezone,
        timezones: Vec::new(),
        error: None,
        script_host: state.config.base_url.clone(),
    })
}

pub async fn latest_event(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let Some(site) = owned_site(&state, &account, &domain).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let store = state.events.clone();
    match tokio::task::spawn_blocking(move || store.latest_pageview(&site.id)).await {
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

pub struct ProviderCard {
    key: &'static str,
    name: &'static str,
    key_label: &'static str,
    placeholder: &'static str,
    help: &'static str,
    help_url: &'static str,
    note: &'static str,
    selected: bool,
}

fn provider_card(p: Provider, selected: bool) -> ProviderCard {
    let (name, key_label, placeholder, help, help_url, note) = match p {
        Provider::Stripe => (
            "Stripe",
            "Restricted key",
            "rk_live_...",
            "How to create a read-only key, 1 minute",
            "https://docs.stripe.com/keys/restricted-api-keys",
            "Create a restricted key with Read access to Charges, Checkout Sessions, Customers, Events, Invoices, Subscriptions and PaymentIntents. Leave everything else at None. Secret keys (sk_) are refused.",
        ),
        Provider::Dodo => (
            "Dodo Payments",
            "API key",
            "Dodo Payments API key",
            "How to create a read-only key, 1 minute",
            "https://docs.dodopayments.com/api-reference/introduction",
            "Virling only reads payments, refunds and subscriptions with this key. Live and test mode keys both work.",
        ),
        Provider::Polar => (
            "Polar",
            "Organization token",
            "polar_oat_...",
            "How to create a read-only key, 1 minute",
            "https://docs.polar.sh/integrate/oat",
            "Create an organization access token with the orders:read and refunds:read scopes. No other scopes are needed.",
        ),
        Provider::LemonSqueezy => (
            "LemonSqueezy",
            "API key",
            "eyJ0eXAiOiJKV1QiLCJhbGciOi...",
            "How to create an API key, 1 minute",
            "https://docs.lemonsqueezy.com/guides/developer-guide/getting-started",
            "LemonSqueezy API keys cannot be made read-only and cover every store on your account. Virling only reads orders and subscription invoices, and stores the key encrypted. Delete it in LemonSqueezy at any time to revoke access.",
        ),
    };
    ProviderCard {
        key: p.as_str(),
        name,
        key_label,
        placeholder,
        help,
        help_url,
        note,
        selected,
    }
}

pub struct ConnectedRow {
    key: &'static str,
    name: &'static str,
    hint: String,
    synced: String,
    error: String,
}

pub struct ConnectedStats {
    rows: Vec<ConnectedRow>,
    payments: String,
    revenue: String,
    attributed: u64,
    syncing: bool,
}

#[derive(Template)]
#[template(path = "setup_payments.html")]
struct PaymentsStep {
    step: u8,
    domain: String,
    cards: Vec<ProviderCard>,
    error: Option<String>,
    key: String,
    connected: Option<ConnectedStats>,
    replacing: bool,
}

#[derive(Deserialize)]
pub struct ProviderQuery {
    provider: Option<String>,
    replace: Option<String>,
}

type ConnectedDbRow = (String, Option<i64>, Option<String>, Vec<u8>, Vec<u8>);

/// The last four characters of a key, never more.
fn key_hint(key: &str) -> String {
    let chars: Vec<char> = key.trim().chars().collect();
    if chars.len() < 8 {
        return String::new();
    }
    chars[chars.len() - 4..].iter().collect()
}

/// Shows a key form for every provider not yet connected, plus the one whose key is being replaced.
fn payments_page(
    site: Site,
    connected: Option<ConnectedStats>,
    want: Option<Provider>,
    replace: Option<Provider>,
    error: Option<String>,
    key: String,
) -> PaymentsStep {
    let linked: Vec<&str> = connected
        .as_ref()
        .map(|c| c.rows.iter().map(|r| r.key).collect())
        .unwrap_or_default();
    let replace = replace.filter(|p| linked.contains(&p.as_str()));
    let open: Vec<Provider> = Provider::ALL
        .into_iter()
        .filter(|p| Some(*p) == replace || !linked.contains(&p.as_str()))
        .collect();
    let selected = replace
        .or(want.filter(|p| open.contains(p)))
        .or(open.first().copied());
    PaymentsStep {
        step: 3,
        domain: site.domain,
        cards: open
            .into_iter()
            .map(|p| provider_card(p, selected == Some(p)))
            .collect(),
        error,
        key,
        connected,
        replacing: replace.is_some(),
    }
}

async fn connected_stats(
    state: &AppState,
    site: &Site,
) -> Result<Option<ConnectedStats>, StatusCode> {
    let rows: Vec<ConnectedDbRow> = sqlx::query_as(
        "SELECT provider, last_synced_at, last_error, key_ciphertext, key_nonce FROM integrations
         WHERE site_id = ? ORDER BY created_at",
    )
    .bind(&site.id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if rows.is_empty() {
        return Ok(None);
    }
    let store = state.events.clone();
    let (id, tz) = (site.id.clone(), site.timezone.clone());
    let (totals, attributed) = blocking(move || {
        let today = time::OffsetDateTime::now_utc()
            .to_offset(store.utc_offset(&tz)?)
            .date();
        let period = Period {
            from: today - time::Duration::days(89),
            to: today,
            tz,
        };
        Ok((
            store.overview(&id, &period)?.current,
            store
                .payment_list(&id, &period, PaymentFilter::Attributed, 0, 0)?
                .total,
        ))
    })
    .await?;
    let now = time::OffsetDateTime::now_utc();
    Ok(Some(ConnectedStats {
        syncing: rows.iter().any(|r| r.1.is_none()),
        rows: rows
            .into_iter()
            .filter_map(|(provider, synced, error, ciphertext, nonce)| {
                let p = Provider::parse(&provider)?;
                let synced = synced.and_then(|t| time::OffsetDateTime::from_unix_timestamp(t).ok());
                Some(ConnectedRow {
                    key: p.as_str(),
                    name: crate::dashboard::provider_name(p.as_str()),
                    hint: state
                        .vault
                        .decrypt(&ciphertext, &nonce)
                        .map(|k| key_hint(&k))
                        .unwrap_or_default(),
                    synced: synced.map_or_else(
                        || "Importing the last 90 days".into(),
                        |t| format!("Last synced {}", crate::dashboard::ago(t, now)),
                    ),
                    error: error.unwrap_or_default(),
                })
            })
            .collect(),
        payments: group(totals.payments),
        revenue: money(totals.revenue_usd),
        attributed,
    }))
}

pub async fn payments_step(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<ProviderQuery>,
) -> Response {
    let Some(site) = owned_site(&state, &account, &domain).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let connected = match connected_stats(&state, &site).await {
        Ok(c) => c,
        Err(code) => return code.into_response(),
    };
    let want = q.provider.as_deref().and_then(Provider::parse);
    let replace = want.filter(|_| q.replace.is_some());
    render(payments_page(
        site,
        connected,
        want,
        replace,
        None,
        String::new(),
    ))
}

#[derive(Deserialize)]
pub struct ConnectInput {
    provider: String,
    key: String,
}

pub async fn connect_payments(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<ConnectInput>,
) -> Response {
    let Some(site) = owned_site(&state, &account, &domain).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(provider) = Provider::parse(&input.provider) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let key = input.key.trim();
    let result = if key.is_empty() {
        Err(anyhow::anyhow!("Paste your key first."))
    } else {
        sync::add_integration(&state, &site.id, provider, key).await
    };
    if let Err(err) = result {
        let connected = match connected_stats(&state, &site).await {
            Ok(c) => c,
            Err(code) => return code.into_response(),
        };
        return render(payments_page(
            site,
            connected,
            Some(provider),
            Some(provider),
            Some(err.to_string()),
            key.to_string(),
        ));
    }
    let bg = state.clone();
    let id = site.id.clone();
    tokio::spawn(async move {
        if let Err(err) = sync::sync_site(&bg, &id).await {
            tracing::warn!(%err, site = %id, "first sync failed");
        }
    });
    Redirect::to(&format!(
        "{}?provider={}",
        sites::app_path(&site.domain, "setup/payments"),
        provider.as_str()
    ))
    .into_response()
}

#[derive(Template)]
#[template(path = "setup_checkout.html")]
struct CheckoutStep {
    step: u8,
    domain: String,
    tabs: Vec<(&'static str, &'static str, bool)>,
}

pub async fn checkout_step(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let Some(site) = owned_site(&state, &account, &domain).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let first: Option<String> = sqlx::query_scalar(
        "SELECT provider FROM integrations WHERE site_id = ? ORDER BY created_at",
    )
    .bind(&site.id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let selected = first
        .as_deref()
        .and_then(Provider::parse)
        .unwrap_or(Provider::Stripe);
    render(CheckoutStep {
        step: 4,
        domain: site.domain,
        tabs: Provider::ALL
            .into_iter()
            .map(|p| {
                (
                    p.as_str(),
                    crate::dashboard::provider_name(p.as_str()),
                    p == selected,
                )
            })
            .collect(),
    })
}
