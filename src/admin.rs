//! Operator panel at /admin, open to the emails in the admins table; everyone else gets a 404.
#![allow(clippy::type_complexity)]
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use askama::Template;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;
use sqlx::SqliteConnection;
use time::OffsetDateTime;
use time::macros::format_description;

use crate::app::AppState;
use crate::auth::{SESSION_COOKIE, cookie, hash_token, random_token};
use crate::waitlist::valid_email;

const PAGE: usize = 50;
const SUPPORT_TTL_SECS: i64 = 30 * 60;
const BETA_TTL: Duration = Duration::from_secs(60);
const RETURN_COOKIE: &str = "virling_return";
pub const PLANS: [(&str, &str); 4] = [
    ("self_hosted", "None"),
    ("paid", "Paid"),
    ("founding", "Founding"),
    ("comped", "Free access"),
];

#[derive(Default)]
pub struct Settings {
    applications: Option<(String, String)>,
    beta: Mutex<Beta>,
}

#[derive(Default)]
struct Beta {
    fetched: Option<Instant>,
    fetching: bool,
    rows: Vec<Application>,
    failed: bool,
}

#[derive(Clone, Deserialize)]
struct Application {
    email: String,
    wedge: String,
    created_at: i64,
    site: Option<String>,
    payments: Option<String>,
    revenue: Option<String>,
    audience: Option<String>,
}

impl Settings {
    pub fn new(applications_url: Option<String>, applications_token: Option<String>) -> Self {
        Settings {
            applications: applications_url
                .zip(applications_token)
                .filter(|(u, t)| !u.is_empty() && !t.is_empty()),
            ..Default::default()
        }
    }
}

async fn is_admin(db: &sqlx::SqlitePool, email: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT 1 FROM admins WHERE email = lower(?)")
        .bind(email)
        .fetch_optional(db)
        .await
        .is_ok_and(|r| r.is_some())
}

async fn admin_emails(db: &sqlx::SqlitePool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT email FROM admins ORDER BY email")
        .fetch_all(db)
        .await
}

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn when(secs: i64) -> String {
    OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .and_then(|t| {
            t.format(format_description!("[year]-[month]-[day] [hour]:[minute]"))
                .ok()
        })
        .unwrap_or_default()
}

fn label(plan: &str) -> &'static str {
    PLANS
        .iter()
        .find(|(k, _)| *k == plan)
        .map_or("Unknown", |(_, l)| l)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin", get(overview))
        .route("/admin/accounts", get(accounts))
        .route("/admin/accounts/{id}", get(account))
        .route("/admin/accounts/{id}/plan", post(set_plan))
        .route("/admin/accounts/{id}/suspend", post(suspend))
        .route("/admin/accounts/{id}/sign-in", post(sign_in_as))
        .route("/admin/accounts/{id}/delete", post(delete_account))
        .route("/admin/sites", get(sites))
        .route("/admin/sites/{id}/delete", post(delete_site))
        .route("/admin/signups", get(signups))
        .route("/admin/applications", get(beta))
        .route("/admin/applications/invite", post(invite))
        .route("/admin/audit", get(audit_page))
        .route("/admin/stop", post(stop))
        .layer(axum::middleware::map_response(private))
}

async fn private(mut res: Response) -> Response {
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert("x-robots-tag", HeaderValue::from_static("noindex"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    res
}

#[derive(Template)]
#[template(
    source = r#"<div class="support-bar" role="status"><span>Signed in as <b>{{ email }}</b> by {{ by }}. Read only, ends in {{ minutes }} min.</span><form method="post" action="/admin/stop"><button class="btn secondary" type="submit">Exit</button></form></div>"#,
    ext = "html"
)]
pub struct Banner<'a> {
    pub email: &'a str,
    pub by: &'a str,
    pub minutes: i64,
}

/// A signed-in operator; any other request to an admin page is a 404.
pub struct Admin {
    email: String,
    csrf: String,
}

fn same_origin(headers: &HeaderMap, base_url: &str) -> bool {
    let get = |name| headers.get(name).and_then(|v| v.to_str().ok());
    match get("origin") {
        Some(origin) => {
            url::Url::parse(base_url).is_ok_and(|u| u.origin().ascii_serialization() == origin)
        }
        None => get("sec-fetch-site") == Some("same-origin"),
    }
}

fn equal(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

impl FromRequestParts<AppState> for Admin {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let not_found = || StatusCode::NOT_FOUND.into_response();
        let token = cookie(&parts.headers, SESSION_COOKIE).ok_or_else(not_found)?;
        let email: Option<String> = sqlx::query_scalar(
            "SELECT a.email FROM sessions s JOIN accounts a ON a.id = s.account_id WHERE s.token_hash = ? AND s.expires_at > ? AND s.impersonator IS NULL AND a.suspended_at IS NULL",
        )
        .bind(hash_token(&token))
        .bind(now())
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
        let email = email.ok_or_else(not_found)?;
        if !is_admin(&state.db, &email).await {
            return Err(not_found());
        }
        let writes = !matches!(parts.method, Method::GET | Method::HEAD);
        if writes && !same_origin(&parts.headers, &state.config.base_url) {
            return Err(StatusCode::FORBIDDEN.into_response());
        }
        if writes && !state.limiter.allow(&format!("admin:{email}"), 2) {
            return Err(
                (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "1")]).into_response(),
            );
        }
        Ok(Admin {
            email,
            csrf: hash_token(&format!("admin-csrf:{token}")),
        })
    }
}

impl Admin {
    fn checks(&self, given: &str) -> bool {
        equal(&self.csrf, given)
    }
}

fn unavailable(err: impl std::fmt::Display, what: &str) -> Response {
    tracing::error!(%err, "{what} failed");
    StatusCode::SERVICE_UNAVAILABLE.into_response()
}

async fn log(
    conn: &mut SqliteConnection,
    admin: &str,
    action: &str,
    target: &str,
    detail: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO audit_log (admin, action, target, detail, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(admin)
    .bind(action)
    .bind(target)
    .bind(detail)
    .bind(now())
    .execute(conn)
    .await?;
    Ok(())
}

fn back(path: &str, note: &str) -> Response {
    Redirect::to(&format!("{path}?n={note}")).into_response()
}

fn notice(code: &str) -> &'static str {
    match code {
        "plan" => "Plan changed.",
        "suspended" => "Account suspended and signed out.",
        "unsuspended" => "Account active again.",
        "deleted" => "Deleted.",
        "invited" => "Invite sent.",
        "already" => "Already invited.",
        "failed" => "The invite email could not be sent. See the server log.",
        "nomail" => "Email is not configured. Set VIRLING_SMTP_URL to send invites.",
        "nomatch" => "That does not match. Nothing was changed.",
        "admin" => "Not allowed for an admin account.",
        "inactive" => "Nothing to sign in to: the account is suspended.",
        "purge" => "Removed, but stored events could not be purged. See the server log.",
        _ => "",
    }
}

struct AccountRow {
    id: String,
    email: String,
    plan: &'static str,
    plan_key: String,
    kind: String,
    created: String,
    suspended: bool,
    admin: bool,
    sites: i64,
}

struct SiteRow {
    id: String,
    domain: String,
    owner: String,
    created: String,
    events: String,
}

struct SignupRow {
    email: String,
    wedge: String,
    clients: String,
    created: String,
}

struct ApplicationRow {
    email: String,
    wedge: String,
    site: String,
    payments: String,
    revenue: String,
    audience: String,
    created: String,
    invited: bool,
}

struct AuditRow {
    admin: String,
    action: String,
    target: String,
    detail: String,
    created: String,
}

#[derive(Template, Default)]
#[template(path = "admin.html")]
struct Page {
    tab: &'static str,
    admin: String,
    csrf: String,
    notice: &'static str,
    note: String,
    q: String,
    prev: String,
    next: String,
    stats: Vec<(String, String)>,
    health: Vec<(String, String)>,
    busiest: Vec<(String, String)>,
    admins: Vec<String>,
    accounts: Vec<AccountRow>,
    account: Option<AccountRow>,
    sites: Vec<SiteRow>,
    signups: Vec<SignupRow>,
    applications: Vec<ApplicationRow>,
    audit: Vec<AuditRow>,
}

fn render(page: Page) -> Response {
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => unavailable(err, "rendering the admin page"),
    }
}

#[derive(Deserialize)]
pub struct View {
    q: Option<String>,
    p: Option<usize>,
    n: Option<String>,
}

impl View {
    fn page(&self, _state: &AppState, admin: &Admin, tab: &'static str) -> Page {
        Page {
            tab,
            admin: admin.email.clone(),
            csrf: admin.csrf.clone(),
            notice: notice(self.n.as_deref().unwrap_or_default()),
            q: self
                .q
                .clone()
                .unwrap_or_default()
                .trim()
                .chars()
                .take(100)
                .collect(),
            ..Default::default()
        }
    }

    fn offset(&self) -> i64 {
        (self.p.unwrap_or(0).min(10_000) * PAGE) as i64
    }

    /// Links to the previous and next pages of `path`, empty when there is none.
    fn links(&self, path: &str, q: &str, more: bool) -> (String, String) {
        let p = self.p.unwrap_or(0);
        let link = |p: usize| {
            let q = url::form_urlencoded::byte_serialize(q.as_bytes()).collect::<String>();
            format!("{path}?q={q}&p={p}")
        };
        (
            if p > 0 { link(p - 1) } else { String::new() },
            if more { link(p + 1) } else { String::new() },
        )
    }
}

/// Bounds of the emails or domains that start with `q`.
fn prefix(q: &str) -> (String, String) {
    let q = q.to_ascii_lowercase();
    (q.clone(), format!("{q}\u{10ffff}"))
}

async fn account_rows(
    state: &AppState,
    q: &str,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<AccountRow>> {
    let (lo, hi) = prefix(q);
    let filter = if q.is_empty() {
        ""
    } else {
        "WHERE a.email >= ?3 AND a.email < ?4"
    };
    let rows: Vec<(String, String, String, String, String, Option<i64>, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT a.id, a.email, a.plan, a.account_type, a.created_at, a.suspended_at, (SELECT count(*) FROM sites s WHERE s.account_id = a.id) FROM accounts a {filter} ORDER BY a.rowid DESC LIMIT ?1 OFFSET ?2"
    )))
    .bind(limit)
    .bind(offset)
    .bind(lo)
    .bind(hi)
    .fetch_all(&state.db)
    .await?;
    let admins = admin_emails(&state.db).await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, email, plan, kind, created, suspended, sites)| AccountRow {
                id,
                admin: admins.contains(&email),
                email,
                plan: label(&plan),
                plan_key: plan,
                kind,
                created: when(created.parse().unwrap_or_default()),
                suspended: suspended.is_some(),
                sites,
            },
        )
        .collect())
}

async fn site_rows(
    state: &AppState,
    where_sql: &str,
    binds: &[&str],
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<SiteRow>> {
    let sql = format!(
        "SELECT s.id, s.domain, a.email, s.created_at FROM sites s JOIN accounts a ON a.id = s.account_id {where_sql} ORDER BY s.rowid DESC LIMIT {limit} OFFSET {offset}"
    );
    let mut query = sqlx::query_as::<_, (String, String, String, String)>(sqlx::AssertSqlSafe(sql));
    for b in binds {
        query = query.bind(*b);
    }
    let rows = query.fetch_all(&state.db).await?;
    let today: HashMap<String, u64> = state
        .events
        .events_today()
        .unwrap_or_default()
        .into_iter()
        .collect();
    Ok(rows
        .into_iter()
        .map(|(id, domain, owner, created)| SiteRow {
            events: today.get(&id).copied().unwrap_or(0).to_string(),
            id,
            domain,
            owner,
            created: when(created.parse().unwrap_or_default()),
        })
        .collect())
}

pub async fn overview(
    State(state): State<AppState>,
    admin: Admin,
    Query(v): Query<View>,
) -> Response {
    let started = Instant::now();
    let plans: sqlx::Result<Vec<(String, i64)>> =
        sqlx::query_as("SELECT plan, count(*) FROM accounts GROUP BY plan")
            .fetch_all(&state.db)
            .await;
    let counts: sqlx::Result<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT (SELECT count(*) FROM sites), (SELECT count(*) FROM waitlist), (SELECT count(*) FROM accounts WHERE suspended_at IS NOT NULL), (SELECT count(*) FROM sessions WHERE impersonator IS NOT NULL AND expires_at > ?)",
    )
    .bind(now())
    .fetch_one(&state.db)
    .await;
    let (Ok(plans), Ok((sites, waitlist, suspended, support))) = (plans, counts) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let sqlite_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let mut today = state.events.events_today().unwrap_or_default();
    let events_ms = started.elapsed().as_secs_f64() * 1000.0;
    let total: u64 = today.iter().map(|(_, n)| n).sum();
    today.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let mut busiest = Vec::new();
    for (id, n) in today.into_iter().take(10) {
        let domain: Option<String> = sqlx::query_scalar("SELECT domain FROM sites WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
        busiest.push((domain.unwrap_or(id), n.to_string()));
    }
    let count = |plan: &str| plans.iter().find(|(p, _)| p == plan).map_or(0, |(_, n)| *n);
    let mut page = v.page(&state, &admin, "overview");
    page.stats = vec![
        (
            "Accounts".into(),
            plans.iter().map(|(_, n)| n).sum::<i64>().to_string(),
        ),
        ("Sites".into(), sites.to_string()),
        ("Paid".into(), count("paid").to_string()),
        ("Founding".into(), count("founding").to_string()),
        ("Free access".into(), count("comped").to_string()),
        ("Suspended".into(), suspended.to_string()),
        ("Signups".into(), waitlist.to_string()),
        ("Events, 24 hours".into(), total.to_string()),
    ];
    let on = |set: bool| if set { "Set" } else { "Not set" }.to_string();
    page.health = vec![
        ("Version".into(), env!("CARGO_PKG_VERSION").into()),
        (
            "Mode".into(),
            if state.config.hosted {
                "Hosted"
            } else {
                "Self-hosted"
            }
            .into(),
        ),
        ("Account database".into(), format!("{sqlite_ms:.1} ms")),
        ("Event database".into(), format!("{events_ms:.1} ms")),
        (
            "Sign-in email".into(),
            if matches!(*state.mailer, crate::mail::Mailer::Smtp { .. }) {
                "SMTP"
            } else {
                "Logged only"
            }
            .into(),
        ),
        (
            "Applications feed".into(),
            on(state.admin.applications.is_some()),
        ),
        ("Support sessions open".into(), support.to_string()),
    ];
    page.busiest = busiest;
    page.admins = admin_emails(&state.db).await.unwrap_or_default();
    render(page)
}

pub async fn accounts(
    State(state): State<AppState>,
    admin: Admin,
    Query(v): Query<View>,
) -> Response {
    let mut page = v.page(&state, &admin, "accounts");
    match account_rows(&state, &page.q, PAGE as i64 + 1, v.offset()).await {
        Ok(mut rows) => {
            let more = rows.len() > PAGE;
            rows.truncate(PAGE);
            (page.prev, page.next) = v.links("/admin/accounts", &page.q, more);
            page.accounts = rows;
            render(page)
        }
        Err(err) => unavailable(err, "listing accounts"),
    }
}

pub async fn account(
    State(state): State<AppState>,
    admin: Admin,
    Path(id): Path<String>,
    Query(v): Query<View>,
) -> Response {
    let found: sqlx::Result<Vec<AccountRow>> = async {
        let email: Option<String> = sqlx::query_scalar("SELECT email FROM accounts WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?;
        match email {
            Some(email) => account_rows(&state, &email, 1, 0).await,
            None => Ok(Vec::new()),
        }
    }
    .await;
    let Ok(rows) = found else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(row) = rows.into_iter().find(|r| r.id == id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(sites) = site_rows(&state, "WHERE s.account_id = ?", &[&id], 100, 0).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let mut page = v.page(&state, &admin, "account");
    page.account = Some(row);
    page.sites = sites;
    render(page)
}

pub async fn sites(State(state): State<AppState>, admin: Admin, Query(v): Query<View>) -> Response {
    let mut page = v.page(&state, &admin, "sites");
    let (lo, hi) = prefix(&page.q);
    let rows = if page.q.is_empty() {
        site_rows(&state, "", &[], PAGE as i64 + 1, v.offset()).await
    } else {
        site_rows(
            &state,
            "WHERE s.domain >= ? AND s.domain < ?",
            &[&lo, &hi],
            PAGE as i64 + 1,
            v.offset(),
        )
        .await
    };
    match rows {
        Ok(mut rows) => {
            let more = rows.len() > PAGE;
            rows.truncate(PAGE);
            (page.prev, page.next) = v.links("/admin/sites", &page.q, more);
            page.sites = rows;
            render(page)
        }
        Err(err) => unavailable(err, "listing sites"),
    }
}

pub async fn signups(
    State(state): State<AppState>,
    admin: Admin,
    Query(v): Query<View>,
) -> Response {
    let rows: sqlx::Result<Vec<(String, String, Option<String>, String)>> = sqlx::query_as(
        "SELECT email, wedge, clients, created_at FROM waitlist ORDER BY rowid DESC LIMIT ? OFFSET ?",
    )
    .bind(PAGE as i64 + 1)
    .bind(v.offset())
    .fetch_all(&state.db)
    .await;
    match rows {
        Ok(mut rows) => {
            let more = rows.len() > PAGE;
            rows.truncate(PAGE);
            let mut page = v.page(&state, &admin, "signups");
            (page.prev, page.next) = v.links("/admin/signups", "", more);
            page.signups = rows
                .into_iter()
                .map(|(email, wedge, clients, created)| SignupRow {
                    email,
                    wedge,
                    clients: clients.unwrap_or_default(),
                    created: when(created.parse().unwrap_or_default()),
                })
                .collect();
            render(page)
        }
        Err(err) => unavailable(err, "listing signups"),
    }
}

async fn fetch_applications(state: &AppState) {
    let fetched = async {
        let (url, token) = state.admin.applications.as_ref()?;
        let res = state
            .http
            .get(url)
            .bearer_auth(token)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        res.json::<Vec<Application>>().await.ok()
    }
    .await;
    let mut beta = state.admin.beta.lock().unwrap();
    beta.fetching = false;
    beta.fetched = Some(Instant::now());
    beta.failed = fetched.is_none();
    if let Some(rows) = fetched {
        beta.rows = rows;
    }
}

pub async fn beta(State(state): State<AppState>, admin: Admin, Query(v): Query<View>) -> Response {
    let (rows, failed) = {
        let mut beta = state.admin.beta.lock().unwrap();
        let stale = beta.fetched.is_none_or(|t| t.elapsed() > BETA_TTL);
        if stale && !beta.fetching && state.admin.applications.is_some() {
            beta.fetching = true;
            tokio::spawn({
                let state = state.clone();
                async move { fetch_applications(&state).await }
            });
        }
        (beta.rows.clone(), beta.failed)
    };
    let invited: sqlx::Result<Vec<String>> = sqlx::query_scalar("SELECT email FROM beta_invites")
        .fetch_all(&state.db)
        .await;
    let Ok(invited) = invited else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let mut page = v.page(&state, &admin, "applications");
    page.note = match (&state.admin.applications, failed, rows.is_empty()) {
        (None, ..) => "Applications live with the website. Set VIRLING_APPLICATIONS_URL and VIRLING_APPLICATIONS_TOKEN to list them here.",
        (_, true, _) => "The applications feed did not answer. Showing the last list; reload to retry.",
        (_, _, true) => "No applications yet, or still loading. Reload in a moment.",
        _ => "",
    }
    .into();
    page.applications = rows
        .into_iter()
        .map(|a| ApplicationRow {
            invited: invited.contains(&a.email.to_ascii_lowercase()),
            email: a.email,
            wedge: a.wedge,
            site: a.site.unwrap_or_default(),
            payments: a.payments.unwrap_or_default(),
            revenue: a.revenue.unwrap_or_default(),
            audience: a.audience.unwrap_or_default(),
            created: when(a.created_at / 1000),
        })
        .collect();
    render(page)
}

pub async fn audit_page(
    State(state): State<AppState>,
    admin: Admin,
    Query(v): Query<View>,
) -> Response {
    let rows: sqlx::Result<Vec<(String, String, String, String, i64)>> = sqlx::query_as(
        "SELECT admin, action, target, detail, created_at FROM audit_log ORDER BY id DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await;
    match rows {
        Ok(rows) => {
            let mut page = v.page(&state, &admin, "audit");
            page.audit = rows
                .into_iter()
                .map(|(admin, action, target, detail, at)| AuditRow {
                    admin,
                    action,
                    target,
                    detail,
                    created: when(at),
                })
                .collect();
            render(page)
        }
        Err(err) => unavailable(err, "reading the audit log"),
    }
}

#[derive(Deserialize)]
pub struct Act {
    csrf: String,
    #[serde(default)]
    value: String,
}

async fn target(state: &AppState, id: &str) -> Option<(String, bool)> {
    let email: Option<String> = sqlx::query_scalar("SELECT email FROM accounts WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let email = email?;
    let admin = is_admin(&state.db, &email).await;
    Some((email, admin))
}

pub async fn set_plan(
    State(state): State<AppState>,
    admin: Admin,
    Path(id): Path<String>,
    Form(f): Form<Act>,
) -> Response {
    if !admin.checks(&f.csrf) || !PLANS.iter().any(|(k, _)| *k == f.value) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some((email, _)) = target(&state, &id).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let done = async {
        let mut tx = state.db.begin().await?;
        sqlx::query("UPDATE accounts SET plan = ? WHERE id = ?")
            .bind(&f.value)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        log(&mut tx, &admin.email, "plan", &email, &f.value).await?;
        tx.commit().await
    }
    .await;
    match done {
        Ok(()) => back(&format!("/admin/accounts/{id}"), "plan"),
        Err(err) => unavailable(err, "changing the plan"),
    }
}

/// Suspends the account with value "on", or lifts it with "off".
pub async fn suspend(
    State(state): State<AppState>,
    admin: Admin,
    Path(id): Path<String>,
    Form(f): Form<Act>,
) -> Response {
    if !admin.checks(&f.csrf) || !matches!(f.value.as_str(), "on" | "off") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some((email, protected)) = target(&state, &id).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = format!("/admin/accounts/{id}");
    let on = f.value == "on";
    if on && protected {
        return back(&path, "admin");
    }
    let done = async {
        let mut tx = state.db.begin().await?;
        sqlx::query("UPDATE accounts SET suspended_at = ? WHERE id = ?")
            .bind(on.then(now))
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        if on {
            sqlx::query("DELETE FROM sessions WHERE account_id = ?")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
        }
        log(
            &mut tx,
            &admin.email,
            if on { "suspend" } else { "unsuspend" },
            &email,
            "",
        )
        .await?;
        tx.commit().await
    }
    .await;
    match done {
        Ok(()) => back(&path, if on { "suspended" } else { "unsuspended" }),
        Err(err) => unavailable(err, "suspending the account"),
    }
}

/// Starts a read-only support session as the account, never for an admin.
pub async fn sign_in_as(
    State(state): State<AppState>,
    admin: Admin,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(f): Form<Act>,
) -> Response {
    if !admin.checks(&f.csrf) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some((email, protected)) = target(&state, &id).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = format!("/admin/accounts/{id}");
    if protected {
        return back(&path, "admin");
    }
    let session = random_token();
    let done = async {
        let mut tx = state.db.begin().await?;
        let active = sqlx::query(
            "INSERT INTO sessions (token_hash, account_id, created_at, expires_at, impersonator) SELECT ?, id, ?, ?, ? FROM accounts WHERE id = ? AND suspended_at IS NULL",
        )
        .bind(hash_token(&session))
        .bind(now())
        .bind(now() + SUPPORT_TTL_SECS)
        .bind(&admin.email)
        .bind(&id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if active == 0 {
            return Ok(false);
        }
        log(&mut tx, &admin.email, "sign_in_as", &email, "30 minutes, read only").await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(true)
    }
    .await;
    match done {
        Ok(true) => {}
        Ok(false) => return back(&path, "inactive"),
        Err(err) => return unavailable(err, "starting a support session"),
    }
    let secure = if state.config.base_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    let set = |name: &str, value: &str| {
        HeaderValue::from_str(&format!(
            "{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SUPPORT_TTL_SECS}{secure}"
        ))
        .expect("ascii cookie")
    };
    let mut res = Redirect::to("/app").into_response();
    let h = res.headers_mut();
    h.append(header::SET_COOKIE, set(SESSION_COOKIE, &session));
    if let Some(own) = cookie(&headers, SESSION_COOKIE) {
        h.append(header::SET_COOKIE, set(RETURN_COOKIE, &own));
    }
    res
}

/// Ends a support session and returns to the operator's own sign-in.
pub async fn stop(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let ended: Option<(String, String)> = match cookie(&headers, SESSION_COOKIE) {
        Some(token) => sqlx::query_as(
            "DELETE FROM sessions WHERE token_hash = ? AND impersonator IS NOT NULL RETURNING impersonator, (SELECT email FROM accounts WHERE id = account_id)",
        )
        .bind(hash_token(&token))
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten(),
        None => None,
    };
    let Some((by, email)) = ended else {
        return Redirect::to("/app").into_response();
    };
    if let Ok(mut conn) = state.db.acquire().await {
        let _ = log(&mut conn, &by, "sign_in_as_end", &email, "").await;
    }
    let own = cookie(&headers, RETURN_COOKIE)
        .filter(|t| t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit()));
    let secure = if state.config.base_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    let mut res = Redirect::to("/admin").into_response();
    let h = res.headers_mut();
    let value = |name: &str, v: &str, age: i64| {
        HeaderValue::from_str(&format!(
            "{name}={v}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}{secure}"
        ))
        .expect("ascii cookie")
    };
    match own {
        Some(t) => h.append(
            header::SET_COOKIE,
            value(SESSION_COOKIE, &t, 30 * 24 * 3600),
        ),
        None => h.append(header::SET_COOKIE, value(SESSION_COOKIE, "", 0)),
    };
    h.append(header::SET_COOKIE, value(RETURN_COOKIE, "", 0));
    res
}

#[derive(Deserialize)]
pub struct Typed {
    csrf: String,
    #[serde(default)]
    confirm: String,
}

async fn purge(state: &AppState, sites: &[(String, String)]) -> bool {
    let mut clean = true;
    for (id, domain) in sites {
        state.sites.invalidate(domain);
        if let Err(err) = state.events.purge_site(id) {
            tracing::error!(%err, site_id = id, "purging site events failed");
            clean = false;
        }
    }
    clean
}

pub async fn delete_site(
    State(state): State<AppState>,
    admin: Admin,
    Path(id): Path<String>,
    Form(f): Form<Typed>,
) -> Response {
    if !admin.checks(&f.csrf) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let site: sqlx::Result<Option<(String, String)>> =
        sqlx::query_as("SELECT domain, account_id FROM sites WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.db)
            .await;
    let Ok(Some((domain, owner))) = site else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = format!("/admin/accounts/{owner}");
    if !equal(&f.confirm.trim().to_ascii_lowercase(), &domain) {
        return back(&path, "nomatch");
    }
    let done = async {
        let mut tx = state.db.begin().await?;
        log(&mut tx, &admin.email, "delete_site", &domain, "").await?;
        sqlx::query("DELETE FROM sites WHERE id = ?")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await
    }
    .await;
    if let Err(err) = done {
        return unavailable(err, "deleting the site");
    }
    back(
        &path,
        if purge(&state, &[(id, domain)]).await {
            "deleted"
        } else {
            "purge"
        },
    )
}

pub async fn delete_account(
    State(state): State<AppState>,
    admin: Admin,
    Path(id): Path<String>,
    Form(f): Form<Typed>,
) -> Response {
    if !admin.checks(&f.csrf) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some((email, protected)) = target(&state, &id).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = format!("/admin/accounts/{id}");
    if protected {
        return back(&path, "admin");
    }
    if !equal(&f.confirm.trim().to_ascii_lowercase(), &email) {
        return back(&path, "nomatch");
    }
    let done = async {
        let mut tx = state.db.begin().await?;
        let sites: Vec<(String, String)> =
            sqlx::query_as("SELECT id, domain FROM sites WHERE account_id = ?")
                .bind(&id)
                .fetch_all(&mut *tx)
                .await?;
        log(
            &mut tx,
            &admin.email,
            "delete_account",
            &email,
            &format!("{} sites", sites.len()),
        )
        .await?;
        sqlx::query("DELETE FROM sites WHERE account_id = ?")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM accounts WHERE id = ?")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(sites)
    }
    .await;
    match done {
        Ok(sites) => back(
            "/admin/accounts",
            if purge(&state, &sites).await {
                "deleted"
            } else {
                "purge"
            },
        ),
        Err(err) => unavailable(err, "deleting the account"),
    }
}

#[derive(Deserialize)]
pub struct Invite {
    csrf: String,
    email: String,
}

async fn send_invite(state: &AppState, to: &str) -> anyhow::Result<()> {
    let link = format!("{}/login", state.config.base_url);
    let text = format!(
        "Your Virling account is ready.\n\nSign in with this email address:\n{link}\n\nWe email you a one-time sign-in link that works for 15 minutes."
    );
    let html = format!(
        "<p>Your Virling account is ready.</p><p><a href=\"{link}\">Sign in to Virling</a> with this email address. We email you a one-time sign-in link that works for 15 minutes.</p>"
    );
    state
        .mailer
        .send(to, "Your Virling account is ready", text, html)
        .await
}

/// Approves an applicant and emails the invite.
pub async fn invite(
    State(state): State<AppState>,
    admin: Admin,
    Form(f): Form<Invite>,
) -> Response {
    if !admin.checks(&f.csrf) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let email = f.email.trim().to_ascii_lowercase();
    let applied = state
        .admin
        .beta
        .lock()
        .unwrap()
        .rows
        .iter()
        .any(|a| a.email.eq_ignore_ascii_case(&email));
    if !applied || !valid_email(&email) {
        return back("/admin/applications", "nomatch");
    }
    if matches!(*state.mailer, crate::mail::Mailer::Log) {
        return back("/admin/applications", "nomail");
    }
    let recorded = async {
        let mut tx = state.db.begin().await?;
        let new = sqlx::query(
            "INSERT INTO beta_invites (email, invited_by, invited_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
        )
        .bind(&email)
        .bind(&admin.email)
        .bind(now())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if new == 1 {
            log(&mut tx, &admin.email, "beta_invite", &email, "").await?;
            tx.commit().await?;
        }
        Ok::<_, sqlx::Error>(new == 1)
    }
    .await;
    match recorded {
        Ok(true) => {}
        Ok(false) => return back("/admin/applications", "already"),
        Err(err) => return unavailable(err, "recording the invite"),
    }
    if let Err(err) = send_invite(&state, &email).await {
        tracing::error!(%err, "sending the invite failed");
        let undo = async {
            let mut tx = state.db.begin().await?;
            sqlx::query("DELETE FROM beta_invites WHERE email = ?")
                .bind(&email)
                .execute(&mut *tx)
                .await?;
            log(&mut tx, &admin.email, "beta_invite_failed", &email, "").await?;
            tx.commit().await
        }
        .await;
        if let Err(err) = undo {
            tracing::error!(%err, "undoing the invite failed");
        }
        return back("/admin/applications", "failed");
    }
    back("/admin/applications", "invited")
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::Request;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tower::ServiceExt;

    use super::*;
    use crate::app::{Config, router};
    use crate::events::DuckStore;

    const ADMIN: &str = "boss@example.com";

    struct Fixture {
        _dir: tempfile::TempDir,
        state: AppState,
        admin: String,
        user: String,
        user_cookie: String,
        site: String,
    }

    fn cookie_of(token: &str) -> String {
        format!("{SESSION_COOKIE}={token}")
    }

    async fn session(state: &AppState, email: &str) -> (String, String) {
        let id = crate::sites::find_or_create_account(&state.db, email)
            .await
            .unwrap();
        let token = random_token();
        sqlx::query("INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
            .bind(hash_token(&token))
            .bind(&id)
            .bind(now())
            .bind(now() + 3600)
            .execute(&state.db)
            .await
            .unwrap();
        (id, token)
    }

    async fn setup() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::open_sqlite(dir.path()).await.unwrap();
        let store = Arc::new(DuckStore::open(&dir.path().join("events.duckdb")).unwrap());
        let config = Config {
            marketing: false,
            app: true,
            trust_proxy: false,
            events_per_second: 200,
            base_url: "http://localhost:8080".into(),
            mcp_url: None,
            waitlist_from: "Virling <hello@virling.com>".into(),
            hosted: true,
            ai_keys: Default::default(),
            ai_budget_usd: None,
            turnstile: None,
            billing: None,
            self_track_site: None,
        };
        let state = AppState::new(
            config,
            db,
            store,
            crate::mail::Mailer::Capture(Default::default()),
            crate::crypto::Vault::load(dir.path(), None).unwrap(),
        );
        for email in [ADMIN, "second@example.com"] {
            sqlx::query("INSERT INTO admins (email, created_at) VALUES (?, ?)")
                .bind(email)
                .bind(now())
                .execute(&state.db)
                .await
                .unwrap();
        }
        let (_, admin) = session(&state, ADMIN).await;
        let (user, token) = session(&state, "me@example.com").await;
        let site = crate::sites::create_site(&state.db, &user, "example.com", "UTC")
            .await
            .unwrap();
        Fixture {
            _dir: dir,
            state,
            admin,
            user,
            user_cookie: cookie_of(&token),
            site,
        }
    }

    fn csrf(token: &str) -> String {
        hash_token(&format!("admin-csrf:{token}"))
    }

    async fn send(state: &AppState, req: Request<Body>) -> Response {
        router(state.clone())
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5555))))
            .oneshot(req)
            .await
            .unwrap()
    }

    /// Every call gets a fresh rate limit window unless a test shares the state itself.
    async fn call(state: &AppState, req: Request<Body>) -> Response {
        let state = AppState {
            limiter: Arc::default(),
            ..state.clone()
        };
        send(&state, req).await
    }

    fn get(uri: &str, token: Option<&str>) -> Request<Body> {
        let mut req = Request::get(uri);
        if let Some(t) = token {
            req = req.header("cookie", cookie_of(t));
        }
        req.body(Body::empty()).unwrap()
    }

    fn post(uri: &str, cookie: &str, origin: Option<&str>, body: String) -> Request<Body> {
        let mut req = Request::post(uri)
            .header("cookie", cookie)
            .header("content-type", "application/x-www-form-urlencoded");
        if let Some(o) = origin {
            req = req.header("origin", o);
        }
        req.body(Body::from(body)).unwrap()
    }

    /// An admin POST from the app's own origin with the right token.
    fn change(uri: &str, token: &str, fields: &str) -> Request<Body> {
        post(
            uri,
            &cookie_of(token),
            Some("http://localhost:8080"),
            format!("csrf={}&{fields}", csrf(token)),
        )
    }

    async fn text(res: Response) -> String {
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn signed_in_users_skip_the_sign_in_page() {
        let f = setup().await;
        let user = f
            .user_cookie
            .trim_start_matches("virling_session=")
            .to_string();
        let res = call(&f.state, get("/login?vid=abc", Some(&user))).await;
        assert_eq!(location(&res), "/app");
        let res = call(&f.state, get("/login", None)).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    fn location(res: &Response) -> String {
        res.headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    }

    async fn audit_actions(state: &AppState) -> Vec<(String, String, String)> {
        sqlx::query_as("SELECT admin, action, target FROM audit_log ORDER BY id")
            .fetch_all(&state.db)
            .await
            .unwrap()
    }

    /// Serves `body` with `status` to every request and records the raw requests.
    async fn mock(status: &'static str, body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 16384];
                let mut n = 0;
                loop {
                    let read = sock.read(&mut buf[n..]).await.unwrap();
                    n += read;
                    let raw = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let Some(end) = raw.find("\r\n\r\n") else {
                        if read == 0 {
                            break;
                        }
                        continue;
                    };
                    let len = raw[..end]
                        .to_ascii_lowercase()
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: ").map(str::to_string))
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    if n >= end + 4 + len || read == 0 {
                        break;
                    }
                }
                record
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[..n]).into_owned());
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                sock.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (url, seen)
    }

    #[tokio::test]
    async fn only_listed_admins_get_in() {
        let f = setup().await;
        let user = f.user_cookie.trim_start_matches("virling_session=");
        for path in [
            "/admin",
            "/admin/accounts",
            "/admin/audit",
            "/admin/applications",
        ] {
            assert_eq!(
                call(&f.state, get(path, None)).await.status(),
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                call(&f.state, get(path, Some(user))).await.status(),
                StatusCode::NOT_FOUND
            );
            let res = call(&f.state, get(path, Some(&f.admin))).await;
            assert_eq!(res.status(), StatusCode::OK, "{path}");
            assert_eq!(res.headers()["cache-control"], "no-store");
        }
        let forged = post(
            "/admin/accounts/x/suspend",
            &cookie_of(user),
            Some("http://localhost:8080"),
            "csrf=x&value=on".into(),
        );
        assert_eq!(call(&f.state, forged).await.status(), StatusCode::NOT_FOUND);
        sqlx::query("DELETE FROM admins WHERE email = ?")
            .bind(ADMIN)
            .execute(&f.state.db)
            .await
            .unwrap();
        assert_eq!(
            call(&f.state, get("/admin", Some(&f.admin))).await.status(),
            StatusCode::NOT_FOUND
        );
        sqlx::query("INSERT INTO admins (email, created_at) VALUES (?, 0)")
            .bind(ADMIN)
            .execute(&f.state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE accounts SET suspended_at = 1 WHERE email = ?")
            .bind(ADMIN)
            .execute(&f.state.db)
            .await
            .unwrap();
        assert_eq!(
            call(&f.state, get("/admin", Some(&f.admin))).await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn changes_need_the_same_origin_and_token() {
        let f = setup().await;
        let uri = format!("/admin/accounts/{}/plan", f.user);
        let body = format!("csrf={}&value=founding", csrf(&f.admin));
        let origins = [None, Some("https://evil.example"), Some("null")];
        for origin in origins {
            let res = call(
                &f.state,
                post(&uri, &cookie_of(&f.admin), origin, body.clone()),
            )
            .await;
            assert_eq!(res.status(), StatusCode::FORBIDDEN, "{origin:?}");
        }
        let fetch = Request::post(&uri)
            .header("cookie", cookie_of(&f.admin))
            .header("sec-fetch-site", "same-origin")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("csrf=wrong&value=founding"))
            .unwrap();
        assert_eq!(
            call(&f.state, fetch).await.status(),
            StatusCode::BAD_REQUEST
        );
        let plan = || async {
            sqlx::query_scalar::<_, String>("SELECT plan FROM accounts WHERE id = ?")
                .bind(&f.user)
                .fetch_one(&f.state.db)
                .await
                .unwrap()
        };
        assert_eq!(plan().await, "self_hosted");
        assert!(!crate::answers::paid(&f.state, &f.site).await.unwrap());
        let bad = change(&uri, &f.admin, "value=gold");
        assert_eq!(call(&f.state, bad).await.status(), StatusCode::BAD_REQUEST);
        let res = call(&f.state, change(&uri, &f.admin, "value=comped")).await;
        assert!(location(&res).ends_with("?n=plan"));
        assert_eq!(plan().await, "comped");
        assert!(
            crate::answers::paid(&f.state, &f.site).await.unwrap(),
            "free access unlocks paid features"
        );
        assert_eq!(
            audit_actions(&f.state).await,
            [(
                ADMIN.to_string(),
                "plan".to_string(),
                "me@example.com".to_string()
            )]
        );
        let page = text(call(&f.state, get("/admin/audit", Some(&f.admin))).await).await;
        assert!(page.contains("comped") && page.contains("me@example.com"));
    }

    #[tokio::test]
    async fn suspended_accounts_are_locked_out() {
        let f = setup().await;
        let key = crate::api::create_key(&f.state.db, &f.user, "ci")
            .await
            .unwrap();
        let user = f
            .user_cookie
            .trim_start_matches("virling_session=")
            .to_string();
        let open = call(&f.state, get("/app/example.com", Some(&user))).await;
        assert_eq!(open.status(), StatusCode::OK);
        let uri = format!("/admin/accounts/{}/suspend", f.user);
        let res = call(&f.state, change(&uri, &f.admin, "value=on")).await;
        assert!(location(&res).ends_with("?n=suspended"));
        let shut = call(&f.state, get("/app/example.com", Some(&user))).await;
        assert_eq!(location(&shut), "/login");
        assert!(crate::api::key_account(&f.state.db, &key).await.is_none());
        let (again, token) = session(&f.state, "me@example.com").await;
        assert_eq!(again, f.user);
        assert_eq!(
            location(&call(&f.state, get("/app/example.com", Some(&token))).await),
            "/login"
        );
        call(&f.state, change(&uri, &f.admin, "value=off")).await;
        assert_eq!(
            call(&f.state, get("/app/example.com", Some(&token)))
                .await
                .status(),
            StatusCode::OK
        );
        assert!(crate::api::key_account(&f.state.db, &key).await.is_some());
        let actions: Vec<String> = audit_actions(&f.state)
            .await
            .into_iter()
            .map(|a| a.1)
            .collect();
        assert_eq!(actions, ["suspend", "unsuspend"]);
    }

    #[tokio::test]
    async fn admins_cannot_be_suspended_or_impersonated() {
        let f = setup().await;
        let (other, _) = session(&f.state, "second@example.com").await;
        for (path, fields) in [
            ("suspend", "value=on"),
            ("sign-in", ""),
            ("delete", "confirm=second%40example.com"),
        ] {
            let uri = format!("/admin/accounts/{other}/{path}");
            let res = call(&f.state, change(&uri, &f.admin, fields)).await;
            assert!(location(&res).ends_with("?n=admin"), "{path}");
        }
        let open: i64 =
            sqlx::query_scalar("SELECT count(*) FROM sessions WHERE impersonator IS NOT NULL")
                .fetch_one(&f.state.db)
                .await
                .unwrap();
        assert_eq!(open, 0);
        let alive: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM accounts WHERE id = ? AND suspended_at IS NULL",
        )
        .bind(&other)
        .fetch_one(&f.state.db)
        .await
        .unwrap();
        assert_eq!(alive, 1);
        assert!(audit_actions(&f.state).await.is_empty());
    }

    #[tokio::test]
    async fn support_sessions_are_read_only_and_timed() {
        let f = setup().await;
        let uri = format!("/admin/accounts/{}/sign-in", f.user);
        let res = call(&f.state, change(&uri, &f.admin, "")).await;
        assert_eq!(location(&res), "/app");
        let cookies: Vec<String> = res
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect();
        assert_eq!(cookies.len(), 2);
        assert!(cookies[0].contains("Max-Age=1800") && cookies[0].contains("HttpOnly"));
        assert!(cookies[1].starts_with(&format!("virling_return={}", f.admin)));
        let support = cookies[0]
            .split(';')
            .next()
            .unwrap()
            .trim_start_matches("virling_session=")
            .to_string();
        let (by, expires): (String, i64) =
            sqlx::query_as("SELECT impersonator, expires_at FROM sessions WHERE token_hash = ?")
                .bind(hash_token(&support))
                .fetch_one(&f.state.db)
                .await
                .unwrap();
        assert_eq!(by, ADMIN);
        assert!(expires <= now() + SUPPORT_TTL_SECS);
        let page = text(call(&f.state, get("/app/example.com", Some(&support))).await).await;
        assert!(
            page.contains("support-bar") && page.contains("me@example.com") && page.contains(ADMIN)
        );
        let own = text(call(&f.state, get("/app/example.com", Some(&f.admin))).await).await;
        assert!(!own.contains("support-bar"));
        let write = post(
            "/app/example.com/account/theme",
            &cookie_of(&support),
            Some("http://localhost:8080"),
            "theme=dark".into(),
        );
        assert_eq!(call(&f.state, write).await.status(), StatusCode::FORBIDDEN);
        for path in ["/admin", "/admin/audit"] {
            assert_eq!(
                call(&f.state, get(path, Some(&support))).await.status(),
                StatusCode::NOT_FOUND
            );
        }
        let stop = post(
            "/admin/stop",
            &format!("{}; virling_return={}", cookie_of(&support), f.admin),
            None,
            String::new(),
        );
        let res = call(&f.state, stop).await;
        assert_eq!(location(&res), "/admin");
        let restored = res.headers().get_all("set-cookie").iter().any(|v| {
            v.to_str()
                .unwrap()
                .starts_with(&format!("virling_session={};", f.admin))
        });
        assert!(restored);
        let gone = call(&f.state, get("/app/example.com", Some(&support))).await;
        assert_eq!(location(&gone), "/login");
        let actions: Vec<String> = audit_actions(&f.state)
            .await
            .into_iter()
            .map(|a| a.1)
            .collect();
        assert_eq!(actions, ["sign_in_as", "sign_in_as_end"]);
    }

    #[tokio::test]
    async fn deleting_needs_the_typed_name_and_removes_events() {
        let f = setup().await;
        let hit = Request::post("/api/event")
            .header(
                "user-agent",
                "Mozilla/5.0 (Macintosh) AppleWebKit/605 Version/17.0 Safari/605.1",
            )
            .body(Body::from(
                r#"{"n":"pageview","u":"https://example.com/","s":"example.com","w":1280}"#,
            ))
            .unwrap();
        assert_eq!(call(&f.state, hit).await.status(), StatusCode::ACCEPTED);
        assert_eq!(f.state.events.count_events(&f.site).unwrap(), 1);
        let uri = format!("/admin/sites/{}/delete", f.site);
        let wrong = call(&f.state, change(&uri, &f.admin, "confirm=other.com")).await;
        assert!(location(&wrong).ends_with("?n=nomatch"));
        assert_eq!(f.state.events.count_events(&f.site).unwrap(), 1);
        let right = call(&f.state, change(&uri, &f.admin, "confirm=Example.com")).await;
        assert!(location(&right).ends_with("?n=deleted"));
        assert_eq!(f.state.events.count_events(&f.site).unwrap(), 0);
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM sites")
            .fetch_one(&f.state.db)
            .await
            .unwrap();
        assert_eq!(rows, 0);
        let again = Request::post("/api/event")
            .header(
                "user-agent",
                "Mozilla/5.0 (Macintosh) AppleWebKit/605 Version/17.0 Safari/605.1",
            )
            .body(Body::from(
                r#"{"n":"pageview","u":"https://example.com/","s":"example.com","w":1280}"#,
            ))
            .unwrap();
        call(&f.state, again).await;
        assert_eq!(
            f.state.events.count_events(&f.site).unwrap(),
            0,
            "the deleted site stops collecting"
        );

        crate::sites::create_site(&f.state.db, &f.user, "second.dev", "UTC")
            .await
            .unwrap();
        let uri = format!("/admin/accounts/{}/delete", f.user);
        let wrong = call(
            &f.state,
            change(&uri, &f.admin, "confirm=nobody%40example.com"),
        )
        .await;
        assert!(location(&wrong).ends_with("?n=nomatch"));
        let right = call(&f.state, change(&uri, &f.admin, "confirm=me%40example.com")).await;
        assert_eq!(location(&right), "/admin/accounts?n=deleted");
        for sql in [
            "SELECT count(*) FROM accounts WHERE email = 'me@example.com'",
            "SELECT count(*) FROM sites",
            "SELECT count(*) FROM sessions WHERE impersonator IS NULL AND account_id NOT IN (SELECT id FROM accounts)",
        ] {
            let n: i64 = sqlx::query_scalar(sql)
                .fetch_one(&f.state.db)
                .await
                .unwrap();
            assert_eq!(n, 0, "{sql}");
        }
        let actions: Vec<String> = audit_actions(&f.state)
            .await
            .into_iter()
            .map(|a| a.1)
            .collect();
        assert_eq!(actions, ["delete_site", "delete_account"]);
    }

    #[tokio::test]
    async fn beta_invites_go_out_through_the_mailer() {
        let mut f = setup().await;
        let feed = r#"[{"email":"Ann@Example.com","wedge":"agency","created_at":1790000000000,"site":"ann.dev","payments":"stripe","revenue":"1k_10k","audience":"clients"},{"email":"bob@example.com","wedge":"selfhost","created_at":1790000100000,"site":null,"payments":null,"revenue":null,"audience":null}]"#;
        let (feed_url, feed_seen) = mock("200 OK", feed).await;
        f.state.admin = Arc::new(Settings::new(
            Some(feed_url.clone()),
            Some("feed_secret".into()),
        ));
        let sent_mail = |f: &Fixture| match &*f.state.mailer {
            crate::mail::Mailer::Capture(m) => m.lock().unwrap().clone(),
            _ => Vec::new(),
        };
        let unknown = call(
            &f.state,
            change(
                "/admin/applications/invite",
                &f.admin,
                "email=ann%40example.com",
            ),
        )
        .await;
        assert!(
            location(&unknown).ends_with("?n=nomatch"),
            "applications must be loaded first"
        );
        fetch_applications(&f.state).await;
        assert!(
            feed_seen.lock().unwrap()[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer feed_secret")
        );
        let page = text(call(&f.state, get("/admin/applications", Some(&f.admin))).await).await;
        assert!(
            page.contains("Ann@Example.com")
                && page.contains("ann.dev")
                && page.contains("Approve and invite")
        );
        assert!(!page.contains("feed_secret"));
        let stranger = call(
            &f.state,
            change(
                "/admin/applications/invite",
                &f.admin,
                "email=eve%40example.com",
            ),
        )
        .await;
        assert!(location(&stranger).ends_with("?n=nomatch"));
        assert!(sent_mail(&f).is_empty());
        let sent = call(
            &f.state,
            change(
                "/admin/applications/invite",
                &f.admin,
                "email=ann%40example.com",
            ),
        )
        .await;
        assert!(location(&sent).ends_with("?n=invited"));
        let mail = sent_mail(&f);
        assert_eq!(mail.len(), 1);
        assert_eq!(mail[0].0, "ann@example.com");
        assert!(mail[0].2.contains("localhost:8080/login"));
        let twice = call(
            &f.state,
            change(
                "/admin/applications/invite",
                &f.admin,
                "email=ann%40example.com",
            ),
        )
        .await;
        assert!(location(&twice).ends_with("?n=already"));
        assert_eq!(sent_mail(&f).len(), 1);
        let page = text(call(&f.state, get("/admin/applications", Some(&f.admin))).await).await;
        assert!(page.contains("Invited"));
        fetch_applications(&f.state).await;
        f.state.mailer = Arc::new(
            crate::mail::Mailer::new(Some("smtp://127.0.0.1:1"), "Virling <login@example.com>")
                .unwrap(),
        );
        let failed = call(
            &f.state,
            change(
                "/admin/applications/invite",
                &f.admin,
                "email=bob%40example.com",
            ),
        )
        .await;
        assert!(location(&failed).ends_with("?n=failed"));
        let kept: i64 =
            sqlx::query_scalar("SELECT count(*) FROM beta_invites WHERE email = 'bob@example.com'")
                .fetch_one(&f.state.db)
                .await
                .unwrap();
        assert_eq!(kept, 0, "a failed send can be retried");
        f.state.mailer = Arc::new(crate::mail::Mailer::Log);
        let unset = call(
            &f.state,
            change(
                "/admin/applications/invite",
                &f.admin,
                "email=bob%40example.com",
            ),
        )
        .await;
        assert!(location(&unset).ends_with("?n=nomail"));
        let actions: Vec<String> = audit_actions(&f.state)
            .await
            .into_iter()
            .map(|a| a.1)
            .collect();
        assert_eq!(
            actions,
            ["beta_invite", "beta_invite", "beta_invite_failed"]
        );
    }

    #[tokio::test]
    async fn pages_list_the_data_and_are_rate_limited() {
        let f = setup().await;
        sqlx::query("INSERT INTO waitlist (email, wedge, created_at, leave_token) VALUES ('w@example.com', 'agency', '1790000000', 'tok')")
            .execute(&f.state.db)
            .await
            .unwrap();
        for (path, needle) in [
            ("/admin", "Events, 24 hours"),
            ("/admin/accounts?q=me%40", "me@example.com"),
            ("/admin/sites?q=exam", "example.com"),
            ("/admin/signups", "w@example.com"),
            ("/admin/audit", "Nothing yet"),
        ] {
            let page = text(call(&f.state, get(path, Some(&f.admin))).await).await;
            assert!(page.contains(needle), "{path}");
        }
        let miss = text(call(&f.state, get("/admin/accounts?q=zzz", Some(&f.admin))).await).await;
        assert!(!miss.contains("me@example.com"));
        let detail = text(
            call(
                &f.state,
                get(&format!("/admin/accounts/{}", f.user), Some(&f.admin)),
            )
            .await,
        )
        .await;
        assert!(detail.contains("example.com") && detail.contains("Sign in as"));
        let missing = call(&f.state, get("/admin/accounts/nope", Some(&f.admin))).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let mut limited = 0;
        for _ in 0..30 {
            if send(&f.state, change("/admin/accounts/x/suspend", &f.admin, ""))
                .await
                .status()
                == StatusCode::TOO_MANY_REQUESTS
            {
                limited += 1;
            }
        }
        assert!(limited > 0, "a flood of admin requests is turned away");
    }

    fn sources(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let code = text.split("#[cfg(test)]").next().unwrap_or_default();
                out.push((path.display().to_string(), code.to_ascii_lowercase()));
            }
        }
    }

    #[test]
    fn no_code_writes_the_admins_table() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        sources(&root.join("src"), &mut files);
        sources(&root.join("worker/src"), &mut files);
        assert!(files.len() > 20);
        for (path, code) in files {
            let words: Vec<&str> = code
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .filter(|w| !w.is_empty())
                .collect();
            for (i, w) in words.iter().enumerate() {
                if *w != "admins" || i == 0 {
                    continue;
                }
                let prev = words[i - 1];
                let before = if i > 1 { words[i - 2] } else { "" };
                let writes = matches!(prev, "into" | "update" | "table")
                    || (prev == "from" && before == "delete");
                assert!(!writes, "{path} writes the admins table");
            }
        }
    }

    #[tokio::test]
    async fn admin_sessions_cannot_change_admins() {
        let f = setup().await;
        let (other, _) = session(&f.state, "second@example.com").await;
        let before: Vec<String> = admin_emails(&f.state.db).await.unwrap();
        assert_eq!(before, ["boss@example.com", "second@example.com"]);
        let mut uris = vec![
            "/admin/admins".to_string(),
            "/admin/admins/new".to_string(),
            "/admin/admins/delete".to_string(),
            format!("/admin/accounts/{}/admin", f.user),
            format!("/admin/accounts/{}/make-admin", f.user),
        ];
        for action in ["plan", "suspend", "sign-in", "delete"] {
            uris.push(format!("/admin/accounts/{other}/{action}"));
            uris.push(format!("/admin/accounts/{}/{action}", f.user));
        }
        for uri in &uris {
            let fields = "value=comped&confirm=me%40example.com&email=me%40example.com";
            let res = call(&f.state, change(uri, &f.admin, fields)).await;
            let unknown = uri.contains("/admins") || uri.ends_with("admin");
            if unknown {
                assert!(
                    matches!(
                        res.status(),
                        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
                    ),
                    "{uri}"
                );
            }
        }
        for method in ["PUT", "PATCH", "DELETE"] {
            let req = Request::builder()
                .method(method)
                .uri("/admin/admins")
                .header("cookie", cookie_of(&f.admin))
                .body(Body::empty())
                .unwrap();
            let res = call(&f.state, req).await;
            assert!(
                matches!(
                    res.status(),
                    StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
                ),
                "{method}"
            );
        }
        assert_eq!(admin_emails(&f.state.db).await.unwrap(), before);
        let page = text(call(&f.state, get("/admin", Some(&f.admin))).await).await;
        assert!(page.contains("second@example.com") && page.contains("Read only"));
    }
}
