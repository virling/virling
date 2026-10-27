//! How long customers take to buy, from their first visit to their first payment.

use std::collections::HashMap;

use super::{Summary, group, plural};
use crate::stats::PathPay;

/// Upper bounds in days of the time-to-buy buckets, and their labels.
const BUCKETS: [(f64, &str); 5] = [
    (1.0, "Under a day"),
    (3.0, "1 to 3 days"),
    (7.0, "3 to 7 days"),
    (28.0, "1 to 4 weeks"),
    (f64::MAX, "Over 4 weeks"),
];

pub struct PathView {
    pub(super) count: u64,
    pub(super) stats: Vec<Summary>,
    /// Label, customers and their share of all, in percent.
    pub(super) buckets: Vec<(&'static str, u64, f64)>,
    /// Channel, customers who first came through it, customers it was credited for.
    pub(super) channels: Vec<(String, u64, u64)>,
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn number(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r:.1}")
    }
}

fn days(v: f64) -> String {
    if v < 1.0 {
        "Under a day".into()
    } else {
        let n = number(v);
        format!("{n} {}", if n == "1" { "day" } else { "days" })
    }
}

pub fn view(rows: &[PathPay]) -> PathView {
    let n = rows.len() as u64;
    let mid = |f: fn(&PathPay) -> f64| median(&mut rows.iter().map(f).collect::<Vec<_>>());
    let first = rows.iter().filter(|r| r.visits <= 1).count() as u64;
    let share = |a: u64| {
        if n > 0 {
            a as f64 * 100.0 / n as f64
        } else {
            0.0
        }
    };
    let (time, visits, pages) = if n > 0 {
        (
            days(mid(|r| r.days)),
            number(mid(|r| r.visits as f64)),
            number(mid(|r| r.pages as f64)),
        )
    } else {
        ("n/a".into(), "n/a".into(), "n/a".into())
    };
    let stats = vec![
        Summary {
            name: "Time to buy",
            value: time,
            sub: "Median, first visit to payment".into(),
        },
        Summary {
            name: "Visits before paying",
            value: visits,
            sub: "Median per customer".into(),
        },
        Summary {
            name: "Buy on first visit",
            value: if n > 0 {
                format!("{:.0}%", share(first))
            } else {
                "n/a".into()
            },
            sub: format!("{} of {}", group(first), plural(n, "customer")),
        },
        Summary {
            name: "Pages before paying",
            value: pages,
            sub: "Median pages seen".into(),
        },
    ];
    let mut counts = [0u64; BUCKETS.len()];
    for r in rows {
        if let Some(i) = BUCKETS.iter().position(|b| r.days < b.0) {
            counts[i] += 1;
        }
    }
    let mut by: HashMap<&str, (u64, u64)> = HashMap::new();
    for r in rows {
        by.entry(&r.first_channel).or_default().0 += 1;
        if let Some(c) = &r.credited_channel {
            by.entry(c).or_default().1 += 1;
        }
    }
    let mut channels: Vec<(String, u64, u64)> = by
        .into_iter()
        .map(|(c, (f, k))| (c.to_string(), f, k))
        .collect();
    channels.sort_by(|a, b| (b.1.max(b.2), &a.0).cmp(&(a.1.max(a.2), &b.0)));
    channels.truncate(6);
    PathView {
        count: n,
        stats,
        buckets: BUCKETS
            .iter()
            .zip(counts)
            .map(|(b, c)| (b.1, c, share(c)))
            .collect(),
        channels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pay(days: f64, visits: u64, pages: u64) -> PathPay {
        PathPay {
            days,
            visits,
            pages,
            first_channel: "Social".into(),
            credited_channel: Some("Direct".into()),
        }
    }

    #[test]
    fn no_payments_show_n_a() {
        let v = view(&[]);
        assert_eq!(v.count, 0);
        assert!(v.stats.iter().all(|s| s.value == "n/a"));
        assert!(v.channels.is_empty());
    }

    #[test]
    fn medians_and_buckets() {
        let v = view(&[pay(0.1, 1, 2), pay(2.0, 2, 5), pay(10.0, 4, 9)]);
        assert_eq!(v.stats[0].value, "2 days");
        assert_eq!(v.stats[1].value, "2");
        assert_eq!(v.stats[2].value, "33%");
        assert_eq!(v.stats[3].value, "5");
        assert_eq!(
            v.buckets.iter().map(|b| b.1).collect::<Vec<_>>(),
            [1, 1, 0, 1, 0]
        );
        assert_eq!(
            v.channels,
            [("Direct".to_string(), 0, 3), ("Social".to_string(), 3, 0)]
        );
    }
}
