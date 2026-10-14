use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime, UtcOffset};

use crate::app::AppState;
use crate::auth::Account;
use crate::fx::minor_to_major;
use crate::onboarding::render;
use crate::sites::{self, Site};
use crate::stats::{BreakdownRow, Journey, PaymentFilter, PaymentRow, Period, Sort, sort_rows};
use crate::{chart, icons};

const RANGES: [(&str, &str, i64); 5] = [
    ("today", "Today", 1),
    ("7d", "Last 7 days", 7),
    ("30d", "Last 30 days", 30),
    ("90d", "Last 90 days", 90),
    ("12m", "Last 12 months", 365),
];
const FILTERS: [(&str, &str); 4] = [
    ("all", "All"),
    ("attributed", "Attributed"),
    ("unattributed", "Unattributed"),
    ("refunded", "Refunded"),
];
const PER_PAGE: u32 = 50;

/// Sidebar and top bar data shared by every app page.
pub struct Shell {
    pub site_id: String,
    pub domain: String,
    pub initial: String,
    pub sites: Vec<Opt>,
    pub nav: &'static str,
    pub plan: String,
    pub usage: String,
    pub live: u64,
}

/// A link in a custom dropdown, tab row or segmented control.
pub struct Opt {
    pub label: String,
    pub href: String,
    pub on: bool,
    pub count: String,
}

pub struct Ctx {
    pub site: Site,
    pub shell: Shell,
    pub offset: UtcOffset,
    pub today: Date,
}

/// Runs event store work off the async runtime; failures become a 503.
pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T, StatusCode> {
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(err)) => {
            tracing::error!(%err, "event store query failed");
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
        Err(err) => {
            tracing::error!(%err, "event store task failed");
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

/// Loads the site if the account owns it (else 404) plus the shell and the site's local clock.
pub async fn load(
    state: &AppState,
    account: &Account,
    site_id: &str,
    nav: &'static str,
) -> Result<Ctx, StatusCode> {
    let all = sites::for_account(&state.db, &account.id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let Some(site) = all.iter().find(|s| s.id == site_id).cloned() else {
        return Err(StatusCode::NOT_FOUND);
    };
    let plan: String = sqlx::query_scalar("SELECT plan FROM accounts WHERE id = ?")
        .bind(&account.id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let store = state.events.clone();
    let (id, tz) = (site.id.clone(), site.timezone.clone());
    let (offset, live, visitors) = blocking(move || {
        let offset = store.utc_offset(&tz)?;
        let today = OffsetDateTime::now_utc().to_offset(offset).date();
        let period = Period {
            from: today - Duration::days(29),
            to: today,
            tz,
        };
        let visitors = store.overview(&id, &period)?.current.visitors;
        Ok((offset, store.live_visitors(&id)?, visitors))
    })
    .await?;
    let shell = Shell {
        site_id: site.id.clone(),
        domain: site.domain.clone(),
        initial: site.domain[..1].to_ascii_uppercase(),
        sites: all
            .iter()
            .map(|s| Opt {
                label: s.domain.clone(),
                href: format!("/app/sites/{}/{}", s.id, nav),
                on: s.id == site.id,
                count: String::new(),
            })
            .collect(),
        nav,
        plan: match plan.as_str() {
            "self_hosted" => "Self-hosted".into(),
            other => other.replace('_', " "),
        },
        usage: format!("{} visitors in the last 30 days", group(visitors)),
        live,
    };
    Ok(Ctx {
        site,
        shell,
        today: OffsetDateTime::now_utc().to_offset(offset).date(),
        offset,
    })
}

fn range(key: Option<&str>) -> (&'static str, &'static str, i64) {
    RANGES
        .into_iter()
        .find(|r| Some(r.0) == key)
        .unwrap_or(RANGES[2])
}

fn period(ctx: &Ctx, days: i64) -> Period {
    Period {
        from: ctx.today - Duration::days(days - 1),
        to: ctx.today,
        tz: ctx.site.timezone.clone(),
    }
}

/// `base` plus the non-empty query params.
fn href(base: &str, params: &[(&str, &str)]) -> String {
    let q: Vec<String> = params
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if q.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", q.join("&"))
    }
}

fn range_opts(base: &str, current: &str, keep: &[(&str, &str)]) -> Vec<Opt> {
    RANGES
        .iter()
        .map(|(key, label, _)| {
            let mut params = vec![("range", *key)];
            params.extend_from_slice(keep);
            Opt {
                label: label.to_string(),
                href: href(base, &params),
                on: *key == current,
                count: String::new(),
            }
        })
        .collect()
}

pub fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn decimals(v: f64, symbol: &str, code: &str) -> String {
    let sign = if v < 0.0 { "-" } else { "" };
    let v = v.abs();
    let c = (v * 100.0).round() as u64;
    let body = if v >= 100.0 {
        group(v.round() as u64)
    } else if c.is_multiple_of(100) {
        group(c / 100)
    } else {
        format!("{}.{:02}", group(c / 100), c % 100)
    };
    if code.is_empty() {
        format!("{sign}{symbol}{body}")
    } else {
        format!("{sign}{body} {code}")
    }
}

/// "$12,480", "$480", "$48.50"
pub fn money(usd: f64) -> String {
    decimals(usd, "$", "")
}

/// A provider amount in its own currency.
pub fn native(minor: i64, currency: &str) -> String {
    let v = minor_to_major(minor, currency);
    match currency {
        "usd" => decimals(v, "$", ""),
        "eur" => decimals(v, "€", ""),
        "gbp" => decimals(v, "£", ""),
        "inr" => decimals(v, "₹", ""),
        "jpy" => decimals(v, "¥", ""),
        other => decimals(v, "", &other.to_ascii_uppercase()),
    }
}

pub fn provider_name(provider: &str) -> &'static str {
    match provider {
        "stripe" => "Stripe",
        "dodo" => "Dodo Payments",
        "polar" => "Polar",
        "lemonsqueezy" => "LemonSqueezy",
        _ => "Other",
    }
}

pub fn ago(ts: OffsetDateTime, now: OffsetDateTime) -> String {
    let secs = (now - ts).whole_seconds().max(0);
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..86400 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

fn local(ts: OffsetDateTime, offset: UtcOffset) -> String {
    ts.to_offset(offset)
        .format(format_description!(
            "[month repr:short] [day padding:none], [hour]:[minute]"
        ))
        .unwrap_or_default()
}

fn day(d: Date) -> String {
    d.format(format_description!("[month repr:short] [day padding:none]"))
        .unwrap_or_default()
}

fn status(status: &str) -> (&'static str, &'static str) {
    match status {
        "refunded" => ("Refunded", "bad"),
        "partially_refunded" => ("Partly refunded", "warn"),
        "failed" => ("Failed", "bad"),
        _ => ("Paid", "ok"),
    }
}

/// "1 payment", "1,204 payments"
fn plural(n: u64, unit: &str) -> String {
    format!("{} {unit}{}", group(n), if n == 1 { "" } else { "s" })
}

/// "$59.88"
pub fn usd(v: f64) -> String {
    let sign = if v < 0.0 { "-" } else { "" };
    let c = (v.abs() * 100.0).round() as u64;
    format!("{sign}${}.{:02}", group(c / 100), c % 100)
}

/// A payment's USD amount, or its own currency when it has no conversion.
fn amount(p: &PaymentRow) -> String {
    p.amount_usd
        .map_or_else(|| native(p.amount, &p.currency), usd)
}

pub fn provider_short(provider: &str) -> &'static str {
    match provider {
        "dodo" => "Dodo",
        other => provider_name(other),
    }
}

/// `base` with `params`, overridden by `set`; empty values are dropped.
fn link(base: &str, params: &[(&str, &str)], set: &[(&str, &str)]) -> String {
    let mut all: Vec<(&str, &str)> = params
        .iter()
        .filter(|(k, _)| !set.iter().any(|(s, _)| s == k))
        .copied()
        .collect();
    all.extend_from_slice(set);
    href(base, &all)
}

fn delta(cur: Option<f64>, prev: Option<f64>) -> (&'static str, String) {
    match (cur, prev) {
        (Some(c), Some(p)) if p > 0.0 => {
            let pct = (c - p) / p * 100.0;
            let text = if pct.abs() >= 10.0 {
                format!("{:+.0}%", pct)
            } else {
                format!("{:+.1}%", pct)
            };
            let trend = if pct > 0.05 {
                "up"
            } else if pct < -0.05 {
                "down"
            } else {
                "flat"
            };
            (trend, text)
        }
        _ => ("", String::new()),
    }
}

pub struct Tile {
    name: &'static str,
    short: &'static str,
    value: String,
    trend: &'static str,
    delta: String,
    sub: String,
    href: String,
    on: bool,
    color: &'static str,
    big: bool,
}

pub struct KindView {
    label: &'static str,
    icon: &'static str,
    href: String,
    on: bool,
}

pub struct RowView {
    icon: String,
    label: String,
    mono: bool,
    logos: String,
    visitors: String,
    revenue: String,
    zero: bool,
    per_visitor: String,
    width: f64,
}

pub struct Card {
    key: &'static str,
    title: &'static str,
    head: &'static str,
    by_visitors: bool,
    revenue_href: String,
    visitors_href: String,
    rows: Vec<RowView>,
    unattributed: Option<RowView>,
    foot: String,
    foot_href: String,
    foot_link: &'static str,
}

pub struct PayView {
    date: String,
    icon: String,
    source: String,
    channel: String,
    unattributed: bool,
    amount: String,
    refunded: bool,
    status: &'static str,
    status_class: &'static str,
    provider_icon: String,
    provider: &'static str,
    href: String,
}

pub struct Step {
    done: bool,
    icon: String,
    title: String,
    sub: String,
}

pub struct EmptyView {
    domain: String,
    script_host: String,
    cookieless: bool,
    checks: Vec<Step>,
    providers: Vec<(String, &'static str, bool)>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardPage {
    shell: Shell,
    title: &'static str,
    range_label: &'static str,
    ranges: Vec<Opt>,
    empty: Option<EmptyView>,
    tiles: Vec<Tile>,
    chart_title: String,
    chart_sub: String,
    kinds: Vec<KindView>,
    kind_label: &'static str,
    chart: String,
    cards: Vec<Card>,
    latest: Vec<PayView>,
    latest_total: u64,
    payments_href: String,
    steps: Vec<Step>,
    steps_done: usize,
    steps_title: &'static str,
    goals_card: bool,
}

#[derive(Deserialize, Default)]
pub struct ReportQuery {
    range: Option<String>,
    filter: Option<String>,
    page: Option<u32>,
}

#[derive(Deserialize, Default)]
pub struct DashboardQuery {
    range: Option<String>,
    metric: Option<String>,
    chart: Option<String>,
    all: Option<String>,
    channels: Option<String>,
    referrers: Option<String>,
    pages: Option<String>,
    countries: Option<String>,
}

const KIND_ICONS: [&str; 4] = [
    r#"<path d="M3 17l5-6 4 3 5-7 4 4"/>"#,
    r#"<path d="M3 17l5-6 4 3 5-7 4 4v9H3z" fill="currentColor" fill-opacity="0.18"/>"#,
    r#"<path d="M5 20v-6M5 11V9M12 20v-9M12 8V4M19 20v-4M19 13v-3"/>"#,
    r#"<path d="M5 20v-5M10 20v-8M15 20v-6M20 20v-10"/><path d="M3 9l5-3 5 4 8-6"/>"#,
];

fn source_of(p: &PaymentRow) -> (String, String) {
    match &p.touch {
        Some(t) => (t.source.clone(), t.channel.clone()),
        None => ("Unattributed".into(), String::new()),
    }
}

pub fn pay_view(p: &PaymentRow, site_id: &str, offset: UtcOffset) -> PayView {
    let (status, status_class) = status(&p.status);
    let (source, channel) = source_of(p);
    PayView {
        date: local(p.ts, offset),
        icon: icons::source(&source, 16),
        channel: if channel == source {
            String::new()
        } else {
            channel
        },
        unattributed: p.touch.is_none(),
        source,
        amount: amount(p),
        refunded: p.status == "refunded",
        status,
        status_class,
        provider_icon: icons::provider(&p.provider, 14),
        provider: provider_short(&p.provider),
        href: format!(
            "/app/sites/{site_id}/payments/{}/{}",
            p.provider, p.provider_payment_id
        ),
    }
}

/// Which breakdown a card shows and how its rows are labelled.
#[derive(Clone, Copy, PartialEq)]
enum CardKind {
    Channel,
    Referrer,
    Page,
    Country,
}

fn row_view(
    kind: CardKind,
    r: &BreakdownRow,
    max: f64,
    by_visitors: bool,
    sources: &[(String, Vec<String>)],
) -> RowView {
    let value = if by_visitors {
        r.visitors as f64
    } else {
        r.revenue_usd
    };
    let key = r.key.as_deref().unwrap_or_default();
    let (icon, label) = match kind {
        CardKind::Channel => (
            format!(
                r#"<i class="dot" style="background:{}"></i>"#,
                chart::channel_color(r.key.as_deref())
            ),
            r.label.clone(),
        ),
        CardKind::Referrer => (icons::source(&r.label, 16), r.label.clone()),
        CardKind::Page => (String::new(), r.label.clone()),
        CardKind::Country if r.key.is_some() => (icons::flag(key), icons::country(key)),
        CardKind::Country => (String::new(), "Unknown".into()),
    };
    let logos = if kind == CardKind::Channel {
        sources
            .iter()
            .find(|(c, _)| c == key)
            .map(|(_, names)| {
                names
                    .iter()
                    .filter_map(|n| icons::brand_slug(n))
                    .map(|s| icons::brand(s, 12))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    RowView {
        icon,
        label,
        mono: kind == CardKind::Page,
        logos,
        visitors: group(r.visitors),
        revenue: money(r.revenue_usd),
        zero: r.revenue_usd <= 0.0,
        per_visitor: usd(r.revenue_per_visitor.unwrap_or(0.0)),
        width: if max > 0.0 { value / max * 100.0 } else { 0.0 },
    }
}

#[allow(clippy::too_many_arguments)]
fn card(
    kind: CardKind,
    rows: &[BreakdownRow],
    sources: &[(String, Vec<String>)],
    params: &[(&str, &str)],
    base: &str,
    by_visitors: bool,
    expanded: bool,
    foot: Option<String>,
) -> Card {
    let (key, title, head) = match kind {
        CardKind::Channel => ("channels", "Channels", "Channel"),
        CardKind::Referrer => ("referrers", "Referrers", "Referrer"),
        CardKind::Page => ("pages", "Landing pages", "Page"),
        CardKind::Country => ("countries", "Countries", "Country"),
    };
    let mut shown: Vec<BreakdownRow> = rows
        .iter()
        .filter(|r| !r.unattributed && !(kind == CardKind::Referrer && r.key.is_none()))
        .cloned()
        .collect();
    sort_rows(
        &mut shown,
        if by_visitors {
            Sort::Visitors
        } else {
            Sort::Revenue
        },
    );
    let total = shown.len();
    shown.truncate(if expanded { 50 } else { 6 });
    let unattributed = rows
        .iter()
        .find(|r| r.unattributed && kind == CardKind::Channel);
    let max = shown
        .iter()
        .chain(unattributed)
        .map(|r| {
            if by_visitors {
                r.visitors as f64
            } else {
                r.revenue_usd
            }
        })
        .fold(0.0, f64::max);
    let (foot, foot_href, foot_link) = match foot {
        Some(f) => (f, String::new(), ""),
        None if expanded => (
            format!("All {total}"),
            link(base, params, &[("all", "")]),
            "Show fewer",
        ),
        None if total > shown.len() => (
            format!("Top {} of {}", shown.len(), group(total as u64)),
            link(base, params, &[("all", key)]),
            "View all",
        ),
        None => (
            format!("{total} {}", if total == 1 { "row" } else { "rows" }),
            String::new(),
            "",
        ),
    };
    Card {
        key,
        title,
        head,
        by_visitors,
        revenue_href: link(base, params, &[(key, "")]),
        visitors_href: link(base, params, &[(key, "visitors")]),
        rows: shown
            .iter()
            .map(|r| row_view(kind, r, max, by_visitors, sources))
            .collect(),
        unattributed: unattributed.map(|r| RowView {
            visitors: "n/a".into(),
            per_visitor: "n/a".into(),
            ..row_view(kind, r, max, by_visitors, sources)
        }),
        foot,
        foot_href,
        foot_link,
    }
}

fn span(p: &Period) -> String {
    format!("{} to {}", day(p.from), day(p.to))
}

pub async fn dashboard(
    State(state): State<AppState>,
    account: Account,
    Path(site_id): Path<String>,
    Query(q): Query<DashboardQuery>,
) -> Response {
    let ctx = match load(&state, &account, &site_id, "dashboard").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let (rkey, rlabel, days) = range(q.range.as_deref());
    let p = period(&ctx, days);
    let settings: Result<(String, i64), _> =
        sqlx::query_as("SELECT attribution_model, window_days FROM sites WHERE id = ?")
            .bind(&ctx.site.id)
            .fetch_one(&state.db)
            .await;
    let providers: Result<Vec<String>, _> = sqlx::query_scalar(
        "SELECT provider FROM integrations WHERE site_id = ? ORDER BY created_at",
    )
    .bind(&ctx.site.id)
    .fetch_all(&state.db)
    .await;
    let (Ok((model, window_days)), Ok(providers)) = (settings, providers) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let store = state.events.clone();
    let (id, period_q) = (ctx.site.id.clone(), p.clone());
    let result = blocking(move || {
        Ok((
            store.report(&id, &period_q)?,
            store.payment_list(&id, &period_q, PaymentFilter::All, 50, 0)?,
        ))
    })
    .await;
    let (report, recent) = match result {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let base = format!("/app/sites/{}/dashboard", ctx.site.id);
    let site_base = format!("/app/sites/{}", ctx.site.id);
    let setup = &report.setup;
    let since = setup.first_event.map(|t| t.to_offset(ctx.offset).date());
    let new_site = since.is_none_or(|d| d > p.previous().from);
    let age = since.map_or(0, |d| (ctx.today - d).whole_days());
    let young = new_site && age <= 30;
    let metric = q
        .metric
        .as_deref()
        .and_then(chart::Metric::parse)
        .unwrap_or(chart::Metric::Revenue);
    let default_kind = if young {
        chart::Kind::Dual
    } else {
        chart::Kind::Line
    };
    let kind = q
        .chart
        .as_deref()
        .and_then(chart::Kind::parse)
        .unwrap_or(default_kind);
    let sort_of = |v: &Option<String>| v.as_deref() == Some("visitors");
    let expanded = q.all.clone().unwrap_or_default();
    let range_param = if rkey == "30d" { "" } else { rkey };
    let metric_param = if metric == chart::Metric::Revenue {
        ""
    } else {
        metric.key()
    };
    let kind_param = if kind == default_kind { "" } else { kind.key() };
    let sorts: Vec<(&str, &str)> = [
        ("channels", &q.channels),
        ("referrers", &q.referrers),
        ("pages", &q.pages),
        ("countries", &q.countries),
    ]
    .into_iter()
    .filter(|(_, v)| sort_of(v))
    .map(|(k, _)| (k, "visitors"))
    .collect();
    let mut params: Vec<(&str, &str)> = vec![
        ("range", range_param),
        ("metric", metric_param),
        ("chart", kind_param),
        ("all", &expanded),
    ];
    params.extend(&sorts);

    let cur = &report.overview.current;
    let prev = &report.overview.previous;
    let lit = |m: chart::Metric| match kind {
        chart::Kind::Channel => m == chart::Metric::Revenue,
        chart::Kind::Dual => matches!(m, chart::Metric::Revenue | chart::Metric::Visitors),
        _ => m == metric,
    };
    let tile_kind = match kind {
        chart::Kind::Area => "area",
        _ if young => "line",
        _ => "",
    };
    let tiles = chart::Metric::ALL
        .into_iter()
        .map(|m| {
            let (value, (trend, delta), sub) = match m {
                chart::Metric::Revenue => (
                    money(cur.revenue_usd),
                    delta(Some(cur.revenue_usd), Some(prev.revenue_usd)),
                    format!(
                        "{} from {}",
                        plural(cur.payments, "payment"),
                        plural(cur.customers, "customer")
                    ),
                ),
                chart::Metric::Visitors => (
                    group(cur.visitors),
                    delta(Some(cur.visitors as f64), Some(prev.visitors as f64)),
                    match since {
                        Some(s) if new_site && s >= p.from => {
                            format!("{} since {}", plural(cur.sessions, "session"), day(s))
                        }
                        _ => plural(cur.sessions, "session"),
                    },
                ),
                chart::Metric::Conversion => (
                    format!("{:.2}%", cur.conversion.unwrap_or(0.0) * 100.0),
                    delta(cur.conversion, prev.conversion),
                    if cur.visitors > 0 && cur.visitors < 100 {
                        format!(
                            "{} of {} visitors. Small sample.",
                            group(cur.customers),
                            group(cur.visitors)
                        )
                    } else {
                        "Visitors who paid".into()
                    },
                ),
                chart::Metric::PerVisitor => (
                    usd(cur.revenue_per_visitor.unwrap_or(0.0)),
                    delta(cur.revenue_per_visitor, prev.revenue_per_visitor),
                    "Revenue divided by visitors".into(),
                ),
            };
            let (trend, delta) = if new_site {
                if m == chart::Metric::Revenue {
                    let label = if young { "New site" } else { "No earlier data" };
                    ("flat", label.to_string())
                } else {
                    ("", String::new())
                }
            } else {
                (trend, delta)
            };
            let m_param = if m == chart::Metric::Revenue {
                ""
            } else {
                m.key()
            };
            Tile {
                name: m.name(),
                short: match m {
                    chart::Metric::Conversion => "Conversion",
                    chart::Metric::PerVisitor => "Per visitor",
                    _ => m.name(),
                },
                value,
                trend,
                delta,
                sub,
                href: link(&base, &params, &[("metric", m_param), ("chart", tile_kind)]),
                on: lit(m),
                color: match m {
                    chart::Metric::Revenue => "#E8471C",
                    chart::Metric::Visitors => "#2F5BEA",
                    _ => "#0E1116",
                },
                big: m == chart::Metric::Revenue,
            }
        })
        .collect();

    let current = chart::series(&report.days);
    let previous = chart::series(&report.previous_days);
    let unit = if current.weekly { "week" } else { "day" };
    let sparse = young && kind == chart::Kind::Dual;
    let (chart_title, chart_sub, chart_html) = match kind {
        chart::Kind::Line | chart::Kind::Area => {
            let area = kind == chart::Kind::Area;
            let compare = !area && !new_site;
            let sub = if compare {
                format!(
                    "{}, compared with the previous {} days",
                    span(&p),
                    days.min(365)
                )
            } else if new_site {
                format!(
                    "{}. Comparisons start once there is a full previous period",
                    span(&p)
                )
            } else {
                span(&p)
            };
            (
                format!("{} per {unit}", metric.name()),
                sub,
                chart::line(
                    &current,
                    compare.then_some(&previous),
                    metric,
                    area,
                    &span(&p),
                ),
            )
        }
        chart::Kind::Channel => (
            format!("Revenue per {unit} by channel"),
            "Which channel each payment is credited to".into(),
            chart::stacked(&current),
        ),
        chart::Kind::Dual if sparse => {
            let mut pills: Vec<(Date, String)> = Vec::new();
            let mut by_day: Vec<(Date, f64, u64, String)> = Vec::new();
            for r in &recent.rows {
                let d = r.ts.to_offset(ctx.offset).date();
                let usd_amount = r.net_usd.unwrap_or(0.0);
                match by_day.iter_mut().find(|b| b.0 == d) {
                    Some(b) => {
                        b.1 += usd_amount;
                        b.2 += 1;
                    }
                    None => by_day.push((d, usd_amount, 1, source_of(r).0)),
                }
            }
            for (d, total, n, source) in by_day {
                let text = if n == 1 {
                    format!(
                        r#"<span class="pill-logo">{}</span>{} from {}"#,
                        icons::source(&source, 12),
                        money(total),
                        chart::esc(&source)
                    )
                } else {
                    format!("{} from {n} payments", money(total))
                };
                pills.push((d, text));
            }
            (
                format!("Visitors and payments per {unit}"),
                "Comparisons start once there is a full previous period".into(),
                chart::sparse(&current, since, ctx.today, &pills),
            )
        }
        chart::Kind::Dual => (
            format!("Visitors and revenue per {unit}"),
            "Visitors on the left axis, revenue on the right".into(),
            chart::dual(&current),
        ),
    };
    let kinds = chart::Kind::ALL
        .into_iter()
        .zip(KIND_ICONS)
        .map(|(k, icon)| KindView {
            label: k.label(),
            icon,
            href: link(
                &base,
                &params,
                &[("chart", if k == default_kind { "" } else { k.key() })],
            ),
            on: k == kind,
        })
        .collect();

    let soon = new_site.then(|| "Rows fill in as traffic grows".to_string());
    let credit = format!(
        "Credited to the {} within {} days",
        if model == "first_touch" {
            "first visit"
        } else {
            "last non-direct visit"
        },
        window_days
    );
    let card_for = |kind: CardKind, rows: &[BreakdownRow], key: &str, foot: Option<String>| {
        card(
            kind,
            rows,
            &report.channel_sources,
            &params,
            &base,
            sorts.iter().any(|(k, _)| *k == key),
            expanded == key,
            foot,
        )
    };
    let cards = vec![
        card_for(
            CardKind::Channel,
            &report.channels,
            "channels",
            Some(soon.clone().unwrap_or(credit)),
        ),
        card_for(
            CardKind::Referrer,
            &report.referrers,
            "referrers",
            soon.clone(),
        ),
        card_for(CardKind::Page, &report.landing_pages, "pages", soon.clone()),
        card_for(CardKind::Country, &report.countries, "countries", soon),
    ];

    let provider_label = |p: &str| provider_name(p).to_string();
    let empty = (setup.first_event.is_none() && setup.payments == 0).then(|| EmptyView {
        domain: ctx.site.domain.clone(),
        script_host: state.config.base_url.clone(),
        cookieless: ctx.site.cookieless,
        checks: vec![
            Step {
                done: false,
                icon: icons::glyph("direct", 14, "#0E1116"),
                title: "Script tag".into(),
                sub: "Not seen yet".into(),
            },
            match providers.first() {
                Some(pr) => Step {
                    done: true,
                    icon: icons::provider(pr, 14),
                    title: provider_label(pr),
                    sub: "Connected".into(),
                },
                None => Step {
                    done: false,
                    icon: icons::glyph("card", 14, "#0E1116"),
                    title: "Payments".into(),
                    sub: "Not connected".into(),
                },
            },
            Step {
                done: false,
                icon: icons::glyph("link", 14, "#0E1116"),
                title: "Checkout link".into(),
                sub: "Passes virling_vid".into(),
            },
        ],
        providers: ["dodo", "stripe", "polar", "lemonsqueezy"]
            .into_iter()
            .map(|pr| {
                (
                    icons::provider(pr, 12),
                    provider_name(pr),
                    providers.iter().any(|c| c == pr),
                )
            })
            .collect(),
    });
    let steps = if young && age <= 14 && empty.is_none() {
        let matched = setup.first_match.as_ref();
        vec![
            Step {
                done: since.is_some(),
                icon: icons::glyph("direct", 14, "#0E1116"),
                title: "Script installed".into(),
                sub: since.map(day).unwrap_or_else(|| "Not seen yet".into()),
            },
            match providers.first() {
                Some(pr) => Step {
                    done: true,
                    icon: icons::provider(pr, 14),
                    title: format!("{} connected", provider_short(pr)),
                    sub: format!(
                        "{} payment{}",
                        group(setup.payments),
                        if setup.payments == 1 { "" } else { "s" }
                    ),
                },
                None => Step {
                    done: false,
                    icon: icons::glyph("card", 14, "#8A909A"),
                    title: "Connect payments".into(),
                    sub: "Stripe, Polar, Dodo, Lemon Squeezy".into(),
                },
            },
            match matched {
                Some(m) => {
                    let source = source_of(m).0;
                    Step {
                        done: true,
                        icon: icons::source(&source, 14),
                        title: "Payment matched".into(),
                        sub: format!("{} from {}", amount(m), source),
                    }
                }
                None => Step {
                    done: false,
                    icon: icons::glyph("link", 14, "#8A909A"),
                    title: "Payment matched".into(),
                    sub: "Pass virling_vid at checkout".into(),
                },
            },
            Step {
                done: setup.has_goals,
                icon: icons::glyph("unattributed", 14, "#8A909A"),
                title: "Track a goal".into(),
                sub: "Signups, trials".into(),
            },
        ]
    } else {
        Vec::new()
    };
    render(DashboardPage {
        title: "Dashboard",
        range_label: rlabel,
        ranges: range_opts(&base, rkey, &[]),
        empty,
        tiles,
        chart_title,
        chart_sub,
        kinds,
        kind_label: kind.label(),
        chart: chart_html,
        cards,
        latest: recent
            .rows
            .iter()
            .take(5)
            .map(|r| pay_view(r, &ctx.site.id, ctx.offset))
            .collect(),
        latest_total: recent.total,
        payments_href: href(&format!("{site_base}/payments"), &[("range", range_param)]),
        steps_done: steps.iter().filter(|s| s.done).count(),
        steps,
        goals_card: young && !setup.has_goals,
        steps_title: if age <= 7 {
            "First week"
        } else {
            "First two weeks"
        },
        shell: ctx.shell,
    })
}

pub struct PaymentView {
    date: String,
    amount: String,
    refunded: String,
    source: String,
    unattributed: bool,
    renewal: bool,
    provider: &'static str,
    status: &'static str,
    status_class: &'static str,
    href: String,
}

#[derive(Template)]
#[template(path = "payments.html")]
struct PaymentsPage {
    shell: Shell,
    title: &'static str,
    range_label: &'static str,
    ranges: Vec<Opt>,
    filters: Vec<Opt>,
    rows: Vec<PaymentView>,
    filter_label: &'static str,
    prev: String,
    next: String,
    showing: String,
}

fn payment_view(p: &PaymentRow, site_id: &str, offset: UtcOffset) -> PaymentView {
    let (status, status_class) = status(&p.status);
    PaymentView {
        date: local(p.ts, offset),
        amount: native(p.amount, &p.currency),
        refunded: if p.refunded > 0 && p.refunded < p.amount {
            format!("{} refunded", native(p.refunded, &p.currency))
        } else {
            String::new()
        },
        source: p
            .touch
            .as_ref()
            .map_or_else(|| "Unattributed".into(), |t| t.source.clone()),
        unattributed: p.touch.is_none(),
        renewal: p.is_renewal,
        provider: provider_name(&p.provider),
        status,
        status_class,
        href: format!(
            "/app/sites/{site_id}/payments/{}/{}",
            p.provider, p.provider_payment_id
        ),
    }
}

pub async fn payments(
    State(state): State<AppState>,
    account: Account,
    Path(site_id): Path<String>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let ctx = match load(&state, &account, &site_id, "payments").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let (rkey, rlabel, days) = range(q.range.as_deref());
    let p = period(&ctx, days);
    let (fkey, flabel, filter) = FILTERS
        .into_iter()
        .find_map(|(k, label)| {
            (Some(k) == q.filter.as_deref()).then(|| Some((k, label, PaymentFilter::parse(k)?)))?
        })
        .unwrap_or(("all", "All", PaymentFilter::All));
    let page = q.page.unwrap_or(1).max(1);
    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let result = blocking(move || {
        let s = store.payment_summary(&id, &p)?;
        let counts = [s.payments, s.attributed, s.unattributed, s.refunded];
        let list = store.payment_list(&id, &p, filter, PER_PAGE, (page - 1) * PER_PAGE)?;
        Ok((counts, list))
    })
    .await;
    let (counts, list) = match result {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let base = format!("/app/sites/{}/payments", ctx.site.id);
    let range_param = if rkey == "30d" { "" } else { rkey };
    let filter_param = if fkey == "all" { "" } else { fkey };
    let page_href = |n: u32| {
        let n = n.to_string();
        href(
            &base,
            &[
                ("range", range_param),
                ("filter", filter_param),
                ("page", if n == "1" { "" } else { &n }),
            ],
        )
    };
    let shown = list.rows.len() as u64;
    let first = (page as u64 - 1) * PER_PAGE as u64;
    render(PaymentsPage {
        title: "Payments",
        range_label: rlabel,
        ranges: range_opts(&base, rkey, &[("filter", filter_param)]),
        filters: FILTERS
            .iter()
            .zip(&counts)
            .map(|((k, label), n)| Opt {
                label: label.to_string(),
                href: href(
                    &base,
                    &[
                        ("range", range_param),
                        ("filter", if *k == "all" { "" } else { k }),
                    ],
                ),
                on: *k == fkey,
                count: group(*n),
            })
            .collect(),
        rows: list
            .rows
            .iter()
            .map(|p| payment_view(p, &ctx.site.id, ctx.offset))
            .collect(),
        filter_label: flabel,
        prev: if page > 1 {
            page_href(page - 1)
        } else {
            String::new()
        },
        next: if first + shown < list.total {
            page_href(page + 1)
        } else {
            String::new()
        },
        showing: if list.total > shown {
            format!(
                "{}–{} of {}",
                group(first + 1),
                group(first + shown),
                group(list.total)
            )
        } else {
            String::new()
        },
        shell: ctx.shell,
    })
}

pub struct SessionView {
    date: String,
    source: String,
    campaign: Option<String>,
    paths: Vec<String>,
    more: usize,
    goals: Vec<String>,
    credited: bool,
}

pub struct JourneyView {
    amount: String,
    usd: String,
    provider: &'static str,
    payment_id: String,
    date: String,
    status: &'static str,
    status_class: &'static str,
    unattributed: bool,
    credited: String,
    first_touch: String,
    time_to_pay: String,
    sessions: Vec<SessionView>,
    checkout_href: String,
}

fn duration(d: Duration) -> String {
    let plural = |n: i64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    if d.whole_days() >= 1 {
        plural(d.whole_days(), "day")
    } else if d.whole_hours() >= 1 {
        plural(d.whole_hours(), "hour")
    } else {
        plural(d.whole_minutes().max(1), "minute")
    }
}

fn journey_view(j: Journey, site_id: &str, offset: UtcOffset) -> JourneyView {
    let p = &j.payment;
    let (status, status_class) = status(&p.status);
    let first = j.sessions.first();
    JourneyView {
        amount: native(p.amount, &p.currency),
        usd: match p.amount_usd {
            Some(usd) if p.currency != "usd" => format!("about {}", money(usd)),
            _ => String::new(),
        },
        provider: provider_name(&p.provider),
        payment_id: p.provider_payment_id.clone(),
        date: local(p.ts, offset),
        status,
        status_class,
        unattributed: p.touch.is_none(),
        credited: p
            .touch
            .as_ref()
            .map_or_else(|| "Unattributed".into(), |t| t.source.clone()),
        first_touch: first.map(|s| s.source.clone()).unwrap_or_default(),
        time_to_pay: first.map(|s| duration(p.ts - s.ts)).unwrap_or_default(),
        checkout_href: format!("/app/sites/{site_id}/setup/checkout"),
        sessions: j
            .sessions
            .iter()
            .map(|s| SessionView {
                date: local(s.ts, offset),
                source: s.source.clone(),
                campaign: s.campaign.clone(),
                paths: s.paths.iter().take(8).cloned().collect(),
                more: s.paths.len().saturating_sub(8),
                goals: s.goals.clone(),
                credited: s.credited,
            })
            .collect(),
    }
}

#[derive(Template)]
#[template(path = "_journey.html")]
struct JourneyFragment {
    j: JourneyView,
}

#[derive(Template)]
#[template(path = "payment.html")]
struct PaymentDetailPage {
    shell: Shell,
    title: &'static str,
    back: String,
    j: JourneyView,
}

pub async fn payment(
    State(state): State<AppState>,
    account: Account,
    Path((site_id, provider, payment_id)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let ctx = match load(&state, &account, &site_id, "payments").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let window_days: i64 = match sqlx::query_scalar("SELECT window_days FROM sites WHERE id = ?")
        .bind(&ctx.site.id)
        .fetch_one(&state.db)
        .await
    {
        Ok(w) => w,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let journey =
        match blocking(move || store.journey(&id, &provider, &payment_id, window_days)).await {
            Ok(Some(j)) => j,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(code) => return code.into_response(),
        };
    let j = journey_view(journey, &ctx.site.id, ctx.offset);
    if headers.contains_key("x-drawer") {
        return render(JourneyFragment { j });
    }
    render(PaymentDetailPage {
        title: "Payment",
        back: format!("/app/sites/{}/payments", ctx.site.id),
        j,
        shell: ctx.shell,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_money_and_counts() {
        assert_eq!(group(1234567), "1,234,567");
        assert_eq!(group(12), "12");
        assert_eq!(money(12480.4), "$12,480");
        assert_eq!(money(48.5), "$48.50");
        assert_eq!(money(480.0), "$480");
        assert_eq!(native(2900, "eur"), "€29");
        assert_eq!(native(123456, "sek"), "1,235 SEK");
        assert_eq!(money(701.52), "$702");
        assert_eq!(native(500, "jpy"), "¥500");
    }
}
