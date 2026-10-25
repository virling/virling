//! A one-line reason when revenue moves a lot: which channel changed most.

use crate::stats::BreakdownRow;

/// Revenue must move at least this share, and this many dollars, before a reason is given.
const MIN_SHARE: f64 = 0.2;
const MIN_DOLLARS: f64 = 20.0;
/// The channel named must account for at least this share of the move.
const MIN_LEAD: f64 = 0.4;

fn money(x: f64) -> String {
    let n = x.abs().round() as i64;
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("${}", out.chars().rev().collect::<String>())
}

fn revenue_of(rows: &[BreakdownRow], label: &str) -> f64 {
    rows.iter()
        .find(|r| !r.unattributed && r.label == label)
        .map_or(0.0, |r| r.revenue_usd)
}

/// Why revenue moved from `prev` to `cur` dollars, from the channels of both periods; None when the move is small or no channel explains it.
pub fn revenue_change(
    cur: f64,
    prev: f64,
    cur_rows: &[BreakdownRow],
    prev_rows: &[BreakdownRow],
) -> Option<String> {
    if prev <= 0.0 {
        return None;
    }
    let delta = cur - prev;
    if delta.abs() < MIN_DOLLARS || delta.abs() / prev < MIN_SHARE {
        return None;
    }
    let (label, moved) = cur_rows
        .iter()
        .chain(prev_rows)
        .filter(|r| !r.unattributed)
        .map(|r| {
            (
                r.label.as_str(),
                revenue_of(cur_rows, &r.label) - revenue_of(prev_rows, &r.label),
            )
        })
        .filter(|(_, d)| d * delta > 0.0)
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))?;
    if moved.abs() / delta.abs() < MIN_LEAD {
        return None;
    }
    let up = delta > 0.0;
    Some(format!(
        "Revenue is {} {:.0}% ({}) on the previous period, mostly from {label} ({}{}).",
        if up { "up" } else { "down" },
        delta.abs() / prev * 100.0,
        money(delta),
        if up { "+" } else { "-" },
        money(moved),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: &str, revenue: f64) -> BreakdownRow {
        BreakdownRow {
            key: Some(label.into()),
            label: label.into(),
            unattributed: false,
            visitors: 100,
            payments: 2,
            customers: 2,
            refunded: 0,
            revenue_usd: revenue,
            revenue_per_visitor: None,
            avg_visit_secs: None,
        }
    }

    #[test]
    fn names_the_channel_behind_a_big_move() {
        let cur = [row("Social", 1500.0), row("Direct", 500.0)];
        let prev = [row("Social", 200.0), row("Direct", 400.0)];
        assert_eq!(
            revenue_change(2000.0, 600.0, &cur, &prev).unwrap(),
            "Revenue is up 233% ($1,400) on the previous period, mostly from Social (+$1,300)."
        );
        let down = revenue_change(
            300.0,
            600.0,
            &[row("Direct", 300.0)],
            &[row("Direct", 600.0)],
        );
        assert!(down.unwrap().contains("down 50%"));
    }

    #[test]
    fn stays_quiet_for_small_moves_and_missing_history() {
        let rows = [row("Direct", 105.0)];
        assert!(revenue_change(105.0, 100.0, &rows, &[row("Direct", 100.0)]).is_none());
        assert!(revenue_change(500.0, 0.0, &rows, &[]).is_none());
        assert!(revenue_change(10.0, 5.0, &rows, &[]).is_none());
    }
}
