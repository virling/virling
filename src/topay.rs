//! How long each channel takes to turn a first visit into a payment, so a slow channel is not cut too early.

use std::collections::BTreeMap;

use crate::stats::PaymentRow;

/// A channel needs this many first payments before its timing says anything.
const MIN_PAYMENTS: usize = 3;

pub struct PayCard {
    pub rows: Vec<PayRow>,
}

pub struct PayRow {
    pub name: String,
    pub payments: usize,
    /// Median time from the first visit to the first payment, such as "same day" or "6 days".
    pub median: String,
    /// Share of those payments made within 1, 7 and 30 days of the first visit, in percent.
    pub within_1: u32,
    pub within_7: u32,
    pub within_30: u32,
}

fn pct(n: usize, of: usize) -> u32 {
    (n as f64 * 100.0 / of as f64).round() as u32
}

fn label(days: f64) -> String {
    if days < 1.0 {
        "same day".into()
    } else {
        let d = days.round() as i64;
        format!("{d} day{}", if d == 1 { "" } else { "s" })
    }
}

/// Timing per channel of the first touch, from the first payments in `rows`; channels with few payments are left out.
pub fn pay_card(rows: &[PaymentRow]) -> Option<PayCard> {
    let mut by: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for p in rows
        .iter()
        .filter(|p| !p.is_renewal && matches!(p.status.as_str(), "paid" | "partially_refunded"))
    {
        if let Some(first) = &p.first_touch {
            let days = (p.ts - first.ts).whole_seconds().max(0) as f64 / 86400.0;
            by.entry(first.channel.as_str()).or_default().push(days);
        }
    }
    let mut out: Vec<PayRow> = by
        .into_iter()
        .filter(|(_, d)| d.len() >= MIN_PAYMENTS)
        .map(|(name, mut d)| {
            d.sort_by(f64::total_cmp);
            let n = d.len();
            let median = if n % 2 == 1 {
                d[n / 2]
            } else {
                (d[n / 2 - 1] + d[n / 2]) / 2.0
            };
            let within = |limit: f64| pct(d.iter().filter(|x| **x <= limit).count(), n);
            PayRow {
                name: name.to_string(),
                payments: n,
                median: label(median),
                within_1: within(1.0),
                within_7: within(7.0),
                within_30: within(30.0),
            }
        })
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.payments));
    (!out.is_empty()).then_some(PayCard { rows: out })
}

#[cfg(test)]
mod tests {
    use time::{Duration, OffsetDateTime};

    use super::*;
    use crate::stats::FirstTouch;

    fn pay(channel: &str, days: i64, renewal: bool) -> PaymentRow {
        let first = OffsetDateTime::UNIX_EPOCH;
        PaymentRow {
            provider: "stripe".into(),
            provider_payment_id: format!("p{days}{channel}"),
            ts: first + Duration::days(days),
            amount: 1000,
            currency: "USD".into(),
            amount_usd: Some(10.0),
            refunded: 0,
            net_usd: Some(10.0),
            status: "paid".into(),
            visitor_id: None,
            customer_id: None,
            is_renewal: renewal,
            visitor: None,
            touch: None,
            first_touch: Some(FirstTouch {
                ts: first,
                channel: channel.into(),
                referrer_host: None,
                source: channel.into(),
            }),
        }
    }

    #[test]
    fn times_each_channel_and_skips_thin_ones_and_renewals() {
        let rows = vec![
            pay("Social", 0, false),
            pay("Social", 3, false),
            pay("Social", 20, false),
            pay("Social", 40, true),
            pay("Direct", 1, false),
            pay("Direct", 2, false),
        ];
        let card = pay_card(&rows).unwrap();
        assert_eq!(card.rows.len(), 1);
        let social = &card.rows[0];
        assert_eq!(social.name, "Social");
        assert_eq!(social.payments, 3);
        assert_eq!(social.median, "3 days");
        assert_eq!(
            (social.within_1, social.within_7, social.within_30),
            (33, 67, 100)
        );
        assert!(pay_card(&[]).is_none());
    }
}
