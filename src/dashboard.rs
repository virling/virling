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
use crate::sites::{self, Site};
use crate::stats::{Activity, Dimension, GoalRow, Journey, PaymentFilter, PaymentRow, Period, Who};
use crate::{alias, favicon, icons, visitor_pages};

mod cards;
mod insight;
pub mod report;
pub mod schedule;

pub(crate) const RANGES: [(&str, &str, i64); 6] = [
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
/// Rows read when searching, since names are generated outside the database.
const SEARCH_LIMIT: u32 = 5000;

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
    let fetched = icons.iter().find(|i| i.0 == site.id).map(|i| (i.1, i.2));
    favicon::refresh(state, &site.id, &site.domain, fetched);
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
    resolve_range(ctx.today, ctx.first_day, key)
}

/// `range` for a site whose local today and first tracked day are known.
pub(crate) fn resolve_range(
    today: Date,
    first_day: Option<Date>,
    key: Option<&str>,
) -> (&'static str, &'static str, i64, &'static str) {
    let tracked = first_day.map(|d| (today - d).whole_days() + 1);
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

pub struct Kpi {
    label: &'static str,
    value: String,
    trend: &'static str,
    delta: String,
    note: String,
}

pub struct AiView {
    visitors_share: String,
    revenue_share: String,
    /// Icon, name, visitors and revenue.
    rows: Vec<(String, String, String, String)>,
    line: String,
}

/// A step in a payment's path: a visit's source, or the last page before paying.
pub struct Chip {
    icon: String,
    label: String,
    credited: bool,
}

/// A payment with the visits that led to it.
pub struct PathRow {
    name: String,
    avatar: String,
    meta: String,
    chips: Vec<Chip>,
    amount: String,
    refunded: bool,
    wait: String,
    date: String,
    href: String,
    visitor_href: String,
}

/// Chips for a journey's visits in order, merging repeats and keeping at most four plus the credited one.
fn path_chips(j: Option<&Journey>) -> Vec<Chip> {
    let mut chips: Vec<Chip> = Vec::new();
    for s in j.map(|j| j.sessions.as_slice()).unwrap_or_default() {
        match chips.last_mut() {
            Some(last) if last.label == s.source => last.credited |= s.credited,
            _ => chips.push(Chip {
                icon: icons::source(&s.source, 14),
                label: s.source.clone(),
                credited: s.credited,
            }),
        }
    }
    let mut hidden = 0;
    while chips.len() > 4 {
        let Some(i) = (1..chips.len() - 1).find(|i| !chips[*i].credited) else {
            break;
        };
        chips.remove(i);
        hidden += 1;
    }
    if hidden > 0 {
        chips.insert(
            1,
            Chip {
                icon: String::new(),
                label: format!("+{hidden}"),
                credited: false,
            },
        );
    }
    if chips.is_empty() {
        chips.push(Chip {
            icon: icons::source("Unattributed", 14),
            label: "No matched visit".into(),
            credited: false,
        });
    }
    if let Some(page) = j.and_then(|j| j.last_page.clone()) {
        chips.push(Chip {
            icon: String::new(),
            label: page,
            credited: false,
        });
    }
    chips
}

fn path_row(p: &PaymentRow, j: Option<&Journey>, domain: &str, offset: UtcOffset) -> PathRow {
    let alias = alias::alias(&payer_key(p));
    let meta: Vec<String> = j
        .map(|j| {
            [
                j.country.as_deref().map(icons::country),
                j.device.clone().filter(|d| d != "Unknown"),
                j.browser.clone().filter(|b| b != "Other"),
            ]
            .into_iter()
            .flatten()
            .collect()
        })
        .unwrap_or_default();
    PathRow {
        name: alias.name,
        avatar: alias.avatar,
        meta: meta.join(" · "),
        chips: path_chips(j),
        amount: amount(p),
        refunded: p.status == "refunded",
        wait: p
            .first_touch
            .as_ref()
            .map(|f| since(p.ts - f.ts))
            .unwrap_or_default(),
        date: local(p.ts, offset),
        href: format!(
            "{}/{}/{}",
            sites::app_path(domain, "payments"),
            p.provider,
            p.provider_payment_id
        ),
        visitor_href: p
            .visitor
            .as_deref()
            .map(|v| visitor_pages::visitor_href(domain, v))
            .unwrap_or_default(),
    }
}

/// A payment or goal completion shown as the person behind it.
pub struct CustomerView {
    name: String,
    avatar: String,
    /// Customer, Renewal, or empty for a goal completion.
    badge: &'static str,
    /// Icon HTML and label for country, device, OS and browser.
    chips: Vec<(String, String)>,
    icon: String,
    source: String,
    channel: String,
    spent: String,
    refunded: bool,
    status: &'static str,
    status_class: &'static str,
    provider_icon: String,
    /// From the first visit to the payment or goal.
    wait: String,
    date: String,
    /// One CSS class per day of the activity strip; the last one marks the payment or goal day.
    days: Vec<&'static str>,
    /// Journey link; empty for goal completions.
    href: String,
    /// Visitor timeline; empty when the visitor is unknown.
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
    report_href: String,
    empty: Option<EmptyView>,
    insight: Option<(String, String)>,
    kpis: Vec<Kpi>,
    cards: Vec<cards::TabCard>,
    unmatched: String,
    credit: String,
    ai: AiView,
    paths: Vec<PathRow>,
    payments_total: String,
    payments_href: String,
    range_param: &'static str,
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
    /// A goal name; payments when empty.
    goal: Option<String>,
    /// Search over names, sources and countries.
    q: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct DashboardQuery {
    range: Option<String>,
    /// Card tabs, read by `cards::picks`.
    #[serde(flatten)]
    cards: std::collections::HashMap<String, String>,
}

fn source_of(p: &PaymentRow) -> (String, String) {
    match &p.touch {
        Some(t) => (t.source.clone(), t.channel.clone()),
        None => ("Unattributed".into(), String::new()),
    }
}

/// Stable key for the person behind a payment, so renewals and goals share one generated name.
pub(crate) fn payer_key(p: &PaymentRow) -> String {
    match (&p.customer_id, &p.visitor_id) {
        (Some(c), _) => format!("c:{}:{c}", p.provider),
        (None, Some(v)) => format!("v:{v}"),
        (None, None) => format!("p:{}:{}", p.provider, p.provider_payment_id),
    }
}

fn encode(v: &str) -> String {
    url::form_urlencoded::byte_serialize(v.as_bytes()).collect()
}

impl CustomerView {
    /// Case-insensitive match on name, source, channel or chip labels; `q` is lowercase.
    fn matches(&self, q: &str) -> bool {
        [&self.name, &self.source, &self.channel]
            .into_iter()
            .chain(self.chips.iter().map(|c| &c.1))
            .any(|s| s.to_lowercase().contains(q))
    }
}

/// "a few seconds", "5 minutes", "an hour", "2 days", "a month"
fn since(d: Duration) -> String {
    let count = |n: i64, one: &str, unit: &str| {
        if n == 1 {
            one.to_string()
        } else {
            format!("{n} {unit}s")
        }
    };
    let days = d.whole_days();
    match d.whole_seconds() {
        ..60 => "a few seconds".into(),
        60..3600 => count(d.whole_minutes(), "a minute", "minute"),
        3600..86400 => count(d.whole_hours(), "an hour", "hour"),
        _ if days < 30 => count(days, "a day", "day"),
        _ if days < 365 => count(days / 30, "a month", "month"),
        _ => count(days / 365, "a year", "year"),
    }
}

fn chips(a: &Activity) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(c) = &a.country {
        out.push((icons::flag(c), icons::country(c)));
    }
    if let Some(d) = a.device.as_deref().filter(|d| *d != "Unknown") {
        out.push((icons::device(d), d.to_string()));
    }
    for name in [&a.os, &a.browser].into_iter().flatten() {
        if name != "Other" {
            out.push((icons::ua(name), name.clone()));
        }
    }
    out
}

/// Tint classes for sessions per day; the last day is the payment or goal day.
fn heat(days: &[u32]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = days
        .iter()
        .map(|n| match n {
            0 => "h0",
            1 => "h1",
            2 => "h2",
            _ => "h3",
        })
        .collect();
    if let Some(last) = out.last_mut() {
        *last = "pay";
    }
    out
}

pub fn payment_customer(
    p: &PaymentRow,
    a: &Activity,
    domain: &str,
    offset: UtcOffset,
) -> CustomerView {
    let alias = alias::alias(&payer_key(p));
    let (status, status_class) = status(&p.status);
    let (source, channel) = source_of(p);
    let first = p.first_touch.as_ref().map(|f| f.ts);
    CustomerView {
        name: alias.name,
        avatar: alias.avatar,
        badge: if p.is_renewal { "Renewal" } else { "Customer" },
        chips: chips(a),
        icon: icons::source(&source, 16),
        channel: if channel == source {
            String::new()
        } else {
            channel
        },
        source,
        spent: amount(p),
        refunded: p.status == "refunded",
        status,
        status_class,
        provider_icon: icons::provider(&p.provider, 12),
        wait: first.map(|f| since(p.ts - f)).unwrap_or_default(),
        date: local(p.ts, offset),
        days: heat(&a.days),
        href: format!(
            "{}/{}/{}",
            sites::app_path(domain, "payments"),
            p.provider,
            p.provider_payment_id
        ),
        visitor_href: p
            .visitor
            .as_deref()
            .map(|v| visitor_pages::visitor_href(domain, v))
            .unwrap_or_default(),
    }
}

fn goal_customer(g: &GoalRow, a: &Activity, domain: &str, offset: UtcOffset) -> CustomerView {
    let key = match &g.customer {
        Some(c) => format!("c:{c}"),
        None => format!("v:{}", g.visitor_id),
    };
    let alias = alias::alias(&key);
    CustomerView {
        name: alias.name,
        avatar: alias.avatar,
        badge: if g.spent_usd > 0.0 { "Customer" } else { "" },
        chips: chips(a),
        icon: icons::source(&g.source, 16),
        channel: if g.channel == g.source {
            String::new()
        } else {
            g.channel.clone()
        },
        source: g.source.clone(),
        spent: usd(g.spent_usd),
        refunded: false,
        status: "",
        status_class: "",
        provider_icon: String::new(),
        wait: g.first_ts.map(|f| since(g.ts - f)).unwrap_or_default(),
        date: local(g.ts, offset),
        days: heat(&a.days),
        href: String::new(),
        visitor_href: visitor_pages::visitor_href(domain, &g.visitor_id),
    }
}

/// Payments listed with their path on the dashboard.
const PATH_ROWS: u32 = 8;

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
    let picks = cards::picks(&q.cards);
    let dims: Vec<Option<Dimension>> = picks.iter().map(|c| c.dimension()).collect();
    let store = state.events.clone();
    let (id, period_q) = (ctx.site.id.clone(), p.clone());
    let result = blocking(move || {
        let recent = store.payment_list(&id, &period_q, PaymentFilter::All, PATH_ROWS, 0)?;
        let journeys = recent
            .rows
            .iter()
            .map(|r| store.journey(&id, &r.provider, &r.provider_payment_id, window_days))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let breakdowns = dims
            .iter()
            .map(|d| match d {
                Some(d) => store.breakdown(&id, &period_q, *d),
                None => store.last_pages(&id, &period_q),
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok((
            store.report(&id, &period_q)?,
            store.breakdown(&id, &period_q, Dimension::ChannelSource)?,
            recent,
            journeys,
            breakdowns,
        ))
    })
    .await;
    let (report, sources, recent, journeys, breakdowns) = match result {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let base = sites::app_path(&ctx.site.domain, "");
    let setup = &report.setup;
    let since = setup.first_event.map(|t| t.to_offset(ctx.offset).date());
    let new_site = since.is_none_or(|d| d > p.previous().from);
    let age = since.map_or(0, |d| (ctx.today - d).whole_days());
    let young = new_site && age <= 30;
    let range_param = if rkey == rdefault { "" } else { rkey };
    let mut params: Vec<(&str, &str)> = vec![("range", range_param)];
    params.extend(cards::params(&picks));

    let cur = &report.overview.current;
    let prev = &report.overview.previous;
    let change = |c: Option<f64>, p: Option<f64>| {
        if new_site {
            ("", String::new())
        } else {
            delta(c, p)
        }
    };
    let kpi = |label, value, (trend, delta): (&'static str, String), note| Kpi {
        label,
        value,
        trend,
        delta,
        note,
    };
    let kpis = vec![
        kpi(
            "Revenue",
            money(cur.revenue_usd),
            change(Some(cur.revenue_usd), Some(prev.revenue_usd)),
            plural(cur.payments, "payment"),
        ),
        kpi(
            "Visitors",
            group(cur.visitors),
            change(Some(cur.visitors as f64), Some(prev.visitors as f64)),
            plural(cur.sessions, "session"),
        ),
        kpi(
            "Conversion",
            cur.conversion
                .map_or_else(|| "n/a".into(), |c| format!("{:.2}%", c * 100.0)),
            change(cur.conversion, prev.conversion),
            format!(
                "{} of {} paid",
                group(cur.converted),
                plural(cur.visitors, "visitor")
            ),
        ),
        kpi(
            "Revenue per visitor",
            cur.revenue_per_visitor.map_or_else(|| "n/a".into(), usd),
            change(cur.revenue_per_visitor, prev.revenue_per_visitor),
            if cur.matched_payments > 0 {
                "Matched revenue only".into()
            } else {
                "Needs a payment matched to a visit".into()
            },
        ),
    ];

    let attributed: f64 = report
        .channels
        .iter()
        .filter(|r| !r.unattributed)
        .map(|r| r.revenue_usd)
        .sum();
    let avg = (cur.visitors > 0 && attributed > 0.0).then(|| attributed / cur.visitors as f64);
    let cards: Vec<cards::TabCard> = picks
        .iter()
        .zip(&breakdowns)
        .map(|(pick, rows)| cards::card(pick, rows, avg, &base, &params))
        .collect();
    let ai_rows = insight::ai_rows(&sources);
    let (visitors_share, revenue_share) = insight::ai_shares(cur, &report.channels);
    let insight = insight::summary(cur, avg, &report.channels, &ai_rows, &report.landing_pages);
    let ai = AiView {
        visitors_share,
        revenue_share,
        line: insight::ai_line(&ai_rows),
        rows: ai_rows
            .iter()
            .take(5)
            .map(|r| {
                (
                    icons::source(&r.host, 16),
                    r.name.clone(),
                    group(r.visitors),
                    money(r.revenue_usd),
                )
            })
            .collect(),
    };
    let credit = format!(
        "Credit: {}, {} days",
        if model == "first_touch" {
            "first visit"
        } else {
            "last non-direct visit"
        },
        window_days
    );

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
        report_href: site_url("report"),
        empty,
        insight,
        kpis,
        cards,
        unmatched: insight::unmatched(&report.channels),
        credit,
        ai,
        paths: recent
            .rows
            .iter()
            .zip(&journeys)
            .map(|(r, j)| path_row(r, j.as_ref(), &ctx.site.domain, ctx.offset))
            .collect(),
        payments_total: plural(recent.total, "payment"),
        payments_href: site_url("payments"),
        range_param,
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
    /// Payment plus each goal completed in the range.
    goals: Vec<Opt>,
    goal_label: String,
    is_goal: bool,
    /// Query params the search form keeps.
    hidden: Vec<(&'static str, String)>,
    search: String,
    customers: Vec<CustomerView>,
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
    let search =
        q.q.as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_lowercase();
    let wanted_goal = q.goal.clone().unwrap_or_default();
    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let (domain, offset) = (ctx.site.domain.clone(), ctx.offset);
    let query = search.clone();
    let result = blocking(move || {
        let s = store.payment_summary(&id, &p)?;
        let goals = store.goal_counts(&id, &p)?;
        let goal = goals
            .iter()
            .find(|g| g.name == wanted_goal)
            .map(|g| g.name.clone());
        let (limit, skip) = if query.is_empty() {
            (PER_PAGE, (page - 1) * PER_PAGE)
        } else {
            (SEARCH_LIMIT, 0)
        };
        let (total, mut customers) = match &goal {
            Some(g) => {
                let (total, rows) = store.goal_list(&id, &p, g, limit, skip)?;
                let who: Vec<Who> = rows
                    .iter()
                    .map(|r| Who {
                        visitor_id: Some(r.visitor_id.clone()),
                        session_id: Some(r.session_id.clone()),
                        ts: r.ts,
                    })
                    .collect();
                let activity = store.activity(&id, &p.tz, &who)?;
                let views = rows
                    .iter()
                    .zip(&activity)
                    .map(|(r, a)| goal_customer(r, a, &domain, offset));
                (total, views.collect::<Vec<_>>())
            }
            None => {
                let list = store.payment_list(&id, &p, filter, limit, skip)?;
                let who: Vec<Who> = list.rows.iter().map(Who::of).collect();
                let activity = store.activity(&id, &p.tz, &who)?;
                let views = list
                    .rows
                    .iter()
                    .zip(&activity)
                    .map(|(r, a)| payment_customer(r, a, &domain, offset));
                (list.total, views.collect::<Vec<_>>())
            }
        };
        let total = if query.is_empty() {
            total
        } else {
            customers.retain(|c| c.matches(&query));
            let n = customers.len() as u64;
            customers = customers
                .into_iter()
                .skip(((page - 1) * PER_PAGE) as usize)
                .take(PER_PAGE as usize)
                .collect();
            n
        };
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
        Ok((s, goals, goal, total, customers, ever))
    })
    .await;
    let (s, goals, goal, total, customers, ever) = match result {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let counts = [s.payments, s.attributed, s.unattributed, s.refunded];
    let base = sites::app_path(&ctx.site.domain, "payments");
    let range_param = if rkey == rdefault { "" } else { rkey };
    let filter_param = if fkey == "all" || goal.is_some() {
        ""
    } else {
        fkey
    };
    let goal_param = goal.as_deref().map(encode).unwrap_or_default();
    let q_param = encode(&search);
    let page_href = |n: u32| {
        let n = n.to_string();
        href(
            &base,
            &[
                ("range", range_param),
                ("filter", filter_param),
                ("goal", &goal_param),
                ("q", &q_param),
                ("page", if n == "1" { "" } else { &n }),
            ],
        )
    };
    let shown = customers.len() as u64;
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
        ranges: range_opts(
            &base,
            rkey,
            rdefault,
            &[
                ("filter", filter_param),
                ("goal", &goal_param),
                ("q", &q_param),
            ],
        ),
        goal_label: match &goal {
            Some(g) => format!(
                "{g} {}",
                group(
                    goals
                        .iter()
                        .find(|c| &c.name == g)
                        .map_or(0, |c| c.completions)
                )
            ),
            None => format!("Payment {}", group(s.payments)),
        },
        goals: std::iter::once(Opt {
            label: "Payment".into(),
            href: href(&base, &[("range", range_param)]),
            on: goal.is_none(),
            count: group(s.payments),
        })
        .chain(goals.iter().map(|g| Opt {
            label: g.name.clone(),
            href: href(&base, &[("range", range_param), ("goal", &encode(&g.name))]),
            on: goal.as_deref() == Some(g.name.as_str()),
            count: group(g.completions),
        }))
        .collect(),
        is_goal: goal.is_some(),
        hidden: [("range", range_param), ("filter", filter_param)]
            .into_iter()
            .chain(goal.as_deref().map(|g| ("goal", g)))
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k, v.to_string()))
            .collect(),
        search: search.clone(),
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
        customers,
        filter_label: flabel,
        prev: if page > 1 {
            page_href(page - 1)
        } else {
            String::new()
        },
        next: if first + shown < total {
            page_href(page + 1)
        } else {
            String::new()
        },
        showing: if shown > 0 {
            format!(
                "Showing {} to {} of {}",
                group(first + 1),
                group(first + shown),
                group(total)
            )
        } else {
            String::new()
        },
        total,
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
    name: String,
    avatar: String,
    amount: String,
    charged: String,
    provider: &'static str,
    provider_icon: String,
    payment_id: String,
    customer_id: String,
    visitor_id: String,
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
    let alias = alias::alias(&payer_key(p));
    JourneyView {
        name: alias.name,
        avatar: alias.avatar,
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
