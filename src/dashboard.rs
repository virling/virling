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
use crate::stats::{DayPoint, Dimension, Journey, PaymentFilter, PaymentRow, Period, Sort, Totals};

const RANGES: [(&str, &str, i64); 5] = [
    ("today", "Today", 1),
    ("7d", "Last 7 days", 7),
    ("30d", "Last 30 days", 30),
    ("90d", "Last 90 days", 90),
    ("12m", "Last 12 months", 365),
];
const DIMENSIONS: [(&str, &str); 6] = [
    ("channel", "Channel"),
    ("referrer", "Referrer"),
    ("campaign", "Campaign"),
    ("landing_page", "Landing page"),
    ("country", "Country"),
    ("device", "Device"),
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

fn cents(usd: f64) -> String {
    format!("${:.2}", usd)
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

pub struct Kpi {
    label: &'static str,
    value: String,
    swatch: &'static str,
    delta: String,
    trend: &'static str,
}

fn kpi(
    label: &'static str,
    swatch: &'static str,
    value: String,
    cur: Option<f64>,
    prev: Option<f64>,
) -> Kpi {
    let (delta, trend) = match (cur, prev) {
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
            (text, trend)
        }
        _ => (String::new(), "flat"),
    };
    Kpi {
        label,
        value,
        swatch,
        delta,
        trend,
    }
}

fn kpis(cur: &Totals, prev: &Totals) -> Vec<Kpi> {
    vec![
        kpi(
            "Visitors",
            "visitors",
            group(cur.visitors),
            Some(cur.visitors as f64),
            Some(prev.visitors as f64),
        ),
        kpi(
            "Revenue",
            "revenue",
            money(cur.revenue_usd),
            Some(cur.revenue_usd),
            Some(prev.revenue_usd),
        ),
        kpi(
            "Revenue per visitor",
            "",
            cents(cur.revenue_per_visitor.unwrap_or(0.0)),
            cur.revenue_per_visitor,
            prev.revenue_per_visitor,
        ),
        kpi(
            "Conversion",
            "",
            format!("{:.1}%", cur.conversion.unwrap_or(0.0) * 100.0),
            cur.conversion,
            prev.conversion,
        ),
    ]
}

pub struct Bar {
    label: String,
    visitors: String,
    revenue: String,
    vh: u32,
    rh: u32,
    edge: &'static str,
}

/// One bar pair per day; months when the range is a year.
fn bars(points: &[DayPoint]) -> Vec<Bar> {
    let mut buckets: Vec<(String, u64, f64)> = Vec::new();
    let monthly = points.len() > 120;
    let mut last_month = None;
    for p in points {
        let month = (p.date.year(), p.date.month());
        if monthly && last_month == Some(month) {
            let b = buckets.last_mut().unwrap();
            b.1 += p.visitors;
            b.2 += p.revenue_usd;
            continue;
        }
        last_month = Some(month);
        let label = if monthly {
            p.date
                .format(format_description!("[month repr:short] [year]"))
                .unwrap_or_default()
        } else {
            day(p.date)
        };
        buckets.push((label, p.visitors, p.revenue_usd));
    }
    let vmax = buckets.iter().map(|b| b.1).max().unwrap_or(0).max(1) as f64;
    let rmax = buckets.iter().map(|b| b.2).fold(0.0, f64::max).max(0.01);
    let height = |v: f64, max: f64| {
        if v <= 0.0 {
            0
        } else {
            ((v / max * 100.0).round() as u32).max(2)
        }
    };
    let n = buckets.len();
    buckets
        .into_iter()
        .enumerate()
        .map(|(i, (label, visitors, revenue))| Bar {
            label,
            vh: height(visitors as f64, vmax),
            rh: height(revenue, rmax),
            visitors: group(visitors),
            revenue: money(revenue),
            edge: if n > 6 && i < 3 {
                "edge-l"
            } else if n > 6 && i + 3 >= n {
                "edge-r"
            } else {
                ""
            },
        })
        .collect()
}

pub struct BreakdownView {
    label: String,
    unattributed: bool,
    visitors: String,
    revenue: String,
    width: u32,
    per_visitor: String,
    dim: bool,
}

pub struct LatestView {
    source: String,
    unattributed: bool,
    ago: String,
    provider: &'static str,
    amount: String,
    href: String,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardPage {
    shell: Shell,
    title: &'static str,
    range_label: &'static str,
    ranges: Vec<Opt>,
    kpis: Vec<Kpi>,
    bars: Vec<Bar>,
    axis: Vec<String>,
    tabs: Vec<Opt>,
    sorts: Vec<Opt>,
    rows: Vec<BreakdownView>,
    latest: Vec<LatestView>,
    payments_href: String,
    empty: bool,
}

#[derive(Deserialize, Default)]
pub struct ReportQuery {
    range: Option<String>,
    by: Option<String>,
    sort: Option<String>,
    filter: Option<String>,
    page: Option<u32>,
}

pub async fn dashboard(
    State(state): State<AppState>,
    account: Account,
    Path(site_id): Path<String>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let ctx = match load(&state, &account, &site_id, "dashboard").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let (rkey, rlabel, days) = range(q.range.as_deref());
    let p = period(&ctx, days);
    let (by, dimension) = DIMENSIONS
        .iter()
        .map(|(k, _)| *k)
        .find(|k| Some(*k) == q.by.as_deref())
        .and_then(|k| Some((k, Dimension::parse(k)?)))
        .unwrap_or(("channel", Dimension::Channel));
    let (sort_key, sort) = match q.sort.as_deref() {
        Some("visitors") => ("visitors", Sort::Visitors),
        _ => ("revenue", Sort::Revenue),
    };
    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let result = blocking(move || {
        Ok((
            store.report(&id, &p)?,
            store.breakdown(&id, &p, dimension, sort)?,
            store.payment_list(&id, &p, PaymentFilter::All, 6, 0)?,
        ))
    })
    .await;
    let (report, rows, latest) = match result {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let (overview, points) = (report.overview, report.days);
    let base = format!("/app/sites/{}/dashboard", ctx.site.id);
    let range_param = if rkey == "30d" { "" } else { rkey };
    let by_param = if by == "channel" { "" } else { by };
    let sort_param = if sort_key == "revenue" { "" } else { sort_key };
    let bars = bars(&points);
    let axis = match bars.len() {
        0 => Vec::new(),
        1 => vec![bars[0].label.clone()],
        n => vec![
            bars[0].label.clone(),
            bars[n / 2].label.clone(),
            bars[n - 1].label.clone(),
        ],
    };
    let rmax = rows.iter().map(|r| r.revenue_usd).fold(0.0, f64::max);
    let now = OffsetDateTime::now_utc();
    let site_base = format!("/app/sites/{}", ctx.site.id);
    render(DashboardPage {
        title: "Dashboard",
        range_label: rlabel,
        ranges: range_opts(&base, rkey, &[("by", by_param), ("sort", sort_param)]),
        kpis: kpis(&overview.current, &overview.previous),
        empty: overview.current.visitors == 0 && overview.current.payments == 0,
        bars,
        axis,
        tabs: DIMENSIONS
            .iter()
            .map(|(k, label)| Opt {
                label: label.to_string(),
                href: href(
                    &base,
                    &[
                        ("range", range_param),
                        ("by", if *k == "channel" { "" } else { k }),
                        ("sort", sort_param),
                    ],
                ),
                on: *k == by,
                count: String::new(),
            })
            .collect(),
        sorts: [("revenue", "Revenue"), ("visitors", "Visitors")]
            .iter()
            .map(|(k, label)| Opt {
                label: label.to_string(),
                href: href(
                    &base,
                    &[
                        ("range", range_param),
                        ("by", by_param),
                        ("sort", if *k == "revenue" { "" } else { k }),
                    ],
                ),
                on: *k == sort_key,
                count: String::new(),
            })
            .collect(),
        rows: rows
            .iter()
            .take(12)
            .map(|r| BreakdownView {
                label: r.label.clone(),
                unattributed: r.unattributed,
                visitors: if r.unattributed {
                    String::new()
                } else {
                    group(r.visitors)
                },
                revenue: money(r.revenue_usd),
                width: if rmax > 0.0 {
                    (r.revenue_usd / rmax * 100.0).round() as u32
                } else {
                    0
                },
                per_visitor: r.revenue_per_visitor.map(cents).unwrap_or_default(),
                dim: r.revenue_per_visitor.is_none_or(|v| v < 0.05),
            })
            .collect(),
        latest: latest
            .rows
            .iter()
            .map(|p| LatestView {
                source: p
                    .touch
                    .as_ref()
                    .map_or_else(|| "Unattributed".into(), |t| t.source.clone()),
                unattributed: p.touch.is_none(),
                ago: ago(p.ts, now),
                provider: provider_name(&p.provider),
                amount: native(p.amount, &p.currency),
                href: format!(
                    "{site_base}/payments/{}/{}",
                    p.provider, p.provider_payment_id
                ),
            })
            .collect(),
        payments_href: href(&format!("{site_base}/payments"), &[("range", range_param)]),
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
