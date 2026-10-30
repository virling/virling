//! What a site spends on each channel each month, and the profit it leaves next to the channel's revenue.

use sqlx::SqlitePool;

use crate::stats::BreakdownRow;

/// The channel names a spend line may start with.
const CHANNELS: [&str; 8] = [
    "Paid",
    "Email",
    "AI",
    "Organic search",
    "Communities",
    "Social",
    "Referral",
    "Direct",
];

/// Days in an average month, to turn a monthly amount into the amount for a range.
const MONTH_DAYS: f64 = 30.4375;

/// Largest monthly amount accepted for one channel.
const MAX_MONTHLY: f64 = 10_000_000.0;

/// The valid `Channel amount` lines of `input`, such as `Paid 300` or `Organic search $1,200`; a channel repeated keeps its last amount.
pub fn parse(input: &str) -> Vec<(&'static str, f64)> {
    let mut out: Vec<(&'static str, f64)> = Vec::new();
    for line in input.lines() {
        let line = line.trim();
        let Some(name) = CHANNELS.iter().find(|c| {
            line.get(..c.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(c))
        }) else {
            continue;
        };
        let amount: String = line[name.len()..]
            .chars()
            .filter(|c| !matches!(c, '$' | ',' | ' '))
            .collect();
        let Ok(amount) = amount.parse::<f64>() else {
            continue;
        };
        if !(amount.is_finite() && amount > 0.0 && amount <= MAX_MONTHLY) {
            continue;
        }
        out.retain(|(n, _)| n != name);
        out.push((name, amount));
    }
    out
}

/// Payment fees as (percent, fixed dollars per payment) from a `Fees 2.9% + 0.30` line.
pub fn fees(input: &str) -> Option<(f64, f64)> {
    let line = input
        .lines()
        .map(str::trim)
        .rfind(|l| l.get(..4).is_some_and(|h| h.eq_ignore_ascii_case("fees")))?;
    let (pct, fixed) = line[4..].split_once('%')?;
    let num = |s: &str| -> Option<f64> {
        let s: String = s
            .chars()
            .filter(|c| !matches!(c, '$' | ' ' | '+'))
            .collect();
        if s.is_empty() {
            Some(0.0)
        } else {
            s.parse().ok()
        }
    };
    let (pct, fixed) = (num(pct)?, num(fixed)?);
    ((0.0..=50.0).contains(&pct) && (0.0..=100.0).contains(&fixed) && pct + fixed > 0.0)
        .then_some((pct, fixed))
}

/// The lines as stored and shown: one `Channel amount` per line.
pub fn lines(entries: &[(&str, f64)]) -> String {
    entries
        .iter()
        .map(|(n, a)| format!("{n} {}", (a * 100.0).round() / 100.0))
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn load(db: &SqlitePool, site_id: &str) -> sqlx::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT lines FROM channel_spend WHERE site_id = ?")
            .bind(site_id)
            .fetch_optional(db)
            .await?
            .unwrap_or_default(),
    )
}

/// Replaces the site's spend with the valid lines of `input`.
pub async fn save(db: &SqlitePool, site_id: &str, input: &str) -> sqlx::Result<()> {
    let mut text = lines(&parse(input));
    if let Some((pct, fixed)) = fees(input) {
        let line = format!("Fees {pct}% + {fixed:.2}");
        text = if text.is_empty() {
            line
        } else {
            format!("{text}\n{line}")
        };
    }
    if text.is_empty() {
        sqlx::query("DELETE FROM channel_spend WHERE site_id = ?")
            .bind(site_id)
            .execute(db)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO channel_spend (site_id, lines) VALUES (?, ?)
             ON CONFLICT (site_id) DO UPDATE SET lines = excluded.lines",
        )
        .bind(site_id)
        .bind(text)
        .execute(db)
        .await?;
    }
    Ok(())
}

/// Spend, revenue, profit and return of each channel with a monthly spend.
pub struct ProfitCard {
    pub rows: Vec<ProfitRow>,
    /// Costs include payment fees.
    pub fees: bool,
}

pub struct ProfitRow {
    pub name: String,
    pub spend: f64,
    /// Estimated payment fees, from the site's fee rate.
    pub fees: f64,
    pub revenue: f64,
    pub profit: f64,
    /// Revenue per dollar spent; NaN without a recorded spend.
    pub ret: f64,
}

/// The profit of every channel with a spend, or with revenue once fees are set, for a range of `days` days; None when there is none.
pub fn profit_card(
    channels: &[BreakdownRow],
    spend: &[(&str, f64)],
    fees: Option<(f64, f64)>,
    days: i64,
) -> Option<ProfitCard> {
    let earning = channels
        .iter()
        .filter(|r| fees.is_some() && !r.unattributed && r.revenue_usd > 0.0)
        .map(|r| r.label.as_str());
    let mut names: Vec<&str> = spend.iter().map(|s| s.0).collect();
    for n in earning {
        if !names.contains(&n) {
            names.push(n);
        }
    }
    let mut rows: Vec<ProfitRow> = names
        .into_iter()
        .map(|name| {
            let monthly = spend.iter().find(|s| s.0 == name).map_or(0.0, |s| s.1);
            let spent = monthly * days.max(1) as f64 / MONTH_DAYS;
            let row = channels.iter().find(|r| !r.unattributed && r.label == name);
            let revenue = row.map_or(0.0, |r| r.revenue_usd);
            let paid = row.map_or(0, |r| r.payments) as f64;
            let fee = fees.map_or(0.0, |(pct, fixed)| revenue * pct / 100.0 + paid * fixed);
            ProfitRow {
                name: name.to_string(),
                spend: spent,
                fees: fee,
                revenue,
                profit: revenue - spent - fee,
                ret: if spent > 0.0 {
                    revenue / (spent + fee)
                } else {
                    f64::NAN
                },
            }
        })
        .collect();
    rows.sort_by(|a, b| b.profit.total_cmp(&a.profit));
    (!rows.is_empty()).then_some(ProfitCard {
        rows,
        fees: fees.is_some(),
    })
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
    fn reads_valid_lines_and_keeps_the_last_amount_of_a_channel() {
        let got = parse(
            "Paid 300\nsocial $1,200.50\nOrganic search 80\nnonsense 5\nPaid 250\nEmail -4\nAI abc",
        );
        assert_eq!(
            got,
            vec![
                ("Social", 1200.5),
                ("Organic search", 80.0),
                ("Paid", 250.0)
            ]
        );
        assert_eq!(lines(&got), "Social 1200.5\nOrganic search 80\nPaid 250");
    }

    #[test]
    fn profit_prorates_a_month_over_the_range() {
        let card = profit_card(
            &[row("Paid", 600.0), row("Social", 0.0)],
            &[("Paid", 304.375), ("Social", 100.0)],
            None,
            30,
        )
        .unwrap();
        assert_eq!(card.rows[0].name, "Paid");
        assert!((card.rows[0].spend - 300.0).abs() < 1.0);
        assert!(card.rows[0].profit > 290.0 && card.rows[0].ret > 1.9);
        assert!(card.rows[1].profit < 0.0 && card.rows[1].ret == 0.0);
        assert!(profit_card(&[], &[], None, 30).is_none());
        let fees = fees("Paid 10\nfees 2.9% + $0.30").unwrap();
        assert_eq!(fees, (2.9, 0.3));
        let card = profit_card(&[row("Email", 100.0)], &[], Some(fees), 30).unwrap();
        assert_eq!(card.rows[0].name, "Email");
        assert!(
            (card.rows[0].fees - 3.5).abs() < 1e-9 && (card.rows[0].profit - 96.5).abs() < 1e-9
        );
        assert_eq!(super::fees("Fees 5%"), Some((5.0, 0.0)));
        assert_eq!(super::fees("Fees 80% + 1"), None);
        assert_eq!(super::fees("Fees lots"), None);
    }
}
