//! The live globe: visitors, payment bursts and revenue by country, read from the in-memory index.

use std::collections::{HashMap, HashSet};

use askama::Template;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::alias::Names;
use crate::app::AppState;
use crate::auth::Account;
use crate::dashboard::{
    RangeArgs, RangeMenu, Shell, ago, amount, blocking, load_ranged, money, payer_key,
};
use crate::events::EventStore;
use crate::icons;
use crate::onboarding::render;
use crate::sites;
use crate::stats::{Activity, BreakdownRow, Dimension, PaymentFilter, PaymentRow, Period, Who};
use crate::visitors::{Entry, Step, Timeline, VisitorFilter, VisitorRow};

/// Visitors drawn on the globe.
const MAX_LIVE: usize = 60;
/// Payments in the feed.
const MAX_PAYMENTS: u32 = 12;
/// Customers whose journey can be replayed.
const MAX_JOURNEYS: usize = 12;
/// Steps kept per journey, the latest ones.
const MAX_STEPS: usize = 16;
/// A visitor counts as live this long after their last event even without an open tab.
const ACTIVE: Duration = Duration::minutes(5);

/// What one read of the index returns, before names are given.
struct Raw {
    now: OffsetDateTime,
    live: Vec<(VisitorRow, Activity)>,
    /// The latest visitor, only when nobody is live.
    last: Option<(VisitorRow, Activity)>,
    pays: Vec<(PaymentRow, Option<String>)>,
    countries: Vec<BreakdownRow>,
    journeys: Vec<Timeline>,
    online: u64,
}

fn load(
    store: &dyn EventStore,
    id: &str,
    p: &Period,
    now: OffsetDateTime,
    online: HashSet<String>,
) -> anyhow::Result<Raw> {
    let tz = &p.tz;
    let today = now.to_offset(store.utc_offset(tz)?).date();
    let day = Period {
        from: today - Duration::days(1),
        to: today,
        tz: tz.clone(),
    };
    let recent = store
        .visitor_list(id, &day, &VisitorFilter::default(), 100, 0)?
        .rows;
    let latest = recent.iter().max_by_key(|r| r.last_seen).cloned();
    let rows: Vec<VisitorRow> = recent
        .into_iter()
        .filter(|r| online.contains(&r.visitor_id) || now - r.last_seen < ACTIVE)
        .take(MAX_LIVE)
        .collect();
    let who: Vec<Who> = rows
        .iter()
        .map(|r| Who {
            visitor_id: Some(r.visitor_id.clone()),
            session_id: None,
            ts: r.last_seen,
        })
        .collect();
    let live: Vec<(VisitorRow, Activity)> = rows
        .into_iter()
        .zip(store.activity(id, tz, &who)?)
        .collect();

    let last = match latest {
        Some(r) if live.is_empty() => {
            let who = Who {
                visitor_id: Some(r.visitor_id.clone()),
                session_id: None,
                ts: r.last_seen,
            };
            let a = store.activity(id, tz, &[who])?.into_iter().next();
            Some((r, a.unwrap_or_default()))
        }
        _ => None,
    };

    let month = Period {
        from: today - Duration::days(29),
        to: today,
        tz: tz.clone(),
    };
    let paid: Vec<PaymentRow> = store
        .payment_list(id, &month, PaymentFilter::All, MAX_PAYMENTS, 0)?
        .rows
        .into_iter()
        .filter(|r| matches!(r.status.as_str(), "paid" | "partially_refunded"))
        .collect();
    let who: Vec<Who> = paid.iter().map(Who::of).collect();
    let pays: Vec<(PaymentRow, Option<String>)> = paid
        .into_iter()
        .zip(store.activity(id, tz, &who)?)
        .map(|(r, a)| {
            let cc = a
                .country
                .or_else(|| r.touch.as_ref().and_then(|t| t.country.clone()));
            (r, cc)
        })
        .collect();

    let live_count = live.len() as u64;
    let mut seen = HashSet::new();
    let mut journeys = Vec::new();
    let ids = live
        .iter()
        .filter(|(r, _)| r.payments > 0)
        .map(|(r, _)| r.visitor_id.as_str())
        .chain(pays.iter().filter_map(|(r, _)| r.visitor.as_deref()));
    for v in ids {
        if journeys.len() < MAX_JOURNEYS
            && seen.insert(v)
            && let Some(t) = store.timeline(id, v)?
        {
            journeys.push(t);
        }
    }
    Ok(Raw {
        now,
        live,
        last,
        pays,
        countries: store.breakdown(id, p, Dimension::Country)?,
        journeys,
        online: live_count,
    })
}

#[derive(Serialize)]
struct GVisitor {
    id: String,
    name: String,
    face: usize,
    cc: String,
    place: String,
    src: String,
    ch: String,
    page: String,
    dev: String,
    paid: u64,
    spent: String,
    at: Option<(f32, f32)>,
}

#[derive(Serialize)]
struct GLast {
    name: String,
    cc: String,
    place: String,
    ago: String,
    at: Option<(f32, f32)>,
}

#[derive(Serialize)]
struct GPay {
    id: String,
    name: String,
    face: usize,
    cc: String,
    place: String,
    amt: String,
    usd: f64,
    src: String,
    ch: String,
    ago: String,
    ts: i64,
    vid: Option<String>,
    at: Option<(f32, f32)>,
}

#[derive(Serialize)]
struct GCountry {
    cc: String,
    name: String,
    visitors: u64,
    rev: f64,
    rpv: f64,
}

#[derive(Serialize)]
pub struct Globe {
    now: i64,
    online: u64,
    label: String,
    /// Animal faces as inline SVG, shared by the visitors and payments that use them.
    faces: Vec<String>,
    visitors: Vec<GVisitor>,
    last: Option<GLast>,
    payments: Vec<GPay>,
    countries: Vec<GCountry>,
    /// Steps of a customer's visits, as (kind, label), by visitor ID.
    journeys: HashMap<String, Vec<(&'static str, String)>>,
}

fn steps(t: &Timeline) -> Vec<(&'static str, String)> {
    let mut visits: Vec<Vec<(&'static str, String)>> = Vec::new();
    for e in &t.entries {
        match e {
            Entry::Visit(v) => {
                let mut out = vec![("from", v.source.clone())];
                for s in &v.steps {
                    match s {
                        Step::Page { path, .. } => out.push(("page", path.clone())),
                        Step::Goal { name, .. } => out.push(("goal", name.clone())),
                        Step::Left { domain, .. } => out.push(("left", domain.clone())),
                        Step::Payment(p) => out.push(("pay", amount(p))),
                        Step::Back { .. } | Step::Exit { .. } => {}
                    }
                }
                visits.push(out);
            }
            Entry::Payment(p) => visits.push(vec![("pay", amount(p))]),
        }
    }
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for v in visits.into_iter().rev() {
        if !out.is_empty() && out.len() + v.len() > MAX_STEPS {
            break;
        }
        out.splice(0..0, v);
    }
    if out.len() > MAX_STEPS {
        out.drain(1..out.len() - MAX_STEPS + 1);
    }
    out
}

fn keys(raw: &Raw) -> Vec<String> {
    let live = raw.live.iter().map(|(r, _)| r.who.clone());
    let last = raw.last.iter().map(|(r, _)| r.who.clone());
    live.chain(last)
        .chain(raw.pays.iter().map(|(p, _)| payer_key(p)))
        .collect()
}

fn view(raw: Raw, names: &Names, label: &str, pin: &dyn Fn(&str) -> Option<(f32, f32)>) -> Globe {
    let mut faces: Vec<String> = Vec::new();
    let mut seen: HashMap<String, (usize, String)> = HashMap::new();
    let mut who = |key: &str| -> (usize, String) {
        seen.entry(key.to_string())
            .or_insert_with(|| {
                let a = names.get(key);
                faces.push(a.avatar);
                (faces.len() - 1, a.name)
            })
            .clone()
    };
    let place = |cc: &Option<String>| cc.as_deref().map(icons::country).unwrap_or_default();
    let visitors = raw
        .live
        .iter()
        .map(|(r, a)| {
            let (face, name) = who(&r.who);
            GVisitor {
                id: r.visitor_id.clone(),
                name,
                face,
                cc: a.country.clone().unwrap_or_default(),
                place: place(&a.country),
                src: r.source.clone(),
                ch: r.channel.clone(),
                page: r.exit_page.clone().unwrap_or_default(),
                dev: a
                    .device
                    .clone()
                    .filter(|d| d != "Unknown")
                    .unwrap_or_default(),
                paid: r.payments,
                spent: if r.payments > 0 {
                    money(r.revenue_usd)
                } else {
                    String::new()
                },
                at: pin(&r.visitor_id),
            }
        })
        .collect();
    let last = raw.last.as_ref().map(|(r, a)| GLast {
        name: who(&r.who).1,
        cc: a.country.clone().unwrap_or_default(),
        place: place(&a.country),
        ago: ago(r.last_seen, raw.now),
        at: pin(&r.visitor_id),
    });
    let payments = raw
        .pays
        .iter()
        .map(|(p, cc)| {
            let (face, name) = who(&payer_key(p));
            let (src, ch) = match &p.touch {
                Some(t) => (t.source.clone(), t.channel.clone()),
                None => ("Unattributed".into(), String::new()),
            };
            GPay {
                id: p.provider_payment_id.clone(),
                name,
                face,
                cc: cc.clone().unwrap_or_default(),
                place: place(cc),
                amt: amount(p),
                usd: p.amount_usd.unwrap_or_default(),
                src,
                ch,
                ago: ago(p.ts, raw.now),
                ts: p.ts.unix_timestamp(),
                vid: p.visitor.clone(),
                at: p.visitor.as_deref().and_then(pin),
            }
        })
        .collect();
    let countries = raw
        .countries
        .iter()
        .filter(|c| c.visitors > 0)
        .filter_map(|c| {
            let cc = c.key.clone()?;
            Some(GCountry {
                name: icons::country(&cc),
                cc,
                visitors: c.visitors,
                rev: c.revenue_usd,
                rpv: c.revenue_per_visitor.unwrap_or_default(),
            })
        })
        .collect();
    Globe {
        now: raw.now.unix_timestamp(),
        online: raw.online,
        label: label.to_string(),
        faces,
        visitors,
        last,
        payments,
        countries,
        journeys: raw
            .journeys
            .iter()
            .map(|t| (t.visitor_id.clone(), steps(t)))
            .collect(),
    }
}

async fn build(
    state: &AppState,
    site: &sites::Site,
    p: Period,
    label: &str,
) -> Result<Globe, StatusCode> {
    let store = state.events.clone();
    let id = site.id.clone();
    let online = state.presence.ids(&id);
    let raw = blocking(move || load(&*store, &id, &p, OffsetDateTime::now_utc(), online)).await?;
    let names = Names::load(&state.db, &site.id, keys(&raw))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let pins = state.pins.clone();
    let sid = site.id.clone();
    Ok(view(raw, &names, label, &|v| pins.get(&sid, v)))
}

#[derive(Template)]
#[template(path = "globe.html")]
struct GlobePage {
    shell: Shell,
    title: &'static str,
    range: RangeMenu,
    /// The first reading as JSON, safe inside a script tag.
    data: String,
}

/// Adds the time spent in the handler as a `Server-Timing` header, visible in the browser's network panel.
fn timing(mut res: Response, t0: std::time::Instant) -> Response {
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    if let Ok(v) = format!("app;dur={ms:.1}").parse::<axum::http::HeaderValue>() {
        res.headers_mut().insert("server-timing", v);
    }
    res
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(args): Query<RangeArgs>,
) -> Response {
    let t0 = std::time::Instant::now();
    let (ctx, picked) = match load_ranged(&state, &account, &domain, "globe", args).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let p = picked.period(&ctx.site.timezone);
    let globe = match build(&state, &ctx.site, p, &picked.label).await {
        Ok(g) => g,
        Err(code) => return code.into_response(),
    };
    let base = sites::app_path(&ctx.site.domain, "globe");
    timing(
        render(GlobePage {
            title: "Live globe",
            range: picked.menu(ctx.today, ctx.first_day, &base, &[]),
            data: serde_json::to_string(&globe)
                .unwrap_or_default()
                .replace('<', "\\u003c"),
            shell: ctx.shell,
        }),
        t0,
    )
}

pub async fn data(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(args): Query<RangeArgs>,
) -> Response {
    let t0 = std::time::Instant::now();
    let (ctx, picked) = match load_ranged(&state, &account, &domain, "globe", args).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let p = picked.period(&ctx.site.timezone);
    match build(&state, &ctx.site, p, &picked.label).await {
        Ok(g) => timing(Json(g).into_response(), t0),
        Err(code) => code.into_response(),
    }
}
