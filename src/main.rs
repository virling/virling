mod app;
mod auth;
mod channel;
mod crypto;
mod events;
mod fx;
mod ingest;
mod mail;
mod onboarding;
mod providers;
mod sites;
mod stats;
mod sync;
mod waitlist;
mod waitlist_copy;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

use crate::app::{AppState, Config};
use crate::events::DuckStore;

#[derive(Parser)]
#[command(name = "virling", version, about = "Revenue analytics in one binary")]
struct Cli {
    /// Directory for the SQLite and DuckDB files
    #[arg(
        long,
        env = "VIRLING_DATA",
        default_value = "./virling-data",
        global = true
    )]
    data_dir: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server (default)
    Serve(ServeArgs),
    /// Add a site to track
    AddSite {
        domain: String,
        /// Owner email; the account is created if it does not exist
        #[arg(long)]
        owner: String,
        #[arg(long, default_value = "UTC")]
        timezone: String,
    },
}

#[derive(Parser)]
struct ServeArgs {
    #[arg(long, env = "VIRLING_ADDR", default_value = "0.0.0.0:8080")]
    addr: SocketAddr,
    /// Serve the hosted marketing pages and waitlist
    #[arg(long, env = "VIRLING_MARKETING")]
    marketing: bool,
    /// Serve sign-in and the app (off during the waitlist phase)
    #[arg(long, env = "VIRLING_APP")]
    app: bool,
    /// 64 hex chars used to encrypt payment-provider keys. Unset: a key file is created in the data dir.
    #[arg(long, env = "VIRLING_SECRET_KEY", hide_env_values = true)]
    secret_key: Option<String>,
    /// Trust X-Forwarded-For from a reverse proxy
    #[arg(long, env = "VIRLING_TRUST_PROXY")]
    trust_proxy: bool,
    #[arg(long, env = "VIRLING_EVENTS_PER_SECOND", default_value_t = 200)]
    events_per_second: u32,
    /// Public URL of this server, used in sign-in links and script snippets
    #[arg(
        long,
        env = "VIRLING_BASE_URL",
        default_value = "http://localhost:8080"
    )]
    base_url: String,
    /// SMTP URL for sign-in emails, e.g. smtps://api_token:TOKEN@smtp.mx.cloudflare.net:465. Unset: links are logged.
    #[arg(long, env = "VIRLING_SMTP_URL", hide_env_values = true)]
    smtp_url: Option<String>,
    #[arg(
        long,
        env = "VIRLING_MAIL_FROM",
        default_value = "Virling <login@virling.com>"
    )]
    mail_from: String,
    /// Sender for waitlist confirmations; replies go here
    #[arg(
        long,
        env = "VIRLING_WAITLIST_FROM",
        default_value = "Rahul at Virling <hello@virling.com>"
    )]
    waitlist_from: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();
    std::fs::create_dir_all(&cli.data_dir)
        .with_context(|| format!("creating {}", cli.data_dir.display()))?;
    let db = open_sqlite(&cli.data_dir).await?;

    let command = match cli.command {
        Some(c) => c,
        None => Command::Serve(ServeArgs::parse_from(["serve"])),
    };
    match command {
        Command::Serve(args) => {
            let events = DuckStore::open(&cli.data_dir.join("events.duckdb"))?;
            let config = Config {
                marketing: args.marketing,
                app: args.app,
                trust_proxy: args.trust_proxy,
                events_per_second: args.events_per_second,
                base_url: args.base_url.trim_end_matches('/').to_string(),
                waitlist_from: args.waitlist_from,
            };
            let mailer = mail::Mailer::new(args.smtp_url.as_deref(), &args.mail_from)?;
            let vault = crypto::Vault::load(&cli.data_dir, args.secret_key.as_deref())?;
            let app_enabled = config.app;
            let state = AppState::new(config, db, Arc::new(events), mailer, vault);
            if app_enabled {
                tokio::spawn(sync::run(state.clone()));
            }
            let listener = tokio::net::TcpListener::bind(args.addr)
                .await
                .with_context(|| format!("binding {}", args.addr))?;
            tracing::info!(
                "listening on http://{}, data in {}",
                args.addr,
                cli.data_dir.display()
            );
            axum::serve(
                listener,
                app::router(state).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                tokio::signal::ctrl_c().await.ok();
            })
            .await?;
        }
        Command::AddSite {
            domain,
            owner,
            timezone,
        } => {
            let domain = sites::normalize_domain(&domain).context("not a valid domain")?;
            let owner = owner.trim().to_ascii_lowercase();
            anyhow::ensure!(waitlist::valid_email(&owner), "not a valid email");
            let account = sites::find_or_create_account(&db, &owner).await?;
            sites::create_site(&db, &account, &domain, &timezone).await?;
            println!(
                "Added {domain}. Script tag:\n<script defer src=\"https://YOUR-VIRLING-HOST/v.js\" data-site=\"{domain}\"></script>"
            );
        }
    }
    Ok(())
}

async fn open_sqlite(dir: &std::path::Path) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(dir.join("virling.sqlite"))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::app::{AppState, Config, router};
    use crate::events::{DuckStore, EventStore};

    const UA: &str = "Mozilla/5.0 (Macintosh) AppleWebKit/605 Version/17.0 Safari/605.1";

    async fn setup() -> (tempfile::TempDir, AppState, Arc<DuckStore>) {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::open_sqlite(dir.path()).await.unwrap();
        let account = crate::sites::find_or_create_account(&db, "me@example.com")
            .await
            .unwrap();
        crate::sites::create_site(&db, &account, "example.com", "UTC")
            .await
            .unwrap();
        let store = Arc::new(DuckStore::open(&dir.path().join("events.duckdb")).unwrap());
        let config = Config {
            marketing: true,
            app: true,
            trust_proxy: false,
            events_per_second: 200,
            base_url: "http://localhost:8080".into(),
            waitlist_from: "Rahul at Virling <hello@virling.com>".into(),
        };
        let state = AppState::new(
            config,
            db,
            store.clone(),
            crate::mail::Mailer::Capture(Default::default()),
            crate::crypto::Vault::load(dir.path(), None).unwrap(),
        );
        (dir, state, store)
    }

    async fn post(state: &AppState, body: &str, ua: &str) -> StatusCode {
        let app = router(state.clone())
            .layer(MockConnectInfo(SocketAddr::from(([203, 0, 113, 7], 5555))));
        let req = Request::post("/api/event")
            .header("user-agent", ua)
            .header("cf-ipcountry", "de")
            .body(Body::from(body.to_string()))
            .unwrap();
        app.oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn ingests_and_classifies_events() {
        let (_dir, state, store) = setup().await;
        let vid = "0b7c2d1e-aaaa-bbbb-cccc-1234567890ab";
        let batch = format!(
            r#"[
            {{"n":"pageview","u":"https://www.example.com/pricing?utm_source=newsletter&utm_medium=email","r":"https://mail.google.com/","s":"example.com","v":"{vid}","w":1440}},
            {{"n":"pageview","u":"https://example.com/","r":"https://news.ycombinator.com/item?id=1","s":"example.com","v":"{vid}","w":390}},
            {{"n":"goal","u":"https://example.com/signup","r":"https://example.com/","s":"example.com","v":"{vid}","w":390,"g":"signup"}},
            {{"n":"pageview","u":"https://evil.io/","s":"example.com","w":390}},
            {{"n":"pageview","u":"https://other.com/","s":"other.com","w":390}},
            {{"n":"goal","u":"https://example.com/","s":"example.com","w":390}}
        ]"#
        );
        assert_eq!(post(&state, &batch, UA).await, StatusCode::ACCEPTED);
        assert_eq!(post(&state, r#"{"n":"pageview","u":"https://example.com/docs","r":"https://chatgpt.com/","s":"example.com","w":800}"#, UA).await, StatusCode::ACCEPTED);
        assert_eq!(
            post(
                &state,
                r#"{"n":"pageview","u":"https://example.com/","s":"example.com"}"#,
                "Googlebot/2.1"
            )
            .await,
            StatusCode::ACCEPTED
        );
        assert_eq!(post(&state, "nope", UA).await, StatusCode::BAD_REQUEST);

        let rows = store.dump();
        assert_eq!(
            rows.len(),
            4,
            "foreign host, unknown site, nameless goal and bot are dropped: {rows:?}"
        );
        let by_path = |p: &str| rows.iter().find(|r| r.0 == p).unwrap().clone();

        let pricing = by_path("/pricing");
        assert_eq!(
            (pricing.1.as_str(), pricing.5.as_deref(), pricing.6.as_str()),
            ("Email", Some("DE"), "Desktop")
        );
        let home = by_path("/");
        assert_eq!(
            (home.1.as_str(), home.2.as_deref(), home.6.as_str()),
            ("Social", Some("news.ycombinator.com"), "Mobile")
        );
        let goal = by_path("/signup");
        assert_eq!(
            (
                goal.1.as_str(),
                goal.2.as_deref(),
                goal.3.as_str(),
                goal.4.as_deref()
            ),
            ("Direct", None, "goal", Some("signup"))
        );
        let docs = by_path("/docs");
        assert_eq!((docs.1.as_str(), docs.6.as_str()), ("AI", "Tablet"));
        assert!(
            docs.7.starts_with("c-"),
            "no visitor ID falls back to a cookieless hash"
        );
        let site_id: String =
            sqlx::query_scalar("SELECT id FROM sites WHERE domain = 'example.com'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(store.count_events(&site_id).unwrap(), 4);
    }

    #[tokio::test]
    async fn waitlist_dedupes_and_validates() {
        let (_dir, state, _store) = setup().await;
        let send = |body: &'static str| {
            let app = router(state.clone());
            async move {
                let req = Request::post("/api/waitlist")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap();
                app.oneshot(req).await.unwrap().status()
            }
        };
        assert_eq!(
            send(r#"{"email":"A@b.co","wedge":"agency","clients":"6 to 15 clients"}"#).await,
            StatusCode::OK
        );
        assert_eq!(
            send(r#"{"email":"a@b.co","wedge":"agency"}"#).await,
            StatusCode::OK
        );
        assert_eq!(
            send(r#"{"email":"bad","wedge":"agency"}"#).await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            send(r#"{"email":"a@b.co","wedge":"other"}"#).await,
            StatusCode::BAD_REQUEST
        );
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM waitlist")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }

    fn sent(state: &AppState) -> Vec<(String, String, String)> {
        match &*state.mailer {
            crate::mail::Mailer::Capture(m) => m.lock().unwrap().clone(),
            _ => unreachable!(),
        }
    }

    fn link_in(text: &str, prefix: &str) -> String {
        let start = text.find(prefix).expect("link in email");
        text[start..].split_whitespace().next().unwrap().to_string()
    }

    async fn call(state: &AppState, req: Request<Body>) -> axum::response::Response {
        router(state.clone())
            .layer(MockConnectInfo(SocketAddr::from(([203, 0, 113, 7], 5555))))
            .oneshot(req)
            .await
            .unwrap()
    }

    async fn body_text(res: axum::response::Response) -> String {
        String::from_utf8(
            axum::body::to_bytes(res.into_body(), 1 << 20)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }

    fn form(uri: &str, body: String, cookie: Option<&str>) -> Request<Body> {
        let mut b = Request::post(uri).header("content-type", "application/x-www-form-urlencoded");
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        b.body(Body::from(body)).unwrap()
    }

    fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
        let mut b = Request::get(uri);
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        b.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn magic_link_sign_in_and_onboarding() {
        let (_dir, state, _store) = setup().await;

        assert_eq!(
            call(&state, get("/app", None)).await.headers()["location"],
            "/login"
        );
        let res = call(
            &state,
            form("/login", "email=New%40Founder.dev".into(), None),
        )
        .await;
        assert!(body_text(res).await.contains("Check your email"));
        let mails = sent(&state);
        assert_eq!(mails.len(), 1);
        assert_eq!(mails[0].0, "new@founder.dev");
        let link = link_in(&mails[0].2, "http://localhost:8080/auth/verify?token=");
        let token = link.split("token=").nth(1).unwrap().to_string();

        let confirm = call(&state, get(&format!("/auth/verify?token={token}"), None)).await;
        assert!(
            body_text(confirm)
                .await
                .contains("Continue as new@founder.dev"),
            "GET only shows a confirm button"
        );

        let res = call(&state, form("/auth/verify", format!("token={token}"), None)).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let set_cookie = res.headers()["set-cookie"].to_str().unwrap().to_string();
        assert!(set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Lax"));
        let cookie = set_cookie.split(';').next().unwrap().to_string();

        let reused = call(&state, form("/auth/verify", format!("token={token}"), None)).await;
        assert!(
            body_text(reused)
                .await
                .contains("expired or was already used"),
            "links work once"
        );

        assert_eq!(
            call(&state, get("/app", Some(&cookie))).await.headers()["location"],
            "/app/setup"
        );
        let bad = call(
            &state,
            form(
                "/app/setup",
                "domain=not+a+domain&timezone=UTC".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert!(body_text(bad).await.contains("Enter a domain"));
        let res = call(
            &state,
            form(
                "/app/setup",
                "domain=https%3A%2F%2Fwww.founder.dev%2F&timezone=Asia%2FKolkata".into(),
                Some(&cookie),
            ),
        )
        .await;
        let script_url = res.headers()["location"].to_str().unwrap().to_string();
        assert!(script_url.ends_with("/setup/script"));
        let page = body_text(call(&state, get(&script_url, Some(&cookie))).await).await;
        assert!(page.contains("data-site=\"founder.dev\""));

        let latest = script_url.replace("/setup/script", "/latest-event");
        assert_eq!(
            body_text(call(&state, get(&latest, Some(&cookie))).await).await,
            "null"
        );
        let ev = r#"{"n":"pageview","u":"https://founder.dev/pricing","r":"https://news.ycombinator.com/","s":"founder.dev","v":"0b7c2d1e-aaaa-bbbb-cccc-1234567890ab","w":1440}"#;
        assert_eq!(post(&state, ev, UA).await, StatusCode::ACCEPTED);
        let got = body_text(call(&state, get(&latest, Some(&cookie))).await).await;
        assert!(
            got.contains("/pricing") && got.contains("news.ycombinator.com"),
            "{got}"
        );

        assert_eq!(
            call(&state, get(&latest, None)).await.headers()["location"],
            "/login"
        );
        let res = call(&state, form("/logout", String::new(), Some(&cookie))).await;
        assert!(
            res.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        assert_eq!(
            call(&state, get("/app", Some(&cookie))).await.headers()["location"],
            "/login"
        );
    }

    #[tokio::test]
    async fn app_routes_are_off_unless_enabled() {
        let (_dir, mut state, _store) = setup().await;
        let mut config = (*state.config).clone();
        config.app = false;
        state.config = Arc::new(config);
        assert_eq!(
            call(&state, get("/login", None)).await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(&state, get("/app", None)).await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(call(&state, get("/", None)).await.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn sign_in_links_are_rate_limited() {
        let (_dir, state, _store) = setup().await;
        for _ in 0..7 {
            call(&state, form("/login", "email=a%40b.co".into(), None)).await;
        }
        assert_eq!(sent(&state).len(), 5);
    }

    #[tokio::test]
    async fn waitlist_sends_one_confirmation_with_working_leave_link() {
        let (_dir, state, _store) = setup().await;
        let join = |body: &'static str| {
            Request::post("/api/waitlist")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap()
        };
        call(
            &state,
            join(r#"{"email":"Agency@Shop.io","wedge":"agency"}"#),
        )
        .await;
        call(
            &state,
            join(r#"{"email":"agency@shop.io","wedge":"agency"}"#),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mails = sent(&state);
        assert_eq!(mails.len(), 1, "duplicates get no second email");
        assert_eq!(mails[0].0, "agency@shop.io");
        assert!(mails[0].1.contains("agencies") && mails[0].2.contains("$47.40"));
        let leave = link_in(&mails[0].2, "http://localhost:8080/waitlist/leave?token=")
            .replace("http://localhost:8080", "");
        assert!(
            body_text(call(&state, get(&leave, None)).await)
                .await
                .contains("left the waitlist")
        );
        assert!(
            body_text(call(&state, get(&leave, None)).await)
                .await
                .contains("Nothing to remove")
        );
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM waitlist")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
}
