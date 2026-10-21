use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::api;
use crate::app::AppState;
use crate::auth::{Account, random_token};
use crate::dashboard::{Opt, Shell, ago, blocking, load, plural, provider_name};
use crate::onboarding::render;
use crate::sites::{app_path, normalize_domain};
use crate::sync;

const WINDOWS: [i64; 4] = [30, 60, 90, 365];
const MODELS: [&str; 2] = ["last_non_direct", "first_touch"];
const RETURNS: [i64; 3] = [1, 5, 30];

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
    /// Same-visit domains, one per line.
    same_visit: String,
    return_minutes: i64,
    /// Bot visitors filtered in the last 30 days.
    bots: String,
    model: String,
    window_days: i64,
    windows: Vec<Opt>,
    connections: Vec<Connection>,
    saved: bool,
    timezone: String,
    timezones: Vec<(String, bool)>,
    tz_saved: bool,
    brand: Brand,
    brand_saved: bool,
    brand_error: &'static str,
    share_link: Option<String>,
    /// The public dashboard's URL, its switches, and whether they were just saved.
    public_link: String,
    public_on: bool,
    public_hide: bool,
    public_saved: bool,
}

#[derive(Template)]
#[template(path = "account.html")]
struct AccountPage {
    shell: Shell,
    title: &'static str,
    plan_saved: bool,
    /// The user's color theme: auto, light or dark.
    theme: String,
    theme_saved: bool,
    api_keys: Vec<KeyView>,
    /// A key just created, shown this once.
    new_key: Option<String>,
    owner: bool,
    team: crate::team::Team,
    /// A message about the last team change, if any.
    team_note: &'static str,
    /// An email waiting for the owner to confirm a paid seat.
    adding: String,
}

pub struct KeyView {
    id: String,
    name: String,
    prefix: String,
    created: String,
    used: String,
}

#[derive(Deserialize, Default)]
pub struct SavedQuery {
    saved: Option<String>,
    error: Option<String>,
    team: Option<String>,
    add: Option<String>,
}

/// Agency branding on a site's client report; empty fields are left off.
#[derive(sqlx::FromRow)]
pub struct Brand {
    pub agency_name: String,
    pub logo_url: String,
    pub accent: String,
    pub footer: String,
    /// Shows the small "Powered by virling" line.
    pub powered_by: bool,
}

impl Default for Brand {
    fn default() -> Self {
        Brand {
            agency_name: String::new(),
            logo_url: String::new(),
            accent: String::new(),
            footer: String::new(),
            powered_by: true,
        }
    }
}

pub async fn brand(db: &SqlitePool, site_id: &str) -> sqlx::Result<Brand> {
    Ok(sqlx::query_as(
        "SELECT agency_name, logo_url, accent, footer, powered_by FROM report_brands WHERE site_id = ?",
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
    let row: Result<(String, i64, bool, bool), _> = sqlx::query_as(
        "SELECT attribution_model, window_days, public_dashboard, public_hide_revenue FROM sites WHERE id = ?",
    )
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
    let token: Result<Option<Option<String>>, _> =
        sqlx::query_scalar("SELECT share_token FROM report_brands WHERE site_id = ?")
            .bind(&ctx.site.id)
            .fetch_optional(&state.db)
            .await;
    let (Ok((model, window_days, public_on, public_hide)), Ok(integrations), Ok(brand), Ok(token)) =
        (row, integrations, brand, token)
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let bots = match blocking(move || store.bots_filtered(&id, 30)).await {
        Ok(n) => plural(n, "bot visitor"),
        Err(code) => return code.into_response(),
    };
    let now = OffsetDateTime::now_utc();
    render(SettingsPage {
        title: "Site settings",
        script_host: state.config.base_url.clone(),
        cookieless: ctx.site.cookieless,
        same_visit: ctx.site.same_visit.replace(' ', "\n"),
        return_minutes: ctx.site.return_minutes,
        bots,
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
        timezones: crate::onboarding::tz_options(&ctx.site.timezone),
        timezone: ctx.site.timezone.clone(),
        tz_saved: q.saved.as_deref() == Some("timezone"),
        brand,
        public_link: format!("{}/share/{}", state.config.base_url, ctx.site.domain),
        public_on,
        public_hide,
        public_saved: q.saved.as_deref() == Some("public"),
        share_link: token
            .flatten()
            .map(|t| format!("{}/r/{t}", state.config.base_url)),
        brand_saved: q.saved.as_deref() == Some("report"),
        brand_error: match q.error.as_deref() {
            Some("logo") => "The logo URL must start with https://.",
            Some("accent") => "The accent color must be a hex color like #2F5BEA.",
            _ => "",
        },
        shell: ctx.shell,
    })
}

/// The account page: email, plan and API keys, which apply to every site.
pub async fn account_page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<SavedQuery>,
) -> Response {
    show_account(&state, &account, &domain, q, None).await
}

async fn show_account(
    state: &AppState,
    account: &Account,
    domain: &str,
    q: SavedQuery,
    new_key: Option<String>,
) -> Response {
    let ctx = match load(state, account, domain, "account").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let keys = if account.owner {
        api::list_keys(&state.db, &account.id).await
    } else {
        Ok(Vec::new())
    };
    let (Ok(keys), Ok(team)) = (
        keys,
        crate::team::load(&state.db, &account.id, state.config.hosted).await,
    ) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok(theme) = sqlx::query_scalar::<_, String>("SELECT theme FROM accounts WHERE id = ?")
        .bind(&account.user)
        .fetch_one(&state.db)
        .await
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let team_note = match (q.saved.as_deref(), q.team.as_deref(), q.error.as_deref()) {
        (Some("invite"), _, _) => "Invite sent. The link works for 7 days.",
        (_, Some("email"), _) => "Enter a valid email.",
        (_, Some("taken"), _) => "That person is already on the team or invited.",
        (_, Some("limit"), _) => "The team is full on this plan.",
        (_, _, Some("team")) => "Remove team members and invites before switching to Personal.",
        _ => "",
    };
    let now = OffsetDateTime::now_utc();
    render(AccountPage {
        title: "Account settings",
        api_keys: keys
            .into_iter()
            .map(|k| {
                let at = |t: i64| OffsetDateTime::from_unix_timestamp(t).unwrap_or(now);
                KeyView {
                    created: format!("Created {}", ago(at(k.created_at), now)),
                    used: k.last_used_at.map_or_else(
                        || "Never used".into(),
                        |t| format!("Used {}", ago(at(t), now)),
                    ),
                    id: k.id,
                    name: k.name,
                    prefix: k.prefix,
                }
            })
            .collect(),
        new_key,
        plan_saved: q.saved.as_deref() == Some("plan"),
        theme_saved: q.saved.as_deref() == Some("theme"),
        theme,
        shell: ctx.shell,
        owner: account.owner,
        team,
        team_note,
        adding: q.add.unwrap_or_default(),
    })
}

#[derive(Deserialize)]
pub struct SettingsInput {
    model: String,
    window_days: i64,
    cookieless: Option<String>,
    #[serde(default)]
    same_visit: String,
    #[serde(default = "default_return")]
    return_minutes: i64,
}

fn default_return() -> i64 {
    5
}

/// Up to 20 distinct domains from free text, normalized, without the site's own domain or its subdomains.
fn same_visit_domains(input: &str, domain: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for d in input
        .split([' ', ',', '\n', '\r', '\t'])
        .filter_map(normalize_domain)
    {
        if !crate::ingest::host_belongs(&d, domain) && !out.contains(&d) && out.len() < 20 {
            out.push(d);
        }
    }
    out.join(" ")
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
    if !MODELS.contains(&input.model.as_str())
        || !WINDOWS.contains(&input.window_days)
        || !RETURNS.contains(&input.return_minutes)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let updated = sqlx::query(
        "UPDATE sites SET attribution_model = ?, window_days = ?, cookieless = ?, same_visit = ?, return_minutes = ? WHERE id = ?",
    )
    .bind(&input.model)
    .bind(input.window_days)
    .bind(input.cookieless.is_some())
    .bind(same_visit_domains(&input.same_visit, &ctx.site.domain))
    .bind(input.return_minutes)
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
pub struct TimezoneInput {
    timezone: String,
}

/// Sets the zone whose midnight starts each day of the site's reports.
pub async fn save_timezone(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<TimezoneInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if !crate::onboarding::valid_timezone(&state, &input.timezone).await {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if let Err(err) = sqlx::query("UPDATE sites SET timezone = ? WHERE id = ?")
        .bind(&input.timezone)
        .bind(&ctx.site.id)
        .execute(&state.db)
        .await
    {
        tracing::error!(%err, "saving the timezone failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    state.sites.invalidate(&ctx.site.domain);
    Redirect::to(&format!(
        "{}?saved=timezone#timezone",
        app_path(&ctx.site.domain, "settings")
    ))
    .into_response()
}

#[derive(Deserialize)]
pub struct PublicInput {
    public_dashboard: Option<String>,
    hide_revenue: Option<String>,
}

/// Turns the public dashboard on or off; off is a 404 on the next request.
pub async fn save_public(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<PublicInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if let Err(err) =
        sqlx::query("UPDATE sites SET public_dashboard = ?, public_hide_revenue = ? WHERE id = ?")
            .bind(input.public_dashboard.is_some())
            .bind(input.hide_revenue.is_some())
            .bind(&ctx.site.id)
            .execute(&state.db)
            .await
    {
        tracing::error!(%err, "saving the public dashboard failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!(
        "{}?saved=public#public",
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
        _ => "settings#payments",
    };
    Redirect::to(&app_path(&ctx.site.domain, back)).into_response()
}

#[derive(Deserialize)]
pub struct BrandInput {
    agency_name: String,
    logo_url: String,
    accent: String,
    footer: String,
    powered_by: Option<String>,
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
        powered_by: input.powered_by.is_some(),
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
        "INSERT INTO report_brands (site_id, agency_name, logo_url, accent, footer, powered_by) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(site_id) DO UPDATE SET agency_name = excluded.agency_name, logo_url = excluded.logo_url, accent = excluded.accent, footer = excluded.footer, powered_by = excluded.powered_by",
    )
    .bind(&ctx.site.id)
    .bind(&b.agency_name)
    .bind(&b.logo_url)
    .bind(&b.accent)
    .bind(&b.footer)
    .bind(b.powered_by)
    .execute(&state.db)
    .await;
    if let Err(err) = saved {
        tracing::error!(%err, "saving report branding failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("{settings}?saved=report#report")).into_response()
}

#[derive(Deserialize)]
pub struct PlanInput {
    account_type: String,
}

#[derive(Deserialize)]
pub struct ThemeInput {
    theme: String,
}

/// Sets the signed-in user's color theme: auto, light or dark.
pub async fn save_theme(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<ThemeInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "account").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if !matches!(input.theme.as_str(), "auto" | "light" | "dark") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if let Err(err) = sqlx::query("UPDATE accounts SET theme = ? WHERE id = ?")
        .bind(&input.theme)
        .bind(&account.user)
        .execute(&state.db)
        .await
    {
        tracing::error!(%err, "saving the theme failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!(
        "{}?saved=theme#theme",
        app_path(&ctx.site.domain, "account")
    ))
    .into_response()
}

/// Sets the account to Personal or Agency.
pub async fn save_plan(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<PlanInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "account").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if !account.owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !matches!(input.account_type.as_str(), "personal" | "agency") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if input.account_type == "personal" {
        let others: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM members WHERE org_id = ?1) + (SELECT count(*) FROM invites WHERE org_id = ?1)",
        )
        .bind(&account.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(1);
        if others > 0 {
            return Redirect::to(&format!(
                "{}?error=team#team",
                app_path(&ctx.site.domain, "account")
            ))
            .into_response();
        }
    }
    if let Err(err) = sqlx::query("UPDATE accounts SET account_type = ? WHERE id = ?")
        .bind(&input.account_type)
        .bind(&account.id)
        .execute(&state.db)
        .await
    {
        tracing::error!(%err, "saving the plan failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!(
        "{}?saved=plan#plan",
        app_path(&ctx.site.domain, "account")
    ))
    .into_response()
}

/// Creates or replaces the read-only client link; the old link stops working.
pub async fn share(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let saved = sqlx::query(
        "INSERT INTO report_brands (site_id, share_token) VALUES (?, ?)
         ON CONFLICT(site_id) DO UPDATE SET share_token = excluded.share_token",
    )
    .bind(&ctx.site.id)
    .bind(random_token())
    .execute(&state.db)
    .await;
    if let Err(err) = saved {
        tracing::error!(%err, "creating the client link failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("{}#share", app_path(&ctx.site.domain, "settings"))).into_response()
}

pub async fn revoke(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if let Err(err) = sqlx::query("UPDATE report_brands SET share_token = NULL WHERE site_id = ?")
        .bind(&ctx.site.id)
        .execute(&state.db)
        .await
    {
        tracing::error!(%err, "revoking the client link failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("{}#share", app_path(&ctx.site.domain, "settings"))).into_response()
}

#[derive(Deserialize)]
pub struct KeyInput {
    name: String,
}

/// Creates an API key for the account and shows it once on the account page.
pub async fn create_key(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<KeyInput>,
) -> Response {
    if !account.owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(code) = load(&state, &account, &domain, "account").await {
        return code.into_response();
    }
    match api::create_key(&state.db, &account.id, &input.name).await {
        Ok(key) => show_account(&state, &account, &domain, SavedQuery::default(), Some(key)).await,
        Err(err) => {
            tracing::error!(%err, "creating an API key failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct RevokeKeyInput {
    id: String,
}

pub async fn revoke_key(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<RevokeKeyInput>,
) -> Response {
    if !account.owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let ctx = match load(&state, &account, &domain, "account").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if let Err(err) = api::revoke_key(&state.db, &account.id, &input.id).await {
        tracing::error!(%err, "revoking an API key failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("{}#api", app_path(&ctx.site.domain, "account"))).into_response()
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
            powered_by: None,
        }
    }

    #[test]
    fn same_visit_domains_are_cleaned() {
        assert_eq!(
            same_visit_domains(
                "https://App.Example.net/x, docs.example.org\nwww.example.com app.example.com\n\nnot a domain app.example.net",
                "example.com"
            ),
            "app.example.net docs.example.org"
        );
        assert_eq!(same_visit_domains("", "example.com"), "");
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
