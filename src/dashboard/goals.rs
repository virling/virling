//! The Goals view: a funnel from any visit to paying, and every goal by channel.

use std::collections::HashMap;

use super::{Href, Opt, cards, encode, group, since};
use crate::events::EventStore;
use crate::icons;
use crate::stats::{BreakdownRow, Dimension, Filter, GoalStat, Period, Step, StepCount, Totals};

/// Custom goals a default funnel takes, most completed first.
const DEFAULT_GOALS: usize = 2;
/// Under this median the step usually happens in the same visit.
const SAME_VISIT_SECS: f64 = 1800.0;

pub struct GoalsData {
    pub(super) table: Vec<GoalStat>,
    pub(super) channels: Vec<BreakdownRow>,
    pub(super) pages: Vec<String>,
    pub(super) steps: Vec<Step>,
    pub(super) counts: Vec<StepCount>,
    pub(super) channel: Option<String>,
}

fn parse(v: &str) -> Option<Step> {
    let (kind, rest) = v.split_once(':')?;
    (!rest.is_empty()).then_some(())?;
    match kind {
        "page" => Some(Step::Page(rest.to_string())),
        "goal" => Some(Step::Goal(rest.to_string())),
        _ => None,
    }
}

fn key(s: &Step) -> String {
    match s {
        Step::Page(p) => format!("page:{p}"),
        Step::Goal(g) => format!("goal:{g}"),
        Step::Visit => "visit".into(),
        Step::Paid => "paid".into(),
    }
}

pub fn load(
    store: &dyn EventStore,
    id: &str,
    p: &Period,
    f: &Filter,
    rest: &HashMap<String, String>,
) -> anyhow::Result<GoalsData> {
    let table = store.goal_table(id, p, &["/pricing".to_string()], f)?;
    let channels = store.breakdown_in(id, p, Dimension::Channel, f)?;
    let pages: Vec<String> = store
        .breakdown_in(id, p, Dimension::Page, f)?
        .into_iter()
        .filter_map(|r| r.key)
        .take(8)
        .collect();
    let picked: Vec<Step> = ["s1", "s2", "s3"]
        .iter()
        .filter_map(|k| rest.get(*k).and_then(|v| parse(v)))
        .collect();
    let mids = if picked.is_empty() && !["s1", "s2", "s3"].iter().any(|k| rest.contains_key(*k)) {
        let mut m: Vec<Step> = table
            .iter()
            .filter(|g| g.key == "page:/pricing")
            .map(|_| Step::Page("/pricing".into()))
            .collect();
        m.extend(
            table
                .iter()
                .filter_map(|g| g.key.strip_prefix("goal:"))
                .take(DEFAULT_GOALS)
                .map(|g| Step::Goal(g.to_string())),
        );
        m
    } else {
        picked
    };
    let steps: Vec<Step> = std::iter::once(Step::Visit)
        .chain(mids)
        .chain(std::iter::once(Step::Paid))
        .collect();
    let counts = store.funnel(id, p, &steps, f)?;
    Ok(GoalsData {
        table,
        channels,
        pages,
        steps,
        counts,
        channel: rest.get("channel").cloned(),
    })
}

/// "trial_started" -> "Trial started".
fn humanize(name: &str) -> String {
    let s = name.replace(['_', '-'], " ");
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn step_label(s: &Step) -> (String, String) {
    match s {
        Step::Visit => ("Visited the site".into(), "Any page".into()),
        Step::Page(p) if p == "/pricing" => ("Viewed pricing".into(), format!("Page: {p}")),
        Step::Page(p) => (format!("Viewed {p}"), format!("Page: {p}")),
        Step::Goal(g) => (humanize(g), format!("Goal: {g}")),
        Step::Paid => ("Paid".into(), "From your payments, automatic".into()),
    }
}

pub struct StepView {
    pub(super) num: usize,
    pub(super) label: String,
    pub(super) sub: String,
    pub(super) value: String,
    pub(super) h: f64,
    pub(super) fill: &'static str,
    pub(super) drop: String,
    pub(super) bad: bool,
    pub(super) pct: String,
    pub(super) time: String,
}

pub struct GoalKid {
    pub(super) icon: String,
    pub(super) name: String,
    pub(super) of: String,
    pub(super) value: String,
    pub(super) rate: String,
    pub(super) ratio: String,
    pub(super) tone: &'static str,
}

pub struct GoalRow {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) sub: String,
    pub(super) paid: bool,
    pub(super) value: String,
    pub(super) rate: String,
    pub(super) per_day: String,
    pub(super) kids: Vec<GoalKid>,
}

pub struct GoalsView {
    pub(super) empty: bool,
    pub(super) funnel: bool,
    pub(super) sub: &'static str,
    pub(super) tabs: Vec<Opt>,
    pub(super) channel_tabs: Vec<Opt>,
    pub(super) head: String,
    pub(super) head2: String,
    pub(super) steps: Vec<StepView>,
    pub(super) foot: &'static str,
    pub(super) rows: Vec<GoalRow>,
    pub(super) hint: bool,
    pub(super) list_foot: &'static str,
    /// (value, label, picked in slot 1, 2, 3)
    pub(super) options: Vec<(String, String, [bool; 3])>,
    pub(super) hidden: Vec<(&'static str, String)>,
    pub(super) base: String,
}

fn kids(by: &[(String, u64)], channels: &[BreakdownRow], base: f64) -> Vec<GoalKid> {
    let mut out: Vec<(f64, GoalKid)> = by
        .iter()
        .filter_map(|(c, n)| {
            let seen = channels
                .iter()
                .find(|r| !r.unattributed && r.key.as_deref() == Some(c.as_str()))?
                .visitors;
            (seen > 0 && *n > 0).then_some(())?;
            let r = *n as f64 / seen as f64;
            let x = (base > 0.0).then(|| r / base);
            Some((
                r,
                GoalKid {
                    icon: icons::source(c, 16),
                    name: c.clone(),
                    of: format!("of {} visitors", group(seen)),
                    value: group(*n),
                    rate: format!("{:.1}%", r * 100.0),
                    ratio: x.map(|x| format!("{x:.1}x")).unwrap_or_default(),
                    tone: x.map_or("", cards::tone),
                },
            ))
        })
        .collect();
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.into_iter().map(|(_, k)| k).collect()
}

pub fn view(
    g: &GoalsData,
    cur: &Totals,
    days: i64,
    no_data: bool,
    href: &Href,
    raw: &[(&'static str, String)],
    base: &str,
) -> GoalsView {
    let funnel = !raw.iter().any(|(k, v)| *k == "gtab" && v == "goals");
    let first = g.counts.first().map_or(0, |c| c.visitors);
    let mut steps: Vec<StepView> = Vec::new();
    let mut worst: Option<(usize, f64)> = None;
    for (i, (s, c)) in g.steps.iter().zip(&g.counts).enumerate() {
        let prev = if i == 0 {
            c.visitors
        } else {
            g.counts[i - 1].visitors
        };
        let drop = if i > 0 && prev > 0 {
            1.0 - c.visitors as f64 / prev as f64
        } else {
            0.0
        };
        if i > 0 && prev > 0 && worst.is_none_or(|w| drop > w.1) {
            worst = Some((i, drop));
        }
        let (label, sub) = step_label(s);
        let prev_label = if i > 0 {
            step_label(&g.steps[i - 1]).0.to_lowercase()
        } else {
            String::new()
        };
        steps.push(StepView {
            num: i + 1,
            label,
            sub,
            value: group(c.visitors),
            h: if first > 0 {
                (c.visitors as f64 / first as f64 * 100.0).max(2.0)
            } else {
                2.0
            },
            fill: if i + 1 == g.steps.len() {
                "pay"
            } else if i == 0 {
                "first"
            } else {
                ""
            },
            drop: if i > 0 && prev > 0 && drop >= 0.005 {
                format!("-{:.0}%", drop * 100.0)
            } else if i > 0 && prev > 0 {
                "0%".into()
            } else {
                String::new()
            },
            bad: drop >= 0.6,
            pct: if i == 0 {
                "100%".into()
            } else if first > 0 {
                format!(
                    "{:.1}% of visitors",
                    c.visitors as f64 / first as f64 * 100.0
                )
            } else {
                String::new()
            },
            time: match c.median_secs {
                Some(s) if s < SAME_VISIT_SECS => "Usually in the same visit".into(),
                Some(s) => format!(
                    "Median {} after {prev_label}",
                    since(time::Duration::seconds(s as i64))
                ),
                None => String::new(),
            },
        });
    }
    let last = g.counts.last().map_or(0, |c| c.visitors);
    let overall = if first > 0 {
        last as f64 / first as f64 * 100.0
    } else {
        0.0
    };
    let head = match &g.channel {
        Some(c) => format!("{overall:.1}% of visitors from {c} became paying customers."),
        None => format!("{overall:.1}% of visitors became paying customers."),
    };
    let head2 = match worst {
        Some((i, d)) if d > 0.0 => format!(
            "Biggest drop: {} to {}, where {:.0}% leave.",
            step_label(&g.steps[i - 1]).0.to_lowercase(),
            step_label(&g.steps[i]).0.to_lowercase(),
            d * 100.0
        ),
        _ => String::new(),
    };
    let mut channel_tabs = vec![Opt {
        label: "All channels".into(),
        href: href(&[("channel", "")]),
        on: g.channel.is_none(),
        count: String::new(),
    }];
    channel_tabs.extend(
        g.channels
            .iter()
            .filter(|r| !r.unattributed && r.visitors > 0)
            .filter_map(|r| r.key.clone())
            .take(6)
            .map(|c| Opt {
                href: href(&[("channel", &encode(&c))]),
                on: g.channel.as_deref() == Some(c.as_str()),
                label: c,
                count: String::new(),
            }),
    );
    let visitors = cur.visitors.max(1) as f64;
    let mut rows: Vec<GoalRow> = g
        .table
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let base_rate = s.visitors as f64 / visitors;
            let (name, sub) = match s.key.split_once(':') {
                Some(("page", p)) => (
                    step_label(&Step::Page(p.into())).0,
                    format!("Page goal: {p}, automatic"),
                ),
                Some((_, n)) => (humanize(n), format!("Custom goal: {n}")),
                None => (s.key.clone(), String::new()),
            };
            GoalRow {
                id: format!("goal-{i}"),
                name,
                sub,
                paid: false,
                value: group(s.completions),
                rate: format!("{:.1}% of visitors", base_rate * 100.0),
                per_day: format!("{:.1}", s.completions as f64 / days.max(1) as f64),
                kids: kids(&s.channels, &g.channels, base_rate),
            }
        })
        .collect();
    let paid_by: Vec<(String, u64)> = g
        .channels
        .iter()
        .filter(|r| !r.unattributed)
        .filter_map(|r| Some((r.key.clone()?, r.customers)))
        .collect();
    let paid_rate = cur.conversion.unwrap_or(0.0);
    rows.push(GoalRow {
        id: "goal-paid".into(),
        name: "Paid".into(),
        sub: "From your payments, automatic".into(),
        paid: true,
        value: group(cur.payments),
        rate: format!("{:.1}% of visitors", paid_rate * 100.0),
        per_day: format!("{:.1}", cur.payments as f64 / days.max(1) as f64),
        kids: kids(&paid_by, &g.channels, paid_rate),
    });
    let mut options: Vec<(String, String, [bool; 3])> = Vec::new();
    let mut add = |s: Step| {
        let k = key(&s);
        if options.iter().any(|o| o.0 == k) {
            return;
        }
        let mids = &g.steps[1..g.steps.len() - 1];
        let mut on = [false; 3];
        for (i, m) in mids.iter().enumerate().take(3) {
            on[i] = key(m) == k;
        }
        options.push((k, step_label(&s).0, on));
    };
    for t in &g.table {
        if let Some(n) = t.key.strip_prefix("goal:") {
            add(Step::Goal(n.into()));
        }
    }
    add(Step::Page("/pricing".into()));
    for p in &g.pages {
        add(Step::Page(p.clone()));
    }
    for s in &g.steps[1..g.steps.len() - 1] {
        add(s.clone());
    }
    let has_custom = g.table.iter().any(|t| t.key.starts_with("goal:"));
    GoalsView {
        empty: no_data,
        funnel,
        sub: if no_data {
            "See which channels bring signups, not just payments."
        } else {
            "Where visitors drop off on the way to paying."
        },
        tabs: [("", "Funnel"), ("goals", "Goals")]
            .iter()
            .map(|(k, label)| Opt {
                label: label.to_string(),
                href: href(&[("gtab", k)]),
                on: (*k == "goals") != funnel,
                count: String::new(),
            })
            .collect(),
        channel_tabs,
        head,
        head2,
        steps,
        foot: if has_custom {
            "Counts unique visitors in the period. Each step only counts people who did the step before it."
        } else {
            "Add a signup goal to see the steps between pricing and paying."
        },
        rows,
        hint: !has_custom,
        list_foot: "Conversion is completions by visitors from the same channel. Open a goal to split it by channel.",
        options,
        hidden: raw
            .iter()
            .filter(|(k, _)| !matches!(*k, "s1" | "s2" | "s3"))
            .cloned()
            .collect(),
        base: base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_parse_and_read() {
        assert_eq!(parse("goal:signup"), Some(Step::Goal("signup".into())));
        assert_eq!(parse("page:/pricing"), Some(Step::Page("/pricing".into())));
        assert_eq!(parse("goal:"), None);
        assert_eq!(parse("x"), None);
        assert_eq!(humanize("trial_started"), "Trial started");
        assert_eq!(
            step_label(&Step::Page("/pricing".into())).0,
            "Viewed pricing"
        );
    }
}
