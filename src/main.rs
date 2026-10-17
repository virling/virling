mod alias;
mod animals;
mod api;
mod app;
mod assets;
mod auth;
mod bench;
mod cache;
mod channel;
mod chart;
mod crawlers;
mod crypto;
mod dashboard;
mod demo;
mod events;
mod favicon;
mod flags;
mod fx;
mod icons;
mod index;
mod ingest;
mod links;
mod mail;
mod mcp;
mod onboarding;
mod providers;
mod query;
mod retention;
mod retention_page;
mod settings;
mod sites;
mod stats;
mod sync;
mod ua;
mod visitor_pages;
mod visitors;
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
use crate::cache::Cached;
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
    #[command(
        about = query::ABOUT,
        long_about = query::LONG_ABOUT,
        after_help = query::EXAMPLES
    )]
    Query(query::QueryArgs),
    /// Render the sample-data demo to static HTML
    #[command(hide = true)]
    Demo {
        out: PathBuf,
        /// Seed OUT as a data directory to serve instead of exporting pages
        #[arg(long)]
        data: bool,
        /// Owner of the seeded site with --data
        #[arg(long, default_value = "maya.chen@tallybird.app")]
        email: String,
    },
    /// Time every page, API endpoint, MCP tool and ingest on the data dir, seeding the demo when empty
    #[command(hide = true)]
    Bench(bench::BenchArgs),
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
    if let Some(Command::Demo { out, data, email }) = &cli.command {
        return if *data {
            demo::data(out, email).await
        } else {
            demo::export(out).await
        };
    }
    if let Some(Command::Bench(args)) = cli.command {
        return bench::run(&cli.data_dir, args).await;
    }
    if let Some(Command::Query(args)) = cli.command {
        match query::run(args).await {
            Ok(out) => print!("{out}"),
            Err(err) => {
                eprintln!("error: {err:#}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    std::fs::create_dir_all(&cli.data_dir)
        .with_context(|| format!("creating {}", cli.data_dir.display()))?;
    let db = open_sqlite(&cli.data_dir).await?;

    let command = match cli.command {
        Some(c) => c,
        None => Command::Serve(ServeArgs::parse_from(["serve"])),
    };
    match command {
        Command::Serve(args) => {
            let events = Cached::new(DuckStore::open(&cli.data_dir.join("events.duckdb"))?);
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
                favicon::spawn_sources(state.clone());
                tokio::spawn(dashboard::schedule::run(state.clone()));
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
        Command::Demo { .. } | Command::Query(_) | Command::Bench(_) => {
            unreachable!("handled before the data dir is opened")
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
    use crate::events::{DuckStore, Event, EventStore, Payment};
    use crate::stats::{PaymentFilter, Period};

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
    async fn tagged_ai_source_stands_in_for_missing_referrer() {
        let (_dir, state, store) = setup().await;
        let body = r#"{"n":"pageview","u":"https://example.com/?utm_source=chatgpt.com","s":"example.com","w":800}"#;
        assert_eq!(post(&state, body, UA).await, StatusCode::ACCEPTED);
        let rows = store.dump();
        assert_eq!(
            (rows[0].1.as_str(), rows[0].2.as_deref()),
            ("AI", Some("chatgpt.com"))
        );
    }

    #[tokio::test]
    async fn ai_crawlers_count_as_crawler_hits_not_visitors() {
        let (_dir, state, store) = setup().await;
        let body =
            r#"{"n":"pageview","u":"https://example.com/pricing","s":"example.com","w":800}"#;
        let ua = "Mozilla/5.0 (compatible; ClaudeBot/1.0; +claudebot@anthropic.com)";
        assert_eq!(post(&state, body, ua).await, StatusCode::ACCEPTED);
        assert_eq!(
            post(&state, body, "Googlebot/2.1").await,
            StatusCode::ACCEPTED
        );
        assert!(store.dump().is_empty());
        let hits: Vec<(String, String, i64)> =
            sqlx::query_as("SELECT bot, path, hits FROM crawler_pages")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(hits, [("ClaudeBot".into(), "/pricing".into(), 1)]);
    }

    #[tokio::test]
    async fn records_exits_and_leaves_inside_a_session() {
        let (_dir, state, store) = setup().await;
        let vid = "0b7c2d1e-aaaa-bbbb-cccc-1234567890ab";
        let ev = |n: &str, g: &str| {
            format!(
                r#"{{"n":"{n}","u":"https://example.com/docs","s":"example.com","v":"{vid}","w":1440,"g":"{g}"}}"#
            )
        };
        // Without a live session, exits and leaves are dropped.
        assert_eq!(
            post(&state, &ev("leave", ""), UA).await,
            StatusCode::ACCEPTED
        );
        assert_eq!(
            post(&state, &ev("pageview", ""), UA).await,
            StatusCode::ACCEPTED
        );
        for (n, g) in [
            ("exit", "https://github.com/virling/virling?tab=readme#top"),
            ("exit", "https://www.example.com/pricing"),
            ("exit", "mailto:hi@example.com"),
            ("exit", ""),
            ("leave", ""),
        ] {
            assert_eq!(post(&state, &ev(n, g), UA).await, StatusCode::ACCEPTED);
        }
        let rows = store.dump();
        let kinds: Vec<(&str, Option<&str>)> = rows
            .iter()
            .map(|r| (r.3.as_str(), r.4.as_deref()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("pageview", None),
                ("exit", Some("github.com/virling/virling")),
                ("leave", None)
            ],
            "same-site, non-web and empty exits are dropped: {rows:?}"
        );
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
            ("Communities", Some("news.ycombinator.com"), "Mobile")
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

    async fn make_agency(state: &AppState) {
        sqlx::query("UPDATE accounts SET account_type = 'agency'")
            .execute(&state.db)
            .await
            .unwrap();
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
    async fn flags_and_art_are_served() {
        let (_dir, state, _store) = setup().await;
        for (uri, status, kind) in [
            ("/assets/flags/de.svg", StatusCode::OK, "image/svg+xml"),
            ("/assets/flags/zz.svg", StatusCode::NOT_FOUND, ""),
            ("/assets/flags/de", StatusCode::NOT_FOUND, ""),
            (
                "/assets/art/03-empty-payments.jpg",
                StatusCode::OK,
                "image/jpeg",
            ),
            ("/assets/art/nope.jpg", StatusCode::NOT_FOUND, ""),
        ] {
            let res = call(&state, get(uri, None)).await;
            assert_eq!(res.status(), status, "{uri}");
            if !kind.is_empty() {
                assert_eq!(res.headers()["content-type"], kind);
            }
        }
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
        assert_eq!(
            call(&state, get("/r/abc", None)).await.status(),
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
        assert!(mails[0].1.contains("agencies") && mails[0].2.contains("$29 a month"));
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

    async fn sign_in(state: &AppState, email: &str) -> String {
        call(state, form("/login", format!("email={email}"), None)).await;
        let mails = sent(state);
        let link = link_in(
            &mails.last().unwrap().2,
            "http://localhost:8080/auth/verify?token=",
        );
        let token = link.split("token=").nth(1).unwrap().to_string();
        let res = call(state, form("/auth/verify", format!("token={token}"), None)).await;
        let set_cookie = res.headers()["set-cookie"].to_str().unwrap();
        set_cookie.split(';').next().unwrap().to_string()
    }

    async fn site_id(state: &AppState) -> String {
        sqlx::query_scalar("SELECT id FROM sites WHERE domain = 'example.com'")
            .fetch_one(&state.db)
            .await
            .unwrap()
    }

    /// Two visits by one visitor (social, then referral) and a payment a day later.
    fn seed(store: &DuckStore, site: &str) {
        let now = time::OffsetDateTime::now_utc();
        let visit = |session: &str, days: i64, channel: &str, host: &str| Event {
            site_id: site.into(),
            ts: now - time::Duration::days(days),
            visitor_id: "v1".into(),
            session_id: session.into(),
            kind: "pageview".into(),
            name: None,
            path: format!("/{session}"),
            referrer_host: Some(host.into()),
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            channel: channel.into(),
            country: Some("DE".into()),
            device: "Desktop".into(),
            browser: "Firefox".into(),
            os: "macOS".into(),
            region: None,
            city: None,
            host: None,
            language: None,
        };
        store
            .insert_events(&[
                visit("first", 5, "Communities", "news.ycombinator.com"),
                visit("last", 3, "Referral", "someblog.dev"),
            ])
            .unwrap();
        let pay = |id: &str, visitor: Option<&str>| Payment {
            site_id: site.into(),
            provider: "stripe".into(),
            provider_payment_id: id.into(),
            ts: now - time::Duration::days(2),
            amount: 4900,
            currency: "usd".into(),
            amount_usd: Some(49.0),
            refunded: 0,
            status: "paid".into(),
            visitor_id: visitor.map(Into::into),
            customer_id: None,
            is_renewal: false,
        };
        store
            .upsert_payments(&[pay("pi_1", Some("v1")), pay("pi_2", None)])
            .unwrap();
    }

    fn credited(store: &DuckStore, site: &str) -> Option<String> {
        let today = time::OffsetDateTime::now_utc().date();
        let period = Period {
            from: today - time::Duration::days(30),
            to: today,
            tz: "UTC".into(),
        };
        store
            .payment_list(site, &period, PaymentFilter::Attributed, 10, 0)
            .unwrap()
            .rows
            .first()
            .and_then(|p| p.touch.as_ref().map(|t| t.session_id.clone()))
    }

    #[tokio::test]
    async fn connect_with_invalid_key_shows_error() {
        let (_dir, state, _store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let uri = "/app/example.com/setup/payments".to_string();
        let page = call(&state, get(&uri, Some(&cookie))).await;
        assert_eq!(page.status(), StatusCode::OK);
        let body = body_text(page).await;
        assert!(body.contains("Restricted key"));
        for name in ["Stripe", "Dodo Payments", "Polar", "Lemon Squeezy"] {
            assert!(
                body.contains(&format!(r#"aria-label="{name}""#)),
                "{name} logo"
            );
        }
        assert!(!body.contains(" api key"), "the label reads API key");
        let res = call(
            &state,
            form(
                &uri,
                "provider=stripe&key=sk_live_abc".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(
            body.contains("Use a restricted read-only key"),
            "check_key error is shown"
        );
        assert!(!body.contains("sk_live_abc"), "the key is never sent back");
        assert!(
            body.contains(r#"<p class="field-err" id="key-err" role="alert" data-key-err>"#),
            "the error sits under the field"
        );
        assert!(!body.contains(r#"class="alert""#));
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM integrations")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn setup_copy_has_no_issue_references() {
        let (_dir, state, _store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        for step in ["script", "payments", "checkout"] {
            let uri = format!("/app/example.com/setup/{step}");
            let body = body_text(call(&state, get(&uri, Some(&cookie))).await).await;
            assert!(!body.to_lowercase().contains("issue #"), "{step}");
        }
    }

    async fn connect_fake(state: &AppState, site: &str, provider: &str, key: &str) {
        let (ciphertext, nonce) = state.vault.encrypt(key).unwrap();
        sqlx::query(
            "INSERT INTO integrations (id, site_id, provider, key_ciphertext, key_nonce, last_synced_at, last_error, created_at)
             VALUES (?, ?, ?, ?, ?, 1, 'Dodo Payments returned HTTP 400', 1)",
        )
        .bind(provider)
        .bind(site)
        .bind(provider)
        .bind(ciphertext)
        .bind(nonce)
        .execute(&state.db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn connected_provider_shows_a_row_not_a_key_field() {
        let (_dir, state, _store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        connect_fake(&state, &site, "dodo", "dodo_test_HIDDENhidden9xQ2").await;
        let uri = "/app/example.com/setup/payments".to_string();
        let body = body_text(call(&state, get(&uri, Some(&cookie))).await).await;
        assert!(body.contains(r#"data-connected="dodo""#));
        assert!(body.contains(r#"Key ending <span class="mono">9xQ2</span>"#));
        assert!(
            !body.contains("HIDDEN"),
            "only the last four characters are shown"
        );
        assert!(
            body.contains("Dodo Payments returned HTTP 400"),
            "last error is shown"
        );
        assert!(body.contains("Connected &#10003;"));
        assert!(body.contains(">Manage</a>"));
        assert!(
            !body.contains(">Add<"),
            "a connected provider never shows Add"
        );
        assert!(
            !body.contains(r#"data-provider="dodo""#),
            "no key form for dodo"
        );
        assert!(
            body.contains(r#"data-provider="stripe""#),
            "others can still connect"
        );
    }

    #[tokio::test]
    async fn replace_key_reveals_the_form() {
        let (_dir, state, _store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        connect_fake(&state, &site, "dodo", "dodo_test_HIDDENhidden9xQ2").await;
        let uri = "/app/example.com/setup/payments".to_string();
        let res = call(
            &state,
            get(&format!("{uri}?provider=dodo&replace=1"), Some(&cookie)),
        )
        .await;
        let body = body_text(res).await;
        assert!(
            body.contains(r#"data-provider="dodo""#),
            "replace reveals the form"
        );
        assert!(body.contains(r#"value="dodo" checked"#));
        assert!(body.contains("Save new key"));
        assert!(!body.contains("HIDDEN"));

        for p in ["stripe", "polar", "lemonsqueezy"] {
            connect_fake(&state, &site, p, "fake_test_value_1234").await;
        }
        let body = body_text(call(&state, get(&uri, Some(&cookie))).await).await;
        assert!(
            !body.contains(r#"name="key""#),
            "no key inputs when all are connected"
        );

        let res = call(
            &state,
            form(
                "/app/example.com/settings/disconnect",
                "provider=dodo&back=setup".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.headers()["location"], uri.as_str());
        let body = body_text(call(&state, get(&uri, Some(&cookie))).await).await;
        assert!(
            body.contains(r#"data-provider="dodo""#),
            "a disconnected provider gets its form back"
        );
    }

    #[tokio::test]
    async fn settings_change_persists_and_rebuilds() {
        let (_dir, state, store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        seed(&store, &site);
        crate::sync::rebuild_attribution(&state, Some(&site)).await;
        assert_eq!(credited(&store, &site).as_deref(), Some("last"));

        let uri = "/app/example.com/settings".to_string();
        let res = call(
            &state,
            form(
                &uri,
                "model=first_touch&window_days=30&cookieless=on".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let row: (String, i64, bool) = sqlx::query_as(
            "SELECT attribution_model, window_days, cookieless FROM sites WHERE id = ?",
        )
        .bind(&site)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, ("first_touch".into(), 30, true));
        assert_eq!(credited(&store, &site).as_deref(), Some("first"));

        let page = body_text(call(&state, get(&uri, Some(&cookie))).await).await;
        assert!(page.contains("data-cookieless") && page.contains("30 days"));
        let bad = call(
            &state,
            form(&uri, "model=linear&window_days=30".into(), Some(&cookie)),
        )
        .await;
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn dashboard_and_payments_render() {
        let (_dir, state, store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        seed(&store, &site);
        crate::sync::rebuild_attribution(&state, Some(&site)).await;

        assert_eq!(
            call(&state, get("/app", Some(&cookie))).await.headers()["location"]
                .to_str()
                .unwrap(),
            "/app/example.com".to_string()
        );
        for query in [
            "",
            "?range=7d&channels=source&location=city",
            "?range=12m&channels=campaign&pages=before",
            "?channels=nope",
            "?from=2020-01-01&to=2099-01-01&chart=channel",
        ] {
            let res = call(
                &state,
                get(&format!("/app/example.com{query}"), Some(&cookie)),
            )
            .await;
            assert_eq!(res.status(), StatusCode::OK, "{query}");
            let body = body_text(res).await;
            assert!(
                body.contains("$98") && body.contains(r#"class="chart rev"#),
                "{query}"
            );
        }
        for query in [
            "?view=goals",
            "?view=goals&gtab=goals",
            "?view=visitors&vtab=recent",
            "?view=payments",
            "?view=retention&ret=refunds",
            "?range=today&channel=Communities",
            "?country=DE&chart=total",
        ] {
            let res = call(
                &state,
                get(&format!("/app/example.com{query}"), Some(&cookie)),
            )
            .await;
            assert_eq!(res.status(), StatusCode::OK, "{query}");
        }
        let body = body_text(call(&state, get("/app/example.com", Some(&cookie))).await).await;
        assert!(body.contains(">Weekly email</a>") && !body.contains("Client report"));
        let res = call(
            &state,
            form(
                "/app/example.com/settings/plan",
                "account_type=agency".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            res.headers()["location"],
            "/app/example.com/settings?saved=plan#plan"
        );
        let body = body_text(call(&state, get("/app/example.com", Some(&cookie))).await).await;
        assert!(body.contains(">Client report</a>") && !body.contains(">Weekly email</a>"));
        assert!(
            body.contains(r#"aria-current="true"><span>Since tracking started</span>"#),
            "new sites default"
        );
        assert!(body.contains("<b>1 of 2</b> payments matched"));
        assert!(body.contains("1 payment from Stripe had no visitor ID"));
        assert!(body.contains("client_reference_id: req.cookies.virling_vid"));
        assert!(body.contains(r#"href="/app/example.com/setup/payments">Connect</a>"#));
        assert!(body.contains(r#"action="/app/example.com/setup/dismiss""#));
        let res = call(
            &state,
            form(
                "/app/example.com/setup/dismiss",
                String::new(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.headers()["location"], "/app/example.com");
        let hidden = body_text(call(&state, get("/app/example.com", Some(&cookie))).await).await;
        assert!(!hidden.contains("Finish setting up"), "dismissed for good");
        let week =
            body_text(call(&state, get("/app/example.com?range=7d", Some(&cookie))).await).await;
        assert!(
            week.contains(r#"aria-current="true"><span>Last 7 days</span>"#),
            "label follows the range"
        );
        assert!(body.contains(r#"aria-label="Hacker News""#), "source logo");
        assert!(body.contains(r#"src="/assets/flags/de.svg""#) && body.contains("Germany"));
        assert!(body.contains(r#"href="/app/example.com?range=7d""#));
        assert!(body.contains(r#"href="/app/example.com/payments""#));
        assert!(body.contains(r#"href="/app/example.com?range=30d""#));
        assert!(!body.contains("range=all") && !body.contains("/app/sites/"));
        let social = body_text(
            call(
                &state,
                get("/app/example.com?channel=Communities", Some(&cookie)),
            )
            .await,
        )
        .await;
        assert!(
            social.contains("Showing only") && social.contains("Communities brought 1 visitor")
        );
        let goals =
            body_text(call(&state, get("/app/example.com?view=goals", Some(&cookie))).await).await;
        assert!(goals.contains("Visited the site") && goals.contains("Edit funnel steps"));
        let res = call(&state, get("/app/example.com/payments", Some(&cookie))).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        assert!(body.contains("Someblog") || body.contains("someblog.dev"));
        assert!(body.contains("Unattributed"));
        assert!(body.contains("Net revenue") && body.contains("1 of 2 matched to a visit"));
        assert!(body.contains(r#"data-total="2""#), "drawer can count rows");
        let payer = crate::alias::Names::load(&state.db, &site, vec!["v:v1".into()])
            .await
            .unwrap()
            .get("v:v1")
            .name;
        assert!(body.contains(&payer) && body.contains(r#"class="heat""#));
        assert!(body.contains("Customer") && body.contains("Payment 2"));
        let q = payer.split(' ').next().unwrap().to_lowercase();
        let found = body_text(
            call(
                &state,
                get(&format!("/app/example.com/payments?q={q}"), Some(&cookie)),
            )
            .await,
        )
        .await;
        assert!(found.contains(&payer) && found.contains("Showing 1 to 1 of 1"));
        store
            .insert_events(&[Event {
                site_id: site.clone(),
                ts: time::OffsetDateTime::now_utc() - time::Duration::days(3),
                visitor_id: "v1".into(),
                session_id: "last".into(),
                kind: "goal".into(),
                name: Some("trial start".into()),
                path: "/signup".into(),
                referrer_host: None,
                utm_source: None,
                utm_medium: None,
                utm_campaign: None,
                utm_content: None,
                utm_term: None,
                channel: "Direct".into(),
                country: Some("DE".into()),
                device: "Desktop".into(),
                browser: "Firefox".into(),
                os: "macOS".into(),
                region: None,
                city: None,
                host: None,
                language: None,
            }])
            .unwrap();
        let goals = body_text(
            call(
                &state,
                get("/app/example.com/payments?goal=trial+start", Some(&cookie)),
            )
            .await,
        )
        .await;
        assert!(goals.contains("trial start 1") && goals.contains(&payer));
        assert!(goals.contains("Someblog") || goals.contains("someblog.dev"));
        let unattributed = body_text(
            call(
                &state,
                get(
                    "/app/example.com/payments?filter=unattributed",
                    Some(&cookie),
                ),
            )
            .await,
        )
        .await;
        assert!(!unattributed.contains("/payments/stripe/pi_1\""));

        let mut req = get("/app/example.com/payments/stripe/pi_1", Some(&cookie));
        req.headers_mut().insert("x-drawer", "1".parse().unwrap());
        let drawer = body_text(call(&state, req).await).await;
        assert!(!drawer.contains("<html"), "drawer is a fragment");
        assert!(drawer.contains("Hacker News") && drawer.contains("Paid $49.00"));
        assert!(drawer.contains("Germany") && drawer.contains("Firefox on Desktop"));
        assert!(drawer.contains("2 visits") && drawer.contains(">v1</span>"));
        let full = call(
            &state,
            get("/app/example.com/payments/stripe/pi_2", Some(&cookie)),
        )
        .await;
        assert_eq!(full.status(), StatusCode::OK);
        assert!(body_text(full).await.contains("How to pass virling_vid"));
        assert_eq!(
            call(
                &state,
                get("/app/example.com/payments/stripe/nope", Some(&cookie)),
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn dashboard_cards_switch_tabs() {
        let (_dir, state, store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        seed(&store, &site);
        crate::sync::rebuild_attribution(&state, Some(&site)).await;
        let page = |q: &'static str| {
            let (state, cookie) = (state.clone(), cookie.clone());
            async move {
                let res = call(&state, get(&format!("/app/example.com{q}"), Some(&cookie))).await;
                assert_eq!(res.status(), StatusCode::OK, "{q}");
                body_text(res).await
            }
        };
        let home = page("").await;
        for text in [
            "Channel",
            "Referral",
            "First page of the visit",
            "No visitors online",
            "Expand all",
        ] {
            assert!(home.contains(text), "{text}");
        }
        assert!(home.contains(r#"href="/app/example.com?channels=source#channels""#));
        assert!(home.contains(r#"href="/app/example.com?pages=before#pages""#));
        assert!(
            home.contains(r#"href="/app/example.com?channel=Referral""#),
            "rows filter"
        );
        assert!(
            home.contains(r#"href="/app/example.com?source=someblog.dev""#),
            "kids filter"
        );
        let pays = page("?view=payments").await;
        assert!(pays.contains("The outlined step is the visit that gets the credit."));
        assert!(pays.contains(r#"class="d3-chip on""#), "credited visit");
        assert!(pays.contains(">/last</span>"), "last page before paying");
        let payer = crate::alias::Names::load(&state.db, &site, vec!["v:v1".into()])
            .await
            .unwrap()
            .get("v:v1")
            .name;
        assert!(pays.contains(&payer) && pays.contains("Germany"));
        let tabs = page("?channels=source&location=city&pages=before").await;
        assert!(tabs.contains("someblog.dev"));
        assert!(tabs.contains("Last page before checkout"));
        assert!(
            tabs.contains(r#"href="/app/example.com?channels=source&#38;location=city#pages""#)
        );
    }

    #[tokio::test]
    async fn short_links_redirect_with_tags_and_count_clicks() {
        let (_dir, state, _store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let bad = form(
            "/app/example.com/links",
            "place=x&destination=https%3A%2F%2Fevil.com%2F".into(),
            Some(&cookie),
        );
        let res = call(&state, bad).await;
        assert!(
            res.headers()["location"]
                .to_str()
                .unwrap()
                .ends_with("?channels=campaign&link_error=destination#channels")
        );
        let ok = form(
            "/app/example.com/links",
            "place=x&destination=%2Fpricing&name=Day+3+post".into(),
            Some(&cookie),
        );
        let res = call(&state, ok).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let ready = res.headers()["location"].to_str().unwrap().to_string();
        let slug: String = sqlx::query_scalar("SELECT slug FROM short_links")
            .fetch_one(&state.db)
            .await
            .unwrap();

        let click = Request::get(format!("/l/{slug}"))
            .header("user-agent", UA)
            .body(Body::empty())
            .unwrap();
        let res = call(&state, click).await;
        assert_eq!(res.status(), StatusCode::FOUND);
        assert_eq!(
            res.headers()["location"],
            format!(
                "https://example.com/pricing?utm_source=x.com&utm_medium=social&utm_campaign={slug}"
            )
            .as_str()
        );
        let bot = Request::get(format!("/l/{slug}"))
            .header("user-agent", "Googlebot/2.1")
            .body(Body::empty())
            .unwrap();
        assert_eq!(call(&state, bot).await.status(), StatusCode::FOUND);
        let clicks: i64 = sqlx::query_scalar("SELECT clicks FROM short_links")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(clicks, 1, "bots are not counted");
        assert_eq!(
            call(&state, get("/l/missing", None)).await.status(),
            StatusCode::NOT_FOUND
        );

        let body = body_text(call(&state, get(&ready, Some(&cookie))).await).await;
        assert!(body.contains("Link ready:") && body.contains(&format!("/l/{slug}")));
        assert!(body.contains("+ Tracked link"));
        let other = sign_in(&state, "other%40example.com").await;
        let id: String = sqlx::query_scalar("SELECT id FROM short_links")
            .fetch_one(&state.db)
            .await
            .unwrap();
        call(
            &state,
            form(
                "/app/example.com/links/delete",
                format!("id={id}"),
                Some(&other),
            ),
        )
        .await;
        call(
            &state,
            form(
                "/app/example.com/links/delete",
                format!("id={id}"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            call(&state, get(&format!("/l/{slug}"), None))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn saved_funnels_save_rename_and_delete() {
        let (_dir, state, store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        seed(&store, &site);
        let res = call(
            &state,
            form(
                "/app/example.com/funnels",
                "view=goals&range=30d&s1=page%3A%2Fpricing&s2=goal%3Asignup&s3=&fname=Trial+path"
                    .into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let (id, steps): (String, String) = sqlx::query_as("SELECT id, steps FROM funnels")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(steps, "page:/pricing\ngoal:signup");
        assert_eq!(
            res.headers()["location"],
            format!("/app/example.com?range=30d&view=goals&funnel={id}#goals").as_str()
        );
        let page = |q: String| {
            let (state, cookie) = (state.clone(), cookie.clone());
            async move {
                body_text(
                    call(
                        &state,
                        get(&format!("/app/example.com?view=goals{q}"), Some(&cookie)),
                    )
                    .await,
                )
                .await
            }
        };
        let body = page(String::new()).await;
        assert!(body.contains(">Suggested</a>") && body.contains(">Trial path</a>"));
        let body = page(format!("&funnel={id}")).await;
        assert!(body.contains(r#"aria-current="true">Trial path</a>"#));
        assert!(body.contains("<b>Signup</b>") && body.contains(r#"value="Trial path""#));
        let body = page(format!("&funnel={id}&s1=goal%3Asignup")).await;
        assert!(!body.contains("<b>Viewed pricing</b>"), "URL steps win");

        let other = sign_in(&state, "other%40example.com").await;
        let rename = format!("view=goals&funnel={id}&s1=page%3A%2Fpricing&fname=Pricing+only");
        call(
            &state,
            form("/app/example.com/funnels", rename.clone(), Some(&other)),
        )
        .await;
        call(
            &state,
            form("/app/example.com/funnels", rename, Some(&cookie)),
        )
        .await;
        let (name, steps): (String, String) = sqlx::query_as("SELECT name, steps FROM funnels")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(
            (name.as_str(), steps.as_str()),
            ("Pricing only", "page:/pricing")
        );

        let res = call(
            &state,
            form(
                "/app/example.com/funnels/delete",
                format!("view=goals&funnel={id}"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            res.headers()["location"],
            "/app/example.com?view=goals#goals"
        );
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM funnels")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }

    #[tokio::test]
    async fn visitors_and_timeline_render() {
        let (_dir, state, store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        seed(&store, &site);
        let exit = Event {
            kind: "exit".into(),
            name: Some("github.com/virling/virling".into()),
            path: "/last".into(),
            referrer_host: None,
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            channel: "Direct".into(),
            country: None,
            device: "Desktop".into(),
            browser: "Firefox".into(),
            os: "macOS".into(),
            region: None,
            city: None,
            host: None,
            language: None,
            site_id: site.clone(),
            ts: time::OffsetDateTime::now_utc() - time::Duration::days(3)
                + time::Duration::minutes(2),
            visitor_id: "v1".into(),
            session_id: "last".into(),
        };
        store.insert_events(&[exit]).unwrap();
        crate::sync::rebuild_attribution(&state, Some(&site)).await;
        let label = crate::alias::Names::load(&state.db, &site, vec!["v:v1".into()])
            .await
            .unwrap()
            .get("v:v1")
            .name;

        let body =
            body_text(call(&state, get("/app/example.com/visitors", Some(&cookie))).await).await;
        assert!(body.contains(&label) && body.contains(r#"href="/app/example.com/visitors/v1""#));
        assert!(body.contains("github.com/virling/virling") && body.contains("/last"));
        assert!(
            body.contains(r#"aria-label="Hacker News""#),
            "first source logo"
        );
        assert!(body.contains("$49"));
        for query in [
            "?exit=page",
            "?paid=no",
            "?channel=Direct",
            "?source=nope.dev&range=7d",
        ] {
            let res = call(
                &state,
                get(&format!("/app/example.com/visitors{query}"), Some(&cookie)),
            )
            .await;
            assert_eq!(res.status(), StatusCode::OK, "{query}");
            assert!(
                body_text(res).await.contains("No visitors match"),
                "{query}"
            );
        }
        let hn = body_text(
            call(
                &state,
                get(
                    "/app/example.com/visitors?source=news.ycombinator.com&exit=link&paid=yes",
                    Some(&cookie),
                ),
            )
            .await,
        )
        .await;
        assert!(hn.contains(&label) && hn.contains("Clear filters"));

        let res = call(&state, get("/app/example.com/visitors/v1", Some(&cookie))).await;
        assert_eq!(res.status(), StatusCode::OK);
        let t = body_text(res).await;
        assert!(t.contains("Visit 1") && t.contains("Visit 2"));
        assert!(t.contains("Clicked out to") && t.contains("github.com/virling/virling"));
        assert!(t.contains("Left from") && t.contains("/first"));
        assert!(t.contains("Paid $49.00") && t.contains("/payments/stripe/pi_1"));
        assert!(t.contains("2m 00s"), "time on page from the exit click");
        assert_eq!(
            call(
                &state,
                get("/app/example.com/visitors/nobody", Some(&cookie))
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );

        let pays =
            body_text(call(&state, get("/app/example.com/payments", Some(&cookie))).await).await;
        assert!(pays.contains(&format!(
            r#"<a class="vis-link" href="/app/example.com/visitors/v1" title="Visitor timeline"><b>{label}</b></a>"#
        )));
        let dash = body_text(
            call(
                &state,
                get(
                    "/app/example.com?view=visitors&vtab=customers",
                    Some(&cookie),
                ),
            )
            .await,
        )
        .await;
        assert!(dash.contains(r#"href="/app/example.com/visitors/v1""#));
        let mut req = get("/app/example.com/payments/stripe/pi_1", Some(&cookie));
        req.headers_mut().insert("x-drawer", "1".parse().unwrap());
        let drawer = body_text(call(&state, req).await).await;
        assert!(drawer.contains(r#"class="j-visitor" href="/app/example.com/visitors/v1""#));
    }

    /// Two visits and a matched payment on the Wednesday of last full week, plus one unmatched payment.
    fn seed_last_week(store: &DuckStore, site: &str) -> time::Date {
        let today = time::OffsetDateTime::now_utc().date();
        let monday = crate::dashboard::report::last_week(today);
        let at = time::PrimitiveDateTime::new(
            monday + time::Duration::days(2),
            time::macros::time!(12:00),
        )
        .assume_utc();
        let visit = |session: &str, visitor: &str, channel: &str, host: &str| Event {
            site_id: site.into(),
            ts: at,
            visitor_id: visitor.into(),
            session_id: session.into(),
            kind: "pageview".into(),
            name: None,
            path: "/pricing".into(),
            referrer_host: Some(host.into()),
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            utm_content: None,
            utm_term: None,
            channel: channel.into(),
            country: Some("DE".into()),
            device: "Desktop".into(),
            browser: "Firefox".into(),
            os: "macOS".into(),
            region: None,
            city: None,
            host: None,
            language: None,
        };
        store
            .insert_events(&[
                visit("s1", "v1", "AI", "chatgpt.com"),
                visit("s2", "v2", "Organic search", "google.com"),
            ])
            .unwrap();
        let pay = |id: &str, visitor: Option<&str>| Payment {
            site_id: site.into(),
            provider: "stripe".into(),
            provider_payment_id: id.into(),
            ts: at + time::Duration::hours(1),
            amount: 4900,
            currency: "usd".into(),
            amount_usd: Some(49.0),
            refunded: 0,
            status: "paid".into(),
            visitor_id: visitor.map(Into::into),
            customer_id: None,
            is_renewal: false,
        };
        store
            .upsert_payments(&[pay("pi_1", Some("v1")), pay("pi_2", None)])
            .unwrap();
        monday
    }

    #[tokio::test]
    async fn client_report_renders() {
        let (_dir, state, store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        make_agency(&state).await;
        let site = site_id(&state).await;
        let monday = seed_last_week(&store, &site);
        crate::sync::rebuild_attribution(&state, Some(&site)).await;
        let res = call(&state, get("/app/example.com/report", Some(&cookie))).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_text(res).await;
        let week = crate::dashboard::report::week_label(monday);
        assert!(body.contains(&week) && body.contains("Weekly revenue report"));
        assert!(
            body.contains("The week in one line") && body.contains("Revenue was $98 this week.")
        );
        assert!(body.contains("New customers") && body.contains("Revenue per visitor"));
        assert!(body.contains("Revenue by day") && body.contains(r#"class="r2-chart""#));
        assert!(body.contains("Where the money came from") && body.contains("chatgpt.com"));
        assert!(body.contains("Pages that sold") && body.contains("/pricing"));
        assert!(body.contains("$49 from 1 payment had no matched visit"));
        assert!(
            body.contains("[Your note to the client."),
            "placeholder in the app"
        );
        assert!(
            body.contains("Edit note")
                && body.contains("Download PDF")
                && body.contains("Send to client")
        );
        assert!(body.contains("The weekly email is off") && body.contains("Set up branding"));
        assert!(body.contains("Powered by <b>virling</b>"), "on by default");
        let older = monday - time::Duration::days(7);
        assert!(body.contains(&format!(r#"href="/app/example.com/report?week={older}""#)));
        let empty = body_text(
            call(
                &state,
                get(
                    &format!("/app/example.com/report?week={older}"),
                    Some(&cookie),
                ),
            )
            .await,
        )
        .await;
        assert!(empty.contains("No visits or payments this week."));

        let uri = "/app/example.com/settings/report";
        let bad = call(
            &state,
            form(
                uri,
                "agency_name=Kettle&logo_url=&accent=red&footer=".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            bad.headers()["location"],
            "/app/example.com/settings?error=accent#report"
        );
        let res = call(
            &state,
            form(
                uri,
                "agency_name=Kettle+and+Co.&logo_url=https%3A%2F%2Fkettle.co%2Flogo.png&accent=%23123ABC&footer=hello%40kettle.co&powered_by=on".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let page = body_text(
            call(
                &state,
                get("/app/example.com/settings?saved=report", Some(&cookie)),
            )
            .await,
        )
        .await;
        assert!(page.contains(r#"value="Kettle and Co.""#) && page.contains("Saved. The report"));
        assert!(page.contains("Show Powered by Virling on reports"));
        let note = call(
            &state,
            form(
                "/app/example.com/report/note",
                "note=Shipped+the+pricing+page.&week=".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            note.headers()["location"],
            "/app/example.com/report?saved=note"
        );
        let body =
            body_text(call(&state, get("/app/example.com/report", Some(&cookie))).await).await;
        assert!(body.contains(r#"style="--accent:#123ABC""#));
        assert!(body.contains(r#"src="https://kettle.co/logo.png""#));
        assert!(body.contains("hello@kettle.co") && !body.contains("Set up branding"));
        assert!(
            body.contains("A note from Kettle and Co.")
                && body.contains("Shipped the pricing page.")
        );
        let saved = body_text(
            call(
                &state,
                get("/app/example.com/report?saved=note", Some(&cookie)),
            )
            .await,
        )
        .await;
        assert!(saved.contains("Saved your note."));

        let other = sign_in(&state, "other%40example.com").await;
        let link = || async {
            let page =
                body_text(call(&state, get("/app/example.com/settings", Some(&cookie))).await)
                    .await;
            page.find("http://localhost:8080/r/")
                .map(|i| page[i + 21..i + 21 + 67].to_string())
        };
        assert_eq!(link().await, None);
        let share = "/app/example.com/settings/share";
        call(&state, form(share, String::new(), Some(&cookie))).await;
        let first = link().await.unwrap();
        let res = call(&state, get(&first, None)).await;
        assert_eq!(res.status(), StatusCode::OK);
        let public = body_text(res).await;
        assert!(public.contains("Kettle and Co.") && public.contains("$98"));
        assert!(public.contains("Shipped the pricing page.") && !public.contains("Edit note"));
        assert_eq!(
            public.to_lowercase().matches("virling").count(),
            1,
            "only the Powered by line"
        );
        assert!(public.contains("Powered by <b>virling</b>"));
        assert!(public.contains(&format!(r#"href="{first}?week={older}""#)));
        call(
            &state,
            form(
                uri,
                "agency_name=Kettle+and+Co.&logo_url=&accent=&footer=".into(),
                Some(&cookie),
            ),
        )
        .await;
        let public = body_text(call(&state, get(&first, None)).await).await;
        assert!(
            !public.to_lowercase().contains("virling"),
            "no Virling branding with the line off"
        );
        assert_eq!(
            call(&state, form(share, String::new(), Some(&other)))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        call(&state, form(share, String::new(), Some(&cookie))).await;
        let second = link().await.unwrap();
        assert_ne!(first, second);
        assert_eq!(
            call(&state, get(&first, None)).await.status(),
            StatusCode::NOT_FOUND
        );
        call(
            &state,
            form(
                "/app/example.com/settings/unshare",
                String::new(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(link().await, None);
        assert_eq!(
            call(&state, get(&second, None)).await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(&state, get("/app/example.com/report", Some(&other)))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn weekly_report_goes_out_once_per_week() {
        use crate::dashboard::schedule;
        let (_dir, state, store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        make_agency(&state).await;
        let site = site_id(&state).await;
        let monday = seed_last_week(&store, &site);
        crate::sync::rebuild_attribution(&state, Some(&site)).await;
        let before = sent(&state).len();
        let send = "/app/example.com/report/send";
        let res = call(&state, form(send, "week=".into(), Some(&cookie))).await;
        assert_eq!(
            res.headers()["location"],
            "/app/example.com/report?error=none"
        );
        let path = "/app/example.com/report/schedule";
        let bad = call(
            &state,
            form(
                path,
                "on=on&weekday=0&minute=540&recipients=nope&week=".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            bad.headers()["location"],
            "/app/example.com/report?error=recipients"
        );
        let ok = call(
            &state,
            form(
                path,
                "on=on&weekday=0&minute=540&recipients=client%40kettle.co%0Aops%40kettle.co&week="
                    .into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            ok.headers()["location"],
            "/app/example.com/report?saved=schedule"
        );
        let page =
            body_text(call(&state, get("/app/example.com/report", Some(&cookie))).await).await;
        assert!(page.contains("Sent every Monday at 9:00 to <b>client@kettle.co</b> and 1 more."));

        // Saved just now, so this week's send time is still ahead or already skipped.
        let (s, _) = schedule::load(&state.db, &site).await.unwrap();
        assert_eq!(s.recipients, ["client@kettle.co", "ops@kettle.co"]);
        sqlx::query("UPDATE report_brands SET send_from = 0 WHERE site_id = ?")
            .bind(&site)
            .execute(&state.db)
            .await
            .unwrap();
        let now = time::OffsetDateTime::now_utc();
        let due_at = time::PrimitiveDateTime::new(
            monday + time::Duration::days(7),
            time::macros::time!(9:00),
        )
        .assume_utc();
        let at = if now > due_at {
            now
        } else {
            due_at + time::Duration::minutes(1)
        };
        assert_eq!(schedule::tick(&state, at).await.unwrap(), 1);
        assert_eq!(
            schedule::tick(&state, at).await.unwrap(),
            0,
            "never twice for a week"
        );
        assert_eq!(
            schedule::tick(&state, at + time::Duration::hours(30))
                .await
                .unwrap(),
            0
        );
        let mails = sent(&state);
        assert_eq!(mails.len(), before + 2);
        let (to, subject, text) = &mails[before];
        assert_eq!(to, "client@kettle.co");
        assert!(subject.starts_with("Weekly revenue report for example.com"));
        assert!(
            text.contains("Revenue was $98 this week.")
                && text.contains("http://localhost:8080/r/")
        );
        let sent_at: Option<i64> =
            sqlx::query_scalar("SELECT sent_at FROM report_sends WHERE site_id = ? AND week = ?")
                .bind(&site)
                .bind(monday.to_string())
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert!(sent_at.is_some());

        let res = call(&state, form(send, "week=".into(), Some(&cookie))).await;
        assert_eq!(res.headers()["location"], "/app/example.com/report?sent=2");
        assert_eq!(
            sent(&state).len(),
            before + 4,
            "sending by hand always goes out"
        );
        let page =
            body_text(call(&state, get("/app/example.com/report?sent=2", Some(&cookie))).await)
                .await;
        assert!(page.contains("Sent the report to 2 people."));
    }

    #[tokio::test]
    async fn personal_plan_gets_a_weekly_email_to_itself() {
        use crate::dashboard::schedule;
        let (_dir, state, store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        let monday = seed_last_week(&store, &site);
        crate::sync::rebuild_attribution(&state, Some(&site)).await;
        // An agency that switched to Personal keeps its old client recipients and branding.
        sqlx::query("INSERT INTO report_brands (site_id, agency_name, note, recipients) VALUES (?, 'Kettle', 'Hi there', 'client@kettle.co')")
            .bind(&site)
            .execute(&state.db)
            .await
            .unwrap();
        let page =
            body_text(call(&state, get("/app/example.com/report", Some(&cookie))).await).await;
        assert!(
            page.contains("<b>Weekly email</b>")
                && page.contains(r#"<div class="r2-name">virling</div>"#)
        );
        for gone in [
            "Send to client",
            "Edit note",
            "Kettle",
            "Hi there",
            "client@kettle.co",
            "Powered by",
        ] {
            assert!(!page.contains(gone), "{gone}");
        }
        let settings =
            body_text(call(&state, get("/app/example.com/settings", Some(&cookie))).await).await;
        assert!(!settings.contains("Agency name") && !settings.contains("Client link"));
        let ok = call(
            &state,
            form(
                "/app/example.com/report/schedule",
                "on=on&weekday=0&minute=540&recipients=&week=".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            ok.headers()["location"],
            "/app/example.com/report?saved=schedule"
        );
        let (s, _) = schedule::load(&state.db, &site).await.unwrap();
        assert_eq!(s.recipients, ["me@example.com"]);
        sqlx::query("UPDATE report_brands SET send_from = 0, recipients = 'client@kettle.co' WHERE site_id = ?")
            .bind(&site)
            .execute(&state.db)
            .await
            .unwrap();
        let before = sent(&state).len();
        let at = time::PrimitiveDateTime::new(
            monday + time::Duration::days(7),
            time::macros::time!(9:00),
        )
        .assume_utc()
        .max(time::OffsetDateTime::now_utc());
        assert_eq!(
            schedule::tick(&state, at + time::Duration::minutes(1))
                .await
                .unwrap(),
            1
        );
        let mails = sent(&state);
        assert_eq!(mails.len(), before + 1);
        let (to, _, text) = &mails[before];
        assert_eq!(to, "me@example.com", "never the old client");
        assert!(!text.contains("Kettle") && !text.contains("Hi there"));
    }

    #[tokio::test]
    async fn report_email_renders_with_our_icons() {
        let (_dir, state, store) = setup().await;
        let site = site_id(&state).await;
        make_agency(&state).await;
        let monday = seed_last_week(&store, &site);
        crate::sync::rebuild_attribution(&state, Some(&site)).await;
        let s: crate::sites::Site =
            sqlx::query_as("SELECT id, domain, timezone, cookieless FROM sites WHERE id = ?")
                .bind(&site)
                .fetch_one(&state.db)
                .await
                .unwrap();
        let r = crate::dashboard::report::view(&state, &s, monday, false)
            .await
            .unwrap();
        let (subject, text, html) =
            crate::dashboard::schedule::email(&r, "https://app.example", "https://app.example/r/t")
                .unwrap();
        assert!(subject.contains("example.com"));
        assert!(html.contains(r#"src="https://app.example/assets/icons/chatgpt.svg""#));
        assert!(
            html.contains("Open the full report")
                && html.contains(r#"href="https://app.example/r/t""#)
        );
        assert!(html.contains("Revenue was $98 this week.") && html.contains("Powered by"));
        assert!(
            !html.contains("<svg") && !html.contains("<script"),
            "no inline SVG or scripts in email"
        );
        assert!(text.contains("Where the money came from"));
        let icon = call(&state, get("/assets/icons/chatgpt.svg", None)).await;
        assert_eq!(icon.status(), StatusCode::OK);
        let glyph = call(&state, get("/assets/icons/glyph-direct.svg", None)).await;
        assert_eq!(glyph.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn empty_dashboard_waits_for_the_first_visit() {
        let (_dir, state, _store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        connect_fake(&state, &site, "dodo", "dodo_test_key_1234").await;
        let body = body_text(call(&state, get("/app/example.com", Some(&cookie))).await).await;
        assert!(body.contains("Waiting for your first visitor"));
        assert!(body.contains("Waiting for the first pageview"));
        assert!(body.contains("Dodo Payments, read-only key"));
        assert!(body.contains(r#"data-site=&#34;example.com&#34;"#));
        assert!(body.contains("after the first visit and payment"));
        let payments =
            body_text(call(&state, get("/app/example.com/payments", Some(&cookie))).await).await;
        assert!(
            payments.contains("No payments yet") && payments.contains("Dodo Payments is connected")
        );
    }

    #[tokio::test]
    async fn old_site_urls_redirect_to_the_domain() {
        let (_dir, state, _store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        for (old, new) in [
            (
                "dashboard?pages=visitors",
                "/app/example.com?pages=visitors",
            ),
            (
                "payments/stripe/pi_1",
                "/app/example.com/payments/stripe/pi_1",
            ),
            ("settings", "/app/example.com/settings"),
        ] {
            let res = call(
                &state,
                get(&format!("/app/sites/{site}/{old}"), Some(&cookie)),
            )
            .await;
            assert_eq!(res.status(), StatusCode::PERMANENT_REDIRECT, "{old}");
            assert_eq!(res.headers()["location"], new, "{old}");
        }
        let other = sign_in(&state, "other%40example.com").await;
        let res = call(
            &state,
            get(&format!("/app/sites/{site}/dashboard"), Some(&other)),
        )
        .await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn favicon_replaces_the_letter_once_cached() {
        let (_dir, state, _store) = setup().await;
        let cookie = sign_in(&state, "me%40example.com").await;
        let site = site_id(&state).await;
        let page = || async {
            body_text(call(&state, get("/app/example.com/settings", Some(&cookie))).await).await
        };
        assert!(page().await.contains(r#"<span class="favicon">E</span>"#));
        let failed = "INSERT INTO site_icons (site_id, content_type, body, fetched_at) VALUES (?, NULL, NULL, 5)";
        sqlx::query(failed)
            .bind(&site)
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            page().await.contains(r#"<span class="favicon">E</span>"#),
            "failed fetch keeps the letter"
        );
        sqlx::query(
            "UPDATE site_icons SET content_type = 'image/svg+xml', body = ? WHERE site_id = ?",
        )
        .bind(b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec())
        .bind(&site)
        .execute(&state.db)
        .await
        .unwrap();
        assert!(
            page()
                .await
                .contains(r#"<img class="favicon" src="/app/example.com/icon?v=5""#)
        );
        let res = call(&state, get("/app/example.com/icon", Some(&cookie))).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["content-type"], "image/svg+xml");
        assert!(
            res.headers()["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("sandbox")
        );
        let other = sign_in(&state, "other%40example.com").await;
        let res = call(&state, get("/app/example.com/icon", Some(&other))).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn source_icons_are_served_without_sign_in() {
        let (_dir, state, _store) = setup().await;
        sqlx::query("INSERT INTO source_icons VALUES ('someblog.dev', 'image/png', ?, 5)")
            .bind(b"PNG".to_vec())
            .execute(&state.db)
            .await
            .unwrap();
        let res = call(&state, get("/source-icons/someblog.dev?v=5", None)).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["content-type"], "image/png");
        assert_eq!(res.headers()["x-content-type-options"], "nosniff");
        assert!(
            res.headers()["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("sandbox")
        );
        assert_eq!(body_text(res).await, "PNG");
        for path in ["/source-icons/other.dev", "/source-icons/127.0.0.1"] {
            assert_eq!(
                call(&state, get(path, None)).await.status(),
                StatusCode::NOT_FOUND
            );
        }
    }

    #[tokio::test]
    async fn other_accounts_cannot_open_a_site() {
        let (_dir, state, _store) = setup().await;
        let site = site_id(&state).await;
        let cookie = sign_in(&state, "other%40example.com").await;
        for path in [
            "dashboard",
            "payments",
            "payments/stripe/pi_1",
            "settings",
            "setup/payments",
            "setup/checkout",
            "setup/script",
            "latest-event",
        ] {
            let res = call(
                &state,
                get(&format!("/app/example.com/{path}"), Some(&cookie)),
            )
            .await;
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
        }
        let res = call(
            &state,
            form(
                "/app/example.com/settings",
                "model=first_touch&window_days=30".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let res = call(
            &state,
            form(
                "/app/example.com/settings/disconnect",
                "provider=stripe".into(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let res = call(
            &state,
            form(
                "/app/example.com/setup/dismiss",
                String::new(),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let model: String = sqlx::query_scalar("SELECT attribution_model FROM sites WHERE id = ?")
            .bind(&site)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(model, "last_non_direct");
    }
}
