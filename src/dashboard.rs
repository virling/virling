use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime, UtcOffset};

use crate::app::AppState;
use crate::auth::Account;
use crate::fx::minor_to_major;
use crate::onboarding::render;
use crate::settings::{self, Brand};
use crate::sites::{self, Site};
use crate::stats::{
    BreakdownRow, Journey, PaymentFilter, PaymentRow, Period, Sort, Totals, sort_rows,
};
use crate::{chart, favicon, icons, visitor_pages, visitors};

const RANGES: [(&str, &str, i64); 6] = [
    ("today", "Today", 1),
    ("7d", "Last 7 days", 7),
    ("30d", "Last 30 days", 30),
    ("90d", "Last 90 days", 90),
    ("12m", "Last 12 months", 365),
    ("all", "Since tracking started", 0),
];
const FILTERS: [(&str, &str); 4] = [
    ("all", "All"),
    ("attributed", "Attributed"),
    ("unattributed", "Unattributed"),
    ("refunded", "Refunded"),
];
pub(crate) const PER_PAGE: u32 = 50;

/// Sidebar and top bar data shared by every app page.
pub struct Shell {
    pub domain: String,
    pub initial: String,
    /// URL of the cached favicon, empty until one is stored.
    pub icon: String,
    pub sites: Vec<SiteOpt>,
    pub nav: &'static str,
    pub plan: String,
    pub usage: String,
    pub email: String,
    pub avatar: String,
    pub live: u64,
}

/// A site in the switcher.
pub struct SiteOpt {
    pub label: String,
    pub href: String,
    pub on: bool,
    pub initial: String,
    pub icon: String,
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
    /// Local date of the site's first event.
    pub first_day: Option<Date>,
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
    domain: &str,
    nav: &'static str,
) -> Result<Ctx, StatusCode> {
    let all = sites::for_account(&state.db, &account.id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let Some(site) = all.iter().find(|s| s.domain == domain).cloned() else {
        return Err(StatusCode::NOT_FOUND);
    };
    let (plan, email): (String, String) =
        sqlx::query_as("SELECT plan, email FROM accounts WHERE id = ?")
            .bind(&account.id)
            .fetch_one(&state.db)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let store = state.events.clone();
    let (id, tz) = (site.id.clone(), site.timezone.clone());
    let (offset, live, visitors, first) = blocking(move || {
        let offset = store.utc_offset(&tz)?;
        let today = OffsetDateTime::now_utc().to_offset(offset).date();
        let period = Period {
            from: today - Duration::days(29),
            to: today,
            tz,
        };
        let visitors = store.overview(&id, &period)?.current.visitors;
        Ok((
            offset,
            store.live_visitors(&id)?,
            visitors,
            store.first_event(&id)?,
        ))
    })
    .await?;
    let icons: Vec<(String, i64, bool)> = sqlx::query_as(
        "SELECT i.site_id, i.fetched_at, i.body IS NOT NULL FROM site_icons i
         JOIN sites s ON s.id = i.site_id WHERE s.account_id = ?",
    )
    .bind(&account.id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let icon_of = |s: &Site| {
        icons
            .iter()
            .find(|i| i.0 == s.id && i.2)
            .map(|i| format!("{}?v={}", sites::app_path(&s.domain, "icon"), i.1))
            .unwrap_or_default()
    };
    let fetched_at = icons.iter().find(|i| i.0 == site.id).map(|i| i.1);
    favicon::refresh(state, &site.id, &site.domain, fetched_at);
    let shell = Shell {
        domain: site.domain.clone(),
        initial: site.domain[..1].to_ascii_uppercase(),
        icon: icon_of(&site),
        sites: all
            .iter()
            .map(|s| SiteOpt {
                label: s.domain.clone(),
                href: sites::app_path(&s.domain, nav),
                on: s.id == site.id,
                initial: s.domain[..1].to_ascii_uppercase(),
                icon: icon_of(s),
            })
            .collect(),
        nav,
        plan: match plan.as_str() {
            "self_hosted" => "Self-hosted".into(),
            other => other.replace('_', " "),
        },
        usage: format!("{} in the last 30 days", plural(visitors, "visitor")),
        avatar: initials(&email),
        email,
        live,
    };
    Ok(Ctx {
        site,
        shell,
        today: OffsetDateTime::now_utc().to_offset(offset).date(),
        first_day: first.map(|t| t.to_offset(offset).date()),
        offset,
    })
}

/// Up to two initials from the local part of an email ("maya.chen@x.com" -> "MC").
fn initials(email: &str) -> String {
    email
        .split('@')
        .next()
        .unwrap_or_default()
        .split(['.', '_', '-', '+'])
        .filter_map(|p| p.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// The active range as (key, label, days, default key); sites under 30 days old default to "all".
pub(crate) fn range(
    ctx: &Ctx,
    key: Option<&str>,
) -> (&'static str, &'static str, i64, &'static str) {
    let tracked = ctx.first_day.map(|d| (ctx.today - d).whole_days() + 1);
    let default = if tracked.is_some_and(|n| n <= 30) {
        "all"
    } else {
        "30d"
    };
    let find = |k: &str| RANGES.into_iter().find(|r| r.0 == k);
    let (k, label, days) = key
        .and_then(find)
        .filter(|r| r.0 != "all" || tracked.is_some())
        .or_else(|| find(default))
        .unwrap_or(RANGES[2]);
    let days = if k == "all" {
        tracked.unwrap_or(30)
    } else {
        days
    };
    (k, label, days, default)
}

pub(crate) fn period(ctx: &Ctx, days: i64) -> Period {
    Period {
        from: ctx.today - Duration::days(days - 1),
        to: ctx.today,
        tz: ctx.site.timezone.clone(),
    }
}

/// `base` plus the non-empty query params.
pub(crate) fn href(base: &str, params: &[(&str, &str)]) -> String {
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

pub(crate) fn range_opts(
    base: &str,
    current: &str,
    default: &str,
    params: &[(&str, &str)],
) -> Vec<Opt> {
    RANGES
        .iter()
        .map(|(key, label, _)| Opt {
            label: label.to_string(),
            href: link(
                base,
                params,
                &[("range", if *key == default { "" } else { key })],
            ),
            on: *key == current,
            count: String::new(),
        })
        .collect()
}

/// "0 of 12 visitors converted. 2 payments not matched to a visit."
fn conversion_sub(t: &Totals) -> String {
    let mut sub = format!(
        "{} of {} converted",
        group(t.converted),
        plural(t.visitors, "visitor")
    );
    let unmatched = t.payments.saturating_sub(t.matched_payments);
    if unmatched > 0 {
        sub.push_str(&format!(
            ". {} not matched to a visit",
            plural(unmatched, "payment")
        ));
    }
    sub
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

pub(crate) fn local(ts: OffsetDateTime, offset: UtcOffset) -> String {
    ts.to_offset(offset)
        .format(format_description!(
            "[month repr:short] [day padding:none], [hour]:[minute]"
        ))
        .unwrap_or_default()
}

pub(crate) fn day(d: Date) -> String {
    d.format(format_description!("[month repr:short] [day padding:none]"))
        .unwrap_or_default()
}

pub(crate) fn status(status: &str) -> (&'static str, &'static str) {
    match status {
        "refunded" => ("Refunded", "ref"),
        "partially_refunded" => ("Partly refunded", "warn"),
        "failed" => ("Failed", "bad"),
        _ => ("Paid", "ok"),
    }
}

/// "1 payment", "1,204 payments"
pub(crate) fn plural(n: u64, unit: &str) -> String {
    format!("{} {unit}{}", group(n), if n == 1 { "" } else { "s" })
}

/// "$59.88"
pub fn usd(v: f64) -> String {
    let sign = if v < 0.0 { "-" } else { "" };
    let c = (v.abs() * 100.0).round() as u64;
    format!("{sign}${}.{:02}", group(c / 100), c % 100)
}

/// A payment's USD amount, or its own currency when it has no conversion.
pub(crate) fn amount(p: &PaymentRow) -> String {
    p.amount_usd
        .map_or_else(|| native(p.amount, &p.currency), usd)
}

pub fn provider_short(provider: &str) -> &'static str {
    match provider {
        "dodo" => "Dodo",
        other => provider_name(other),
    }
}

/// `base` with `params`, overridden in place by `set`; empty values are dropped.
pub(crate) fn link(base: &str, params: &[(&str, &str)], set: &[(&str, &str)]) -> String {
    let mut all: Vec<(&str, &str)> = params
        .iter()
        .map(|&(k, v)| set.iter().copied().find(|(s, _)| *s == k).unwrap_or((k, v)))
        .collect();
    all.extend(
        set.iter()
            .filter(|(s, _)| !params.iter().any(|(k, _)| k == s)),
    );
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
    renewal: bool,
    first_icon: String,
    first: String,
    amount: String,
    refunded: bool,
    status: &'static str,
    status_class: &'static str,
    provider_icon: String,
    provider: &'static str,
    href: String,
    visitor: String,
    visitor_href: String,
}

pub struct Step {
    done: bool,
    icon: String,
    title: String,
    sub: String,
    /// Where an unfinished step is completed.
    href: String,
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
    dismiss_href: String,
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
    #[serde(alias = "channels")]
    sort_channels: Option<String>,
    #[serde(alias = "referrers")]
    sort_referrers: Option<String>,
    #[serde(alias = "pages")]
    sort_pages: Option<String>,
    #[serde(alias = "countries")]
    sort_countries: Option<String>,
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

pub fn pay_view(p: &PaymentRow, domain: &str, offset: UtcOffset) -> PayView {
    let (status, status_class) = status(&p.status);
    let (source, channel) = source_of(p);
    let first = p.first_touch.as_ref().map(|f| f.source.clone());
    PayView {
        date: local(p.ts, offset),
        icon: icons::source(&source, 16),
        channel: if channel == source {
            String::new()
        } else {
            channel
        },
        unattributed: p.touch.is_none(),
        renewal: p.is_renewal,
        first_icon: first
            .as_deref()
            .map(|f| icons::source(f, 16))
            .unwrap_or_default(),
        first: first.unwrap_or_default(),
        source,
        amount: amount(p),
        refunded: p.status == "refunded",
        status,
        status_class,
        provider_icon: icons::provider(&p.provider, 14),
        provider: provider_short(&p.provider),
        href: format!(
            "{}/{}/{}",
            sites::app_path(domain, "payments"),
            p.provider,
            p.provider_payment_id
        ),
        visitor: p
            .visitor
            .as_deref()
            .map(visitors::label)
            .unwrap_or_default(),
        visitor_href: p
            .visitor
            .as_deref()
            .map(|v| visitor_pages::visitor_href(domain, v))
            .unwrap_or_default(),
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
    matched: bool,
) -> RowView {
    let value = if by_visitors {
        r.visitors as f64
    } else {
        r.revenue_usd
    };
    let key = r.key.as_deref().unwrap_or_default();
    let (icon, label) = match kind {
        CardKind::Channel => (
            icons::glyph(
                icons::channel_glyph(key).unwrap_or("link"),
                16,
                chart::channel_color(r.key.as_deref()),
            ),
            r.label.clone(),
        ),
        CardKind::Referrer => (icons::source(&r.label, 16), r.label.clone()),
        CardKind::Page => (String::new(), r.label.clone()),
        CardKind::Country if r.key.is_some() => (icons::flag(key), icons::country(key)),
        CardKind::Country => (String::new(), "Unknown".into()),
    };
    RowView {
        icon,
        label,
        mono: kind == CardKind::Page,
        visitors: group(r.visitors),
        revenue: money(r.revenue_usd),
        zero: r.revenue_usd <= 0.0,
        per_visitor: if matched {
            usd(r.revenue_per_visitor.unwrap_or(0.0))
        } else {
            "n/a".into()
        },
        width: if max > 0.0 { value / max * 100.0 } else { 0.0 },
    }
}

#[allow(clippy::too_many_arguments)]
fn card(
    kind: CardKind,
    rows: &[BreakdownRow],
    params: &[(&str, &str)],
    base: &str,
    by_visitors: bool,
    expanded: bool,
    foot: Option<String>,
) -> Card {
    let (key, sort, title, head) = match kind {
        CardKind::Channel => ("channels", "sort_channels", "Channels", "Channel"),
        CardKind::Referrer => ("referrers", "sort_referrers", "Referrers", "Referrer"),
        CardKind::Page => ("pages", "sort_pages", "Landing pages", "Page"),
        CardKind::Country => ("countries", "sort_countries", "Countries", "Country"),
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
    let matched = rows.iter().any(|r| !r.unattributed && r.payments > 0);
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
        revenue_href: link(base, params, &[(sort, "")]),
        visitors_href: link(base, params, &[(sort, "visitors")]),
        rows: shown
            .iter()
            .map(|r| row_view(kind, r, max, by_visitors, matched))
            .collect(),
        unattributed: unattributed.map(|r| RowView {
            icon: icons::glyph("unattributed", 16, "#B9BCC2"),
            visitors: "n/a".into(),
            per_visitor: "n/a".into(),
            ..row_view(kind, r, max, by_visitors, matched)
        }),
        foot,
        foot_href,
        foot_link,
    }
}

fn span(p: &Period) -> String {
    format!("{} to {}", day(p.from), day(p.to))
}

/// A metric's value, change against the previous period and subtitle; `since` notes when tracking started.
fn metric_tile(
    m: chart::Metric,
    cur: &Totals,
    prev: &Totals,
    since: Option<Date>,
) -> (String, (&'static str, String), String) {
    match m {
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
                Some(s) => format!("{} since {}", plural(cur.sessions, "session"), day(s)),
                None => plural(cur.sessions, "session"),
            },
        ),
        chart::Metric::Conversion => (
            cur.conversion
                .map_or_else(|| "n/a".into(), |c| format!("{:.2}%", c * 100.0)),
            delta(cur.conversion, prev.conversion),
            conversion_sub(cur),
        ),
        chart::Metric::PerVisitor => (
            cur.revenue_per_visitor.map_or_else(|| "n/a".into(), usd),
            delta(cur.revenue_per_visitor, prev.revenue_per_visitor),
            if cur.matched_payments > 0 {
                "Matched revenue divided by visitors".into()
            } else {
                "Needs a payment matched to a visit".into()
            },
        ),
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
    let (rkey, rlabel, days, rdefault) = range(&ctx, q.range.as_deref());
    let p = period(&ctx, days);
    let settings: Result<(String, i64, bool), _> = sqlx::query_as(
        "SELECT attribution_model, window_days, setup_dismissed FROM sites WHERE id = ?",
    )
    .bind(&ctx.site.id)
    .fetch_one(&state.db)
    .await;
    let providers: Result<Vec<String>, _> = sqlx::query_scalar(
        "SELECT provider FROM integrations WHERE site_id = ? ORDER BY created_at",
    )
    .bind(&ctx.site.id)
    .fetch_all(&state.db)
    .await;
    let (Ok((model, window_days, dismissed)), Ok(providers)) = (settings, providers) else {
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
    let base = sites::app_path(&ctx.site.domain, "");
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
    let range_param = if rkey == rdefault { "" } else { rkey };
    let metric_param = if metric == chart::Metric::Revenue {
        ""
    } else {
        metric.key()
    };
    let kind_param = if kind == default_kind { "" } else { kind.key() };
    let sorts: Vec<(&str, &str)> = [
        ("sort_channels", &q.sort_channels),
        ("sort_referrers", &q.sort_referrers),
        ("sort_pages", &q.sort_pages),
        ("sort_countries", &q.sort_countries),
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
            let (value, (trend, delta), sub) =
                metric_tile(m, cur, prev, since.filter(|s| new_site && *s >= p.from));
            let (trend, delta) = if new_site {
                if m == chart::Metric::Revenue && !young {
                    ("flat", "No earlier data".to_string())
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
                color: m.color(),
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
                format!("Visitors and revenue per {unit}"),
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
            &params,
            &base,
            sorts
                .iter()
                .any(|(k, _)| k.strip_prefix("sort_") == Some(key)),
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
                href: String::new(),
            },
            match providers.first() {
                Some(pr) => Step {
                    done: true,
                    icon: icons::provider(pr, 14),
                    title: provider_label(pr),
                    sub: "Connected".into(),
                    href: String::new(),
                },
                None => Step {
                    done: false,
                    icon: icons::glyph("card", 14, "#0E1116"),
                    title: "Payments".into(),
                    sub: "Not connected".into(),
                    href: String::new(),
                },
            },
            Step {
                done: false,
                icon: icons::glyph("link", 14, "#0E1116"),
                title: "Checkout link".into(),
                sub: "Passes virling_vid".into(),
                href: String::new(),
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
    let site_url = |page: &str| sites::app_path(&ctx.site.domain, page);
    let mut steps = if young && age <= 14 && empty.is_none() && !dismissed {
        let matched = setup.first_match.as_ref();
        vec![
            Step {
                done: since.is_some(),
                icon: icons::glyph("direct", 14, "#0E1116"),
                title: "Script installed".into(),
                sub: since.map(day).unwrap_or_else(|| "Not seen yet".into()),
                href: site_url("setup/script"),
            },
            match providers.first() {
                Some(pr) => Step {
                    done: true,
                    icon: icons::provider(pr, 14),
                    title: format!("{} connected", provider_short(pr)),
                    sub: format!("{} total", plural(setup.payments, "payment")),
                    href: String::new(),
                },
                None => Step {
                    done: false,
                    icon: icons::glyph("card", 14, "#8A909A"),
                    title: "Connect payments".into(),
                    sub: "Stripe, Polar, Dodo, Lemon Squeezy".into(),
                    href: site_url("setup/payments"),
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
                        href: String::new(),
                    }
                }
                None => Step {
                    done: false,
                    icon: icons::glyph("link", 14, "#8A909A"),
                    title: "Payment matched".into(),
                    sub: "Pass virling_vid at checkout".into(),
                    href: site_url("setup/checkout"),
                },
            },
            Step {
                done: setup.has_goals,
                icon: icons::glyph("unattributed", 14, "#8A909A"),
                title: "Track a goal".into(),
                sub: "Signups, trials".into(),
                href: "#goals".into(),
            },
        ]
    } else {
        Vec::new()
    };
    if steps.iter().all(|s| s.done) {
        steps.clear();
    }
    render(DashboardPage {
        title: "Dashboard",
        range_label: rlabel,
        ranges: range_opts(&base, rkey, rdefault, &params),
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
            .map(|r| pay_view(r, &ctx.site.domain, ctx.offset))
            .collect(),
        latest_total: recent.total,
        payments_href: href(
            &sites::app_path(&ctx.site.domain, "payments"),
            &[("range", range_param)],
        ),
        steps_done: steps.iter().filter(|s| s.done).count(),
        dismiss_href: site_url("setup/dismiss"),
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

pub struct Summary {
    name: &'static str,
    value: String,
    sub: String,
}

#[derive(Template)]
#[template(path = "payments.html")]
struct PaymentsPage {
    shell: Shell,
    title: &'static str,
    range_label: &'static str,
    ranges: Vec<Opt>,
    summary: Vec<Summary>,
    filters: Vec<Opt>,
    rows: Vec<PayView>,
    filter_label: &'static str,
    prev: String,
    next: String,
    showing: String,
    total: u64,
    offset: u64,
    never_paid: bool,
    connected: String,
    providers: Vec<(String, &'static str, bool)>,
}

/// The site's cached favicon; SVGs get a CSP that blocks scripts.
pub async fn icon(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let Some(site) = sites::for_account(&state.db, &account.id)
        .await
        .ok()
        .and_then(|all| all.into_iter().find(|s| s.domain == domain))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let row: Option<(String, Vec<u8>)> = sqlx::query_as(
        "SELECT content_type, body FROM site_icons WHERE site_id = ? AND body IS NOT NULL",
    )
    .bind(&site.id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let Some((kind, body)) = row else {
        return StatusCode::NOT_FOUND.into_response();
    };
    image(kind, body, "private, max-age=86400")
}

/// A referrer's cached favicon, served by us so no page loads images from third parties.
pub async fn source_icon(State(state): State<AppState>, Path(host): Path<String>) -> Response {
    let row = match favicon::source_host(&host) {
        Some(host) => favicon::source_body(&state.db, &host).await,
        None => None,
    };
    let Some((kind, body)) = row else {
        return StatusCode::NOT_FOUND.into_response();
    };
    image(kind, body, "public, max-age=604800")
}

fn image(kind: String, body: Vec<u8>, cache: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, kind),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; sandbox".into(),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
            (header::CACHE_CONTROL, cache.into()),
        ],
        body,
    )
        .into_response()
}

/// Hides the first-weeks checklist for good.
pub async fn dismiss_setup(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "dashboard").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if let Err(err) = sqlx::query("UPDATE sites SET setup_dismissed = 1 WHERE id = ?")
        .bind(&ctx.site.id)
        .execute(&state.db)
        .await
    {
        tracing::error!(%err, "dismissing the checklist failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&sites::app_path(&ctx.site.domain, "")).into_response()
}

pub async fn payments(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "payments").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let (rkey, rlabel, days, rdefault) = range(&ctx, q.range.as_deref());
    let p = period(&ctx, days);
    let (fkey, flabel, filter) = FILTERS
        .into_iter()
        .find_map(|(k, label)| {
            (Some(k) == q.filter.as_deref()).then(|| Some((k, label, PaymentFilter::parse(k)?)))?
        })
        .unwrap_or(("all", "All", PaymentFilter::All));
    let page = q.page.unwrap_or(1).max(1);
    let providers: Vec<String> = match sqlx::query_scalar(
        "SELECT provider FROM integrations WHERE site_id = ? ORDER BY created_at",
    )
    .bind(&ctx.site.id)
    .fetch_all(&state.db)
    .await
    {
        Ok(p) => p,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let result = blocking(move || {
        let s = store.payment_summary(&id, &p)?;
        let list = store.payment_list(&id, &p, filter, PER_PAGE, (page - 1) * PER_PAGE)?;
        let ever = if s.payments > 0 {
            s.payments
        } else {
            store
                .payment_list(
                    &id,
                    &Period {
                        from: time::macros::date!(2000 - 01 - 01),
                        to: p.to,
                        tz: p.tz.clone(),
                    },
                    PaymentFilter::All,
                    0,
                    0,
                )?
                .total
        };
        Ok((s, list, ever))
    })
    .await;
    let (s, list, ever) = match result {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let counts = [s.payments, s.attributed, s.unattributed, s.refunded];
    let base = sites::app_path(&ctx.site.domain, "payments");
    let range_param = if rkey == rdefault { "" } else { rkey };
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
    let summary = vec![
        Summary {
            name: "Net revenue",
            value: money(s.net_usd),
            sub: if s.refunded_usd > 0.0 {
                format!("After {} in refunds", money(s.refunded_usd))
            } else {
                "No refunds".into()
            },
        },
        Summary {
            name: "Payments",
            value: group(s.payments),
            sub: format!("From {}", plural(s.customers, "customer")),
        },
        Summary {
            name: "Attributed",
            value: if s.payments > 0 {
                format!("{:.0}%", s.attributed as f64 / s.payments as f64 * 100.0)
            } else {
                "n/a".into()
            },
            sub: if s.payments > 0 {
                format!(
                    "{} of {} matched to a visit",
                    group(s.attributed),
                    group(s.payments)
                )
            } else {
                "Needs a first payment".into()
            },
        },
        Summary {
            name: "Average payment",
            value: if s.payments > 0 {
                usd(s.net_usd / s.payments as f64)
            } else {
                "n/a".into()
            },
            sub: "All shown in USD".into(),
        },
    ];
    render(PaymentsPage {
        title: "Payments",
        range_label: rlabel,
        ranges: range_opts(&base, rkey, rdefault, &[("filter", filter_param)]),
        summary,
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
            .map(|p| pay_view(p, &ctx.site.domain, ctx.offset))
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
        showing: if shown > 0 {
            format!(
                "Showing {} to {} of {}",
                group(first + 1),
                group(first + shown),
                group(list.total)
            )
        } else {
            String::new()
        },
        total: list.total,
        offset: first,
        never_paid: ever == 0,
        connected: providers
            .first()
            .map(|p| provider_name(p).to_string())
            .unwrap_or_default(),
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
        shell: ctx.shell,
    })
}

pub struct SessionView {
    date: String,
    icon: String,
    source: String,
    channel: String,
    campaign: Option<String>,
    paths: Vec<String>,
    more: usize,
    goals: Vec<String>,
    credited: bool,
}

pub struct JourneyView {
    amount: String,
    charged: String,
    provider: &'static str,
    provider_icon: String,
    payment_id: String,
    customer_id: String,
    visitor_id: String,
    visitor: String,
    visitor_href: String,
    date: String,
    flag: String,
    country: String,
    browser: String,
    status: &'static str,
    status_class: &'static str,
    unattributed: bool,
    credited_icon: String,
    credited: String,
    credited_channel: String,
    first_icon: String,
    first_touch: String,
    first_date: String,
    time_to_pay: String,
    visits: String,
    sessions: Vec<SessionView>,
    checkout_href: String,
}

/// "2 days 18 h", "5 h", "12 minutes"
pub(crate) fn duration(d: Duration) -> String {
    let (days, hours) = (d.whole_days(), d.whole_hours() % 24);
    match days {
        0 if d.whole_hours() >= 1 => format!("{} h", d.whole_hours()),
        0 => plural(d.whole_minutes().max(1) as u64, "minute"),
        _ if hours > 0 => format!("{} {hours} h", plural(days as u64, "day")),
        _ => plural(days as u64, "day"),
    }
}

fn journey_view(j: Journey, domain: &str, offset: UtcOffset) -> JourneyView {
    let p = &j.payment;
    let (status, status_class) = status(&p.status);
    let (credited, credited_channel) = source_of(p);
    let first_ts = p
        .first_touch
        .as_ref()
        .map(|f| f.ts)
        .or(j.sessions.first().map(|s| s.ts));
    let first_touch = p
        .first_touch
        .as_ref()
        .map(|f| f.source.clone())
        .or(j.sessions.first().map(|s| s.source.clone()))
        .unwrap_or_default();
    JourneyView {
        amount: amount(p),
        charged: match p.amount_usd {
            Some(u) if p.currency != "usd" => format!(
                "{} {}, shown as {}",
                p.currency.to_ascii_uppercase(),
                group_minor(p.amount, &p.currency),
                usd(u)
            ),
            _ => native(p.amount, &p.currency),
        },
        provider: provider_name(&p.provider),
        provider_icon: icons::provider(&p.provider, 14),
        payment_id: p.provider_payment_id.clone(),
        customer_id: p.customer_id.clone().unwrap_or_default(),
        visitor_id: p.visitor_id.clone().unwrap_or_default(),
        visitor: p
            .visitor
            .as_deref()
            .map(visitors::label)
            .unwrap_or_default(),
        visitor_href: p
            .visitor
            .as_deref()
            .map(|v| visitor_pages::visitor_href(domain, v))
            .unwrap_or_default(),
        date: p
            .ts
            .to_offset(offset)
            .format(format_description!(
                "[month repr:short] [day padding:none], [year] at [hour]:[minute]"
            ))
            .unwrap_or_default(),
        flag: j.country.as_deref().map(icons::flag).unwrap_or_default(),
        country: j.country.as_deref().map(icons::country).unwrap_or_default(),
        browser: match (&j.browser, &j.device) {
            (Some(b), Some(d)) if d != "Unknown" => format!("{b} on {d}"),
            (Some(b), _) => b.clone(),
            _ => String::new(),
        },
        status,
        status_class,
        unattributed: p.touch.is_none(),
        credited_icon: icons::source(&credited, 14),
        credited_channel: if credited_channel == credited {
            String::new()
        } else {
            credited_channel
        },
        credited,
        first_icon: icons::source(&first_touch, 14),
        first_touch,
        first_date: first_ts
            .map(|t| day(t.to_offset(offset).date()))
            .unwrap_or_default(),
        time_to_pay: first_ts.map(|t| duration(p.ts - t)).unwrap_or_default(),
        visits: plural(j.sessions.len() as u64, "visit"),
        checkout_href: sites::app_path(domain, "setup/checkout"),
        sessions: j
            .sessions
            .iter()
            .map(|s| SessionView {
                date: local(s.ts, offset),
                icon: icons::source(&s.source, 14),
                channel: if s.channel == s.source {
                    String::new()
                } else {
                    s.channel.clone()
                },
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

/// "4,999.00" in the currency's minor units.
fn group_minor(minor: i64, currency: &str) -> String {
    let v = minor_to_major(minor, currency);
    let c = (v * 100.0).round() as u64;
    format!("{}.{:02}", group(c / 100), c % 100)
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
    Path((domain, provider, payment_id)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let ctx = match load(&state, &account, &domain, "payments").await {
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
    let j = journey_view(journey, &ctx.site.domain, ctx.offset);
    if headers.contains_key("x-drawer") {
        return render(JourneyFragment { j });
    }
    render(PaymentDetailPage {
        title: "Payment",
        back: sites::app_path(&ctx.site.domain, "payments"),
        j,
        shell: ctx.shell,
    })
}

/// The client report body, shared by the app page and the public link.
pub struct ReportView {
    site: String,
    span: String,
    tiles: Vec<Tile>,
    chart_title: String,
    chart_sub: String,
    chart: String,
    cards: Vec<(&'static str, Card)>,
}

#[derive(Template)]
#[template(path = "report.html")]
struct ReportPage {
    shell: Shell,
    title: &'static str,
    range_label: &'static str,
    ranges: Vec<Opt>,
    report: ReportView,
    brand: Brand,
}

#[derive(Template)]
#[template(path = "report_shared.html")]
struct SharedReportPage {
    range_label: &'static str,
    ranges: Vec<Opt>,
    report: ReportView,
    brand: Brand,
}

/// The client report for a range, with range links pointing at `base`.
async fn client_report(
    state: &AppState,
    ctx: &Ctx,
    key: Option<&str>,
    base: &str,
) -> Result<(ReportView, &'static str, Vec<Opt>), StatusCode> {
    let (rkey, rlabel, days, _) = range(ctx, Some(key.unwrap_or("7d")));
    let p = period(ctx, days);
    let (model, window_days): (String, i64) =
        sqlx::query_as("SELECT attribution_model, window_days FROM sites WHERE id = ?")
            .bind(&ctx.site.id)
            .fetch_one(&state.db)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let store = state.events.clone();
    let (id, period_q) = (ctx.site.id.clone(), p.clone());
    let report = blocking(move || store.report(&id, &period_q)).await?;
    let since = report
        .setup
        .first_event
        .map(|t| t.to_offset(ctx.offset).date());
    let new_site = since.is_none_or(|d| d > p.previous().from);
    let (cur, prev) = (&report.overview.current, &report.overview.previous);
    let tiles = chart::Metric::ALL
        .into_iter()
        .map(|m| {
            let (value, (trend, delta), sub) =
                metric_tile(m, cur, prev, since.filter(|s| new_site && *s >= p.from));
            let (trend, delta) = if new_site {
                ("", String::new())
            } else {
                (trend, delta)
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
                href: String::new(),
                on: matches!(m, chart::Metric::Revenue | chart::Metric::Visitors),
                color: m.color(),
                big: m == chart::Metric::Revenue,
            }
        })
        .collect();
    let current = chart::series(&report.days);
    let unit = if current.weekly { "week" } else { "day" };
    let credit = format!(
        "Credited to the {} within {} days",
        if model == "first_touch" {
            "first visit"
        } else {
            "last non-direct visit"
        },
        window_days
    );
    let card_for = |kind: CardKind, rows: &[BreakdownRow], foot: String| {
        card(kind, rows, &[], base, false, false, Some(foot))
    };
    let view = ReportView {
        site: ctx.site.domain.clone(),
        span: format!("{}, {}", span(&p), p.to.year()),
        tiles,
        chart_title: format!("Visitors and revenue per {unit}"),
        chart_sub: if new_site {
            span(&p)
        } else {
            format!(
                "{}. Changes are against the previous {} days",
                span(&p),
                days.min(365)
            )
        },
        chart: chart::dual(&current),
        cards: vec![
            (
                "Top sources",
                card_for(CardKind::Referrer, &report.referrers, credit),
            ),
            (
                "Top pages",
                card_for(CardKind::Page, &report.landing_pages, String::new()),
            ),
        ],
    };
    Ok((view, rlabel, range_opts(base, rkey, "7d", &[])))
}

pub async fn report(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "report").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let base = sites::app_path(&ctx.site.domain, "report");
    let (report, range_label, ranges) =
        match client_report(&state, &ctx, q.range.as_deref(), &base).await {
            Ok(r) => r,
            Err(code) => return code.into_response(),
        };
    let Ok(brand) = settings::brand(&state.db, &ctx.site.id).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    render(ReportPage {
        title: "Client report",
        range_label,
        ranges,
        report,
        brand,
        shell: ctx.shell,
    })
}

/// The client report behind a read-only link, without sign-in or Virling branding.
pub async fn shared(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let site: Result<Option<Site>, _> = sqlx::query_as(
        "SELECT s.id, s.domain, s.timezone, s.cookieless FROM report_brands b JOIN sites s ON s.id = b.site_id WHERE b.share_token = ?",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await;
    let site = match site {
        Ok(Some(site)) => site,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let store = state.events.clone();
    let (id, tz) = (site.id.clone(), site.timezone.clone());
    let (offset, first) =
        match blocking(move || Ok((store.utc_offset(&tz)?, store.first_event(&id)?))).await {
            Ok(r) => r,
            Err(code) => return code.into_response(),
        };
    let ctx = Ctx {
        shell: Shell {
            domain: site.domain.clone(),
            initial: String::new(),
            icon: String::new(),
            sites: Vec::new(),
            nav: "report",
            plan: String::new(),
            usage: String::new(),
            email: String::new(),
            avatar: String::new(),
            live: 0,
        },
        site,
        today: OffsetDateTime::now_utc().to_offset(offset).date(),
        first_day: first.map(|t| t.to_offset(offset).date()),
        offset,
    };
    let base = format!("/r/{token}");
    let (report, range_label, ranges) =
        match client_report(&state, &ctx, q.range.as_deref(), &base).await {
            Ok(r) => r,
            Err(code) => return code.into_response(),
        };
    let Ok(brand) = settings::brand(&state.db, &ctx.site.id).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    render(SharedReportPage {
        range_label,
        ranges,
        report,
        brand,
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

    #[test]
    fn conversion_sub_uses_the_same_numbers() {
        let t = Totals {
            visitors: 1,
            payments: 2,
            ..Totals::default()
        };
        assert_eq!(
            conversion_sub(&t),
            "0 of 1 visitor converted. 2 payments not matched to a visit"
        );
        let t = Totals {
            visitors: 12,
            payments: 1,
            converted: 1,
            matched_payments: 1,
            ..Totals::default()
        };
        assert_eq!(conversion_sub(&t), "1 of 12 visitors converted");
    }

    #[test]
    fn links_drop_defaults_and_keep_other_params() {
        let base = "/app/virling.com";
        assert_eq!(link(base, &[("range", ""), ("metric", "")], &[]), base);
        let params = [
            ("range", "7d"),
            ("metric", ""),
            ("chart", ""),
            ("sort_pages", "visitors"),
        ];
        assert_eq!(
            link(base, &params, &[("metric", "visitors")]),
            "/app/virling.com?range=7d&metric=visitors&sort_pages=visitors"
        );
        assert_eq!(
            link(base, &params, &[("range", "")]),
            "/app/virling.com?sort_pages=visitors"
        );
        let ranges = range_opts(base, "7d", "30d", &params);
        assert_eq!(
            ranges[1].href,
            "/app/virling.com?range=7d&sort_pages=visitors"
        );
        assert_eq!(ranges[2].href, "/app/virling.com?sort_pages=visitors");
    }
}
