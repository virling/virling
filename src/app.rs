use std::sync::Arc;

use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use sqlx::SqlitePool;

use crate::events::EventStore;
use crate::ingest::{self, DailySalt, RateLimiter, Sessions};
use crate::mail::Mailer;
use crate::sites::SiteCache;
use crate::{auth, onboarding, waitlist};

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
}

impl AppState {
    pub fn new(config: Config, db: SqlitePool, events: Arc<dyn EventStore>, mailer: Mailer) -> Self {
        Self {
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
        .route("/api/event", post(ingest::collect).options(ingest::preflight))
        .route("/health", get(|| async { "ok" }))
        .route("/assets/site.css", get(|| asset("text/css", include_str!("../assets/site.css"))))
        .route("/assets/site.js", get(|| asset("text/javascript", include_str!("../assets/site.js"))))
        .route("/assets/app.css", get(|| asset("text/css", include_str!("../assets/app.css"))))
        .route("/assets/app.js", get(|| asset("text/javascript", include_str!("../assets/app.js"))))
        .route("/assets/email-logo.png", get(|| async {
            ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400")], include_bytes!("../assets/email-logo.png").as_slice())
        }))
        .route("/favicon.svg", get(|| asset("image/svg+xml", include_str!("../assets/favicon.svg"))));
    if state.config.app {
        app = app
            .route("/login", get(auth::login_form).post(auth::send_link))
            .route("/auth/verify", get(auth::confirm).post(auth::verify))
            .route("/logout", post(auth::logout))
            .route("/app", get(onboarding::home))
            .route("/app/setup", get(onboarding::site_form).post(onboarding::create_site))
            .route("/app/sites/{site_id}/setup/script", get(onboarding::script_step))
            .route("/app/sites/{site_id}/latest-event", get(onboarding::latest_event));
    }
    if state.config.marketing {
        app = app
            .route("/", get(|| async { Html(include_str!("../assets/selfhost.html")) }))
            .route("/agency", get(|| async { Html(include_str!("../assets/agency.html")) }))
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

async fn asset(content_type: &'static str, body: &'static str) -> Response {
    (StatusCode::OK, [(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "public, max-age=3600")], body).into_response()
}
