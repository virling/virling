//! The Goals card: visit to signup to checkout to paid, and the custom goals behind it.

use super::goals::humanize;
use super::{Opt, group};
use crate::events::EventStore;
use crate::icons;
use crate::stats::{Filter, GoalStat, Period, Step, StepCount};

/// Goal names that stand for a signup and for a started checkout.
const SIGNUP: [&str; 4] = ["signup", "sign_up", "sign-up", "register"];
const CHECKOUT: [&str; 1] = ["checkout"];

pub struct Data {
    table: Vec<GoalStat>,
    steps: Vec<Step>,
    counts: Vec<StepCount>,
}

fn names(table: &[GoalStat]) -> Vec<&str> {
    table
        .iter()
        .filter_map(|g| g.key.strip_prefix("goal:"))
        .collect()
}

/// Visit, the signup and checkout goals (else the two most completed), then paid.
fn steps(table: &[GoalStat]) -> Vec<Step> {
    let all = names(table);
    let find = |keys: &[&str]| {
        all.iter()
            .find(|n| keys.iter().any(|k| n.to_ascii_lowercase().contains(k)))
    };
    let mut mids: Vec<&str> = [find(&SIGNUP), find(&CHECKOUT)]
        .into_iter()
        .flatten()
        .copied()
        .collect();
    if mids.is_empty() {
        mids = all.iter().take(2).copied().collect();
    }
    std::iter::once(Step::Visit)
        .chain(mids.into_iter().map(|n| Step::Goal(n.into())))
        .chain(std::iter::once(Step::Paid))
        .collect()
}

pub fn load(store: &dyn EventStore, site: &str, p: &Period) -> anyhow::Result<Data> {
    let f = Filter::default();
    let table = store.goal_table(site, p, &[], &f)?;
    let steps = steps(&table);
    let counts = if names(&table).is_empty() {
        Vec::new()
    } else {
        store.funnel(site, p, &steps, &f)?
    };
    Ok(Data {
        table,
        steps,
        counts,
    })
}

fn step_name(s: &Step) -> String {
    match s {
        Step::Visit => "visit".into(),
        Step::Paid => "paid".into(),
        Step::Goal(g) => g.clone(),
        Step::Page(p) => p.clone(),
    }
}

pub struct StepRow {
    pub(super) icon: String,
    pub(super) name: String,
    pub(super) count: String,
    pub(super) conv: String,
    /// "-62%" of those at the step before, empty for the first step.
    pub(super) drop: String,
    pub(super) bad: bool,
}

pub struct GoalLine {
    pub(super) name: String,
    pub(super) sub: String,
    pub(super) count: String,
    pub(super) per_visitor: String,
}

pub struct Card {
    pub(super) tabs: Vec<Opt>,
    pub(super) on_list: bool,
    pub(super) steps: Vec<StepRow>,
    pub(super) goals: Vec<GoalLine>,
    pub(super) foot: String,
    pub(super) docs: &'static str,
}

pub fn card(d: &Data, on_list: bool, visitors: u64, href: impl Fn(&str) -> String) -> Card {
    let first = d.counts.first().map_or(0, |c| c.visitors);
    let steps = d
        .steps
        .iter()
        .zip(&d.counts)
        .enumerate()
        .map(|(i, (s, c))| {
            let prev = if i > 0 { d.counts[i - 1].visitors } else { 0 };
            let drop = if i > 0 && prev > 0 {
                1.0 - c.visitors as f64 / prev as f64
            } else {
                0.0
            };
            let (glyph, name) = match s {
                Step::Visit => ("page", "Visit".to_string()),
                Step::Paid => ("paid", "Paid".to_string()),
                Step::Goal(g)
                    if i == 1 && SIGNUP.iter().any(|k| g.to_ascii_lowercase().contains(k)) =>
                {
                    ("direct", humanize(g))
                }
                Step::Goal(g) => ("card", humanize(g)),
                Step::Page(p) => ("page", p.clone()),
            };
            StepRow {
                icon: icons::glyph(glyph, 16, "var(--ink-2)"),
                name,
                count: group(c.visitors),
                conv: if first > 0 {
                    format!("{:.1}%", c.visitors as f64 / first as f64 * 100.0)
                } else {
                    "\u{2013}".into()
                },
                drop: if i > 0 && prev > 0 {
                    format!(
                        "-{:.0}% after {}",
                        drop * 100.0,
                        humanize(&step_name(&d.steps[i - 1]))
                    )
                } else {
                    String::new()
                },
                bad: drop >= 0.6,
            }
        })
        .collect();
    let goals: Vec<GoalLine> = d
        .table
        .iter()
        .filter_map(|g| {
            let name = g.key.strip_prefix("goal:")?;
            Some(GoalLine {
                name: humanize(name),
                sub: name.to_string(),
                count: group(g.completions),
                // Revenue per goal visitor is not in the session index yet.
                per_visitor: "\u{2013}".into(),
            })
        })
        .collect();
    let foot = match (goals.len(), visitors) {
        (0, _) => String::new(),
        (_, ..100) => "Early numbers. They settle after about 100 visitors.".into(),
        _ if on_list => "Goals your site reports with the tracking script.".into(),
        _ => "Visitors who reached each step, in order.".into(),
    };
    Card {
        tabs: [("", "Funnel"), ("list", "Goals")]
            .iter()
            .map(|(k, label)| Opt {
                label: label.to_string(),
                href: format!("{}#goals", href(k)),
                on: (*k == "list") == on_list,
                count: String::new(),
            })
            .collect(),
        on_list,
        steps,
        goals,
        foot,
        docs: "https://docs.virling.com/track/goals",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(name: &str) -> GoalStat {
        GoalStat {
            key: format!("goal:{name}"),
            completions: 1,
            visitors: 1,
            channels: Vec::new(),
        }
    }

    #[test]
    fn the_funnel_runs_visit_signup_checkout_paid() {
        let table = [goal("trial"), goal("checkout_started"), goal("signup")];
        assert_eq!(
            steps(&table),
            [
                Step::Visit,
                Step::Goal("signup".into()),
                Step::Goal("checkout_started".into()),
                Step::Paid
            ]
        );
        assert_eq!(
            steps(&[goal("trial"), goal("demo"), goal("x")]),
            [
                Step::Visit,
                Step::Goal("trial".into()),
                Step::Goal("demo".into()),
                Step::Paid
            ]
        );
    }
}
