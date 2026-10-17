//! Times every app page, API endpoint, MCP tool and ingest on a seeded data dir.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration as Elapsed, Instant};

use anyhow::Context;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::date;

use crate::app::{self, AppState, Config};
use crate::cache::Cached;
use crate::events::{DuckStore, EventStore};
use crate::stats::{Dimension, PaymentFilter, Period};
use crate::{api, auth, crypto, demo, mail, sync};

const EMAIL: &str = "bench@virling.local";

#[derive(clap::Args)]
pub struct BenchArgs {
    /// Copies of the demo data when seeding, each with its own visitors and shifted up to a year back
    #[arg(long, default_value_t = 1)]
    scale: u32,
    /// Timed requests per route after the first
    #[arg(long, default_value_t = 20)]
    runs: usize,
    /// File of "route p95_ms" lines; the run fails when a route's p95 is over its budget
    #[arg(long)]
    budgets: Option<PathBuf>,
    /// Read every request from the store, without the result cache
    #[arg(long)]
    no_cache: bool,
    /// Only routes whose name contains this
    #[arg(long)]
    only: Option<String>,
}

/// How a route authenticates.
#[derive(Clone, Copy)]
enum Auth {
    Session,
    Key,
    None,
}

struct Route {
    name: String,
    path: String,
    body: Option<Value>,
    auth: Auth,
    drawer: bool,
    /// Sends a new pageview before each timed request, so every read misses the cache.
    fresh: bool,
    /// Sends this many pageviews at once per run, each from a new visitor, timing every one.
    visitors: usize,
}

fn get(name: &str, path: String, auth: Auth) -> Route {
    Route {
        name: name.into(),
        path,
        body: None,
        auth,
        drawer: false,
        fresh: false,
        visitors: 0,
    }
}

struct Timing {
    name: String,
    cold: f64,
    p50: f64,
    p95: f64,
    max: f64,
}

fn ms(d: Elapsed) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Seeds `dir` with the demo when it is empty, scaled `scale` times.
async fn prepare(dir: &Path, scale: u32) -> anyhow::Result<bool> {
    if dir.join("virling.sqlite").exists() {
        return Ok(false);
    }
    let started = Instant::now();
    demo::data(dir, EMAIL).await?;
    if scale > 1 {
        let conn = duckdb::Connection::open(dir.join("events.duckdb"))?;
        conn.execute_batch(&format!(
            "LOAD icu;
            SET TimeZone = 'UTC';
            CREATE TEMP TABLE e0 AS SELECT * FROM events;
            CREATE TEMP TABLE p0 AS SELECT * FROM payments;
            CREATE TEMP TABLE s0 AS SELECT * FROM subscriptions;
            INSERT INTO events SELECT e0.* REPLACE (
                e0.ts - to_days(CAST(k * 37 % 365 AS INTEGER)) AS ts,
                e0.visitor_id || '-' || k AS visitor_id,
                e0.session_id || '-' || k AS session_id
            ) FROM e0, range(1, {scale}) t(k);
            INSERT INTO payments SELECT p0.* REPLACE (
                p0.ts - to_days(CAST(k * 37 % 365 AS INTEGER)) AS ts,
                p0.provider_payment_id || '-' || k AS provider_payment_id,
                p0.visitor_id || '-' || k AS visitor_id,
                p0.customer_id || '-' || k AS customer_id
            ) FROM p0, range(1, {scale}) t(k);
            INSERT INTO subscriptions SELECT s0.* REPLACE (
                s0.subscription_id || '-' || k AS subscription_id,
                s0.customer_id || '-' || k AS customer_id
            ) FROM s0, range(1, {scale}) t(k);
            CREATE OR REPLACE TABLE events AS SELECT * FROM events ORDER BY ts;
            DROP TABLE touches;
            DROP TABLE first_touches;
            CHECKPOINT;"
        ))?;
    }
    eprintln!(
        "seeded {} at {scale}x in {:.1}s",
        dir.display(),
        started.elapsed().as_secs_f64()
    );
    Ok(true)
}

pub async fn run(dir: &Path, args: BenchArgs) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let seeded = prepare(dir, args.scale).await?;
    let db = crate::open_sqlite(dir).await?;
    let opened = Instant::now();
    let store = DuckStore::open(&dir.join("events.duckdb"))?;
    eprintln!("opened the event store in {:.0} ms", ms(opened.elapsed()));
    let events: Arc<dyn EventStore> = if args.no_cache {
        Arc::new(store)
    } else {
        Arc::new(Cached::new(store))
    };
    let (site_id, domain, account): (String, String, String) =
        sqlx::query_as("SELECT id, domain, account_id FROM sites ORDER BY created_at LIMIT 1")
            .fetch_one(&db)
            .await
            .context("the data dir has no site")?;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let token = auth::random_token();
    sqlx::query(
        "INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(auth::hash_token(&token))
    .bind(&account)
    .bind(now)
    .bind(now + 3600)
    .execute(&db)
    .await?;
    let key = api::create_key(&db, &account, "bench").await?;
    let share = auth::random_token();
    sqlx::query("INSERT INTO report_brands (site_id, share_token) VALUES (?, ?) ON CONFLICT (site_id) DO UPDATE SET share_token = excluded.share_token")
        .bind(&site_id)
        .bind(&share)
        .execute(&db)
        .await?;
    sqlx::query("UPDATE sites SET public_dashboard = 1 WHERE id = ?")
        .bind(&site_id)
        .execute(&db)
        .await?;
    let config = Config {
        marketing: false,
        app: true,
        trust_proxy: false,
        events_per_second: u32::MAX,
        base_url: "http://localhost".into(),
        waitlist_from: String::new(),
    };
    let state = AppState::new(
        config,
        db,
        events.clone(),
        mail::Mailer::Log,
        crypto::Vault::load(dir, None)?,
    );
    if seeded {
        let started = Instant::now();
        sync::rebuild_attribution(&state, Some(&site_id)).await;
        eprintln!("rebuilt attribution in {:.0} ms", ms(started.elapsed()));
    }

    let started = Instant::now();
    while !events.settled(&site_id) {
        tokio::time::sleep(Elapsed::from_millis(20)).await;
    }
    eprintln!("loaded the site in {:.0} ms", ms(started.elapsed()));

    let all = Period {
        from: date!(2000 - 01 - 01),
        to: date!(2100 - 01 - 01),
        tz: "UTC".into(),
    };
    let (store, id) = (events.clone(), site_id.clone());
    let (payment, goal, channel) = tokio::task::spawn_blocking(move || {
        let page = store.payment_list(&id, &all, PaymentFilter::Attributed, 1, 0)?;
        let goal = store
            .goal_counts(&id, &all)?
            .first()
            .map(|g| g.name.clone());
        let channel = store
            .breakdown(&id, &all, Dimension::Channel)?
            .into_iter()
            .find_map(|r| r.key);
        anyhow::Ok((page.rows.into_iter().next(), goal, channel))
    })
    .await??;
    let payment = payment.context("the data dir has no attributed payment")?;
    let visitor = payment.visitor.clone().unwrap_or_default();
    let channel = channel.unwrap_or_default();
    let enc = |v: &str| -> String { url::form_urlencoded::byte_serialize(v.as_bytes()).collect() };

    let app = format!("/app/{domain}");
    let mut routes = vec![get("page.dashboard", app.clone(), Auth::Session)];
    for range in [
        "today",
        "yesterday",
        "7d",
        "30d",
        "90d",
        "month",
        "12m",
        "all",
    ] {
        routes.push(get(
            &format!("page.dashboard.{range}"),
            format!("{app}?range={range}"),
            Auth::Session,
        ));
    }
    routes.extend([
        get(
            "page.dashboard.channel_filter",
            format!("{app}?channel={}", enc(&channel)),
            Auth::Session,
        ),
        get(
            "page.dashboard.viewed_filter",
            format!("{app}?viewed=%2Fpricing"),
            Auth::Session,
        ),
        get(
            "page.dashboard.compare_off",
            format!("{app}?compare=off"),
            Auth::Session,
        ),
    ]);
    for view in ["goals", "visitors", "payments", "retention"] {
        routes.push(get(
            &format!("page.dashboard.view_{view}"),
            format!("{app}?view={view}"),
            Auth::Session,
        ));
    }
    let drawer = format!(
        "{app}/payments/{}/{}",
        payment.provider, payment.provider_payment_id
    );
    routes.extend([
        get("page.payments", format!("{app}/payments"), Auth::Session),
        get(
            "page.payments.90d",
            format!("{app}/payments?range=90d"),
            Auth::Session,
        ),
        get(
            "page.payments.attributed",
            format!("{app}/payments?filter=attributed"),
            Auth::Session,
        ),
        get(
            "page.payments.search",
            format!("{app}/payments?q=fox"),
            Auth::Session,
        ),
        get("page.payment", drawer.clone(), Auth::Session),
        Route {
            drawer: true,
            ..get("page.payment.drawer", drawer, Auth::Session)
        },
        get("page.visitors", format!("{app}/visitors"), Auth::Session),
        get(
            "page.visitors.paid",
            format!("{app}/visitors?paid=yes"),
            Auth::Session,
        ),
        get(
            "page.visitors.channel",
            format!("{app}/visitors?channel={}", enc(&channel)),
            Auth::Session,
        ),
        get(
            "page.visitor",
            format!("{app}/visitors/{}", enc(&visitor)),
            Auth::Session,
        ),
        get("page.retention", format!("{app}/retention"), Auth::Session),
        get("page.report", format!("{app}/report"), Auth::Session),
        get("page.settings", format!("{app}/settings"), Auth::Session),
        get("page.share", format!("/r/{share}"), Auth::None),
        get("page.public", format!("/share/{domain}"), Auth::None),
        get(
            "page.public.90d",
            format!("/share/{domain}?range=90d"),
            Auth::None,
        ),
    ]);
    if let Some(g) = &goal {
        routes.push(get(
            "page.payments.goal",
            format!("{app}/payments?goal={}", enc(g)),
            Auth::Session,
        ));
    }
    let api_base = format!("/api/v1/sites/{domain}");
    routes.push(get("api.sites", "/api/v1/sites".into(), Auth::Key));
    for range in ["7d", "30d", "90d"] {
        routes.push(get(
            &format!("api.overview.{range}"),
            format!("{api_base}/overview?range={range}"),
            Auth::Key,
        ));
    }
    for (dim, _, _) in api::DIMENSIONS {
        routes.push(get(
            &format!("api.breakdown.{dim}"),
            format!("{api_base}/breakdown/{dim}"),
            Auth::Key,
        ));
    }
    routes.extend([
        get("api.payments", format!("{api_base}/payments"), Auth::Key),
        get(
            "api.payments.unattributed",
            format!("{api_base}/payments?filter=unattributed"),
            Auth::Key,
        ),
        get("api.retention", format!("{api_base}/retention"), Auth::Key),
        get(
            "api.visitor",
            format!("{api_base}/visitors/{}", enc(&visitor)),
            Auth::Key,
        ),
    ]);
    let tools = [
        ("list_sites", json!({})),
        ("get_overview", json!({ "site": domain })),
        (
            "get_breakdown",
            json!({ "site": domain, "dimension": "channel" }),
        ),
        ("get_retention", json!({ "site": domain })),
        ("list_payments", json!({ "site": domain })),
        (
            "get_visitor_timeline",
            json!({ "site": domain, "visitor_id": visitor }),
        ),
    ];
    for (n, (tool, arguments)) in tools.into_iter().enumerate() {
        routes.push(Route {
            body: Some(json!({ "jsonrpc": "2.0", "id": n, "method": "tools/call",
                "params": { "name": tool, "arguments": arguments } })),
            ..get(&format!("mcp.{tool}"), "/mcp".into(), Auth::Key)
        });
    }
    let pageview = json!({ "n": "pageview", "u": format!("https://{domain}/pricing"), "s": domain,
        "v": "benchvisitor0001", "w": 1280 });
    routes.push(Route {
        body: Some(pageview.clone()),
        ..get("ingest.pageview", "/api/event".into(), Auth::None)
    });
    for (name, visitors) in [("ingest.new_visitor", 1), ("ingest.concurrent16", 16)] {
        routes.push(Route {
            body: Some(pageview.clone()),
            visitors,
            ..get(name, "/api/event".into(), Auth::None)
        });
    }
    for (name, path, auth) in [
        ("fresh.page.dashboard", app.clone(), Auth::Session),
        (
            "fresh.page.dashboard.90d",
            format!("{app}?range=90d"),
            Auth::Session,
        ),
        (
            "fresh.page.payments",
            format!("{app}/payments"),
            Auth::Session,
        ),
        (
            "fresh.page.visitors",
            format!("{app}/visitors"),
            Auth::Session,
        ),
        ("fresh.page.public", format!("/share/{domain}"), Auth::None),
        (
            "fresh.api.overview.30d",
            format!("{api_base}/overview?range=30d"),
            Auth::Key,
        ),
    ] {
        routes.push(Route {
            fresh: true,
            ..get(name, path, auth)
        });
    }
    if let Some(only) = &args.only {
        routes.retain(|r| r.name.contains(only.as_str()));
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(
        axum::serve(
            listener,
            app::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .into_future(),
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let cookie = format!("{}={token}", auth::SESSION_COOKIE);
    let send = async |r: &Route| -> anyhow::Result<Elapsed> {
        let url = format!("{base}{}", r.path);
        let mut req = match &r.body {
            Some(b) => client.post(&url).json(b),
            None => client.get(&url),
        };
        req = req.header("user-agent", "Mozilla/5.0 (Macintosh) Firefox/130.0");
        req = match r.auth {
            Auth::Session => req.header("cookie", &cookie),
            Auth::Key => req.bearer_auth(&key),
            Auth::None => req,
        };
        if r.drawer {
            req = req.header("x-drawer", "1");
        }
        let started = Instant::now();
        let res = req.send().await?;
        let status = res.status();
        let body = res.bytes().await?;
        let took = started.elapsed();
        anyhow::ensure!(
            status.is_success(),
            "{} answered {status}: {}",
            r.name,
            String::from_utf8_lossy(&body)
                .chars()
                .take(200)
                .collect::<String>()
        );
        Ok(took)
    };
    let nudge = Route {
        body: Some(pageview),
        ..get("nudge", "/api/event".into(), Auth::None)
    };

    let mut sent = 0;
    let mut burst = async |r: &Route| -> anyhow::Result<Vec<f64>> {
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..r.visitors {
            sent += 1;
            let mut body = r.body.clone().unwrap_or_default();
            body["v"] = json!(format!("benchnew{sent:08}"));
            let req = client
                .post(format!("{base}{}", r.path))
                .header("user-agent", "Mozilla/5.0 (Macintosh) Firefox/130.0")
                .json(&body);
            set.spawn(async move {
                let started = Instant::now();
                let res = req.send().await?;
                anyhow::ensure!(
                    res.status().is_success(),
                    "ingest answered {}",
                    res.status()
                );
                Ok(ms(started.elapsed()))
            });
        }
        let mut out = Vec::new();
        while let Some(t) = set.join_next().await {
            out.push(t??);
        }
        Ok(out)
    };

    let mut timings = Vec::new();
    for r in &routes {
        if r.visitors > 0 {
            let cold = burst(r).await?.into_iter().fold(0.0, f64::max);
            let mut runs = Vec::new();
            for _ in 0..args.runs {
                runs.extend(burst(r).await?);
            }
            runs.sort_by(f64::total_cmp);
            let at =
                |q: f64| runs[((runs.len() as f64 * q).ceil() as usize).clamp(1, runs.len()) - 1];
            timings.push(Timing {
                name: r.name.clone(),
                cold,
                p50: at(0.5),
                p95: at(0.95),
                max: at(1.0),
            });
            continue;
        }
        if r.fresh {
            send(&nudge).await?;
        }
        let cold = ms(send(r).await?);
        let mut runs = Vec::with_capacity(args.runs);
        for _ in 0..args.runs {
            if r.fresh {
                send(&nudge).await?;
            }
            runs.push(ms(send(r).await?));
        }
        runs.sort_by(f64::total_cmp);
        let at = |q: f64| runs[((runs.len() as f64 * q).ceil() as usize).clamp(1, runs.len()) - 1];
        timings.push(Timing {
            name: r.name.clone(),
            cold,
            p50: at(0.5),
            p95: at(0.95),
            max: at(1.0),
        });
    }

    let budgets = match &args.budgets {
        Some(path) => parse_budgets(&std::fs::read_to_string(path)?)?,
        None => Vec::new(),
    };
    let budget_of = |name: &str| {
        budgets
            .iter()
            .filter(|(pattern, _)| {
                pattern == name
                    || pattern == "*"
                    || name.starts_with(pattern.trim_end_matches('*')) && pattern.ends_with('*')
            })
            .max_by_key(|(pattern, _)| pattern.len())
            .map(|b| b.1)
    };
    println!(
        "{:<36} {:>9} {:>9} {:>9} {:>9} {:>8}",
        "route", "cold ms", "p50 ms", "p95 ms", "max ms", "budget"
    );
    let mut over = Vec::new();
    for t in &timings {
        let budget = budget_of(&t.name);
        let flag = match budget {
            Some(b) if t.p95 > b => {
                over.push(t.name.clone());
                " OVER"
            }
            _ => "",
        };
        println!(
            "{:<36} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>8}{flag}",
            t.name,
            t.cold,
            t.p50,
            t.p95,
            t.max,
            budget.map_or_else(|| "-".into(), |b| format!("{b}"))
        );
    }
    anyhow::ensure!(over.is_empty(), "over budget: {}", over.join(", "));
    Ok(())
}

/// "route ms" per line; a trailing * matches a prefix and "*" alone any route; # starts a comment.
fn parse_budgets(text: &str) -> anyhow::Result<Vec<(String, f64)>> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (name, ms) = l
                .split_once(char::is_whitespace)
                .with_context(|| format!("budget line {l:?} needs a route and milliseconds"))?;
            Ok((name.to_string(), ms.trim().parse()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn budgets_match_exact_names_before_prefixes() {
        let b = super::parse_budgets("# local\n* 50\napi.* 10\napi.retention 20\n").unwrap();
        assert_eq!(b.len(), 3);
        assert_eq!(b[1], ("api.*".to_string(), 10.0));
    }
}
