//! Seeds a sample site and renders its app pages to static HTML for virling.com/demo.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use time::{Duration, OffsetDateTime};

use crate::app::{self, AppState, Config};
use crate::events::{DuckStore, Event, EventStore, Payment};
use crate::providers::Subscription;
use crate::{auth, channel, crypto, favicon, flags, icons, mail, sites, sync};

const DOMAIN: &str = "tallybird.app";
const DAYS: i64 = 180;
/// Visitor timelines the demo prebuilds, taken from the first payments it reaches.
const TIMELINES: usize = 12;
/// A made-up mark for the made-up site, so the site switcher shows an icon.
const SITE_ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="8" fill="#1F6F5C"/><path d="M9 9v14M13.5 9v14M18 9v14M22.5 9v14M7 20l18-8" stroke="#fff" stroke-width="2.4" stroke-linecap="round"/></svg>"##;
const BANNER: &str = r#"<div class="notice row"><span>Demo with sample data.</span><a class="b" href="/#join">Join the waitlist</a></div>"#;

/// Where visitors come from: referrer host, utm source/medium/campaign, share of traffic, growth over the period, chance to buy.
struct Source {
    host: Option<&'static str>,
    utm: Option<(&'static str, &'static str, &'static str)>,
    weight: f64,
    growth: f64,
    convert: f64,
    paths: &'static [&'static str],
}

const BLOG: &[&str] = &[
    "/blog/how-to-invoice-a-client",
    "/blog/late-payment-email-templates",
    "/templates/freelance-invoice",
    "/blog/net-30-explained",
];
const PRODUCT: &[&str] = &["/", "/pricing", "/features/recurring-invoices", "/compare"];
const DOCS: &[&str] = &["/docs/getting-started", "/docs/stripe", "/pricing"];

const SOURCES: &[Source] = &[
    Source {
        host: Some("www.google.com"),
        utm: None,
        weight: 32.0,
        growth: 1.2,
        convert: 0.011,
        paths: BLOG,
    },
    Source {
        host: Some("chatgpt.com"),
        utm: None,
        weight: 4.0,
        growth: 2.6,
        convert: 0.032,
        paths: PRODUCT,
    },
    Source {
        host: Some("www.perplexity.ai"),
        utm: None,
        weight: 1.8,
        growth: 2.4,
        convert: 0.03,
        paths: PRODUCT,
    },
    Source {
        host: Some("claude.ai"),
        utm: None,
        weight: 1.5,
        growth: 3.0,
        convert: 0.034,
        paths: DOCS,
    },
    Source {
        host: Some("t.co"),
        utm: None,
        weight: 8.0,
        growth: 1.0,
        convert: 0.011,
        paths: PRODUCT,
    },
    Source {
        host: Some("news.ycombinator.com"),
        utm: None,
        weight: 2.0,
        growth: 1.0,
        convert: 0.007,
        paths: &["/blog/we-stopped-chasing-invoices", "/"],
    },
    Source {
        host: None,
        utm: Some(("newsletter", "email", "weekly-digest")),
        weight: 4.0,
        growth: 1.3,
        convert: 0.026,
        paths: &["/blog/net-30-explained", "/changelog", "/pricing"],
    },
    Source {
        host: Some("github.com"),
        utm: None,
        weight: 2.5,
        growth: 1.0,
        convert: 0.012,
        paths: DOCS,
    },
    Source {
        host: Some("www.bing.com"),
        utm: None,
        weight: 2.0,
        growth: 1.0,
        convert: 0.01,
        paths: BLOG,
    },
    Source {
        host: Some("www.reddit.com"),
        utm: None,
        weight: 2.5,
        growth: 1.1,
        convert: 0.011,
        paths: BLOG,
    },
    Source {
        host: Some("www.linkedin.com"),
        utm: Some(("linkedin", "social", "founder-posts")),
        weight: 3.0,
        growth: 1.5,
        convert: 0.014,
        paths: PRODUCT,
    },
    Source {
        host: Some("duckduckgo.com"),
        utm: None,
        weight: 1.5,
        growth: 1.1,
        convert: 0.012,
        paths: BLOG,
    },
    Source {
        host: Some("stackoverflow.com"),
        utm: None,
        weight: 1.2,
        growth: 1.0,
        convert: 0.008,
        paths: DOCS,
    },
    Source {
        host: None,
        utm: None,
        weight: 18.0,
        growth: 1.2,
        convert: 0.016,
        paths: PRODUCT,
    },
];

const COUNTRIES: &[(&str, f64)] = &[
    ("US", 36.0),
    ("GB", 9.0),
    ("DE", 8.0),
    ("IN", 7.0),
    ("CA", 5.0),
    ("FR", 4.0),
    ("NL", 3.0),
    ("AU", 3.0),
    ("BR", 3.0),
    ("ES", 2.5),
    ("SE", 2.0),
    ("PL", 2.0),
    ("JP", 1.5),
    ("SG", 1.0),
];
const DEVICES: &[(&str, f64)] = &[("Desktop", 66.0), ("Mobile", 31.0), ("Tablet", 3.0)];
const BROWSERS: &[(&str, f64)] = &[
    ("Chrome", 63.0),
    ("Safari", 24.0),
    ("Firefox", 10.0),
    ("Opera", 3.0),
];
/// Monthly plans in cents.
const PLANS: &[(i64, f64)] = &[(1200, 55.0), (2900, 35.0), (7900, 10.0)];
/// Currency and its USD rate.
const CURRENCIES: &[((&str, f64), f64)] = &[
    (("usd", 1.0), 80.0),
    (("eur", 1.08), 13.0),
    (("gbp", 1.27), 7.0),
];

/// xorshift64*, so the demo data is the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn below(&mut self, n: usize) -> usize {
        (self.f() * n as f64) as usize % n
    }

    fn pick<T: Copy>(&mut self, items: &[(T, f64)]) -> T {
        let total: f64 = items.iter().map(|i| i.1).sum();
        let mut x = self.f() * total;
        for (item, w) in items {
            if x < *w {
                return *item;
            }
            x -= w;
        }
        items[items.len() - 1].0
    }

    fn id(&mut self, prefix: &str) -> String {
        format!("{prefix}{:016x}", self.next())
    }
}

struct Visitor {
    id: String,
    country: &'static str,
    device: &'static str,
    browser: &'static str,
    os: &'static str,
    place: Option<(&'static str, &'static str)>,
}

/// Region and city per country, as a proxy with location headers would report them.
const PLACES: &[(&str, &[(&str, &str)])] = &[
    (
        "US",
        &[
            ("California", "San Francisco"),
            ("New York", "New York"),
            ("Texas", "Austin"),
            ("Washington", "Seattle"),
        ],
    ),
    (
        "GB",
        &[
            ("England", "London"),
            ("England", "Manchester"),
            ("Scotland", "Edinburgh"),
        ],
    ),
    (
        "DE",
        &[
            ("Berlin", "Berlin"),
            ("Bavaria", "Munich"),
            ("Hamburg", "Hamburg"),
        ],
    ),
    (
        "IN",
        &[("Karnataka", "Bengaluru"), ("Maharashtra", "Mumbai")],
    ),
    (
        "CA",
        &[("Ontario", "Toronto"), ("British Columbia", "Vancouver")],
    ),
    ("FR", &[("Ile-de-France", "Paris")]),
    ("NL", &[("North Holland", "Amsterdam")]),
    (
        "AU",
        &[("New South Wales", "Sydney"), ("Victoria", "Melbourne")],
    ),
    ("BR", &[("Sao Paulo", "Sao Paulo")]),
    ("ES", &[("Madrid", "Madrid"), ("Catalonia", "Barcelona")]),
];
const EXITS: &[&str] = &[
    "github.com/tallybird/tallybird-js",
    "docs.stripe.com/invoicing",
    "x.com/tallybird",
    "www.youtube.com/watch",
    "www.linkedin.com/company/tallybird",
];
/// Paying customers coming back to sign in or read the changelog.
const RETURNING: Source = Source {
    host: None,
    utm: None,
    weight: 0.0,
    growth: 1.0,
    convert: 0.0,
    paths: &["/login", "/changelog", "/docs/getting-started"],
};

fn visitor(rng: &mut Rng) -> Visitor {
    let device = rng.pick(DEVICES);
    let browser = rng.pick(BROWSERS);
    let os = match (device, browser) {
        ("Desktop", "Safari") => "macOS",
        ("Desktop", _) => rng.pick(&[("Windows", 55.0), ("macOS", 38.0), ("Linux", 7.0)]),
        (_, "Safari") => "iOS",
        _ => rng.pick(&[("Android", 70.0), ("iOS", 30.0)]),
    };
    let country = rng.pick(COUNTRIES);
    let place = PLACES
        .iter()
        .find(|p| p.0 == country)
        .map(|p| p.1[rng.below(p.1.len())]);
    Visitor {
        id: rng.id("v"),
        country,
        device,
        browser,
        os,
        place,
    }
}

fn session(
    rng: &mut Rng,
    site: &str,
    v: &Visitor,
    src: &Source,
    ts: OffsetDateTime,
    out: &mut Vec<Event>,
) {
    let session_id = rng.id("s");
    let (utm_source, utm_medium, utm_campaign) = match src.utm {
        Some((s, m, c)) => (
            Some(s.to_string()),
            Some(m.to_string()),
            Some(c.to_string()),
        ),
        None => (None, None, None),
    };
    let channel =
        channel::classify(utm_medium.as_deref(), utm_source.as_deref(), src.host).as_str();
    let pages = 1 + (rng.f() * rng.f() * 5.0) as usize;
    let mut path = src.paths[rng.below(src.paths.len())];
    let mut last = path;
    for n in 0..pages {
        out.push(Event {
            site_id: site.into(),
            ts: ts + Duration::seconds(n as i64 * (20 + rng.below(90) as i64)),
            visitor_id: v.id.clone(),
            session_id: session_id.clone(),
            kind: "pageview".into(),
            name: None,
            path: path.into(),
            referrer_host: if n == 0 {
                src.host.map(Into::into)
            } else {
                None
            },
            utm_source: utm_source.clone(),
            utm_medium: utm_medium.clone(),
            utm_campaign: utm_campaign.clone(),
            utm_content: None,
            utm_term: None,
            channel: channel.into(),
            country: Some(v.country.into()),
            device: v.device.into(),
            browser: v.browser.into(),
            os: v.os.into(),
            region: v.place.map(|p| p.0.into()),
            city: v.place.map(|p| p.1.into()),
            host: Some(
                if path.starts_with("/docs") {
                    "docs.tallybird.app"
                } else {
                    DOMAIN
                }
                .into(),
            ),
        });
        last = path;
        path = [
            "/pricing",
            "/features/recurring-invoices",
            "/docs/getting-started",
            "/",
        ][rng.below(4)];
    }
    if rng.f() < 0.2 {
        let mut exit = out[out.len() - 1].clone();
        exit.ts += Duration::seconds(30);
        exit.kind = "exit".into();
        exit.name = Some(EXITS[rng.below(EXITS.len())].into());
        exit.path = last.into();
        exit.referrer_host = None;
        out.push(exit);
    }
}

/// About 180 days of visits for a small invoicing SaaS, with Stripe payments, renewals and refunds.
fn seed(site: &str, now: OffsetDateTime) -> (Vec<Event>, Vec<Payment>, Vec<Subscription>) {
    let mut rng = Rng(0x5EED_DE30_0000_0001);
    let mut events = Vec::new();
    let mut payments = Vec::new();
    let mut subscriptions = Vec::new();
    let start = now.date() - Duration::days(DAYS);
    for d in 0..=DAYS {
        let day = (start + Duration::days(d)).midnight().assume_utc();
        let progress = d as f64 / DAYS as f64;
        let weekend = matches!(
            day.weekday(),
            time::Weekday::Saturday | time::Weekday::Sunday
        );
        let base = (40.0 + 80.0 * progress) * if weekend { 0.68 } else { 1.0 };
        let weights: Vec<(usize, f64)> = SOURCES
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut w = s.weight * (1.0 + (s.growth - 1.0) * progress);
                if s.utm.is_some() && day.weekday() == time::Weekday::Tuesday {
                    w *= 4.0;
                }
                if s.host == Some("news.ycombinator.com") && (DAYS - d == 41 || DAYS - d == 40) {
                    w *= if DAYS - d == 41 { 40.0 } else { 12.0 };
                }
                (i, w)
            })
            .collect();
        let count = (base * (0.85 + 0.3 * rng.f())) as usize;
        for _ in 0..count {
            let src = &SOURCES[rng.pick(&weights)];
            let hour = 7.0 + rng.f() * 16.0;
            let ts = day + Duration::seconds((hour * 3600.0) as i64);
            if ts > now {
                continue;
            }
            let v = visitor(&mut rng);
            session(&mut rng, site, &v, src, ts, &mut events);
            let mut last = ts;
            if rng.f() < 0.28 {
                let direct = SOURCES.len() - 1;
                let back = &SOURCES[rng.pick(&[(direct, 50.0), (0, 20.0), (6, 15.0), (1, 15.0)])];
                let again = ts + Duration::hours(6 + rng.below(24 * 9) as i64);
                if again < now {
                    session(&mut rng, site, &v, back, again, &mut events);
                    last = again;
                }
            }
            if rng.f() >= src.convert * 1.4 {
                continue;
            }
            if rng.f() < 0.5 {
                let direct = SOURCES.len() - 1;
                let back = &SOURCES[rng.pick(&[(0, 35.0), (6, 25.0), (1, 20.0), (direct, 20.0)])];
                let again = last + Duration::hours(3 + rng.below(24 * 6) as i64);
                if again < now {
                    session(&mut rng, site, &v, back, again, &mut events);
                    last = again;
                }
            }
            let paid = last + Duration::minutes(5 + rng.below(40) as i64);
            if paid > now {
                continue;
            }
            let mut goal = events
                .iter()
                .rev()
                .find(|e| e.visitor_id == v.id && e.kind == "pageview")
                .expect("the buyer visited")
                .clone();
            goal.ts = paid - Duration::minutes(3);
            goal.kind = "goal".into();
            goal.name = Some("signup".into());
            goal.path = "/signup".into();
            goal.referrer_host = None;
            goal.host = Some(DOMAIN.into());
            events.push(goal);
            let plan = rng.pick(PLANS);
            let (currency, rate) = rng.pick(CURRENCIES);
            let customer = rng.id("cus_");
            let refunded = rng.f() < 0.04;
            let tracked = rng.f() < 0.92;
            let mut at = paid;
            let mut renewal = false;
            while at <= now {
                if renewal && tracked && rng.f() < 0.93 {
                    let back = at - Duration::minutes(10 + rng.below(6 * 60) as i64);
                    session(&mut rng, site, &v, &RETURNING, back, &mut events);
                }
                payments.push(Payment {
                    site_id: site.into(),
                    provider: "stripe".into(),
                    provider_payment_id: rng.id("pi_"),
                    ts: at,
                    amount: plan,
                    currency: currency.into(),
                    amount_usd: Some((plan as f64 * rate).round() / 100.0),
                    refunded: if refunded { plan } else { 0 },
                    status: if refunded { "refunded" } else { "paid" }.into(),
                    visitor_id: tracked.then(|| v.id.clone()),
                    customer_id: Some(customer.clone()),
                    is_renewal: renewal,
                });
                if refunded || rng.f() < 0.06 {
                    break;
                }
                at += Duration::days(30);
                renewal = true;
            }
            let live = at > now;
            subscriptions.push(Subscription {
                subscription_id: rng.id("sub_"),
                customer_id: customer,
                status: if live { "active" } else { "canceled" }.into(),
                live,
                ends_at: None,
            });
        }
    }
    for n in 0..4 {
        let v = visitor(&mut rng);
        let ts = now - Duration::seconds(30 + n * 50);
        session(&mut rng, site, &v, &SOURCES[n as usize], ts, &mut events);
    }
    events.retain(|e| e.ts <= now);
    (events, payments, subscriptions)
}

/// Seeds a throwaway data dir, serves the app on localhost and writes the reachable demo pages to `out`.
pub async fn export(out: &Path) -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("virling-demo-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;
    let result = run(&dir, out).await;
    std::fs::remove_dir_all(&dir).ok();
    result
}

async fn run(dir: &Path, out: &Path) -> anyhow::Result<()> {
    let db = crate::open_sqlite(dir).await?;
    let account = sites::find_or_create_account(&db, "maya.chen@tallybird.app").await?;
    let site = sites::create_site(&db, &account, DOMAIN, "UTC").await?;
    let now = OffsetDateTime::now_utc();
    sqlx::query(
        "INSERT INTO site_icons (site_id, content_type, body, fetched_at) VALUES (?, 'image/svg+xml', ?, ?)",
    )
    .bind(&site)
    .bind(SITE_ICON.as_bytes())
    .bind(now.unix_timestamp())
    .execute(&db)
    .await?;
    sqlx::query("INSERT INTO report_brands (site_id, agency_name) VALUES (?, ?)")
        .bind(&site)
        .bind("Fieldwork Studio")
        .execute(&db)
        .await?;
    let token = auth::random_token();
    sqlx::query(
        "INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(auth::hash_token(&token))
    .bind(&account)
    .bind(now.unix_timestamp())
    .bind(now.unix_timestamp() + 3600)
    .execute(&db)
    .await?;
    fetch_favicons(&db).await?;

    let store = Arc::new(DuckStore::open(&dir.join("events.duckdb"))?);
    let (events, payments, subscriptions) = seed(&site, now);
    store.insert_events(&events)?;
    store.upsert_payments(&payments)?;
    store.replace_subscriptions(&site, "stripe", &subscriptions)?;
    let config = Config {
        marketing: false,
        app: true,
        trust_proxy: false,
        events_per_second: 200,
        base_url: "http://localhost".into(),
        waitlist_from: String::new(),
    };
    let state = AppState::new(
        config,
        db.clone(),
        store,
        mail::Mailer::Log,
        crypto::Vault::load(dir, None)?,
    );
    sync::rebuild_attribution(&state, Some(&site)).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(axum::serve(listener, app::router(state)).into_future());

    let client = reqwest::Client::new();
    let cookie = format!("{}={token}", auth::SESSION_COOKIE);
    let fetch = async |url: &str, drawer: bool| -> anyhow::Result<String> {
        let mut req = client.get(format!("{base}{url}")).header("cookie", &cookie);
        if drawer {
            req = req.header("x-drawer", "1");
        }
        let res = req.send().await?.error_for_status()?;
        anyhow::ensure!(res.url().path().starts_with("/app/"), "{url} redirected");
        Ok(res.text().await?)
    };
    let mut queue: VecDeque<String> = ["", "/payments", "/visitors", "/retention", "/report"]
        .iter()
        .map(|p| format!("/app/{DOMAIN}{p}"))
        .collect();
    let mut seen: HashSet<String> = queue.iter().filter_map(|u| target(u)).collect();
    let mut timelines = 0;
    let mut pages = Vec::new();
    while let Some(url) = queue.pop_front() {
        let page = fetch(&url, false)
            .await
            .with_context(|| format!("fetching {url}"))?;
        for link in links(&page) {
            let Some(t) = target(&link) else { continue };
            let timeline = is_timeline(&t);
            if (!timeline || timelines < TIMELINES) && seen.insert(t) {
                timelines += usize::from(timeline);
                queue.push_back(link);
            }
        }
        let path = target(&url).expect("queued urls have a target");
        if path.starts_with("/demo/payments/") && path.matches('/').count() == 5 {
            pages.push((path.clone(), "drawer.html", fetch(&url, true).await?));
        }
        pages.push((path, "index.html", page));
    }

    if out.exists() {
        std::fs::remove_dir_all(out)?;
    }
    let mut rewritten = Vec::new();
    let mut hosts = HashSet::new();
    for (path, file, html) in &pages {
        let html = rewrite(html, &seen);
        if let Some(i) = html
            .find("class=\"letter\"")
            .or_else(|| html.find("<span class=\"favicon\">"))
            .or_else(|| html.find("<span class=\"favicon sm\">"))
        {
            anyhow::bail!(
                "{path}{file} shows a letter tile: {}",
                &html[i.saturating_sub(300)..i]
            );
        }
        hosts.extend(source_icons(&html));
        rewritten.push((path, file, html));
    }
    let icon_dir = out.join("icons");
    std::fs::create_dir_all(&icon_dir)?;
    std::fs::write(icon_dir.join("site.svg"), SITE_ICON)?;
    for (slug, svg) in icons::UA_ICONS {
        std::fs::write(icon_dir.join(format!("{slug}.svg")), svg)?;
    }
    let mut kinds = HashMap::new();
    for host in &hosts {
        let (kind, body) = favicon::source_body(&db, host)
            .await
            .with_context(|| format!("no cached icon for {host}"))?;
        std::fs::write(icon_dir.join(icon_file(host, &kind)), body)?;
        kinds.insert(host.clone(), kind);
    }
    for (path, file, html) in &rewritten {
        write(out, path, file, &local_icons(html, &kinds))?;
    }
    std::fs::write(out.join("app.css"), include_str!("../assets/app.css"))?;
    std::fs::write(out.join("app.js"), include_str!("../assets/app.js"))?;
    let flag_dir = out.join("flags");
    std::fs::create_dir_all(&flag_dir)?;
    for (code, _) in COUNTRIES {
        let (_, svg) = flags::lookup(code).context("flag")?;
        std::fs::write(
            flag_dir.join(format!("{}.svg", code.to_ascii_lowercase())),
            svg,
        )?;
    }
    println!(
        "Wrote {} pages and {} favicons to {}",
        pages.len(),
        hosts.len(),
        out.display()
    );
    Ok(())
}

/// Caches the favicon of every seeded host without a bundled logo, so no row falls back to a letter.
async fn fetch_favicons(db: &sqlx::SqlitePool) -> anyhow::Result<()> {
    let client = favicon::client();
    let exits = EXITS.iter().filter_map(|e| e.split('/').next());
    for name in SOURCES.iter().filter_map(|s| s.host).chain(exits) {
        if icons::brand_slug(name).is_some() {
            continue;
        }
        let host = favicon::source_host(name).context("source host")?;
        favicon::save_source(db, &client, &host, format!("https://{host}/").parse()?).await;
        anyhow::ensure!(
            favicon::source_icon(&host).is_some(),
            "no favicon for {host}"
        );
    }
    Ok(())
}

fn is_timeline(target: &str) -> bool {
    target.starts_with("/demo/visitors/v")
}

/// Hosts of the cached favicons a page shows.
fn source_icons(html: &str) -> Vec<String> {
    html.split("src=\"/source-icons/")
        .skip(1)
        .filter_map(|s| s.split(['?', '"']).next())
        .map(str::to_string)
        .collect()
}

fn icon_file(host: &str, kind: &str) -> String {
    let ext = match kind {
        "image/svg+xml" => "svg",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "ico",
    };
    format!("{host}.{ext}")
}

/// Points favicon and logo images at the files the export writes under /demo/icons.
fn local_icons(html: &str, kinds: &HashMap<String, String>) -> String {
    let site = format!("src=\"/app/{DOMAIN}/icon");
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some((i, prefix, is_site)) = [("src=\"/source-icons/", false), (&*site, true)]
        .into_iter()
        .filter_map(|(p, is_site)| rest.find(p).map(|i| (i, p, is_site)))
        .min_by_key(|m| m.0)
    {
        out.push_str(&rest[..i]);
        let tail = &rest[i + prefix.len()..];
        let end = tail.find('"').unwrap_or(tail.len());
        let file = if is_site {
            "site.svg".to_string()
        } else {
            let host = tail[..end].split('?').next().unwrap_or_default();
            icon_file(host, kinds.get(host).map_or("", String::as_str))
        };
        out.push_str(&format!("src=\"/demo/icons/{file}"));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out.replace("/assets/icons/", "/demo/icons/")
}

fn write(out: &Path, path: &str, file: &str, html: &str) -> anyhow::Result<()> {
    let dir = out.join(path.trim_start_matches("/demo/"));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(file), html)?;
    Ok(())
}

/// App links in a page, unescaped.
fn links(html: &str) -> Vec<String> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .filter(|h| h.starts_with("/app/"))
        .map(|h| h.replace("&#38;", "&"))
        .collect()
}

/// Static path for an app URL the demo prebuilds, or None for pages it leaves out.
fn target(url: &str) -> Option<String> {
    let url = url.split('#').next().unwrap_or_default();
    let rest = url.strip_prefix("/app/")?.strip_prefix(DOMAIN)?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let params: Vec<(&str, &str)> = query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
        .collect();
    let (page, keys): (&str, &[&str]) = match path {
        "" => ("", &["range", "sources", "location", "pages"]),
        "/payments" => ("payments/", &["range", "filter"]),
        "/report" => ("report/", &[]),
        "/retention" => ("retention/", &["range", "by"]),
        "/visitors" => ("visitors/", &["range", "exit", "paid", "channel", "goal"]),
        p if p.starts_with("/visitors/") => {
            let id = &p["/visitors/".len()..];
            let ok =
                query.is_empty() && !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric());
            return ok.then(|| format!("/demo/visitors/{id}/"));
        }
        p => {
            let id = p.strip_prefix("/payments/")?;
            let ok = query.is_empty()
                && id.split('/').count() == 2
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/');
            return ok.then(|| format!("/demo/payments/{id}/"));
        }
    };
    let ranges_ok = params
        .iter()
        .all(|(k, v)| *k != "range" || matches!(*v, "7d" | "90d"));
    if !ranges_ok || params.iter().any(|(k, _)| !keys.contains(k)) {
        return None;
    }
    let extra = params.iter().filter(|(k, _)| *k != "range").count();
    let unranged = |k: &str| matches!(k, "channel" | "goal");
    if extra > 1 || (params.len() > 1 && params.iter().any(|(k, _)| unranged(k))) {
        return None;
    }
    let slug: Vec<String> = keys
        .iter()
        .filter_map(|k| params.iter().find(|p| p.0 == *k))
        .map(|(k, v)| match *k {
            "range" | "filter" => v.to_string(),
            _ => format!("{k}-{}", v.replace("%20", "-").to_ascii_lowercase()),
        })
        .collect();
    Some(if slug.is_empty() {
        format!("/demo/{page}")
    } else {
        format!("/demo/{page}{}/", slug.join("-"))
    })
}

/// Points app links at the static pages, drops what needs a server and adds the demo banner.
fn rewrite(html: &str, built: &HashSet<String>) -> String {
    let html = html
        .replace(
            &format!(r#"<a href="/app/{DOMAIN}/settings">Account settings</a>"#),
            r#"<a href="/#join">Account settings</a>"#,
        )
        .replace(
            r#"<form method="post" action="/logout"><button type="submit">Sign out</button></form>"#,
            r#"<a href="/#join">Sign out</a>"#,
        );
    let mut out = String::with_capacity(html.len());
    let mut rest = html.as_str();
    while let Some(i) = rest.find("<a ") {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let tag = &rest[..end];
        let href = tag
            .split_once("href=\"")
            .and_then(|(_, h)| h.split('"').next())
            .unwrap_or_default();
        if href == "/app" {
            out.push_str(&tag.replacen("href=\"/app\"", "href=\"/\"", 1));
        } else if !href.starts_with("/app/") {
            out.push_str(tag);
        } else if let Some(t) = target(&href.replace("&#38;", "&")).filter(|t| built.contains(t)) {
            let plain = href.replace("&#38;", "&");
            let frag = plain.find('#').map_or("", |i| &plain[i..]);
            let tag = tag.replacen(
                &format!("href=\"{href}\""),
                &format!("href=\"{t}{frag}\""),
                1,
            );
            out.push_str(&tag.replace(
                " data-drawer>",
                &format!(" data-drawer=\"{t}drawer.html\">"),
            ));
        } else if !tag.contains("class=")
            || tag.contains("class=\"b")
            || tag.contains("class=\"add\"")
            || tag.contains("class=\"j-visitor\"")
            || href.contains("/setup")
            || href.contains("/settings")
        {
            rest = &rest[rest.find("</a>").map_or(rest.len(), |c| c + 4)..];
            continue;
        } else {
            out.push_str(&tag.replacen(&format!(" href=\"{href}\""), "", 1));
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    let sep = r#"<span class="vf-sep">Referrers</span>"#;
    if let Some(i) = out.find(sep)
        && out[i + sep.len()..].trim_start().starts_with("</div>")
    {
        out.replace_range(i..i + sep.len(), "");
    }
    while let Some(i) = out.find("<form") {
        let end = out[i..].find("</form>").map_or(out.len(), |e| i + e + 7);
        out.replace_range(i..end, "");
    }
    out.replace("/assets/flags/", "/demo/flags/")
        .replace("/assets/app.css", "/demo/app.css")
        .replace("/assets/app.js", "/demo/app.js")
        .replacen(
            "<main class=\"content\">",
            &format!("<main class=\"content\">\n{BANNER}"),
            1,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_prebuilt_pages_only() {
        let d = format!("/app/{DOMAIN}");
        assert_eq!(target(&d).as_deref(), Some("/demo/"));
        assert_eq!(
            target(&format!("{d}?sources=referrer&range=7d")).as_deref(),
            Some("/demo/7d-sources-referrer/")
        );
        assert_eq!(
            target(&format!("{d}/payments?filter=refunded")).as_deref(),
            Some("/demo/payments/refunded/")
        );
        assert_eq!(
            target(&format!("{d}/payments/stripe/pi_1")).as_deref(),
            Some("/demo/payments/stripe/pi_1/")
        );
        assert_eq!(
            target(&format!("{d}/visitors?range=90d&exit=link")).as_deref(),
            Some("/demo/visitors/90d-exit-link/")
        );
        assert_eq!(
            target(&format!("{d}/visitors?channel=Organic%20search")).as_deref(),
            Some("/demo/visitors/channel-organic-search/")
        );
        assert_eq!(
            target(&format!("{d}/visitors/v1a2b")).as_deref(),
            Some("/demo/visitors/v1a2b/")
        );
        for gone in [
            "?range=12m",
            "?sort_pages=visitors",
            "?sources=referrer&pages=last",
            "/settings",
            "/payments?page=2",
            "/visitors?range=7d&channel=AI",
            "/visitors?exit=link&paid=yes",
        ] {
            assert_eq!(target(&format!("{d}{gone}")), None, "{gone}");
        }
    }

    #[test]
    fn rewrite_drops_server_actions() {
        let d = format!("/app/{DOMAIN}");
        let html = format!(
            r#"<main class="content"><a href="{d}/payments/stripe/pi_1" data-drawer>x</a><a href="{d}/settings">Settings</a><a class="num sort" href="{d}?sort_pages=visitors">V</a><form method="post" action="/logout"><button>Sign out</button></form><img src="/assets/flags/de.svg"><a class="j-visitor" href="{d}/visitors/v9">Visitor timeline</a><a class="tab" href="{d}?range=7d&#38;sources=referrer#sources">R</a>"#
        );
        let built = HashSet::from([
            "/demo/payments/stripe/pi_1/".to_string(),
            "/demo/7d-sources-referrer/".to_string(),
        ]);
        let got = rewrite(&html, &built);
        assert!(got.contains(r#"<a href="/demo/payments/stripe/pi_1/" data-drawer="/demo/payments/stripe/pi_1/drawer.html">"#));
        assert!(!got.contains("Settings") && !got.contains("Sign out"));
        assert!(got.contains(r#"<a class="num sort">V</a>"#));
        assert!(got.contains("/demo/flags/de.svg") && got.contains("Demo with sample data."));
        assert!(!got.contains("Visitor timeline"));
        assert!(got.contains(r#"<a class="tab" href="/demo/7d-sources-referrer/#sources">"#));
    }
}
