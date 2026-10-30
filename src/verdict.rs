//! One plain, rule-based line per channel: which earned most, which earned nothing, what to double.

use crate::dashboard::money;
use crate::stats::BreakdownRow;

/// A channel needs this many visitors before its per-visitor value or its silence is judged.
const MIN_VISITORS: u64 = 20;
/// Revenue must move this share, and this many dollars, against the period before to be called out.
const MIN_MOVE: f64 = 0.3;
const MIN_DOLLARS: f64 = 20.0;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Verdict {
    pub channel: String,
    /// good, bad or plain.
    pub tone: &'static str,
    pub line: String,
}

fn per_visitor(r: &BreakdownRow) -> f64 {
    if r.visitors > 0 {
        r.revenue_usd / r.visitors as f64
    } else {
        0.0
    }
}

/// A line for each channel of `cur` with visitors or revenue, judged against `prev`, the period before; most revenue first.
pub fn verdicts(cur: &[BreakdownRow], prev: &[BreakdownRow]) -> Vec<Verdict> {
    let mut rows: Vec<&BreakdownRow> = cur
        .iter()
        .filter(|r| !r.unattributed && (r.visitors > 0 || r.revenue_usd > 0.0))
        .collect();
    rows.sort_by(|a, b| {
        b.revenue_usd
            .total_cmp(&a.revenue_usd)
            .then(b.visitors.cmp(&a.visitors))
    });
    let (visitors, revenue) = rows
        .iter()
        .fold((0u64, 0.0), |(v, m), r| (v + r.visitors, m + r.revenue_usd));
    let avg = if visitors > 0 {
        revenue / visitors as f64
    } else {
        0.0
    };
    let top = rows
        .first()
        .filter(|r| r.revenue_usd > 0.0)
        .map(|r| r.label.clone());
    let best = rows
        .iter()
        .filter(|r| r.visitors >= MIN_VISITORS && r.revenue_usd > 0.0)
        .max_by(|a, b| per_visitor(a).total_cmp(&per_visitor(b)))
        .map(|r| r.label.clone())
        .filter(|b| top.as_ref() != Some(b));
    rows.into_iter()
        .map(|r| {
            let was = prev
                .iter()
                .find(|p| !p.unattributed && p.label == r.label)
                .map_or(0.0, |p| p.revenue_usd);
            let moved = r.revenue_usd - was;
            let (tone, line) = if top.as_ref() == Some(&r.label) {
                (
                    "good",
                    format!(
                        "Earned the most: {} from {} visitors.",
                        money(r.revenue_usd),
                        r.visitors
                    ),
                )
            } else if best.as_ref() == Some(&r.label) && avg > 0.0 {
                (
                    "good",
                    format!(
                        "Best value at {} a visitor, {:.1}x your average. Worth doubling.",
                        money(per_visitor(r)),
                        per_visitor(r) / avg
                    ),
                )
            } else if r.revenue_usd <= 0.0 && r.visitors >= MIN_VISITORS {
                (
                    "bad",
                    format!(
                        "{} visitors and no revenue. Check where they land.",
                        r.visitors
                    ),
                )
            } else if was > 0.0 && moved.abs() >= MIN_DOLLARS && moved.abs() / was >= MIN_MOVE {
                (
                    if moved > 0.0 { "good" } else { "bad" },
                    format!(
                        "Revenue {} {:.0}%, to {}.",
                        if moved > 0.0 { "up" } else { "down" },
                        moved.abs() / was * 100.0,
                        money(r.revenue_usd)
                    ),
                )
            } else if r.revenue_usd > 0.0 {
                (
                    "plain",
                    format!("{} from {} visitors.", money(r.revenue_usd), r.visitors),
                )
            } else {
                (
                    "plain",
                    format!("{} visitors, too few to judge yet.", r.visitors),
                )
            };
            Verdict {
                channel: r.label.clone(),
                tone,
                line,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: &str, visitors: u64, revenue: f64) -> BreakdownRow {
        BreakdownRow {
            key: Some(label.into()),
            label: label.into(),
            unattributed: false,
            visitors,
            payments: 1,
            customers: 1,
            refunded: 0,
            revenue_usd: revenue,
            revenue_per_visitor: None,
            avg_visit_secs: None,
        }
    }

    #[test]
    fn names_the_top_the_best_value_the_silent_and_the_movers() {
        let cur = [
            row("Organic search", 400, 800.0),
            row("AI", 50, 300.0),
            row("Social", 120, 0.0),
            row("Email", 30, 100.0),
            row("Referral", 5, 0.0),
        ];
        let prev = [row("Email", 30, 40.0)];
        let v = verdicts(&cur, &prev);
        let line = |c: &str| v.iter().find(|x| x.channel == c).unwrap().line.clone();
        assert_eq!(v[0].channel, "Organic search");
        assert_eq!(
            line("Organic search"),
            "Earned the most: $800 from 400 visitors."
        );
        assert!(line("AI").starts_with("Best value at $6 a visitor"));
        assert_eq!(
            line("Social"),
            "120 visitors and no revenue. Check where they land."
        );
        assert_eq!(line("Email"), "Revenue up 150%, to $100.");
        assert_eq!(line("Referral"), "5 visitors, too few to judge yet.");
        assert!(verdicts(&[], &[]).is_empty());
    }
}
