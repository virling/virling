//! The Visitors, Latest payments and Customers who stay views.

use std::collections::{HashMap, HashSet};

use time::{Duration, OffsetDateTime, UtcOffset};

use super::{
    Chip, Href, Opt, ago, amount, group, local, money, path_chips, plural, provider_name, since,
    source_of, usd, visitor_pages,
};
use crate::alias::Names;
use crate::events::EventStore;
use crate::retention::{LTV_DAYS, Mrr, Retention, RetentionRow};
use crate::stats::{
    Activity, Filter, Journey, PaymentFilter, PaymentPage, PaymentSummary, Period, ProviderSplit,
    Totals, Who,
};
use crate::visitors::{VisitorFilter, VisitorRow};
use crate::{icons, sites};

/// Rows each list shows.
const ROWS: usize = 6;

pub struct VisitorsData {
    pub(super) live: Vec<(VisitorRow, Activity)>,
    pub(super) recent: Vec<(VisitorRow, Activity)>,
    pub(super) customers: Vec<(VisitorRow, Activity)>,
    pub(super) today: Totals,
    pub(super) goals_today: u64,
    pub(super) paying: u64,
    /// When the lists were read.
    pub(super) now: OffsetDateTime,
    /// Visitors with a visible tab of the site open.
    pub(super) online: HashSet<String>,
}

fn with_activity(
    store: &dyn EventStore,
    id: &str,
    tz: &str,
    rows: Vec<VisitorRow>,
) -> anyhow::Result<Vec<(VisitorRow, Activity)>> {
    let who: Vec<Who> = rows
        .iter()
        .map(|r| Who {
            visitor_id: Some(r.visitor_id.clone()),
            session_id: None,
            ts: r.last_seen,
        })
        .collect();
    Ok(rows
        .into_iter()
        .zip(store.activity(id, tz, &who)?)
        .collect())
}

pub fn load_visitors(
    store: &dyn EventStore,
    id: &str,
    p: &Period,
    tz: &str,
    now: OffsetDateTime,
    online: HashSet<String>,
) -> anyhow::Result<VisitorsData> {
    let offset = store.utc_offset(tz)?;
    let today = now.to_offset(offset).date();
    let day = Period {
        from: today - Duration::days(1),
        to: today,
        tz: tz.to_string(),
    };
    let all = store.visitor_list(id, &day, &VisitorFilter::default(), 50, 0)?;
    let live: Vec<VisitorRow> = all
        .rows
        .iter()
        .filter(|r| online.contains(&r.visitor_id))
        .take(ROWS)
        .cloned()
        .collect();
    let recent: Vec<VisitorRow> = all.rows.into_iter().take(ROWS).collect();
    let paid = VisitorFilter {
        paid: Some(true),
        ..Default::default()
    };
    let customers = store.visitor_list(id, p, &paid, ROWS as u32, 0)?;
    let today_p = Period {
        from: today,
        to: today,
        tz: tz.to_string(),
    };
    Ok(VisitorsData {
        live: with_activity(store, id, tz, live)?,
        recent: with_activity(store, id, tz, recent)?,
        paying: customers.total,
        customers: with_activity(store, id, tz, customers.rows)?,
        today: store.overview(id, &today_p)?.current,
        goals_today: store
            .goal_counts(id, &today_p)?
            .iter()
            .map(|g| g.completions)
            .sum(),
        now,
        online,
    })
}

pub struct VRow {
    pub(super) avatar: String,
    pub(super) name: String,
    pub(super) tag: String,
    pub(super) tag_class: &'static str,
    pub(super) online: bool,
    pub(super) flag: String,
    pub(super) place: String,
    pub(super) device_icon: String,
    pub(super) device: String,
    pub(super) src_icon: String,
    pub(super) src: String,
    pub(super) chan: String,
    pub(super) mid_label: &'static str,
    pub(super) mid: String,
    pub(super) mid_sub: String,
    pub(super) right: String,
    pub(super) right_sub: String,
    pub(super) href: String,
}

pub struct VisitorsView {
    pub(super) tabs: Vec<Opt>,
    pub(super) sub: &'static str,
    pub(super) stats: Vec<(String, &'static str)>,
    pub(super) empty: Option<(&'static str, &'static str)>,
    pub(super) cols: [&'static str; 3],
    pub(super) rows: Vec<VRow>,
    pub(super) foot: &'static str,
    pub(super) all_href: String,
}

#[allow(clippy::too_many_arguments)]
fn vrow(
    r: &VisitorRow,
    a: &Activity,
    names: &Names,
    tab: &str,
    domain: &str,
    now: OffsetDateTime,
    offset: UtcOffset,
    online: bool,
) -> VRow {
    let who = names.get(&r.who);
    let (tag, tag_class) = if r.payments > 0 {
        (
            if tab == "customers" && online {
                "Online now".to_string()
            } else {
                "Customer".to_string()
            },
            "cust",
        )
    } else if let Some(g) = r.goals.first() {
        (format!("Goal: {g}"), "hot")
    } else if r.visits > 1 {
        (format!("Returning, {} visits", r.visits), "")
    } else {
        ("New".to_string(), "")
    };
    let pages = plural(r.pageviews, "page");
    let (mid_label, mid, mid_sub, right, right_sub) = match tab {
        "customers" => (
            "Paid",
            plural(r.payments, "payment"),
            format!("Last seen {}", local(r.last_seen, offset)),
            money(r.revenue_usd),
            "paid so far".to_string(),
        ),
        "live" => (
            "Now on",
            r.exit_page.clone().unwrap_or_default(),
            pages,
            ago(r.last_seen, now),
            "last active".to_string(),
        ),
        _ => (
            "Last page",
            r.exit_page.clone().unwrap_or_default(),
            pages,
            ago(r.last_seen, now),
            String::new(),
        ),
    };
    VRow {
        avatar: who.avatar,
        name: who.name,
        tag,
        tag_class,
        online,
        flag: a.country.as_deref().map(icons::flag).unwrap_or_default(),
        place: a.country.as_deref().map(icons::country).unwrap_or_default(),
        device_icon: a.device.as_deref().map(icons::device).unwrap_or_default(),
        device: a
            .device
            .clone()
            .filter(|d| d != "Unknown")
            .unwrap_or_default(),
        src_icon: icons::source(&r.source, 16),
        chan: if r.channel == r.source {
            String::new()
        } else {
            r.channel.clone()
        },
        src: r.source.clone(),
        mid_label,
        mid,
        mid_sub,
        right,
        right_sub,
        href: visitor_pages::visitor_href(domain, &r.visitor_id),
    }
}

/// Keys of everyone the visitors view may name.
pub fn visitor_keys(v: &VisitorsData) -> Vec<String> {
    [&v.live, &v.recent, &v.customers]
        .into_iter()
        .flatten()
        .map(|(r, _)| r.who.clone())
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn visitors_view(
    v: VisitorsData,
    names: &Names,
    domain: &str,
    offset: UtcOffset,
    rest: &HashMap<String, String>,
    href: &Href,
    live_now: u64,
    connected: bool,
) -> VisitorsView {
    let now = v.now;
    let tab = match rest.get("vtab").map(String::as_str) {
        Some("recent") => "recent",
        Some("customers") => "customers",
        _ => "live",
    };
    let list = match tab {
        "recent" => &v.recent,
        "customers" => &v.customers,
        _ => &v.live,
    };
    let rows: Vec<VRow> = list
        .iter()
        .map(|(r, a)| {
            vrow(
                r,
                a,
                names,
                tab,
                domain,
                now,
                offset,
                v.online.contains(&r.visitor_id),
            )
        })
        .collect();
    let on_pricing = v
        .live
        .iter()
        .filter(|(r, _)| r.exit_page.as_deref() == Some("/pricing"))
        .count();
    let paying_live = v.live.iter().filter(|(r, _)| r.payments > 0).count();
    let (sub, cols, foot, stats, empty) = match tab {
        "recent" => (
            "Last 24 hours, newest first",
            ["Visitor", "Last page", "Last seen"],
            "Click anyone to see every visit and payment.",
            vec![
                (group(v.today.visitors), "visitors today"),
                (group(v.today.payments), "payments today"),
                (group(v.goals_today), "goals reached today"),
            ],
            (
                "No visitors yet",
                "Once the script is live, the latest visitors appear here.",
            ),
        ),
        "customers" => (
            "People who have paid, last seen first",
            ["Customer", "Paid", "Paid so far"],
            "Click a customer to see their whole path.",
            vec![(group(v.paying), "paying customers in this range")],
            (
                "No customers yet",
                if connected {
                    "Every customer shows up here with the channel that brought them."
                } else {
                    "Connect payments and every customer shows up here with the channel that brought them."
                },
            ),
        ),
        _ => (
            "Updating live",
            ["Visitor", "Right now", "Last active"],
            "Visitors drop off this list after 5 minutes without activity.",
            vec![
                (group(live_now), "on the site now"),
                (on_pricing.to_string(), "on pricing"),
                (paying_live.to_string(), "customers"),
            ],
            (
                "Nobody on the site right now",
                "Visitors show up here the second they land on your site.",
            ),
        ),
    };
    VisitorsView {
        tabs: [
            ("", "Live now"),
            ("recent", "Recent"),
            ("customers", "Customers"),
        ]
        .iter()
        .map(|(k, label)| Opt {
            label: if k.is_empty() && !v.live.is_empty() {
                format!("{label} ({})", v.live.len())
            } else {
                label.to_string()
            },
            href: href(&[("vtab", k)]),
            on: (k.is_empty() && tab == "live") || *k == tab,
            count: String::new(),
        })
        .collect(),
        sub,
        stats: if rows.is_empty() { Vec::new() } else { stats },
        empty: rows.is_empty().then_some(empty),
        cols,
        rows,
        foot,
        all_href: sites::app_path(domain, "visitors"),
    }
}

pub struct PaymentsData {
    pub(super) page: PaymentPage,
    pub(super) journeys: Vec<Option<Journey>>,
    pub(super) split: Vec<ProviderSplit>,
    pub(super) summary: PaymentSummary,
    pub(super) mrr: Option<Mrr>,
}

pub fn load_payments(
    store: &dyn EventStore,
    id: &str,
    p: &Period,
    window_days: i64,
    f: &Filter,
    now: OffsetDateTime,
) -> anyhow::Result<PaymentsData> {
    let page = store.payment_list_in(id, p, PaymentFilter::All, 10, 0, f)?;
    let journeys = page
        .rows
        .iter()
        .map(|r| store.journey(id, &r.provider, &r.provider_payment_id, window_days))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(PaymentsData {
        page,
        journeys,
        split: store.payment_split(id, p)?,
        summary: store.payment_summary(id, p)?,
        mrr: store.mrr(id, p, now)?,
    })
}

pub struct PayRow {
    pub(super) name: String,
    pub(super) avatar: String,
    pub(super) vid: String,
    pub(super) flag: String,
    pub(super) place: String,
    pub(super) device_icon: String,
    pub(super) device: String,
    pub(super) src_icon: String,
    pub(super) src: String,
    pub(super) chan: String,
    pub(super) chips: Vec<Chip>,
    pub(super) wait: String,
    pub(super) amount: String,
    pub(super) refunded: bool,
    pub(super) when: String,
    pub(super) provider: &'static str,
    pub(super) href: String,
}

pub struct PaymentsView {
    pub(super) sum: Vec<(&'static str, String, String, bool)>,
    pub(super) rows: Vec<PayRow>,
    /// Some path has two visits or more, so the credited one is worth marking.
    pub(super) multi: bool,
    pub(super) total: u64,
    pub(super) all_href: String,
    pub(super) connect_href: String,
}

/// Keys of everyone the payments view may name.
pub fn payer_keys(v: &PaymentsData) -> Vec<String> {
    v.page.rows.iter().map(super::payer_key).collect()
}

pub fn payments_view(
    v: PaymentsData,
    names: &Names,
    domain: &str,
    offset: UtcOffset,
) -> PaymentsView {
    let first: (u64, f64) = v
        .split
        .iter()
        .fold((0, 0.0), |a, s| (a.0 + s.first_payments, a.1 + s.first_usd));
    let renew: (u64, f64) = v
        .split
        .iter()
        .fold((0, 0.0), |a, s| (a.0 + s.renewals, a.1 + s.renewal_usd));
    let s = &v.summary;
    let gross = s.net_usd + s.refunded_usd;
    let rows: Vec<PayRow> = v
        .page
        .rows
        .iter()
        .zip(&v.journeys)
        .map(|(p, j)| {
            let j = j.as_ref();
            let (src, chan) = source_of(p);
            let who = names.get(&super::payer_key(p));
            PayRow {
                name: who.name,
                avatar: who.avatar,
                vid: p
                    .visitor
                    .as_deref()
                    .map(|v| {
                        v.chars()
                            .rev()
                            .take(8)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect()
                    })
                    .unwrap_or_default(),
                flag: j
                    .and_then(|j| j.country.as_deref())
                    .map(icons::flag)
                    .unwrap_or_default(),
                place: j
                    .and_then(|j| j.country.as_deref())
                    .map(icons::country)
                    .unwrap_or_default(),
                device_icon: j
                    .and_then(|j| j.device.as_deref())
                    .map(icons::device)
                    .unwrap_or_default(),
                device: j
                    .and_then(|j| j.device.clone())
                    .filter(|d| d != "Unknown")
                    .unwrap_or_default(),
                src_icon: icons::source(&src, 16),
                chan: if chan == src { String::new() } else { chan },
                src,
                chips: path_chips(j)
                    .into_iter()
                    .map(|mut c| {
                        if c.icon.is_empty() && c.label.starts_with('/') {
                            c.icon = icons::glyph("page", 14, "#6B717C");
                        }
                        c
                    })
                    .collect(),
                wait: p
                    .first_touch
                    .as_ref()
                    .map(|f| since(p.ts - f.ts))
                    .unwrap_or_default(),
                amount: amount(p),
                refunded: p.status == "refunded",
                when: local(p.ts, offset),
                provider: provider_name(&p.provider),
                href: format!(
                    "{}/{}/{}",
                    sites::app_path(domain, "payments"),
                    p.provider,
                    p.provider_payment_id
                ),
            }
        })
        .collect();
    PaymentsView {
        sum: if s.payments == 0 {
            Vec::new()
        } else {
            vec![
                (
                    "From new customers",
                    money(first.1),
                    plural(first.0, "first payment"),
                    false,
                ),
                (
                    "From renewals",
                    money(renew.1),
                    if renew.0 == 0 {
                        "none yet".into()
                    } else {
                        plural(renew.0, "payment")
                    },
                    false,
                ),
                (
                    "Refunded",
                    if s.refunded_usd > 0.0 {
                        format!("-{}", money(s.refunded_usd))
                    } else {
                        "$0".into()
                    },
                    if s.refunded == 0 {
                        "no refunds".into()
                    } else {
                        format!(
                            "{}, {:.1}% of revenue",
                            plural(s.refunded, "refund"),
                            if gross > 0.0 {
                                s.refunded_usd / gross * 100.0
                            } else {
                                0.0
                            }
                        )
                    },
                    s.refunded_usd > 0.0,
                ),
                match &v.mrr {
                    Some(m) => (
                        "MRR",
                        money(m.usd),
                        plural(m.customers, "paying customer"),
                        false,
                    ),
                    None => (
                        "Customers",
                        group(s.customers),
                        "paid in this range".into(),
                        false,
                    ),
                },
            ]
        },
        multi: rows
            .iter()
            .any(|r| r.chips.iter().filter(|c| !c.label.starts_with('/')).count() > 1),
        rows,
        total: v.page.total,
        all_href: sites::app_path(domain, "payments"),
        connect_href: sites::app_path(domain, "setup/payments"),
    }
}

pub struct RetRow {
    pub(super) icon: String,
    pub(super) name: String,
    pub(super) n: String,
    pub(super) val: String,
    pub(super) badge: String,
    pub(super) tone: &'static str,
    pub(super) val2: String,
    pub(super) w: f64,
}

pub struct RetentionView {
    /// Title, text, progress percent and its label while there is too little to show.
    pub(super) waiting: Option<(String, &'static str, f64, String)>,
    pub(super) tabs: Vec<Opt>,
    pub(super) refunds: bool,
    pub(super) head: String,
    pub(super) col: &'static str,
    pub(super) col2: &'static str,
    pub(super) rows: Vec<RetRow>,
    pub(super) foot: String,
}

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "–".into(), |v| format!("{:.0}%", v * 100.0))
}

pub fn retention_view(
    r: &Retention,
    mrr: Option<&Mrr>,
    rest: &HashMap<String, String>,
    href: &Href,
) -> RetentionView {
    let refunds = rest.get("ret").map(String::as_str) == Some("refunds");
    let t = &r.total;
    let avg = (t.customers > 0).then(|| t.still_paying as f64 / t.customers as f64);
    let avg_refund = t.refund_rate.unwrap_or(0.0);
    let text = "This shows which channels bring customers who keep paying, and which bring customers who cancel or ask for refunds.";
    let waiting = if t.customers == 0 {
        Some((
            "Customers who stay starts with your first sale".to_string(),
            text,
            0.0,
            "No sales in this range yet".to_string(),
        ))
    } else if r.day_one {
        Some((
            format!("Customers who stay shows up after {LTV_DAYS} days"),
            text,
            r.days_of_data as f64 / LTV_DAYS as f64 * 100.0,
            format!("Day {} of {LTV_DAYS} since your first sale", r.days_of_data),
        ))
    } else {
        None
    };
    let row = |x: &RetentionRow| {
        let icon = icons::source(
            if x.unattributed {
                "Unattributed"
            } else {
                &x.label
            },
            16,
        );
        if refunds {
            let rate = x.refund_rate.unwrap_or(0.0);
            let (badge, tone) = if rate <= avg_refund * 0.67 {
                ("Low", "up")
            } else if rate >= avg_refund * 1.6 && rate > 0.0 {
                ("High", "down")
            } else {
                ("Average", "flat")
            };
            RetRow {
                icon,
                name: x.label.clone(),
                n: group(x.customers),
                val: format!("{:.1}%", rate * 100.0),
                badge: badge.into(),
                tone,
                val2: group((x.customers as f64 * rate).round() as u64),
                w: (rate * 1000.0).min(100.0),
            }
        } else {
            let share = (x.customers > 0).then(|| x.still_paying as f64 / x.customers as f64);
            let d = match (share, avg) {
                (Some(s), Some(a)) => ((s - a) * 100.0).round() as i64,
                _ => 0,
            };
            RetRow {
                icon,
                name: x.label.clone(),
                n: group(x.customers),
                val: pct(share),
                badge: format!("{}{d} pts", if d > 0 { "+" } else { "" }),
                tone: if d >= 6 {
                    "up"
                } else if d <= -10 {
                    "down"
                } else {
                    "flat"
                },
                val2: x.avg_ltv_usd.map_or_else(|| "–".into(), usd),
                w: share.unwrap_or(0.0) * 100.0,
            }
        }
    };
    let head = if refunds {
        r.channels
            .iter()
            .filter(|x| !x.unattributed && x.customers >= 3)
            .max_by(|a, b| {
                a.refund_rate
                    .unwrap_or(0.0)
                    .total_cmp(&b.refund_rate.unwrap_or(0.0))
            })
            .filter(|x| x.refund_rate.unwrap_or(0.0) > 0.0)
            .map(|x| {
                format!(
                    "{} has the highest refund rate, {:.1}%, against your average of {:.1}%.",
                    x.label,
                    x.refund_rate.unwrap_or(0.0) * 100.0,
                    avg_refund * 100.0
                )
            })
            .unwrap_or_else(|| "No channel stands out for refunds yet.".into())
    } else {
        r.headline.clone().unwrap_or_else(|| {
            format!(
                "{} of {} from this range still pay.",
                pct(avg),
                plural(t.customers, "customer")
            )
        })
    };
    RetentionView {
        waiting,
        tabs: [("", "Still paying"), ("refunds", "Refunds")]
            .iter()
            .map(|(k, label)| Opt {
                label: label.to_string(),
                href: href(&[("ret", k)]),
                on: (*k == "refunds") == refunds,
                count: String::new(),
            })
            .collect(),
        refunds,
        head,
        col: if refunds { "Refunded" } else { "Still paying" },
        col2: if refunds { "Refunds" } else { "Value each" },
        rows: r.channels.iter().map(row).collect(),
        foot: if refunds {
            "Share of these customers with any refund or chargeback.".into()
        } else {
            format!(
                "Value each is net revenue per customer so far, after refunds. Average still paying: {}.{}",
                pct(avg),
                mrr.filter(|m| m.cohort_customers > 0)
                    .map(|m| format!(
                        " The ones still paying bring in {} a month.",
                        money(m.cohort_usd)
                    ))
                    .unwrap_or_default()
            )
        },
    }
}
