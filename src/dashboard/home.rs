//! The dashboard page: range, filters, matching, and the five views.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};

use super::{
    Ctx, Opt, Shell, blocking, cards, day, delta, encode, goals, group, insight, link, load, money,
    plural, provider_name, resolve_range, source_of, usd, views,
};
use crate::alias::Names;
use crate::app::AppState;
use crate::auth::Account;
use crate::channel::source_domain;
use crate::chart::{self, Kind};
use crate::onboarding::render;
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

const PRESETS: [(&str, &str); 8] = [
    ("today", "Today"),
    ("yesterday", "Yesterday"),
    ("7d", "Last 7 days"),
    ("30d", "Last 30 days"),
    ("90d", "Last 90 days"),
    ("month", "This month"),
    ("12m", "Last 12 months"),
    ("all", "Since tracking started"),
];

/// Filter params, and the first-pageview column each one matches; "viewed" matches any page of the visit.
const FILTERS: [(&str, &str); 12] = [
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
];

/// Params kept by every link besides the range, filters and card tabs.
const KEEP: [&str; 9] = [
    "compare", "view", "chart", "gtab", "vtab", "ret", "s1", "s2", "s3",
];

const VIEWS: [(&str, &str); 5] = [
    ("", "Overview"),
    ("goals", "Goals"),
    ("visitors", "Visitors"),
    ("payments", "Latest payments"),
    ("retention", "Customers who stay"),
];

/// "Oct 16", "Oct 9 to 15", "Sep 16 to Oct 15", "Oct 2025 on".
pub(crate) fn span(from: Date, to: Date) -> String {
    if from == to {
        day(from)
    } else if from.year() != to.year() {
        from.format(format_description!("[month repr:short] [year] on"))
            .unwrap_or_default()
    } else if from.month() == to.month() {
        format!("{} to {}", day(from), to.day())
    } else {
        format!("{} to {}", day(from), day(to))
    }
}

/// The preset's local dates, or None when it does not apply yet.
fn preset(key: &str, today: Date, first: Option<Date>) -> Option<(Date, Date)> {
    let back = |n: i64| (today - Duration::days(n - 1), today);
    Some(match key {
        "today" => (today, today),
        "yesterday" => (today - Duration::days(1), today - Duration::days(1)),
        "7d" => back(7),
        "30d" => back(30),
        "90d" => back(90),
        "month" => (today.replace_day(1).ok()?, today),
        "12m" => back(365),
        "all" => (first?, today),
        _ => return None,
    })
}

fn ymd(s: Option<&str>) -> Option<Date> {
    Date::parse(s?, format_description!("[year]-[month]-[day]")).ok()
}

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
    range_label: String,
    compare: bool,
    /// Shows "vs previous" on the range button.
    vs: bool,
    compare_href: String,
    ranges: Vec<Opt>,
    custom: bool,
    custom_from: String,
    custom_to: String,
    today: String,
    base: String,
    /// Params a custom range keeps, as raw values.
    hidden: Vec<(&'static str, String)>,
    report_href: String,
    chips: Vec<FilterChip>,
    privacy: bool,
    waiting: Option<Waiting>,
    setup: Option<SetupCard>,
    insight: Option<(String, String)>,
    kpis: Vec<Kpi>,
    /// Revenue in range that came from visits before it.
    earlier: String,
    chart: String,
    chart_title: &'static str,
    chart_note: String,
    chart_tabs: Vec<Opt>,
    cards: Vec<cards::TabCard>,
    ai: Option<cards::AiCard>,
    goals: Option<goals::GoalsView>,
    visitors: Option<views::VisitorsView>,
    payments: Option<views::PaymentsView>,
    retention: Option<views::RetentionView>,
}

/// One-line code that passes the visitor ID into a provider's checkout.
fn fix_line(provider: &str) -> &'static str {
    match provider {
        "stripe" => "client_reference_id: req.cookies.virling_vid",
        "lemonsqueezy" => "checkout_data: { custom: { virling_vid: req.cookies.virling_vid } }",
        _ => "metadata: { virling_vid: req.cookies.virling_vid }",
    }
}

fn matching(split: &[ProviderSplit], connected: bool, domain: &str, window: i64) -> Matching {
    let total: u64 = split.iter().map(|s| s.first_payments + s.renewals).sum();
    let unmatched: u64 = split.iter().map(|s| s.no_visitor + s.unseen).sum();
    let matched = total - unmatched;
    let connected = connected || total > 0;
    let (bold, label) = if !connected {
        (String::new(), "Payments not connected".to_string())
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
        bold,
        label,
        title,
        desc,
        fixes,
    }
}

fn chip_view(key: &str, value: &str) -> (String, String) {
    let none = value == "(none)";
    match key {
        "channel" | "source" => (icons::source(value, 16), value.to_string()),
        "campaign" if none => (icons::glyph("paid", 16, "#6B717C"), "No campaign".into()),
        "campaign" => (icons::glyph("paid", 16, "#6B717C"), value.to_string()),
        "keyword" if none => (
            icons::glyph("search", 16, "#6B717C"),
            "No search term".into(),
        ),
        "keyword" => (icons::glyph("search", 16, "#6B717C"), value.to_string()),
        "page" => (
            icons::glyph("page", 16, "#6B717C"),
            format!("Landed on {value}"),
        ),
        "viewed" => (
            icons::glyph("page", 16, "#6B717C"),
            format!("Viewed {value}"),
        ),
        "country" => (icons::flag(value), icons::country(value)),
        "city" => (icons::glyph("pin", 16, "#6B717C"), value.to_string()),
        "language" => (cards::language_icon(value), cards::language_name(value)),
        "device" => (icons::device(value), value.to_string()),
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

/// KPI tiles: against the previous period, the whole site when filtered, or early-data notes on a new site.
fn kpis(
    cur: &Totals,
    prev: &Totals,
    all: Option<&Totals>,
    filter_label: &str,
    compare: bool,
    young: bool,
    since: Option<Date>,
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
                if cur.payments > 0 {
                    format!("from {}", plural(cur.payments, "payment"))
                } else {
                    "connect payments to see revenue".into()
                },
            ),
            kpi(
                "New customers",
                group(cur.new_customers),
                String::new(),
                "",
                if cur.new_customers > 0 && !since_txt.is_empty() {
                    format!("since {since_txt}")
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
    ]
}

/// Days before `first` as a share of the period, for the "not tracking yet" hatch.
fn pre(p: &Period, first: Option<Date>) -> Option<(f64, String)> {
    let days = ((p.to - p.from).whole_days() + 1) as f64;
    match first {
        None => Some((
            1.0,
            "Revenue per day shows up here after the first visit and payment.".into(),
        )),
        Some(f) if f > p.from && f <= p.to => Some((
            (f - p.from).whole_days() as f64 / days,
            format!("Not tracking yet. Script went live {}.", day(f)),
        )),
        _ => None,
    }
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
    let providers: Result<Vec<String>, _> = sqlx::query_scalar(
        "SELECT provider FROM integrations WHERE site_id = ? ORDER BY created_at",
    )
    .bind(&ctx.site.id)
    .fetch_all(&state.db)
    .await;
    let (Ok((window_days, dismissed)), Ok(providers)) = (settings, providers) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    page(state, ctx, q, window_days, dismissed, providers)
        .await
        .unwrap_or_else(|code| code.into_response())
}

async fn page(
    state: AppState,
    ctx: Ctx,
    q: DashboardQuery,
    window_days: i64,
    dismissed: bool,
    providers: Vec<String>,
) -> Result<Response, StatusCode> {
    let today = ctx.today;
    let default = resolve_range(today, ctx.first_day, None).3;
    let custom = match (ymd(q.from.as_deref()), ymd(q.to.as_deref())) {
        (Some(f), Some(t)) if f <= t && f <= today => Some((f, t.min(today))),
        _ => None,
    };
    let key = q
        .range
        .as_deref()
        .filter(|k| preset(k, today, ctx.first_day).is_some())
        .unwrap_or(default);
    let (rkey, (from, to)) = match custom {
        Some(c) => ("custom", c),
        None => match preset(key, today, ctx.first_day) {
            Some(r) => (key, r),
            None => ("30d", preset("30d", today, None).unwrap_or((today, today))),
        },
    };
    let range_label = match PRESETS.iter().find(|p| p.0 == rkey) {
        Some(p) => p.1.to_string(),
        None => span(from, to),
    };
    let p = Period {
        from,
        to,
        tz: ctx.site.timezone.clone(),
    };
    let compare = q.compare.as_deref() != Some("off");
    let view = VIEWS
        .iter()
        .map(|v| v.0)
        .find(|v| !v.is_empty() && q.view.as_deref() == Some(*v))
        .unwrap_or("");

    // Raw params of the current page; links encode them.
    let mut raw: Vec<(&'static str, String)> = Vec::new();
    if custom.is_some() {
        raw.push(("from", from.to_string()));
        raw.push(("to", to.to_string()));
    } else if rkey != default {
        raw.push(("range", rkey.to_string()));
    }
    if !compare {
        raw.push(("compare", "off".into()));
    }
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
    for (k, _) in FILTERS {
        if let Some(v) = q.rest.get(k).filter(|v| !v.is_empty() && v.len() <= 300) {
            filters.push((k, v.clone()));
            raw.push((k, v.clone()));
        }
    }
    for k in KEEP.iter().skip(3) {
        if let Some(v) = q.rest.get(*k).filter(|v| !v.is_empty() && v.len() <= 300) {
            raw.push((k, v.clone()));
        }
    }
    let base = sites::app_path(&ctx.site.domain, "");
    let enc: Vec<(&str, String)> = raw.iter().map(|(k, v)| (*k, encode(v))).collect();
    let params: Vec<(&str, &str)> = enc.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let href = |set: &[(&str, &str)]| link(&base, &params, set);
    let filter_href = |set: &[(&'static str, String)]| {
        let set: Vec<(&str, String)> = set.iter().map(|(k, v)| (*k, encode(v))).collect();
        let set: Vec<(&str, &str)> = set.iter().map(|(k, v)| (*k, v.as_str())).collect();
        link(&base, &params, &set)
    };

    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let wanted = filters.clone();
    let pq = p.clone();
    let rest = q.rest.clone();
    let now = OffsetDateTime::now_utc();
    let tz = ctx.site.timezone.clone();
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
                    let kids = match t.kids() {
                        Some(d) => rows_of(Some(d))?,
                        None => Vec::new(),
                    };
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
            "visitors" => Data::Visitors(views::load_visitors(&*store, &id, &pq, &tz, now, &f)?),
            "payments" => Data::Payments(views::load_payments(&*store, &id, &pq, window_days, &f)?),
            _ => Data::Retention(Box::new(store.retention(&id, &pq, now)?)),
        };
        Ok((split, overview, all, data))
    })
    .await?;
    let (split, overview, all, data) = data;
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
    let ranges: Vec<Opt> = PRESETS
        .iter()
        .filter_map(|(k, label)| {
            let (f, t) = preset(k, today, ctx.first_day)?;
            Some(Opt {
                label: label.to_string(),
                href: href(&[
                    ("range", if *k == default { "" } else { k }),
                    ("from", ""),
                    ("to", ""),
                ]),
                on: *k == rkey,
                count: span(f, t),
            })
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
    let mut shell = ctx.shell;
    shell.usage = format!(
        "{} {}",
        plural(cur.visitors, "visitor"),
        match rkey {
            "today" => "today".to_string(),
            "yesterday" => "yesterday".to_string(),
            "all" => "since tracking started".to_string(),
            "custom" => span(from, to),
            _ => range_label.to_lowercase().replace("last", "in the last"),
        }
    );
    let mut page = DashboardPage {
        title: "Dashboard",
        view,
        views: views_opts,
        live_href: href(&[("view", "visitors"), ("vtab", "")]),
        matching: matching(&split, !providers.is_empty(), &ctx.site.domain, window_days),
        range_label,
        compare,
        vs: compare && rkey != "all",
        compare_href: href(&[("compare", if compare { "off" } else { "" })]),
        ranges,
        custom: custom.is_some(),
        custom_from: from.to_string(),
        custom_to: to.to_string(),
        today: today.to_string(),
        base: base.clone(),
        hidden: raw
            .iter()
            .filter(|(k, _)| !matches!(*k, "range" | "from" | "to"))
            .cloned()
            .collect(),
        report_href: site_url("report"),
        chips,
        privacy: ctx.site.cookieless,
        waiting: None,
        setup: None,
        insight: None,
        kpis: Vec::new(),
        earlier: String::new(),
        chart: String::new(),
        chart_title: "",
        chart_note: String::new(),
        chart_tabs: Vec::new(),
        cards: Vec::new(),
        ai: None,
        goals: None,
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
            if empty {
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
            let mut steps = if empty {
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
                    "Read-only key for Stripe, Dodo Payments, Polar or LemonSqueezy".into(),
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
                    "Link checkout to visitors",
                    "One line so every payment matches a visit".into(),
                    "How it works",
                    site_url("setup/checkout"),
                ),
            });
            if !empty {
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
            if empty || (age <= 14 && !dismissed && done < steps.len()) {
                page.setup = Some(SetupCard {
                    title: if empty {
                        format!("Get {} tracking", ctx.site.domain)
                    } else {
                        "Finish setting up".into()
                    },
                    count: format!("{done} of {} done", steps.len()),
                    steps,
                    dismiss: if empty {
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
            if earlier > 0.0 && filters.is_empty() {
                page.earlier = format!(
                    "{} {when} came from visits before {before}. Revenue counts on the payment date.",
                    money(earlier)
                );
            }

            let kind = match q.chart.as_deref() {
                Some("line") => Kind::Line,
                Some("channel") => Kind::Channel,
                Some("total") => Kind::Total,
                _ => Kind::Bars,
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
            let hatch = pre(&p, since);
            page.chart = chart::revenue(
                &s,
                show_prev.then_some(&prev),
                kind,
                hatch.as_ref().map(|(f, l)| (*f, l.as_str())),
                (!filters.is_empty()).then_some(filter_label.as_str()),
            );
            let unit = if hours.is_some() {
                "hour"
            } else if s.weekly {
                "week"
            } else {
                "day"
            };
            page.chart_title = match kind {
                Kind::Bars if hours.is_some() => "Revenue per hour",
                Kind::Bars => "Revenue",
                Kind::Line => "Revenue against the previous period",
                Kind::Channel => "Revenue by channel",
                Kind::Total => "Revenue so far this period",
            };
            let total: f64 = s.points.iter().map(|d| d.revenue_usd).sum();
            let prev_total: f64 = prev.points.iter().map(|d| d.revenue_usd).sum();
            let best = s
                .points
                .iter()
                .filter(|d| d.revenue_usd > 0.0)
                .max_by(|a, b| a.revenue_usd.total_cmp(&b.revenue_usd));
            page.chart_note = if total <= 0.0 {
                "Nothing to show yet.".into()
            } else {
                match kind {
                    Kind::Bars => match best {
                        Some(b) => format!(
                            "Best {unit}{}: {}, {} from {}. Hover a bar for the details.",
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
                            plural(b.payments, "payment")
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
            page.chart_tabs = [
                ("", "Bars"),
                ("line", "Line"),
                ("channel", "By channel"),
                ("total", "Cumulative"),
            ]
            .iter()
            .map(|(k, label)| Opt {
                label: label.to_string(),
                href: format!("{}#revenue", href(&[("chart", k)])),
                on: q.chart.as_deref().unwrap_or("") == *k || (k.is_empty() && kind == Kind::Bars),
                count: String::new(),
            })
            .collect();
            let base_numbers = cards::Base {
                visitors: cur.visitors,
                revenue: cur.revenue_usd,
                avg,
                warn_small: !young,
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
            let links = crate::links::site_links(&state.db, &ctx.site.id)
                .await
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            cards::with_links(
                &mut page.cards,
                &links,
                &state.config.base_url,
                &crate::sites::app_path(&ctx.site.domain, "links"),
                q.rest.get("new_link").map(String::as_str),
                q.rest.get("link_error").map(String::as_str),
            );
            page.ai = Some(cards::ai_card(
                &sources,
                &base_numbers,
                &report.days,
                (show_prev).then_some(report.previous_days.as_slice()),
                &filter_href,
            ));
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
        Data::Retention(r) => {
            page.retention = Some(views::retention_view(&r, &q.rest, &href));
        }
    }
    Ok(render(page))
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
    Retention(Box<crate::retention::Retention>),
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;

    #[test]
    fn spans_and_presets() {
        let t = date!(2026 - 10 - 16);
        assert_eq!(span(t, t), "Oct 16");
        assert_eq!(
            span(date!(2026 - 10 - 09), date!(2026 - 10 - 15)),
            "Oct 9 to 15"
        );
        assert_eq!(span(date!(2026 - 09 - 16), t), "Sep 16 to Oct 16");
        assert_eq!(preset("month", t, None), Some((date!(2026 - 10 - 01), t)));
        assert_eq!(
            preset("yesterday", t, None),
            Some((date!(2026 - 10 - 15), date!(2026 - 10 - 15)))
        );
        assert_eq!(preset("all", t, None), None);
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
        let m = matching(&split, true, "x.com", 30);
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
        assert!(!matching(&[], false, "x.com", 30).connected);
    }
}
