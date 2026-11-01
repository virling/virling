use std::sync::Arc;

use askama::Template;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::crypto::Vault;
use crate::events::EventStore;
use crate::fx::Rates;
use crate::ingest::{self, DailySalt, RateLimiter, Sessions};
use crate::mail::Mailer;
use crate::sites::SiteCache;
use crate::{
    api, auth, crawlers, dashboard, flags, links, mcp, oauth, onboarding, retention_page, settings,
    visitor_pages, waitlist,
};

#[derive(Debug, Clone)]
pub struct Config {
    /// Serve the hosted product's marketing pages at / and /agency.
    pub marketing: bool,
    /// Serve sign-in and the app. Off during the validation phase.
    pub app: bool,
    /// Read the client IP from X-Forwarded-For (behind a reverse proxy).
    pub trust_proxy: bool,
    pub events_per_second: u32,
    /// Public URL of this server, used in emails and script snippets, without a trailing slash.
    pub base_url: String,
    /// Optional separate https origin that serves only the MCP endpoint, without a trailing slash.
    pub mcp_url: Option<String>,
    /// Sender for waitlist emails; replies go to this address.
    pub waitlist_from: String,
    /// The hosted service, where Agency teams can buy seats past the 3 included.
    pub hosted: bool,
    /// Assistant keys every site's AI answers use; a site may add its own only when self-hosted.
    pub ai_keys: crate::answers::ServerKeys,
    /// Monthly USD after which AI answer runs pause, server-wide.
    pub ai_budget_usd: Option<f64>,
    /// Cloudflare Turnstile on the sign-in form; off unless both keys are set.
    pub turnstile: Option<Turnstile>,
    /// Dodo billing for the hosted plans; None on self-hosted servers.
    pub billing: Option<crate::billing::Dodo>,
    /// Site whose script the sign-in page loads, so visits that start on the app are tracked too.
    pub self_track_site: Option<String>,
}

#[derive(Clone)]
pub struct Turnstile {
    pub site_key: String,
    pub secret: String,
    pub verify_url: String,
}

impl std::fmt::Debug for Turnstile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Turnstile")
            .field("site_key", &self.site_key)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: SqlitePool,
    pub events: Arc<dyn EventStore>,
    pub sites: Arc<SiteCache>,
    pub sessions: Arc<Sessions>,
    pub salt: Arc<DailySalt>,
    pub limiter: Arc<RateLimiter>,
    pub mailer: Arc<Mailer>,
    pub vault: Arc<Vault>,
    pub http: reqwest::Client,
    /// Fetches site favicons from public hosts only.
    pub favicons: reqwest::Client,
    pub rates: Arc<Rates>,
    pub presence: Arc<crate::live::Presence>,
    pub pins: Arc<crate::live::Pins>,
    pub admin: Arc<crate::admin::Settings>,
}

impl AppState {
    pub fn new(
        config: Config,
        db: SqlitePool,
        events: Arc<dyn EventStore>,
        mailer: Mailer,
        vault: Vault,
    ) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("virling/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("http client");
        Self {
            vault: Arc::new(vault),
            rates: Arc::new(Rates::new(http.clone())),
            http,
            favicons: crate::favicon::client(),
            config: Arc::new(config),
            db,
            events,
            sites: Arc::default(),
            sessions: Arc::default(),
            salt: Arc::default(),
            limiter: Arc::default(),
            mailer: Arc::new(mailer),
            presence: Arc::default(),
            pins: Arc::default(),
            admin: Arc::default(),
        }
    }
}

pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/v.js", get(tracker))
        .route(
            "/v-compat.js",
            get(|| {
                asset(
                    "text/javascript; charset=utf-8",
                    include_str!("../assets/v-compat.js"),
                )
            }),
        )
        .route(
            "/api/event",
            post(ingest::collect)
                .get(ingest::command)
                .options(ingest::preflight),
        )
        .route("/api/crawl", post(crawlers::collect))
        .route("/health", get(|| async { "ok" }))
        .route(
            "/assets/site.css",
            get(|| asset("text/css", include_str!("../assets/site.css"))),
        )
        .route(
            "/assets/site.js",
            get(|| asset("text/javascript", include_str!("../assets/site.js"))),
        )
        .route(
            "/assets/app.css",
            get(|| asset("text/css", include_str!("../assets/app.css"))),
        )
        .route(
            "/assets/app.js",
            get(|| asset("text/javascript", include_str!("../assets/app.js"))),
        )
        .route(
            "/assets/globe.js",
            get(|| asset("text/javascript", include_str!("../assets/globe.js"))),
        )
        .route(
            "/assets/globe/countries.json",
            get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, "application/json"),
                        (header::CACHE_CONTROL, "public, max-age=604800"),
                    ],
                    include_str!("../assets/globe/countries.json"),
                )
            }),
        )
        .route(
            "/assets/vendor/three-r170.module.min.js",
            get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, "text/javascript"),
                        (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                    ],
                    include_str!("../assets/vendor/three-r170.module.min.js"),
                )
            }),
        )
        .route(
            "/assets/vendor/three-LICENSE.txt",
            get(|| {
                asset(
                    "text/plain",
                    include_str!("../assets/vendor/three-LICENSE.txt"),
                )
            }),
        )
        .route(
            "/assets/email-logo.png",
            get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, "image/png"),
                        (header::CACHE_CONTROL, "public, max-age=86400"),
                    ],
                    include_bytes!("../assets/email-logo.png").as_slice(),
                )
            }),
        )
        .route("/assets/flags/{file}", get(flag))
        .route("/assets/art/{file}", get(art))
        .route("/assets/fonts/{file}", get(font))
        .route("/assets/dia/{file}", get(diagram))
        .route("/assets/icons/{file}", get(ua_icon))
        .route(
            "/favicon.ico",
            get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, "image/x-icon"),
                        (header::CACHE_CONTROL, "public, max-age=604800"),
                    ],
                    include_bytes!("../assets/favicon.ico").as_slice(),
                )
            }),
        )
        .route(
            "/favicon.svg",
            get(|| asset("image/svg+xml", include_str!("../assets/favicon.svg"))),
        );
    if state.config.app {
        app = app
            .route("/login", get(auth::login_form).post(auth::send_link))
            .route("/auth/verify", get(auth::confirm).post(auth::verify))
            .route("/logout", post(auth::logout))
            .route("/app", get(onboarding::home))
            .route(
                "/app/setup",
                get(onboarding::site_form).post(onboarding::create_site),
            )
            .route(
                "/app/sites/{*path}",
                get(onboarding::legacy).post(onboarding::legacy),
            )
            .route("/app/{domain}/setup/script", get(onboarding::script_step))
            .route("/app/{domain}/setup/stack", get(onboarding::stack))
            .route("/app/{domain}/latest-event", get(onboarding::latest_event))
            .route(
                "/app/{domain}/setup/payments",
                get(onboarding::payments_step).post(onboarding::connect_payments),
            )
            .route(
                "/app/{domain}/setup/checkout",
                get(onboarding::checkout_step),
            )
            .route("/app/{domain}/icon", get(dashboard::icon))
            .route("/app/{domain}/live", get(crate::live::poll))
            .route("/source-icons/{host}", get(dashboard::source_icon))
            .route(
                "/app/{domain}/setup/dismiss",
                post(dashboard::dismiss_setup),
            )
            .route("/app/{domain}/ai-test", post(onboarding::ai_test))
            .route("/app/{domain}/sample", post(crate::sample::add))
            .route("/app/{domain}/sample/remove", post(crate::sample::remove))
            .route("/app/{domain}", get(dashboard::dashboard))
            .route("/app/{domain}/payments", get(dashboard::payments))
            .route(
                "/app/{domain}/answers/{assistant}",
                get(dashboard::answers::page),
            )
            .route("/app/{domain}/visitors", get(visitor_pages::list))
            .route("/app/{domain}/globe", get(crate::globe::page))
            .route("/app/{domain}/globe/data", get(crate::globe::data))
            .route("/app/{domain}/retention", get(retention_page::page))
            .route(
                "/app/{domain}/clients",
                get(crate::dashboard::clients::page),
            )
            .route("/app/{domain}/share-card", get(crate::share_card::page))
            .route(
                "/app/{domain}/share-card.svg",
                get(crate::share_card::image),
            )
            .route(
                "/app/{domain}/launches",
                get(crate::launches::page).post(crate::launches::create),
            )
            .route(
                "/app/{domain}/launches/remove",
                post(crate::launches::remove),
            )
            .route("/app/{domain}/links", get(links::page).post(links::create))
            .route("/app/{domain}/links/delete", post(links::delete))
            .route("/app/{domain}/funnels", post(dashboard::save_funnel))
            .route(
                "/app/{domain}/funnels/delete",
                post(dashboard::delete_funnel),
            )
            .route("/l/{slug}", get(links::follow))
            .route(
                "/app/{domain}/visitors/{visitor_id}",
                get(visitor_pages::timeline),
            )
            .route("/app/{domain}/report", get(dashboard::report::page))
            .route(
                "/app/{domain}/report/note",
                post(dashboard::report::save_note),
            )
            .route(
                "/app/{domain}/report/schedule",
                post(dashboard::report::save_schedule),
            )
            .route(
                "/app/{domain}/report/send",
                post(dashboard::report::send_now),
            )
            .route(
                "/app/{domain}/payments/{provider}/{payment_id}",
                get(dashboard::payment),
            )
            .route(
                "/app/{domain}/settings",
                get(settings::page).post(settings::save),
            )
            .route("/app/{domain}/settings/report", post(settings::save_brand))
            .route("/app/{domain}/settings/public", post(settings::save_public))
            .route(
                "/app/{domain}/settings/timezone",
                post(settings::save_timezone),
            )
            .route("/app/{domain}/settings/client", post(settings::save_client))
            .route("/app/{domain}/settings/ai", post(settings::save_ai))
            .route("/app/{domain}/settings/ai/key", post(settings::save_ai_key))
            .route(
                "/app/{domain}/settings/ai/remove",
                post(settings::remove_ai_key),
            )
            .route("/app/{domain}/settings/ai/run", post(settings::run_ai))
            .route("/app/{domain}/settings/share", post(settings::share))
            .route("/app/{domain}/settings/unshare", post(settings::revoke))
            .route("/app/{domain}/account", get(settings::account_page))
            .route("/app/{domain}/account/plan", post(settings::save_plan))
            .route("/app/{domain}/account/team", post(crate::team::invite))
            .route(
                "/app/{domain}/account/team/revoke",
                post(crate::team::revoke),
            )
            .route(
                "/app/{domain}/account/team/remove",
                post(crate::team::remove),
            )
            .route(
                "/invite",
                get(crate::team::confirm).post(crate::team::accept),
            )
            .route("/app/{domain}/account/billing", post(crate::billing::start))
            .route(
                "/app/{domain}/account/billing/cancel",
                post(crate::billing::cancel),
            )
            .route(
                "/app/{domain}/account/billing/resume",
                post(crate::billing::resume),
            )
            .route(
                "/app/{domain}/account/billing/portal",
                post(crate::billing::portal),
            )
            .route("/app/{domain}/account/theme", post(settings::save_theme))
            .route("/app/{domain}/account/api-keys", post(settings::create_key))
            .route(
                "/app/{domain}/account/api-keys/revoke",
                post(settings::revoke_key),
            )
            .route("/api/v1/sites", get(api::sites))
            .route("/api/v1/sites/{domain}/overview", get(api::overview))
            .route(
                "/api/v1/sites/{domain}/breakdown/{dimension}",
                get(api::breakdown),
            )
            .route("/api/v1/sites/{domain}/payments", get(api::payments))
            .route("/api/v1/sites/{domain}/retention", get(api::retention))
            .route("/api/v1/sites/{domain}/ai-answers", get(api::ai_answers))
            .route("/api/v1/sites/{domain}/annotations", post(api::annotate))
            .route(
                "/api/v1/sites/{domain}/visitors/{visitor_id}",
                get(api::visitor),
            )
            .route("/mcp", post(mcp::post))
            .route("/api/webhooks/dodo", post(crate::billing::webhook))
            .route(
                "/api/webhooks/lemonsqueezy/{site_id}",
                post(crate::sync::lemonsqueezy_webhook),
            )
            .route(
                "/api/webhooks/creem/{site_id}",
                post(crate::sync::creem_webhook),
            )
            .route(
                "/api/webhooks/revenuecat/{site_id}",
                post(crate::sync::revenuecat_webhook),
            )
            .route("/r/{token}", get(dashboard::report::shared))
            .route("/share/{domain}", get(dashboard::public))
            .route("/share/{domain}/globe", get(crate::globe::public_page))
            .route("/share/{domain}/globe/data", get(crate::globe::public_data))
            .route("/share/{domain}/icon", get(dashboard::public_icon))
            .route(
                "/app/{domain}/settings/disconnect",
                post(settings::disconnect),
            )
            .route("/app/{domain}/settings/products", post(settings::products))
            .route("/app/{domain}/settings/alerts", post(crate::alerts::save))
            .route(
                "/app/{domain}/settings/telegram",
                post(crate::telegram::save),
            )
            .route(
                "/app/{domain}/settings/telegram/remove",
                post(crate::telegram::remove),
            );
        app = app.merge(crate::admin::routes());
        if oauth::enabled(&state.config) {
            app = app
                .route(
                    "/.well-known/oauth-protected-resource",
                    get(oauth::protected_resource),
                )
                .route(
                    "/.well-known/oauth-protected-resource/mcp",
                    get(oauth::protected_resource),
                )
                .route(
                    "/.well-known/oauth-authorization-server",
                    get(oauth::authorization_server),
                )
                .route("/oauth/jwks", get(oauth::jwks))
                .route("/oauth/register", post(oauth::register))
                .route(
                    "/oauth/authorize",
                    get(oauth::authorize).post(oauth::consent),
                )
                .route("/oauth/token", post(oauth::token))
                .route(
                    "/app/{domain}/account/apps/revoke",
                    post(settings::revoke_app),
                );
        }
    }
    if state.config.marketing {
        app = app
            .route(
                "/",
                get(|| async { Html(include_str!("../assets/selfhost.html")) }),
            )
            .route(
                "/agency",
                get(|| async { Html(include_str!("../assets/agency.html")) }),
            )
            .route("/api/waitlist", post(waitlist::join))
            .route("/waitlist/leave", get(waitlist::leave));
    } else if state.config.app {
        app = app.route("/", get(|| async { Redirect::temporary("/app") }));
    }
    app.layer(axum::middleware::from_fn(error_page))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::markdown::negotiate,
        ))
        .layer(axum::middleware::from_fn_with_state(state.clone(), theme))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            keep_out_of_search,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            mcp_host_only,
        ))
        .with_state(state)
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorPage {
    title: &'static str,
    message: &'static str,
}

/// Gives a browser a readable page for a bare 404 or 5xx; API and JSON responses keep their own bodies.
async fn error_page(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let html = req
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|a| a.to_str().ok())
        .is_some_and(|a| a.contains("text/html"));
    let res = next.run(req).await;
    let status = res.status();
    if !html
        || res.headers().contains_key(axum::http::header::CONTENT_TYPE)
        || !(status == StatusCode::NOT_FOUND || status.is_server_error())
    {
        return res;
    }
    let (title, message) = if status == StatusCode::NOT_FOUND {
        ("Page not found", "This page does not exist or has moved.")
    } else {
        (
            "Something went wrong",
            "We could not load this page. Try again in a minute.",
        )
    };
    let page = ErrorPage { title, message }.render().unwrap_or_default();
    (status, Html(page)).into_response()
}

/// The app and its sign-in page are not for search results; only the marketing pages are.
async fn keep_out_of_search(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let mut res = next.run(req).await;
    if !state.config.marketing {
        res.headers_mut()
            .entry("x-robots-tag")
            .or_insert(axum::http::HeaderValue::from_static("noindex"));
    }
    res
}

/// On the MCP host only the MCP endpoint and its metadata answer; everything else is a bare 404.
async fn mcp_host_only(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if oauth::on_mcp_host(&state.config, req.headers()) {
        let path = req.uri().path();
        let allowed = matches!(
            path,
            "/mcp"
                | "/.well-known/oauth-protected-resource"
                | "/.well-known/oauth-protected-resource/mcp"
        );
        if !allowed {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
    next.run(req).await
}

/// Marks a signed-in user's pages with their Light or Dark choice, so they render in it without a flash.
/// A support session is read only and shows a banner.
async fn theme(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let token = auth::cookie(req.headers(), auth::SESSION_COOKIE);
    if let Some(token) = token.as_ref().filter(|_| {
        !matches!(
            *req.method(),
            axum::http::Method::GET | axum::http::Method::HEAD
        ) && req.uri().path() != "/admin/stop"
    }) {
        let support: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM sessions WHERE token_hash = ? AND impersonator IS NOT NULL",
        )
        .bind(auth::hash_token(token))
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
        if support.is_some() {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let res = next.run(req).await;
    let html = res
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"text/html"));
    let Some(token) = token.filter(|_| html) else {
        return res;
    };
    let row: Option<(String, Option<String>, i64, String)> = sqlx::query_as(
        "SELECT a.theme, s.impersonator, s.expires_at, a.email FROM sessions s JOIN accounts a ON a.id = s.account_id WHERE s.token_hash = ?",
    )
    .bind(auth::hash_token(&token))
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let Some((theme, by, expires, email)) = row else {
        return res;
    };
    let themed = theme == "light" || theme == "dark";
    if !themed && by.is_none() {
        return res;
    }
    let (mut parts, body) = res.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut page = String::from_utf8_lossy(&bytes).into_owned();
    if themed {
        page = page.replacen(
            r#"<html lang="en">"#,
            &format!(r#"<html lang="en" data-theme="{theme}">"#),
            1,
        );
    }
    if let Some(by) = by
        && let Some(at) = page
            .find("<body")
            .and_then(|i| page[i..].find('>').map(|j| i + j + 1))
    {
        let minutes = ((expires - OffsetDateTime::now_utc().unix_timestamp()) / 60).max(0);
        let banner = crate::admin::Banner {
            email: &email,
            by: &by,
            minutes,
        }
        .render()
        .unwrap_or_default();
        page.insert_str(at, &banner);
    }
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, axum::body::Body::from(page))
}

async fn tracker(State(state): State<AppState>, headers: HeaderMap) -> Response {
    crawlers::script_fetch(&state, &headers);
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        include_str!("../assets/v.js"),
    )
        .into_response()
}

const ART: [(&str, &[u8]); 4] = [
    (
        "02-empty-waiting.jpg",
        include_bytes!("../assets/art/02-empty-waiting.jpg"),
    ),
    (
        "03-empty-payments.jpg",
        include_bytes!("../assets/art/03-empty-payments.jpg"),
    ),
    (
        "04-empty-goals.jpg",
        include_bytes!("../assets/art/04-empty-goals.jpg"),
    ),
    ("og-share.jpg", include_bytes!("../assets/art/og-share.jpg")),
];

const FONTS: &[(&str, &[u8])] = &[
    (
        "schibsted-grotesk.woff2",
        include_bytes!("../assets/fonts/schibsted-grotesk.woff2"),
    ),
    (
        "geist-mono.woff2",
        include_bytes!("../assets/fonts/geist-mono.woff2"),
    ),
    (
        "caveat.woff2",
        include_bytes!("../assets/fonts/caveat.woff2"),
    ),
];

const DIAGRAMS: &[(&str, &str)] = &[
    ("journey-d.svg", include_str!("../assets/dia/journey-d.svg")),
    ("journey-m.svg", include_str!("../assets/dia/journey-m.svg")),
    (
        "channels-d.svg",
        include_str!("../assets/dia/channels-d.svg"),
    ),
    (
        "channels-m.svg",
        include_str!("../assets/dia/channels-m.svg"),
    ),
];

async fn diagram(Path(file): Path<String>) -> Response {
    match DIAGRAMS.iter().find(|(name, _)| *name == file) {
        Some((_, svg)) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=604800"),
            ],
            *svg,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn font(Path(file): Path<String>) -> Response {
    match FONTS.iter().find(|(name, _)| *name == file) {
        Some((_, bytes)) => (
            [
                (header::CONTENT_TYPE, "font/woff2"),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            *bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn art(Path(file): Path<String>) -> Response {
    match ART.iter().find(|(name, _)| *name == file) {
        Some((_, bytes)) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            *bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn ua_icon(Path(file): Path<String>) -> Response {
    let name = file.strip_suffix(".svg").unwrap_or_default();
    match crate::icons::file(name) {
        Some(svg) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            svg,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn flag(Path(file): Path<String>) -> Response {
    let code = file.strip_suffix(".svg").unwrap_or_default();
    match flags::lookup(&code.to_ascii_uppercase()) {
        Some((_, svg)) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            svg,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn asset(content_type: &'static str, body: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        body,
    )
        .into_response()
}
