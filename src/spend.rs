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
    let text = lines(&parse(input));
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
}

pub struct ProfitRow {
    pub name: String,
    pub spend: f64,
    pub revenue: f64,
    pub profit: f64,
    /// Revenue per dollar spent.
    pub ret: f64,
}

/// The profit of every channel with a spend, for a range of `days` days; None when no channel has one.
pub fn profit_card(
    channels: &[BreakdownRow],
    spend: &[(&str, f64)],
    days: i64,
) -> Option<ProfitCard> {
    let mut rows: Vec<ProfitRow> = spend
        .iter()
        .map(|(name, monthly)| {
            let spent = monthly * days.max(1) as f64 / MONTH_DAYS;
            let revenue = channels
                .iter()
                .find(|r| !r.unattributed && r.label == *name)
                .map_or(0.0, |r| r.revenue_usd);
            ProfitRow {
                name: (*name).to_string(),
                spend: spent,
                revenue,
                profit: revenue - spent,
                ret: revenue / spent,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.profit.total_cmp(&a.profit));
    (!rows.is_empty()).then_some(ProfitCard { rows })
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
            30,
        )
        .unwrap();
        assert_eq!(card.rows[0].name, "Paid");
        assert!((card.rows[0].spend - 300.0).abs() < 1.0);
        assert!(card.rows[0].profit > 290.0 && card.rows[0].ret > 1.9);
        assert!(card.rows[1].profit < 0.0 && card.rows[1].ret == 0.0);
        assert!(profit_card(&[], &[], 30).is_none());
    }
}
