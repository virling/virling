use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use time::macros::format_description;
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::app::AppState;
use crate::auth::Account;
use crate::channel::{source_domain, source_name};
use crate::dashboard::{
    Opt, PER_PAGE, Shell, ago, amount, blocking, day, duration, group, href, link, load, local,
    money, period, plural, provider_short, range, range_opts, status,
};
use crate::icons;
use crate::onboarding::render;
use crate::sites;
use crate::stats::PaymentRow;
use crate::visitors::{Entry, Step, Timeline, VisitorFilter, VisitorRow, label};

/// Percent-encodes a query value or path segment.
fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

pub fn visitor_href(domain: &str, visitor_id: &str) -> String {
    format!(
        "{}/{}",
        sites::app_path(domain, "visitors"),
        enc(visitor_id)
    )
}

#[derive(Deserialize, Default)]
pub struct VisitorsQuery {
    range: Option<String>,
    channel: Option<String>,
    source: Option<String>,
    goal: Option<String>,
    exit: Option<String>,
    paid: Option<String>,
    page: Option<u32>,
}

/// An option in a filter dropdown, with an optional logo.
pub struct MenuOpt {
    label: String,
    icon: String,
    href: String,
    on: bool,
    count: String,
}

pub struct VisitorView {
    href: String,
    label: String,
    paid: bool,
    icon: String,
    source: String,
    channel: String,
    visits: String,
    pageviews: String,
    last_seen: String,
    last_full: String,
    exit_page: String,
    exit_icon: String,
    exit_link: String,
    goals: Vec<String>,
    more_goals: usize,
    revenue: String,
}

#[derive(Template)]
#[template(path = "visitors.html")]
struct VisitorsPage {
    shell: Shell,
    title: &'static str,
    range_label: &'static str,
    ranges: Vec<Opt>,
    source_label: String,
    all_sources: MenuOpt,
    sources: Vec<MenuOpt>,
    channels: Vec<MenuOpt>,
    goal_label: String,
    goals: Vec<MenuOpt>,
    exits: Vec<Opt>,
    paid: Vec<Opt>,
    rows: Vec<VisitorView>,
    filtered: bool,
    clear_href: String,
    showing: String,
    prev: String,
    next: String,
}

fn visitor_view(
    v: &VisitorRow,
    domain: &str,
    offset: UtcOffset,
    now: OffsetDateTime,
) -> VisitorView {
    VisitorView {
        href: visitor_href(domain, &v.visitor_id),
        label: label(&v.visitor_id),
        paid: v.payments > 0,
        icon: icons::source(&v.source, 16),
        channel: if v.channel == v.source {
            String::new()
        } else {
            v.channel.clone()
        },
        source: v.source.clone(),
        visits: group(v.visits),
        pageviews: group(v.pageviews),
        last_seen: ago(v.last_seen, now),
        last_full: local(v.last_seen, offset),
        exit_page: v.exit_page.clone().unwrap_or_default(),
        exit_icon: v
            .exit_link
            .as_deref()
            .map(|l| icons::source(l.split('/').next().unwrap_or(l), 16))
            .unwrap_or_default(),
        exit_link: v.exit_link.clone().unwrap_or_default(),
        goals: v.goals.iter().take(2).cloned().collect(),
        more_goals: v.goals.len().saturating_sub(2),
        revenue: if v.payments > 0 {
            money(v.revenue_usd)
        } else {
            String::new()
        },
    }
}

pub async fn list(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<VisitorsQuery>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "visitors").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let (rkey, rlabel, days, rdefault) = range(&ctx, q.range.as_deref());
    let p = period(&ctx, days);
    let page = q.page.unwrap_or(1).max(1);
    let nonempty = |v: &Option<String>| v.clone().filter(|s| !s.is_empty());
    let channel = nonempty(&q.channel);
    let source = nonempty(&q.source).filter(|_| channel.is_none());
    let goal = nonempty(&q.goal);
    let exit = q.exit.as_deref().filter(|e| matches!(*e, "link" | "page"));
    let paid = q.paid.as_deref().filter(|e| matches!(*e, "yes" | "no"));

    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let (want_source, want_channel, want_goal) = (source.clone(), channel.clone(), goal.clone());
    let (clicked_out, want_paid) = (exit.map(|e| e == "link"), paid.map(|e| e == "yes"));
    let result = blocking(move || {
        let facets = store.visitor_facets(&id, &p)?;
        let hosts = want_source.map(|s| {
            let mut hosts: Vec<String> = facets
                .sources
                .iter()
                .filter_map(|(_, h, _)| h.clone())
                .filter(|h| source_domain(h) == s)
                .collect();
            hosts.dedup();
            if hosts.is_empty() {
                hosts.push(s);
            }
            hosts
        });
        let filter = VisitorFilter {
            channel: want_channel,
            hosts,
            goal: want_goal,
            clicked_out,
            paid: want_paid,
        };
        let list = store.visitor_list(&id, &p, &filter, PER_PAGE, (page - 1) * PER_PAGE)?;
        Ok((facets, list))
    })
    .await;
    let (facets, list) = match result {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };

    let base = sites::app_path(&ctx.site.domain, "visitors");
    let enc_opt = |v: &Option<String>| v.as_deref().map(enc).unwrap_or_default();
    let (channel_q, source_q, goal_q) = (enc_opt(&channel), enc_opt(&source), enc_opt(&goal));
    let range_q = if rkey == rdefault { "" } else { rkey };
    let params: [(&str, &str); 6] = [
        ("range", range_q),
        ("channel", &channel_q),
        ("source", &source_q),
        ("goal", &goal_q),
        ("exit", exit.unwrap_or("")),
        ("paid", paid.unwrap_or("")),
    ];

    let mut channels: Vec<(String, u64)> = Vec::new();
    let mut sources: Vec<(String, String, u64)> = Vec::new();
    for (ch, host, n) in &facets.sources {
        match channels.iter_mut().find(|(c, _)| c == ch) {
            Some(c) => c.1 += n,
            None => channels.push((ch.clone(), *n)),
        }
        if let Some(h) = host {
            let d = source_domain(h);
            match sources.iter_mut().find(|(k, _, _)| *k == d) {
                Some(s) => s.2 += n,
                None => sources.push((d, source_name(h), *n)),
            }
        }
    }
    channels.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sources.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    let source_label = match (&channel, &source) {
        (Some(c), _) => c.clone(),
        (_, Some(s)) => sources
            .iter()
            .find(|(d, _, _)| d == s)
            .map(|(_, name, _)| name.clone())
            .unwrap_or_else(|| s.clone()),
        _ => "All sources".into(),
    };
    let all_sources = MenuOpt {
        label: "All sources".into(),
        icon: String::new(),
        href: link(&base, &params, &[("channel", ""), ("source", "")]),
        on: channel.is_none() && source.is_none(),
        count: String::new(),
    };
    let channel_opts = channels
        .iter()
        .map(|(c, n)| MenuOpt {
            label: c.clone(),
            icon: icons::source(c, 16),
            href: link(&base, &params, &[("channel", &enc(c)), ("source", "")]),
            on: channel.as_deref() == Some(c.as_str()),
            count: group(*n),
        })
        .collect();
    let source_opts = sources
        .iter()
        .take(20)
        .map(|(d, name, n)| MenuOpt {
            label: name.clone(),
            icon: icons::source(name, 16),
            href: link(&base, &params, &[("source", &enc(d)), ("channel", "")]),
            on: source.as_deref() == Some(d.as_str()),
            count: group(*n),
        })
        .collect();
    let mut goal_opts = Vec::new();
    if !facets.goals.is_empty() || goal.is_some() {
        goal_opts.push(MenuOpt {
            label: "Any goal".into(),
            icon: String::new(),
            href: link(&base, &params, &[("goal", "")]),
            on: goal.is_none(),
            count: String::new(),
        });
        goal_opts.extend(facets.goals.iter().map(|(g, n)| MenuOpt {
            label: g.clone(),
            icon: String::new(),
            href: link(&base, &params, &[("goal", &enc(g))]),
            on: goal.as_deref() == Some(g.as_str()),
            count: group(*n),
        }));
    }
    let seg = |key: &str, current: Option<&str>, opts: &[(&str, &str)]| -> Vec<Opt> {
        opts.iter()
            .map(|(v, l)| Opt {
                label: l.to_string(),
                href: link(&base, &params, &[(key, v)]),
                on: current.unwrap_or("") == *v,
                count: String::new(),
            })
            .collect()
    };
    let exits = seg(
        "exit",
        exit,
        &[
            ("", "Any exit"),
            ("link", "Clicked out"),
            ("page", "Left on a page"),
        ],
    );
    let paid_opts = seg(
        "paid",
        paid,
        &[("", "Everyone"), ("yes", "Paid"), ("no", "Not paid")],
    );

    let page_href = |n: u32| {
        let n = n.to_string();
        link(&base, &params, &[("page", if n == "1" { "" } else { &n })])
    };
    let shown = list.rows.len() as u64;
    let first = (page as u64 - 1) * PER_PAGE as u64;
    let now = OffsetDateTime::now_utc();
    render(VisitorsPage {
        title: "Visitors",
        range_label: rlabel,
        ranges: range_opts(&base, rkey, rdefault, &params[1..]),
        source_label,
        all_sources,
        sources: source_opts,
        channels: channel_opts,
        goal_label: goal.clone().unwrap_or_else(|| "Any goal".into()),
        goals: goal_opts,
        exits,
        paid: paid_opts,
        rows: list
            .rows
            .iter()
            .map(|v| visitor_view(v, &ctx.site.domain, ctx.offset, now))
            .collect(),
        filtered: channel.is_some()
            || source.is_some()
            || goal.is_some()
            || exit.is_some()
            || paid.is_some(),
        clear_href: href(&base, &[("range", range_q)]),
        showing: if shown > 0 {
            format!(
                "Showing {} to {} of {}",
                group(first + 1),
                group(first + shown),
                plural(list.total, "visitor")
            )
        } else {
            String::new()
        },
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
        shell: ctx.shell,
    })
}

pub enum StepView {
    Page {
        time: String,
        path: String,
        stay: String,
    },
    Goal {
        time: String,
        name: String,
    },
    Exit {
        time: String,
        page: String,
        icon: String,
        link: String,
    },
    Pay(PayStep),
}

pub struct PayStep {
    time: String,
    amount: String,
    renewal: bool,
    status: &'static str,
    status_class: &'static str,
    refunded: String,
    provider_icon: String,
    provider: &'static str,
    href: String,
}

pub struct VisitView {
    n: usize,
    date: String,
    length: String,
    icon: String,
    source: String,
    channel: String,
    campaign: String,
    pages: String,
    after_paid: bool,
    steps: Vec<StepView>,
}

pub enum EntryView {
    Visit(VisitView),
    Pay(PayStep),
}

#[derive(Template)]
#[template(path = "visitor.html")]
struct VisitorPage {
    shell: Shell,
    title: &'static str,
    back: String,
    label: String,
    paid: bool,
    revenue: String,
    payments: String,
    flag: String,
    country: String,
    browser: String,
    first_seen: String,
    last_seen: String,
    first_icon: String,
    first_source: String,
    first_channel: String,
    visits: String,
    pageviews: String,
    goals: String,
    exit: String,
    entries: Vec<EntryView>,
}

/// "45s", "3m 05s", "1h 12m"
fn stay(secs: i64) -> String {
    match secs.max(0) {
        s @ 0..60 => format!("{s}s"),
        s @ 60..3600 => format!("{}m {:02}s", s / 60, s % 60),
        s => format!("{}h {:02}m", s / 3600, s % 3600 / 60),
    }
}

fn clock(ts: OffsetDateTime, offset: UtcOffset) -> String {
    ts.to_offset(offset)
        .format(format_description!("[hour]:[minute]:[second]"))
        .unwrap_or_default()
}

fn pay_step(p: &PaymentRow, domain: &str, time: String) -> PayStep {
    let (status, status_class) = status(&p.status);
    PayStep {
        time,
        amount: amount(p),
        renewal: p.is_renewal,
        status,
        status_class,
        refunded: match p.amount_usd {
            Some(usd) if p.refunded > 0 && p.amount > 0 => {
                money(usd * p.refunded as f64 / p.amount as f64)
            }
            _ => String::new(),
        },
        provider_icon: icons::provider(&p.provider, 14),
        provider: provider_short(&p.provider),
        href: format!(
            "{}/{}/{}",
            sites::app_path(domain, "payments"),
            p.provider,
            enc(&p.provider_payment_id)
        ),
    }
}

fn timeline_view(t: &Timeline, domain: &str, offset: UtcOffset) -> Vec<EntryView> {
    let first_paid = t.payments.first().map(|p| p.ts);
    let mut n = 0;
    t.entries
        .iter()
        .map(|e| match e {
            Entry::Payment(p) => EntryView::Pay(pay_step(p, domain, local(p.ts, offset))),
            Entry::Visit(v) => {
                n += 1;
                let len = v.end - v.start;
                EntryView::Visit(VisitView {
                    n,
                    date: local(v.start, offset),
                    length: if len < Duration::minutes(1) {
                        "under a minute".into()
                    } else {
                        duration(len)
                    },
                    icon: icons::source(&v.source, 16),
                    channel: if v.channel == v.source {
                        String::new()
                    } else {
                        v.channel.clone()
                    },
                    source: v.source.clone(),
                    campaign: v.campaign.clone().unwrap_or_default(),
                    pages: plural(v.pageviews, "page"),
                    after_paid: first_paid.is_some_and(|t| v.start > t),
                    steps: v
                        .steps
                        .iter()
                        .map(|s| match s {
                            Step::Page { ts, path, secs } => StepView::Page {
                                time: clock(*ts, offset),
                                path: path.clone(),
                                stay: secs.map(stay).unwrap_or_default(),
                            },
                            Step::Goal { ts, name } => StepView::Goal {
                                time: clock(*ts, offset),
                                name: name.clone(),
                            },
                            Step::Exit { ts, page, link } => StepView::Exit {
                                time: clock(*ts, offset),
                                page: page.clone(),
                                icon: link
                                    .as_deref()
                                    .map(|l| icons::source(l.split('/').next().unwrap_or(l), 16))
                                    .unwrap_or_default(),
                                link: link.clone().unwrap_or_default(),
                            },
                            Step::Payment(p) => {
                                StepView::Pay(pay_step(p, domain, clock(p.ts, offset)))
                            }
                        })
                        .collect(),
                })
            }
        })
        .collect()
}

pub async fn timeline(
    State(state): State<AppState>,
    account: Account,
    Path((domain, visitor_id)): Path<(String, String)>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "visitors").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let store = state.events.clone();
    let id = ctx.site.id.clone();
    let vid = visitor_id.clone();
    let t = match blocking(move || store.timeline(&id, &vid)).await {
        Ok(Some(t)) => t,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(code) => return code.into_response(),
    };
    let visits: Vec<_> = t.visits().collect();
    let first = visits.first();
    let pageviews: u64 = visits.iter().map(|v| v.pageviews).sum();
    let goals = visits
        .iter()
        .flat_map(|v| &v.steps)
        .filter(|s| matches!(s, Step::Goal { .. }))
        .count() as u64;
    let last_exit = visits
        .iter()
        .rev()
        .flat_map(|v| v.steps.iter().rev())
        .filter_map(|s| match s {
            Step::Exit { page, link, .. } => Some(match link {
                Some(l) => format!("Clicked out to {l}"),
                None => format!("Left from {page}"),
            }),
            _ => None,
        })
        .next()
        .unwrap_or_default();
    let revenue: f64 = t.payments.iter().filter_map(|p| p.net_usd).sum();
    let last = visits.last().map(|v| v.end);
    let now = OffsetDateTime::now_utc();
    render(VisitorPage {
        title: "Visitor",
        back: sites::app_path(&ctx.site.domain, "visitors"),
        label: label(&t.visitor_id),
        paid: !t.payments.is_empty(),
        revenue: money(revenue),
        payments: plural(t.payments.len() as u64, "payment"),
        flag: t.country.as_deref().map(icons::flag).unwrap_or_default(),
        country: t.country.as_deref().map(icons::country).unwrap_or_default(),
        browser: match (&t.browser, &t.device) {
            (Some(b), Some(d)) if d != "Unknown" => format!("{b} on {d}"),
            (Some(b), _) => b.clone(),
            _ => String::new(),
        },
        first_seen: first
            .map(|v| day(v.start.to_offset(ctx.offset).date()))
            .unwrap_or_default(),
        last_seen: last.map(|t| ago(t, now)).unwrap_or_default(),
        first_icon: first
            .map(|v| icons::source(&v.source, 16))
            .unwrap_or_default(),
        first_source: first.map(|v| v.source.clone()).unwrap_or_default(),
        first_channel: first
            .filter(|v| v.channel != v.source)
            .map(|v| v.channel.clone())
            .unwrap_or_default(),
        visits: group(visits.len() as u64),
        pageviews: group(pageviews),
        goals: group(goals),
        exit: last_exit,
        entries: timeline_view(&t, &ctx.site.domain, ctx.offset),
        shell: ctx.shell,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_read_naturally() {
        assert_eq!(stay(45), "45s");
        assert_eq!(stay(125), "2m 05s");
        assert_eq!(stay(3720), "1h 02m");
    }

    #[test]
    fn hrefs_encode_ids() {
        assert_eq!(
            visitor_href("example.com", "a b/c"),
            "/app/example.com/visitors/a%20b%2Fc"
        );
    }
}
