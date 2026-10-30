//! The dashboard page: range, filters, matching, and the five views.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use time::{Date, Duration, OffsetDateTime};

use super::daterange::{Picked, RangeMenu};
use super::{
    Ctx, Opt, Shell, answers, blocking, cards, customers, day, delta, encode, goal_card, goals,
    group, insight, link, load, money, plural, provider_name, source_of, usd, views, visit_time,
};
use crate::alias::Names;
use crate::app::AppState;
use crate::auth::Account;
use crate::channel::source_domain;
use crate::chart::{self, Kind};
use crate::onboarding::render;
use crate::sites::Site;
use crate::stats::{BreakdownRow, DayPoint, Dimension, Filter, Period, ProviderSplit, Totals};
use crate::{icons, sites};

#[derive(Deserialize, Default)]
pub struct DashboardQuery {
    range: Option<String>,
    from: Option<String>,
    to: Option<String>,
    compare: Option<String>,
    view: Option<String>,
    chart: Option<String>,
    /// Card tabs, filters and view options.
    #[serde(flatten)]
    rest: HashMap<String, String>,
}

/// Filter params, and the first-pageview column each one matches; "viewed" matches any page of the visit.
const FILTERS: [(&str, &str); 13] = [
    ("channel", "channel"),
    ("source", "referrer_host"),
    ("campaign", "utm_campaign"),
    ("keyword", "utm_term"),
    ("page", "path"),
    ("viewed", ""),
    ("country", "country"),
    ("city", "city"),
    ("language", "language"),
    ("device", "device"),
    ("browser", "browser"),
    ("os", "os"),
    ("direct", "direct"),
];

/// Params kept by every link besides the range, filters and card tabs.
const KEEP: [&str; 12] = [
    "compare", "view", "chart", "gtab", "vtab", "ret", "s1", "s2", "s3", "funnel", "ai", "goals",
];

const VIEWS: [(&str, &str); 5] = [
    ("", "Overview"),
    ("goals", "Goals"),
    ("visitors", "Visitors"),
    ("payments", "Latest payments"),
    ("retention", "Customers who stay"),
];

pub struct Kpi {
    label: &'static str,
    value: String,
    delta: String,
    tone: &'static str,
    note: String,
}

/// A fix for unmatched payments; `code` empty for ones the site cannot fix.
pub struct Fix {
    title: String,
    text: String,
    code: String,
    href: String,
    link: String,
}

pub struct Matching {
    connected: bool,
    syncing: bool,
    bold: String,
    label: String,
    title: String,
    desc: String,
    fixes: Vec<Fix>,
}

pub struct FilterChip {
    icon: String,
    label: String,
    href: String,
}

pub struct SetupStep {
    done: bool,
    num: usize,
    label: String,
    sub: String,
    act: &'static str,
    href: String,
}

pub struct SetupCard {
    title: String,
    count: String,
    steps: Vec<SetupStep>,
    /// Hides the card for good; empty while the site has no data.
    dismiss: String,
}

pub struct Waiting {
    domain: String,
    snippet: String,
    install_href: String,
    sample_href: String,
}

/// One revenue chart type, all rendered so the tabs switch without a reload.
pub struct ChartPanel {
    title: &'static str,
    svg: String,
    note: String,
    on: bool,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardPage {
    shell: Shell,
    title: &'static str,
    view: &'static str,
    views: Vec<Opt>,
    live_href: String,
    matching: Matching,
    range: RangeMenu,
    report_href: String,
    chips: Vec<FilterChip>,
    /// The Visitors list with the same filters, when it can apply them all.
    visitors_href: String,
    privacy: bool,
    waiting: Option<Waiting>,
    setup: Option<SetupCard>,
    insight: Option<(String, String)>,
    kpis: Vec<Kpi>,
    /// Revenue in range that came from visits before it.
    earlier: String,
    /// Share of revenue matched to a visit and why the rest was not.
    unmatched: String,
    charts: Vec<ChartPanel>,
    chart_tabs: Vec<Opt>,
    cards: Vec<cards::TabCard>,
    ai: Option<cards::AiCard>,
    profit: Option<crate::spend::ProfitCard>,
    topay: Option<crate::topay::PayCard>,
    reading: Option<crate::reading::ReadCard>,
    almost: Option<crate::almost::AlmostCard>,
    verdicts: Vec<crate::verdict::Verdict>,
    why: Option<String>,
    goals: Option<goals::GoalsView>,
    goal_card: Option<goal_card::Card>,
    customers: Option<customers::Card>,
    visitors: Option<views::VisitorsView>,
    payments: Option<views::PaymentsView>,
    retention: Option<views::RetentionView>,
}

/// The read-only dashboard anyone can open at /share/{domain}.
#[derive(Template)]
#[template(path = "public.html")]
struct PublicPage {
    domain: String,
    icon: String,
    url: String,
    og_title: String,
    og_desc: String,
    og_image: String,
    live: u64,
    range_label: String,
    ranges: Vec<Opt>,
    insight: Option<(String, String)>,
    kpis: Vec<Kpi>,
    earlier: String,
    unmatched: String,
    charts: Vec<ChartPanel>,
    chart_tabs: Vec<Opt>,
    cards: Vec<cards::TabCard>,
    ai: Option<cards::AiCard>,
    profit: Option<crate::spend::ProfitCard>,
    reading: Option<crate::reading::ReadCard>,
    almost: Option<crate::almost::AlmostCard>,
    verdicts: Vec<crate::verdict::Verdict>,
    topay: Option<crate::topay::PayCard>,
    why: Option<String>,
    retention: Option<views::RetentionView>,
    goal_card: Option<goal_card::Card>,
    customers: Option<customers::Card>,
}

/// One-line code that passes the visitor ID into a provider's checkout.
fn fix_line(provider: &str) -> &'static str {
    match provider {
        "stripe" => "client_reference_id: req.cookies.virling_vid",
        "lemonsqueezy" => "checkout_data: { custom: { virling_vid: req.cookies.virling_vid } }",
        _ => "metadata: { virling_vid: req.cookies.virling_vid }",
    }
}

/// `sync` is None when no provider is connected, else whether one is still on its first sync.
fn matching(split: &[ProviderSplit], sync: Option<bool>, domain: &str, window: i64) -> Matching {
    let total: u64 = split.iter().map(|s| s.first_payments + s.renewals).sum();
    let unmatched: u64 = split.iter().map(|s| s.no_visitor + s.unseen).sum();
    let matched = total - unmatched;
    let connected = sync.is_some() || total > 0;
    let syncing = sync == Some(true) && total == 0;
    let (bold, label) = if !connected {
        (String::new(), "Payments not connected".to_string())
    } else if syncing {
        (String::new(), SYNCING.to_string())
    } else if total == 0 {
        (String::new(), "No payments yet".to_string())
    } else if total < 20 {
        (
            format!("{} of {}", group(matched), group(total)),
            " payments matched".to_string(),
        )
    } else {
        (
            format!("{:.0}%", matched as f64 / total as f64 * 100.0),
            " of payments matched".to_string(),
        )
    };
    let checkout = sites::app_path(domain, "setup/checkout");
    let mut fixes: Vec<Fix> = split
        .iter()
        .filter(|s| s.no_visitor > 0)
        .map(|s| {
            let name = provider_name(&s.provider);
            Fix {
                title: format!(
                    "{} from {name} had no visitor ID",
                    plural(s.no_visitor, "payment")
                ),
                text: format!(
                    "Your server creates these checkouts. Add one line where you call {name}:"
                ),
                code: fix_line(&s.provider).into(),
                href: checkout.clone(),
                link: String::new(),
            }
        })
        .collect();
    let unseen: u64 = split.iter().map(|s| s.unseen).sum();
    if unseen > 0 {
        fixes.push(Fix {
            title: format!("{} had a visitor ID but no visit", plural(unseen, "payment")),
            text: format!(
                "The visit was blocked by an ad blocker or came more than {window} days before paying, so it cannot be credited. These count in revenue, not in any channel."
            ),
            code: String::new(),
            href: format!(
                "{}?filter=unattributed",
                sites::app_path(domain, "payments")
            ),
            link: format!("See the {} unmatched payments", group(unmatched)),
        });
    }
    let (title, desc) = if !connected {
        (
            "Payments are not connected yet".to_string(),
            "Connect a payment provider with a read-only key so revenue can be matched to the visits that led to it.".to_string(),
        )
    } else if syncing {
        (
            SYNCING.to_string(),
            "Importing the last 90 days. Matches show up here as payments come in.".to_string(),
        )
    } else if total == 0 {
        (
            "No payments in this range".to_string(),
            "Matches show up here once a payment comes in.".to_string(),
        )
    } else if unmatched == 0 {
        (
            format!(
                "{} of {} matched to a visit",
                group(matched),
                plural(total, "payment")
            ),
            "Every payment so far has a visitor attached. Nothing to fix.".to_string(),
        )
    } else {
        (
            format!("{} of {} matched to a visit", group(matched), plural(total, "payment")),
            "Every number on this page is built on these matches. Here is what is missing and how to fix it.".to_string(),
        )
    };
    Matching {
        connected,
        syncing,
        bold,
        label,
        title,
        desc,
        fixes,
    }
}

/// Share of revenue matched to a visit and the reasons for the rest; empty when nothing is missing.
fn unmatched_line(cur: &Totals, split: &[ProviderSplit], window: i64) -> String {
    let gap = cur.revenue_usd - cur.matched_revenue_usd;
    if cur.revenue_usd <= 0.0 || gap < 0.005 {
        return String::new();
    }
    let sum = |f: fn(&ProviderSplit) -> u64| split.iter().map(f).sum::<u64>();
    let (no_visitor, unseen) = (sum(|s| s.no_visitor), sum(|s| s.unseen));
    let mut why = Vec::new();
    if no_visitor > 0 {
        why.push(format!(
            "{} had no visitor ID",
            plural(no_visitor, "payment")
        ));
    }
    if unseen > 0 {
        why.push(format!(
            "{} had no visit in the {window} days before paying",
            plural(unseen, "payment")
        ));
    }
    let pct = (cur.matched_revenue_usd / cur.revenue_usd * 100.0).floor();
    let head = format!("{pct:.0}% of revenue is matched to a visit.");
    if why.is_empty() {
        return head;
    }
    format!("{head} {}.", why.join(", "))
}

fn chip_view(key: &str, value: &str) -> (String, String) {
    let none = value == "(none)";
    match key {
        "channel" | "source" => (icons::source(value, 16), value.to_string()),
        "campaign" if none => (
            icons::glyph("paid", 16, "var(--ink-2)"),
            "No campaign".into(),
        ),
        "campaign" => (icons::glyph("paid", 16, "var(--ink-2)"), value.to_string()),
        "keyword" if none => (
            icons::glyph("search", 16, "var(--ink-2)"),
            "No search term".into(),
        ),
        "keyword" => (
            icons::glyph("search", 16, "var(--ink-2)"),
            value.to_string(),
        ),
        "page" => (
            icons::glyph("page", 16, "var(--ink-2)"),
            format!("Landed on {value}"),
        ),
        "viewed" => (
            icons::glyph("page", 16, "var(--ink-2)"),
            format!("Viewed {value}"),
        ),
        "country" => (icons::flag(value), icons::country(value)),
        "city" => (icons::glyph("pin", 16, "var(--ink-2)"), value.to_string()),
        "language" => (cards::language_icon(value), cards::language_name(value)),
        "device" => (icons::device(value), value.to_string()),
        "direct" => (
            icons::glyph("direct", 16, "var(--ink-2)"),
            value.to_string(),
        ),
        _ => (icons::ua(value), value.to_string()),
    }
}

fn kpi(label: &'static str, value: String, delta: String, tone: &'static str, note: String) -> Kpi {
    Kpi {
        label,
        value,
        delta,
        tone,
        note,
    }
}

fn pv_text(t: &Totals) -> String {
    t.revenue_per_visitor.map_or_else(|| "–".into(), usd)
}

fn time_text(t: &Totals) -> String {
    t.avg_visit_secs.map_or_else(|| "–".into(), visit_time)
}

const SYNCING: &str = "Syncing payments…";

/// KPI tiles: against the previous period, the whole site when filtered, or early-data notes on a new site.
#[allow(clippy::too_many_arguments)]
fn kpis(
    cur: &Totals,
    prev: &Totals,
    all: Option<&Totals>,
    filter_label: &str,
    compare: bool,
    young: bool,
    since: Option<Date>,
    start: Date,
    sync: Option<bool>,
) -> Vec<Kpi> {
    let conv = |t: &Totals| {
        t.conversion
            .map(|c| format!("{:.1}% became customers", c * 100.0))
            .unwrap_or_else(|| {
                if t.visitors == 0 {
                    "no visits in this range".into()
                } else {
                    "none matched to a payment yet".into()
                }
            })
    };
    if let Some(all) = all {
        let share = |a: f64, b: f64| {
            if b > 0.0 {
                format!("{:.0}%", a / b * 100.0)
            } else {
                String::new()
            }
        };
        let x = match (cur.revenue_per_visitor, all.revenue_per_visitor) {
            (Some(a), Some(b)) if b > 0.0 => Some(a / b),
            _ => None,
        };
        return vec![
            kpi(
                "Revenue",
                money(cur.revenue_usd),
                share(cur.revenue_usd, all.revenue_usd),
                "flat",
                "of revenue".into(),
            ),
            kpi(
                "New customers",
                group(cur.new_customers),
                String::new(),
                "",
                format!("credited to {filter_label}"),
            ),
            kpi(
                "Revenue per visitor",
                pv_text(cur),
                x.map(|x| format!("{x:.1}x")).unwrap_or_default(),
                x.map_or("", cards::tone),
                all.revenue_per_visitor
                    .map(|a| format!("your average of {}", usd(a)))
                    .unwrap_or_default(),
            ),
            kpi(
                "Visitors",
                group(cur.visitors),
                share(cur.visitors as f64, all.visitors as f64),
                "flat",
                "of all visitors".into(),
            ),
            kpi(
                "Avg visit time",
                time_text(cur),
                String::new(),
                "",
                all.avg_visit_secs
                    .map(|a| format!("your average of {}", visit_time(a)))
                    .unwrap_or_default(),
            ),
        ];
    }
    if young {
        let since_txt = since.map(day).unwrap_or_default();
        return vec![
            kpi(
                "Revenue",
                money(cur.revenue_usd),
                String::new(),
                "",
                match sync {
                    _ if cur.payments > 0 => format!("from {}", plural(cur.payments, "payment")),
                    None => "connect payments to see revenue".into(),
                    Some(true) => SYNCING.into(),
                    Some(false) => "no payments in this range".into(),
                },
            ),
            kpi(
                "New customers",
                group(cur.new_customers),
                String::new(),
                "",
                if cur.new_customers > 0 {
                    format!("since {}", day(start))
                } else {
                    "none yet".into()
                },
            ),
            kpi(
                "Revenue per visitor",
                pv_text(cur),
                if cur.revenue_per_visitor.is_some() {
                    "Early".into()
                } else {
                    String::new()
                },
                "flat",
                if cur.revenue_per_visitor.is_some() {
                    "settles after about 100 visitors".into()
                } else {
                    "needs visits and payments".into()
                },
            ),
            kpi(
                "Visitors",
                group(cur.visitors),
                String::new(),
                "",
                if since_txt.is_empty() {
                    "waiting for the first visit".into()
                } else {
                    format!("since the script went live {since_txt}")
                },
            ),
            kpi(
                "Avg visit time",
                time_text(cur),
                String::new(),
                "",
                "visits with 2 or more events".into(),
            ),
        ];
    }
    let d = |c: Option<f64>, p: Option<f64>| {
        if compare {
            delta(c, p)
        } else {
            ("", String::new())
        }
    };
    let from = |text: String| {
        if compare {
            format!("from {text}")
        } else {
            String::new()
        }
    };
    let tile = |label, value, (tone, delta): (&'static str, String), note| {
        kpi(label, value, delta, tone, note)
    };
    vec![
        tile(
            "Revenue",
            money(cur.revenue_usd),
            d(Some(cur.revenue_usd), Some(prev.revenue_usd)),
            from(money(prev.revenue_usd)),
        ),
        tile(
            "New customers",
            group(cur.new_customers),
            d(
                Some(cur.new_customers as f64),
                Some(prev.new_customers as f64),
            ),
            from(group(prev.new_customers)),
        ),
        tile(
            "Revenue per visitor",
            pv_text(cur),
            d(cur.revenue_per_visitor, prev.revenue_per_visitor),
            prev.revenue_per_visitor
                .map(|p| from(usd(p)))
                .unwrap_or_default(),
        ),
        tile(
            "Visitors",
            group(cur.visitors),
            d(Some(cur.visitors as f64), Some(prev.visitors as f64)),
            conv(cur),
        ),
        tile(
            "Avg visit time",
            time_text(cur),
            d(cur.avg_visit_secs, prev.avg_visit_secs),
            prev.avg_visit_secs
                .map(|p| from(visit_time(p)))
                .unwrap_or_default(),
        ),
    ]
}

/// Days before `first` and before any day with data, as a share of the period, for the "not tracking yet" hatch.
fn pre(p: &Period, first: Option<Date>, days: &[DayPoint]) -> Option<(f64, String)> {
    let data = days
        .iter()
        .find(|d| d.visitors > 0 || d.payments > 0)
        .map(|d| d.date);
    let start = first.into_iter().chain(data).min();
    let days = ((p.to - p.from).whole_days() + 1) as f64;
    match start {
        None => Some((
            1.0,
            "Revenue per day shows up here after the first visit and payment.".into(),
        )),
        Some(f) if f > p.from && f <= p.to => Some((
            (f - p.from).whole_days() as f64 / days,
            if Some(f) == first {
                format!("Not tracking yet. Script went live {}.", day(f))
            } else {
                format!("Not tracking yet. First payment {}.", day(f))
            },
        )),
        _ => None,
    }
}

fn joined<T, E>(r: Result<Result<T, E>, tokio::task::JoinError>) -> Result<T, StatusCode> {
    r.ok()
        .and_then(Result::ok)
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

pub async fn dashboard(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<DashboardQuery>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "dashboard").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let settings: Result<(i64, bool), _> =
        sqlx::query_as("SELECT window_days, setup_dismissed FROM sites WHERE id = ?")
            .bind(&ctx.site.id)
            .fetch_one(&state.db)
            .await;
    let providers = super::providers(&state.db, &ctx.site.id).await;
    let (Ok((window_days, dismissed)), Ok(providers)) = (settings, providers) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    page(state, ctx, q, window_days, dismissed, providers, None)
        .await
        .unwrap_or_else(|code| code.into_response())
}

async fn page(
    state: AppState,
    ctx: Ctx,
    q: DashboardQuery,
    window_days: i64,
    dismissed: bool,
    (providers, syncing): (Vec<String>, bool),
    // Some(hide revenue) renders the public page.
    public: Option<bool>,
) -> Result<Response, StatusCode> {
    let today = ctx.today;
    let picked = Picked::read(
        today,
        ctx.first_day,
        q.range.as_deref(),
        q.from.as_deref(),
        q.to.as_deref(),
        q.compare.as_deref(),
    );
    let (rkey, from, to, compare) = (picked.key, picked.from, picked.to, picked.compare);
    let p = picked.period(&ctx.site.timezone);
    let view = VIEWS
        .iter()
        .map(|v| v.0)
        .find(|v| !v.is_empty() && public.is_none() && q.view.as_deref() == Some(*v))
        .unwrap_or("");

    // Raw params of the current page; links encode them.
    let mut raw: Vec<(&'static str, String)> = picked.params();
    if !view.is_empty() {
        raw.push(("view", view.into()));
    }
    if let Some(c) = q
        .chart
        .as_deref()
        .filter(|c| matches!(*c, "line" | "channel" | "total"))
    {
        raw.push(("chart", c.into()));
    }
    let picks = cards::picks(&q.rest);
    for (k, v) in cards::params(&picks) {
        raw.push((k, v.into()));
    }
    let mut filters: Vec<(&'static str, String)> = Vec::new();
    for (k, _) in FILTERS.iter().filter(|_| public.is_none()) {
        if let Some(v) = q.rest.get(*k).filter(|v| !v.is_empty() && v.len() <= 300) {
            filters.push((*k, v.clone()));
            raw.push((*k, v.clone()));
        }
    }
    for k in KEEP.iter().skip(3) {
        if let Some(v) = q.rest.get(*k).filter(|v| !v.is_empty() && v.len() <= 300) {
            raw.push((k, v.clone()));
        }
    }
    let base = match public {
        Some(_) => format!("/share/{}", ctx.site.domain),
        None => sites::app_path(&ctx.site.domain, ""),
    };
    let enc: Vec<(&str, String)> = raw.iter().map(|(k, v)| (*k, encode(v))).collect();
    let params: Vec<(&str, &str)> = enc.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let href = |set: &[(&str, &str)]| link(&base, &params, set);
    let filter_href = |set: &[(&'static str, String)]| {
        if public.is_some() {
            return String::new();
        }
        let set: Vec<(&str, String)> = set.iter().map(|(k, v)| (*k, encode(v))).collect();
        let set: Vec<(&str, &str)> = set.iter().map(|(k, v)| (*k, v.as_str())).collect();
        link(&base, &params, &set)
    };

    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let wanted = filters.clone();
    let pq = p.clone();
    let funnels = if view == "goals" {
        goals::saved(&state.db, &ctx.site.id)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    } else {
        Vec::new()
    };
    let mut rest = q.rest.clone();
    goals::apply_saved(&mut rest, &funnels);
    let now = OffsetDateTime::now_utc();
    let tz = ctx.site.timezone.clone();
    let online = state.presence.ids(&ctx.site.id);
    // The Overview cards that read their own tables run while the main pass builds the charts.
    let early = view.is_empty().then(|| {
        let at = |d: Date| d.midnight().assume_offset(ctx.offset).unix_timestamp();
        let (db, site) = (state.db.clone(), ctx.site.id.clone());
        let (lo, hi) = (at(from), at(to + Duration::days(1)) - 1);
        let links = tokio::spawn({
            let (db, site) = (db.clone(), site.clone());
            async move { crate::links::site_links(&db, &site).await }
        });
        let bots = tokio::spawn({
            let (db, site) = (db.clone(), site.clone());
            async move { crate::crawlers::hits(&db, &site, lo, hi).await }
        });
        let own = public.is_none().then(|| {
            let spend = tokio::spawn({
                let (db, site) = (db.clone(), site.clone());
                async move { crate::spend::load(&db, &site).await }
            });
            let (store, sid, per) = (state.events.clone(), site.clone(), p.clone());
            let pays = tokio::spawn(blocking(move || {
                let page =
                    store.payment_list(&sid, &per, crate::stats::PaymentFilter::All, 500, 0)?;
                let seen = customers::journeys(&*store, &sid, &page.rows, window_days)?;
                anyhow::Ok((page, seen))
            }));
            let (store, sid, per) = (state.events.clone(), site.clone(), p.clone());
            let goals = tokio::spawn(blocking(move || goal_card::load(&*store, &sid, &per)));
            let (store, sid, per) = (state.events.clone(), site.clone(), p.clone());
            let read = tokio::spawn(blocking(move || store.reading(&sid, &per)));
            let (store, sid, per) = (state.events.clone(), site.clone(), p.clone());
            let almost = tokio::spawn(blocking(move || store.almost_paid(&sid, &per)));
            let earlier = filters.is_empty().then(|| {
                let days = (to - from).whole_days() + 1;
                let before = Period {
                    from: from - Duration::days(days),
                    to: from - Duration::days(1),
                    tz: p.tz.clone(),
                };
                let (store, sid) = (state.events.clone(), site.clone());
                tokio::spawn(blocking(move || {
                    store.breakdown(&sid, &before, Dimension::Channel)
                }))
            });
            (spend, pays, goals, read, almost, earlier)
        });
        (links, bots, own)
    });
    let data = blocking(move || {
        let mut f = Filter::default();
        for (k, v) in &wanted {
            let col = FILTERS
                .iter()
                .find(|x| x.0 == *k)
                .map(|x| x.1)
                .unwrap_or("");
            if *k == "viewed" {
                f.viewed = Some(v.clone());
                continue;
            }
            let vals: Vec<Option<String>> = if *k == "source" {
                let mut hosts: Vec<Option<String>> = store
                    .visitor_facets(&id, &pq)?
                    .sources
                    .into_iter()
                    .filter_map(|(_, h, _)| h)
                    .filter(|h| source_domain(h) == *v)
                    .map(Some)
                    .collect();
                hosts.dedup();
                if hosts.is_empty() {
                    hosts.push(Some(v.clone()));
                }
                hosts
            } else if v == "(none)" {
                vec![None]
            } else {
                vec![Some(v.clone())]
            };
            f.first.push((col, vals));
        }
        let split = store.payment_split(&id, &pq)?;
        let overview = store.overview_in(&id, &pq, &f)?;
        let all = if f.is_empty() {
            None
        } else {
            Some(store.overview(&id, &pq)?.current)
        };
        let data = match view {
            "" => {
                let report = store.report_in(&id, &pq, &f)?;
                let mut cache: Vec<(Option<Dimension>, Vec<BreakdownRow>)> = vec![
                    (Some(Dimension::Channel), report.channels.clone()),
                    (Some(Dimension::LandingPage), report.landing_pages.clone()),
                    (Some(Dimension::Country), report.countries.clone()),
                ];
                let mut rows_of = |d: Option<Dimension>| -> anyhow::Result<Vec<BreakdownRow>> {
                    if let Some(c) = cache.iter().find(|c| c.0 == d) {
                        return Ok(c.1.clone());
                    }
                    let rows = match d {
                        Some(d) => store.breakdown_in(&id, &pq, d, &f)?,
                        None => store.last_pages_in(&id, &pq, &f)?,
                    };
                    cache.push((d, rows.clone()));
                    Ok(rows)
                };
                let mut tabs = Vec::new();
                for t in cards::all_tabs() {
                    let rows = rows_of(t.dimension())?;
                    let mut kids = match t.kids() {
                        Some(d) => rows_of(Some(d))?,
                        None => Vec::new(),
                    };
                    if t == cards::Tab::Channel {
                        let direct = rows_of(Some(Dimension::DirectKind))?;
                        kids.extend(direct.into_iter().filter(|r| !r.unattributed).filter_map(
                            |mut r| {
                                r.key = Some(format!("Direct|{}", r.key?));
                                Some(r)
                            },
                        ));
                    }
                    tabs.push((rows, kids));
                }
                let sources = rows_of(Some(Dimension::ChannelSource))?;
                let hours = if pq.from == pq.to {
                    Some((
                        store.hourly(&id, &pq, &f)?,
                        store.hourly(&id, &pq.previous(), &f)?,
                    ))
                } else {
                    None
                };
                let channels = if f.is_empty() {
                    report.channels.clone()
                } else {
                    store.breakdown(&id, &pq, Dimension::Channel)?
                };
                let visitors = all.as_ref().unwrap_or(&overview.current).visitors;
                let attributed: f64 = channels
                    .iter()
                    .filter(|r| !r.unattributed)
                    .map(|r| r.revenue_usd)
                    .sum();
                let avg = (visitors > 0 && attributed > 0.0).then(|| attributed / visitors as f64);
                Data::Overview(Box::new(report), tabs, hours, avg, sources)
            }
            "goals" => Data::Goals(goals::load(&*store, &id, &pq, &f, &rest)?),
            "visitors" => {
                Data::Visitors(views::load_visitors(&*store, &id, &pq, &tz, now, online)?)
            }
            "payments" => Data::Payments(views::load_payments(
                &*store,
                &id,
                &pq,
                window_days,
                &f,
                now,
            )?),
            _ => Data::Retention(
                Box::new(store.retention(&id, &pq, now)?),
                store.mrr(&id, &pq, now)?,
            ),
        };
        let ret = match public {
            Some(_) => Some((store.retention(&id, &pq, now)?, store.mrr(&id, &pq, now)?)),
            None => None,
        };
        Ok((split, overview, all, data, ret))
    })
    .await?;
    let (split, overview, all, data, ret) = data;
    let hide = public == Some(true);
    let cur = &overview.current;
    let filter_label = filters
        .iter()
        .map(|(k, v)| chip_view(k, v).1)
        .collect::<Vec<_>>()
        .join(", ");
    let site_url = |page: &str| sites::app_path(&ctx.site.domain, page);
    let views_opts: Vec<Opt> = VIEWS
        .iter()
        .map(|(k, label)| Opt {
            label: label.to_string(),
            href: href(&[("view", k), ("gtab", ""), ("vtab", ""), ("ret", "")]),
            on: *k == view,
            count: match *k {
                "goals" => cur
                    .conversion
                    .map(|c| format!("{:.1}%", c * 100.0))
                    .unwrap_or_default(),
                "visitors" if ctx.shell.live > 0 => format!("{} live", ctx.shell.live),
                "payments" if cur.payments > 0 => group(cur.payments),
                _ => String::new(),
            },
        })
        .collect();
    let chips: Vec<FilterChip> = filters
        .iter()
        .map(|(k, v)| {
            let (icon, label) = chip_view(k, v);
            FilterChip {
                icon,
                label,
                href: href(&[(k, "")]),
            }
        })
        .collect();
    let visitors_href = match filters.as_slice() {
        [(k @ ("channel" | "source"), v)] => {
            let mut set: Vec<(&str, String)> = picked
                .params()
                .into_iter()
                .map(|(k, v)| (k, encode(&v)))
                .collect();
            set.push((k, encode(v)));
            let set: Vec<(&str, &str)> = set.iter().map(|(k, v)| (*k, v.as_str())).collect();
            super::href(&site_url("visitors"), &set)
        }
        _ => String::new(),
    };
    let mut shell = ctx.shell;
    shell.usage = picked.usage(cur.visitors);
    shell.keep = picked.query();
    let mut page = DashboardPage {
        title: "Dashboard",
        view,
        views: views_opts,
        live_href: href(&[("view", "visitors"), ("vtab", "")]),
        matching: matching(
            &split,
            (!providers.is_empty()).then_some(syncing),
            &ctx.site.domain,
            window_days,
        ),
        range: picked.menu(today, ctx.first_day, &base, &raw),
        report_href: site_url("report"),
        chips,
        visitors_href,
        privacy: ctx.site.cookieless,
        waiting: None,
        setup: None,
        insight: None,
        kpis: Vec::new(),
        earlier: String::new(),
        unmatched: String::new(),
        charts: Vec::new(),
        chart_tabs: Vec::new(),
        cards: Vec::new(),
        ai: None,
        profit: None,
        topay: None,
        reading: None,
        almost: None,
        verdicts: Vec::new(),
        why: None,
        goals: None,
        goal_card: None,
        customers: None,
        visitors: None,
        payments: None,
        retention: None,
        shell,
    };
    match data {
        Data::Overview(report, tabs, hours, avg, sources) => {
            let setup = &report.setup;
            let since = setup.first_event.map(|t| t.to_offset(ctx.offset).date());
            let age = since.map_or(0, |d| (today - d).whole_days());
            let young = since.is_none() || cur.visitors < 100 && age <= 30;
            let empty = setup.first_event.is_none() && setup.payments == 0;
            if empty && public.is_none() {
                page.waiting = Some(Waiting {
                    domain: ctx.site.domain.clone(),
                    snippet: format!(
                        "<script defer src=\"{}/v.js\"\n  data-site=\"{}\"{}></script>",
                        state.config.base_url,
                        ctx.site.domain,
                        if ctx.site.cookieless {
                            " data-cookieless"
                        } else {
                            ""
                        }
                    ),
                    install_href: site_url("setup/script"),
                    sample_href: site_url("sample"),
                });
            }
            let paid_total: u64 = split.iter().map(|s| s.first_payments + s.renewals).sum();
            let unmatched: u64 = split.iter().map(|s| s.no_visitor + s.unseen).sum();
            let provider = providers.first().map(|p| provider_name(p));
            let step =
                |done: bool, label: &str, sub: String, act: &'static str, href: String| SetupStep {
                    done,
                    num: 0,
                    label: label.into(),
                    sub,
                    act,
                    href,
                };
            let fresh = empty || ctx.site.sample_data;
            let mut steps = if fresh {
                vec![
                    step(
                        true,
                        "Site added",
                        ctx.site.domain.clone(),
                        "",
                        String::new(),
                    ),
                    step(
                        false,
                        "Add the script",
                        "Waiting for the first pageview".into(),
                        "Install guide",
                        site_url("setup/script"),
                    ),
                ]
            } else {
                vec![step(
                    true,
                    "Script installed",
                    since
                        .map(|d| format!("First visit on {}", day(d)))
                        .unwrap_or_else(|| "Payments arrived before any visit".into()),
                    "",
                    String::new(),
                )]
            };
            steps.push(match provider {
                Some(name) => step(
                    true,
                    "Payments connected",
                    format!("{name}, read-only key"),
                    "",
                    String::new(),
                ),
                None => step(
                    false,
                    "Connect payments",
                    "Read-only key for Stripe, Dodo Payments, Polar, LemonSqueezy or Creem".into(),
                    "Connect",
                    site_url("setup/payments"),
                ),
            });
            steps.push(match &setup.first_match {
                Some(_) if paid_total > 0 => step(
                    true,
                    "Checkout linked to visitors",
                    format!(
                        "{} of {} matched",
                        group(paid_total - unmatched),
                        plural(paid_total, "payment")
                    ),
                    "",
                    String::new(),
                ),
                Some(_) => step(
                    true,
                    "Checkout linked to visitors",
                    "Payments match their visits".into(),
                    "",
                    String::new(),
                ),
                None => step(
                    false,
                    "Link checkout",
                    "One line so every payment matches a visit".into(),
                    "How it works",
                    site_url("setup/checkout"),
                ),
            });
            if !fresh {
                steps.push(step(
                    setup.has_goals,
                    "Track a goal",
                    "Signups or trials show which channels convert before anyone pays".into(),
                    "Add a goal",
                    href(&[("view", "goals")]),
                ));
            }
            for (i, s) in steps.iter_mut().enumerate() {
                s.num = i + 1;
            }
            let done = steps.iter().filter(|s| s.done).count();
            if public.is_none() && (fresh || (age <= 14 && !dismissed && done < steps.len())) {
                page.setup = Some(SetupCard {
                    title: if fresh {
                        format!("Get {} tracking", ctx.site.domain)
                    } else {
                        "Finish setting up".into()
                    },
                    count: format!("{done} of {} done", steps.len()),
                    steps,
                    dismiss: if fresh {
                        String::new()
                    } else {
                        site_url("setup/dismiss")
                    },
                });
            }

            page.insight = if !filters.is_empty() {
                let pv = cur.revenue_per_visitor;
                Some((
                    format!(
                        "{filter_label} brought {} and {} in revenue.",
                        plural(cur.visitors, "visitor"),
                        money(cur.revenue_usd)
                    ),
                    match (pv, all.as_ref().and_then(|a| a.revenue_per_visitor)) {
                        (Some(pv), Some(a)) => format!(
                            "That is {} per visitor, {:.1}x your site average. Remove the filter to see everything again.",
                            usd(pv),
                            pv / a
                        ),
                        _ => "Remove the filter to see everything again.".into(),
                    },
                ))
            } else if empty {
                None
            } else {
                insight::summary(
                    cur,
                    avg,
                    &report.channels,
                    &insight::ai_rows(&sources),
                    &report.landing_pages,
                )
                .or_else(|| {
                    let m = setup.first_match.as_ref().filter(|_| young)?;
                    Some((
                        format!(
                            "Your first sale came from {}: {}.",
                            source_of(m).0,
                            super::amount(m)
                        ),
                        "Too early for trends. The numbers start to mean something after about 100 visitors and 10 payments.".into(),
                    ))
                })
            };
            page.kpis = kpis(
                cur,
                &overview.previous,
                all.as_ref(),
                &filter_label,
                compare,
                young && filters.is_empty(),
                since,
                from,
                page.matching.connected.then_some(page.matching.syncing),
            );
            let (when, before) = match rkey {
                "today" => ("today", "today".to_string()),
                "yesterday" => ("yesterday", "yesterday".to_string()),
                _ => ("in this range", day(from)),
            };
            if cur.visitors == 0 && cur.payments > 0 {
                for k in page
                    .kpis
                    .iter_mut()
                    .filter(|k| k.label == "Revenue per visitor")
                {
                    k.note = if when == "in this range" {
                        "No visitors in this range".into()
                    } else {
                        format!("No visitors yet {when}")
                    };
                }
            }
            let earlier: f64 = split.iter().map(|s| s.earlier_usd).sum();
            if earlier > 0.0 && filters.is_empty() && !hide {
                page.earlier = format!(
                    "{} {when} came from visits before {before}. Revenue counts on the payment date.",
                    money(earlier)
                );
            }
            if public.is_none() && filters.is_empty() {
                page.unmatched = unmatched_line(cur, &split, window_days);
            }

            let picked = match q.chart.as_deref() {
                Some("line") => 1,
                Some("channel") => 2,
                Some("total") => 3,
                _ => 0,
            };
            let (s, prev) = match &hours {
                Some((h, ph)) => (
                    chart::Series {
                        points: h.clone(),
                        weekly: false,
                    },
                    chart::Series {
                        points: ph.clone(),
                        weekly: false,
                    },
                ),
                None => (
                    chart::series(&report.days),
                    chart::series(&report.previous_days),
                ),
            };
            let show_prev = compare && !young && filters.is_empty();
            let hatch = pre(&p, since, &report.days);
            let unit = if hours.is_some() {
                "hour"
            } else if s.weekly {
                "week"
            } else {
                "day"
            };
            let total: f64 = s.points.iter().map(|d| d.revenue_usd).sum();
            let visits: u64 = s.points.iter().map(|d| d.visitors).sum();
            let prev_total: f64 = prev.points.iter().map(|d| d.revenue_usd).sum();
            let best = s
                .points
                .iter()
                .filter(|d| d.revenue_usd > 0.0)
                .max_by(|a, b| a.revenue_usd.total_cmp(&b.revenue_usd));
            let kinds = [
                ("", "Bars", Kind::Bars),
                ("line", "Line", Kind::Line),
                ("channel", "By channel", Kind::Channel),
                ("total", "Cumulative", Kind::Total),
            ];
            let weekly = s.weekly;
            let day_href = |d: Date| {
                let end = if weekly { d + Duration::days(6) } else { d };
                let set = [
                    ("range", String::new()),
                    ("from", d.to_string()),
                    ("to", end.min(to).to_string()),
                ];
                filter_href(&set)
            };
            let day_link: Option<&(dyn Fn(Date) -> String + Sync)> =
                (public.is_none() && hours.is_none()).then_some(&day_href);
            // Every chart type is rendered so switching needs no reload.
            for (i, (k, label, kind)) in kinds.into_iter().enumerate() {
                let svg = chart::revenue(
                    &s,
                    show_prev.then_some(&prev),
                    kind,
                    hatch.as_ref().map(|(f, l)| (*f, l.as_str())),
                    (!filters.is_empty()).then_some(filter_label.as_str()),
                    hide,
                    day_link,
                );
                let title = match kind {
                    Kind::Bars if hours.is_some() => "Revenue per hour",
                    Kind::Bars => "Revenue",
                    Kind::Line if show_prev => "Revenue against the previous period",
                    Kind::Line => "Revenue over time",
                    Kind::Channel => "Revenue by channel",
                    Kind::Total => "Revenue so far this period",
                };
                let note = if total <= 0.0 && visits == 0 {
                    "Nothing to show yet.".into()
                } else if total <= 0.0 {
                    match kind {
                        Kind::Bars => format!(
                            "No revenue {when}. The dots are visitors per {unit}, on the right axis."
                        ),
                        _ => format!("No revenue {when}."),
                    }
                } else if hide && matches!(kind, Kind::Bars | Kind::Total) {
                    format!("Amounts are hidden. Hover for each {unit}'s share of the revenue.")
                } else {
                    match kind {
                        Kind::Bars => match best {
                            Some(b) => format!(
                                "Best {unit}{}: {}, {} from {}. Hover a bar for the details{}.",
                                if filters.is_empty() {
                                    String::new()
                                } else {
                                    format!(" for {filter_label}")
                                },
                                match b.hour {
                                    Some(h) => format!("{h:02}:00"),
                                    None => day(b.date),
                                },
                                money(b.revenue_usd),
                                plural(b.payments, "payment"),
                                if day_link.is_some() {
                                    format!(", or click it to see that {unit}")
                                } else {
                                    String::new()
                                }
                            ),
                            None => String::new(),
                        },
                        Kind::Line if show_prev => format!(
                            "The dashed line is the previous {} {unit}s, lined up by {unit}. Hover for both values.",
                            s.points.len()
                        ),
                        Kind::Line => format!("Hover the chart for each {unit}."),
                        Kind::Channel if !filters.is_empty() => format!(
                            "Showing {filter_label} only. Remove the filter to see every channel."
                        ),
                        Kind::Channel => "Hover a bar to see the split.".into(),
                        Kind::Total if show_prev && prev_total > 0.0 => {
                            let pct = (total / prev_total - 1.0) * 100.0;
                            format!(
                                "You are at {}, {:.0}% {} the previous period ({}).",
                                money(total),
                                pct.abs(),
                                if pct >= 0.0 { "ahead of" } else { "behind" },
                                money(prev_total)
                            )
                        }
                        Kind::Total => format!("{} so far this period.", money(total)),
                    }
                };
                page.charts.push(ChartPanel {
                    title,
                    svg,
                    note,
                    on: i == picked,
                });
                page.chart_tabs.push(Opt {
                    label: label.to_string(),
                    href: format!("{}#revenue", href(&[("chart", k)])),
                    on: i == picked,
                    count: String::new(),
                });
            }
            let base_numbers = cards::Base {
                visitors: cur.visitors,
                revenue: cur.revenue_usd,
                avg,
                warn_small: !young,
                hide,
            };
            let mut left = tabs.as_slice();
            page.cards = picks
                .iter()
                .zip(cards::SPECS.iter())
                .map(|(pick, spec)| {
                    let (mine, rest) = left.split_at(spec.tabs.len());
                    left = rest;
                    cards::card(pick, mine, &base_numbers, &base, &params, &filter_href)
                })
                .collect();
            let Some((links, bots, own)) = early else {
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            };
            let links = joined(links.await)?;
            cards::with_links(
                &mut page.cards,
                &links,
                &state.config.base_url,
                &match public {
                    Some(_) => String::new(),
                    None => crate::sites::app_path(&ctx.site.domain, "links"),
                },
                q.rest.get("new_link").map(String::as_str),
                q.rest.get("link_error").map(String::as_str),
            );
            let at = |d: Date| d.midnight().assume_offset(ctx.offset).unix_timestamp();
            let bots = joined(bots.await)?;
            let mut ai = cards::ai_card(
                &sources,
                &tabs[0].0,
                &bots,
                &base_numbers,
                &report.days,
                (show_prev).then_some(report.previous_days.as_slice()),
                &filter_href,
                now,
            );
            if public.is_none() {
                let range = (at(from), at(to + Duration::days(1)) - 1);
                let inputs = answers::Inputs::load(&state, &ctx.site.id, (from, to), range)
                    .await
                    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
                let drawer = |a: crate::answers::Assistant| {
                    format!(
                        "{}?from={from}&to={to}",
                        site_url(&format!("answers/{}", a.as_str()))
                    )
                };
                ai.answers = Some(answers::tab(
                    &inputs,
                    &bots,
                    &sources,
                    drawer,
                    format!("{}#ai-answers", site_url("settings")),
                    state.config.hosted,
                ));
            }
            if let Some((spend, pays, goals, read, almost, earlier)) = own {
                let spend = joined(spend.await)?;
                page.profit = crate::spend::profit_card(
                    &report.channels,
                    &crate::spend::parse(&spend),
                    (to - from).whole_days() + 1,
                );
                let (pays, seen) = joined(pays.await)?;
                page.topay = crate::topay::pay_card(&pays.rows);
                let names = Names::load(&state.db, &ctx.site.id, customers::keys(&pays.rows))
                    .await
                    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
                page.customers = Some(customers::card(
                    &pays.rows,
                    &seen,
                    &names,
                    &ctx.site.domain,
                    cur.visitors,
                ));
                let goals = joined(goals.await)?;
                page.goal_card = Some(goal_card::card(
                    &goals,
                    q.rest.get("goals").is_some_and(|v| v == "list"),
                    cur.visitors,
                    |v| href(&[("goals", v)]),
                ));
                let read = joined(read.await)?;
                page.reading = crate::reading::card(
                    &read,
                    q.rest.get("rd").is_some_and(|v| v == "buyers"),
                    (
                        format!("{}#reading", href(&[("rd", "")])),
                        format!("{}#reading", href(&[("rd", "buyers")])),
                    ),
                );
                let almost = joined(almost.await)?;
                page.almost = crate::almost::card(
                    &almost,
                    &report.channels,
                    q.rest.get("ap").is_some_and(|v| v == "pages"),
                    (
                        format!("{}#almost", href(&[("ap", "")])),
                        format!("{}#almost", href(&[("ap", "pages")])),
                    ),
                );
                if let Some(earlier) = earlier {
                    let earlier = joined(earlier.await)?;
                    page.why = crate::why::revenue_change(
                        cur.revenue_usd,
                        overview.previous.revenue_usd,
                        &report.channels,
                        &earlier,
                    );
                    page.verdicts = crate::verdict::verdicts(&report.channels, &earlier);
                }
            }
            cards::ai_tabs(&mut ai, q.rest.get("ai").map(String::as_str), |v| {
                href(&[("ai", v)])
            });
            page.ai = Some(ai);
        }
        Data::Goals(g) => {
            page.goals = Some(goals::view(
                &g,
                cur,
                (p.to - p.from).whole_days() + 1,
                ctx.first_day.is_none(),
                &href,
                &raw,
                &base,
                &funnels,
            ));
        }
        Data::Visitors(v) => {
            let names = Names::load(&state.db, &ctx.site.id, views::visitor_keys(&v))
                .await
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            page.visitors = Some(views::visitors_view(
                v,
                &names,
                &ctx.site.domain,
                ctx.offset,
                &q.rest,
                &href,
                page.shell.live,
                page.matching.connected,
            ));
        }
        Data::Payments(v) => {
            let names = Names::load(&state.db, &ctx.site.id, views::payer_keys(&v))
                .await
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            page.payments = Some(views::payments_view(
                v,
                &names,
                &ctx.site.domain,
                ctx.offset,
            ));
        }
        Data::Retention(r, mrr) => {
            page.retention = Some(views::retention_view(&r, mrr.as_ref(), &q.rest, &href));
        }
    }
    if let Some((r, mrr)) = &ret {
        page.retention = Some(views::retention_view(r, mrr.as_ref(), &q.rest, &href));
    }
    if public.is_none() {
        return Ok(render(page));
    }
    if hide {
        hide_amounts(&mut page, cur, &overview.previous);
    }
    // Rows do not filter here, so drop the hints that say they do.
    for p in page.cards.iter_mut().flat_map(|c| &mut c.panels) {
        let kept: Vec<&str> = p
            .foot
            .split(". ")
            .filter(|s| !s.starts_with("Click"))
            .collect();
        p.foot = match kept.join(". ") {
            f if f.is_empty() || f.ends_with('.') => f,
            f => format!("{f}."),
        };
    }
    let url = format!("{}{base}", state.config.base_url);
    let (og_title, og_desc) = og(&ctx.site.domain, cur, &page.range.label, hide);
    let mut res = render(PublicPage {
        domain: ctx.site.domain.clone(),
        icon: page.shell.icon,
        og_image: format!("{}/assets/art/og-share.jpg", state.config.base_url),
        url,
        og_title,
        og_desc,
        live: page.shell.live,
        range_label: page.range.label,
        ranges: page.range.ranges,
        insight: page.insight,
        kpis: page.kpis,
        earlier: page.earlier,
        unmatched: String::new(),
        charts: page.charts,
        chart_tabs: page.chart_tabs,
        cards: page.cards,
        ai: page.ai,
        profit: page.profit,
        topay: page.topay,
        reading: None,
        almost: None,
        verdicts: Vec::new(),
        why: page.why,
        retention: page.retention,
        goal_card: None,
        customers: None,
    });
    // Off means gone right away, so nothing may keep a copy.
    res.headers_mut()
        .insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    Ok(res)
}

/// Title and description for link previews; amounts only when the owner shows them.
fn og(domain: &str, cur: &Totals, range: &str, hide: bool) -> (String, String) {
    let range = range.to_lowercase().replace("last", "in the last");
    let lead = if hide {
        format!("{} {range}.", plural(cur.visitors, "visitor"))
    } else {
        format!(
            "{} in revenue from {} {range}.",
            money(cur.revenue_usd),
            plural(cur.visitors, "visitor")
        )
    };
    (
        format!("{domain} revenue dashboard"),
        format!("{lead} See which channels bring paying customers, live on Virling."),
    )
}

/// Swaps revenue amounts for shares and ratios on the public page.
fn hide_amounts(page: &mut DashboardPage, cur: &Totals, prev: &Totals) {
    for k in &mut page.kpis {
        match k.label {
            "Revenue" => {
                let (tone, change) = delta(Some(cur.revenue_usd), Some(prev.revenue_usd));
                *k = if change.is_empty() {
                    kpi(
                        "Revenue growth",
                        "–".into(),
                        String::new(),
                        "",
                        "needs revenue in the previous period".into(),
                    )
                } else {
                    kpi(
                        "Revenue growth",
                        change,
                        String::new(),
                        tone,
                        "against the previous period".into(),
                    )
                };
            }
            "Revenue per visitor" => {
                *k = kpi(
                    "Conversion",
                    cur.conversion
                        .map_or_else(|| "–".into(), |c| format!("{:.1}%", c * 100.0)),
                    String::new(),
                    "",
                    "of visitors became customers".into(),
                );
            }
            "Visitors" => k.note = String::new(),
            _ => {}
        }
    }
    page.insight = page.insight.take().and_then(|(lead, fix)| {
        let lead = match lead.split_once(". ") {
            Some((first, _)) if lead.contains('$') => format!("{first}."),
            _ => lead,
        };
        (!lead.contains('$')).then_some((lead, fix))
    });
    if let Some(rt) = page.retention.as_mut().filter(|rt| !rt.refunds) {
        rt.col2 = "";
        for r in &mut rt.rows {
            r.val2 = String::new();
        }
        rt.foot = rt
            .foot
            .split(". ")
            .find(|s| s.starts_with("Average still paying"))
            .map(|s| format!("{}.", s.trim_end_matches('.')))
            .unwrap_or_default();
    }
}

/// Id, timezone, cookieless, window, hide revenue, and the favicon's fetch time.
type PublicSite = (String, String, bool, i64, bool, Option<i64>);

/// The read-only dashboard at /share/{domain}; a 404 unless the owner turned it on.
pub async fn public(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(q): Query<DashboardQuery>,
) -> Response {
    let row: Result<Option<PublicSite>, _> = sqlx::query_as(
        "SELECT s.id, s.timezone, s.cookieless, s.window_days, s.public_hide_revenue, i.fetched_at
         FROM sites s LEFT JOIN site_icons i ON i.site_id = s.id AND i.body IS NOT NULL
         WHERE s.domain = ? AND s.public_dashboard = 1 AND s.sample_data = 0",
    )
    .bind(&domain)
    .fetch_optional(&state.db)
    .await;
    let (id, timezone, cookieless, window_days, hide, icon) = match row {
        Ok(Some(r)) => r,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let store = state.events.clone();
    let (sid, tz) = (id.clone(), timezone.clone());
    let live = state.presence.count(&id);
    let clock =
        blocking(move || Ok((store.utc_offset(&tz)?, store.first_event(&sid)?, live))).await;
    let (offset, first, live) = match clock {
        Ok(c) => c,
        Err(code) => return code.into_response(),
    };
    let ctx = Ctx {
        shell: Shell {
            domain: domain.clone(),
            initial: domain[..1].to_ascii_uppercase(),
            icon: icon
                .map(|v| format!("/share/{domain}/icon?v={v}"))
                .unwrap_or_default(),
            sites: Vec::new(),
            nav: "dashboard",
            plan: String::new(),
            usage: String::new(),
            keep: String::new(),
            email: String::new(),
            avatar: String::new(),
            live,
            agency: false,
            sample: None,
            banner: String::new(),
        },
        site: Site {
            id,
            domain,
            timezone,
            cookieless,
            same_visit: String::new(),
            return_minutes: 5,
            sample_data: false,
            ignore_ips: String::new(),
        },
        today: OffsetDateTime::now_utc().to_offset(offset).date(),
        first_day: first.map(|t| t.to_offset(offset).date()),
        offset,
    };
    page(
        state,
        ctx,
        q,
        window_days,
        true,
        (Vec::new(), false),
        Some(hide),
    )
    .await
    .unwrap_or_else(|code| code.into_response())
}

enum Data {
    Overview(
        Box<crate::stats::Report>,
        Vec<(Vec<BreakdownRow>, Vec<BreakdownRow>)>,
        Option<(Vec<DayPoint>, Vec<DayPoint>)>,
        Option<f64>,
        Vec<BreakdownRow>,
    ),
    Goals(goals::GoalsData),
    Visitors(views::VisitorsData),
    Payments(views::PaymentsData),
    Retention(
        Box<crate::retention::Retention>,
        Option<crate::retention::Mrr>,
    ),
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::super::daterange::preset;
    use super::*;

    #[test]
    fn new_customers_are_counted_since_the_range_start() {
        let t = Totals {
            new_customers: 2,
            ..Default::default()
        };
        // The script went live Oct 18 (Asia/Kolkata), the payments synced from Oct 14, the range is 7 days.
        let k = kpis(
            &t,
            &Totals::default(),
            None,
            "",
            true,
            true,
            Some(date!(2026 - 10 - 18)),
            preset("7d", date!(2026 - 10 - 18), None).unwrap().0,
            Some(false),
        );
        let new = k.iter().find(|k| k.label == "New customers").unwrap();
        assert_eq!(new.note, "since Oct 12");
    }

    #[test]
    fn the_hatch_never_covers_a_day_with_data() {
        let p = Period {
            from: date!(2026 - 10 - 12),
            to: date!(2026 - 10 - 18),
            tz: "Asia/Kolkata".into(),
        };
        let mut days: Vec<DayPoint> = (0..7)
            .map(|i| DayPoint {
                date: p.from + time::Duration::days(i),
                visitors: 0,
                payments: 0,
                customers: 0,
                revenue_usd: 0.0,
                converted: 0,
                matched_revenue_usd: 0.0,
                conversion: None,
                revenue_per_visitor: None,
                hour: None,
                by_channel: Vec::new(),
            })
            .collect();
        let live = Some(date!(2026 - 10 - 18));
        assert_eq!(
            pre(&p, live, &days),
            Some((
                6.0 / 7.0,
                "Not tracking yet. Script went live Oct 18.".into()
            ))
        );
        days[2].payments = 2;
        days[2].revenue_usd = 2.98;
        assert_eq!(
            pre(&p, live, &days),
            Some((2.0 / 7.0, "Not tracking yet. First payment Oct 14.".into()))
        );
        assert_eq!(pre(&p, None, &days).unwrap().0, 2.0 / 7.0);
        days[0].visitors = 1;
        assert_eq!(pre(&p, live, &days), None);
    }

    #[test]
    fn revenue_follows_the_provider_connection() {
        let note = |sync: Option<bool>| {
            let k = kpis(
                &Totals::default(),
                &Totals::default(),
                None,
                "",
                true,
                true,
                None,
                date!(2026 - 10 - 12),
                sync,
            );
            k.into_iter().find(|k| k.label == "Revenue").unwrap().note
        };
        assert_eq!(note(None), "connect payments to see revenue");
        assert_eq!(note(Some(true)), "Syncing payments…");
        assert_eq!(note(Some(false)), "no payments in this range");
        let m = matching(&[], Some(true), "x.com", 30);
        assert!(m.connected && m.syncing);
        assert_eq!(m.title, "Syncing payments…");
        let m = matching(&[], Some(false), "x.com", 30);
        assert!(m.connected && !m.syncing);
        assert_eq!(m.label, "No payments yet");
    }

    #[test]
    fn previews_hide_amounts_when_asked() {
        let t = Totals {
            visitors: 1200,
            revenue_usd: 480.0,
            ..Default::default()
        };
        let (title, desc) = og("x.com", &t, "Last 30 days", false);
        assert_eq!(title, "x.com revenue dashboard");
        assert!(desc.starts_with("$480 in revenue from 1,200 visitors in the last 30 days."));
        let (_, desc) = og("x.com", &t, "Last 30 days", true);
        assert!(desc.starts_with("1,200 visitors in the last 30 days.") && !desc.contains('$'));
    }

    #[test]
    fn matching_counts_and_fixes() {
        let split = [ProviderSplit {
            provider: "stripe".into(),
            first_payments: 20,
            renewals: 30,
            no_visitor: 2,
            unseen: 1,
            ..Default::default()
        }];
        let m = matching(&split, Some(false), "x.com", 30);
        assert_eq!(
            (m.bold.as_str(), m.label.as_str()),
            ("94%", " of payments matched")
        );
        assert_eq!(m.fixes.len(), 2);
        assert_eq!(
            m.fixes[0].code,
            "client_reference_id: req.cookies.virling_vid"
        );
        assert!(m.fixes[1].code.is_empty());
        assert!(!matching(&[], None, "x.com", 30).connected);
    }

    #[test]
    fn unmatched_line_gives_share_and_reasons() {
        let split = [ProviderSplit {
            no_visitor: 2,
            unseen: 1,
            ..Default::default()
        }];
        let t = Totals {
            revenue_usd: 200.0,
            matched_revenue_usd: 150.0,
            ..Default::default()
        };
        assert_eq!(
            unmatched_line(&t, &split, 30),
            "75% of revenue is matched to a visit. 2 payments had no visitor ID, 1 payment had no visit in the 30 days before paying."
        );
        let all = Totals {
            matched_revenue_usd: 200.0,
            ..t
        };
        assert_eq!(unmatched_line(&all, &split, 30), "");
        assert_eq!(unmatched_line(&Totals::default(), &[], 30), "");
    }
}
