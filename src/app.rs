use std::sync::Arc;

use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use sqlx::SqlitePool;

use crate::crypto::Vault;
use crate::events::EventStore;
use crate::fx::Rates;
use crate::ingest::{self, DailySalt, RateLimiter, Sessions};
use crate::mail::Mailer;
use crate::sites::SiteCache;
use crate::{api, auth, dashboard, flags, onboarding, settings, visitor_pages, waitlist};

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
    /// Sender for waitlist emails; replies go to this address.
    pub waitlist_from: String,
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
        }
    }
}

pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/v.js", get(tracker))
        .route(
            "/api/event",
            post(ingest::collect).options(ingest::preflight),
        )
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
        .route("/assets/icons/{file}", get(ua_icon))
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
            .route("/source-icons/{host}", get(dashboard::source_icon))
            .route(
                "/app/{domain}/setup/dismiss",
                post(dashboard::dismiss_setup),
            )
            .route("/app/{domain}", get(dashboard::dashboard))
            .route("/app/{domain}/payments", get(dashboard::payments))
            .route("/app/{domain}/visitors", get(visitor_pages::list))
            .route(
                "/app/{domain}/visitors/{visitor_id}",
                get(visitor_pages::timeline),
            )
            .route("/app/{domain}/report", get(dashboard::report))
            .route(
                "/app/{domain}/payments/{provider}/{payment_id}",
                get(dashboard::payment),
            )
            .route(
                "/app/{domain}/settings",
                get(settings::page).post(settings::save),
            )
            .route("/app/{domain}/settings/report", post(settings::save_brand))
            .route("/app/{domain}/settings/share", post(settings::share))
            .route("/app/{domain}/settings/unshare", post(settings::revoke))
            .route(
                "/app/{domain}/settings/api-keys",
                post(settings::create_key),
            )
            .route(
                "/app/{domain}/settings/api-keys/revoke",
                post(settings::revoke_key),
            )
            .route("/api/v1/sites", get(api::sites))
            .route("/api/v1/sites/{domain}/overview", get(api::overview))
            .route(
                "/api/v1/sites/{domain}/breakdown/{dimension}",
                get(api::breakdown),
            )
            .route("/api/v1/sites/{domain}/payments", get(api::payments))
            .route(
                "/api/v1/sites/{domain}/visitors/{visitor_id}",
                get(api::visitor),
            )
            .route("/r/{token}", get(dashboard::shared))
            .route(
                "/app/{domain}/settings/disconnect",
                post(settings::disconnect),
            );
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
    app.with_state(state)
}

async fn tracker() -> Response {
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

const ART: [(&str, &[u8]); 3] = [
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
    let slug = file.strip_suffix(".svg").unwrap_or_default();
    match crate::icons::UA_ICONS.iter().find(|(s, _)| *s == slug) {
        Some((_, svg)) => (
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            *svg,
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
