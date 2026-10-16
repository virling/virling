//! Generated one-line summaries for the dashboard's insight and AI assistants cards.

use super::{group, money, usd};
use crate::channel;
use crate::stats::{BreakdownRow, Totals};

/// Below this many visitors or matched payments the card stays hidden.
const MIN_VISITORS: u64 = 50;
const MIN_MATCHED: u64 = 3;
/// A source or page needs this many visitors before its per-visitor value is quoted.
const MIN_ROW_VISITORS: u64 = 10;

pub struct AiRow {
    pub(super) host: String,
    pub(super) name: String,
    pub(super) visitors: u64,
    pub(super) revenue_usd: f64,
}

impl AiRow {
    fn per_visitor(&self) -> f64 {
        if self.visitors > 0 {
            self.revenue_usd / self.visitors as f64
        } else {
            0.0
        }
    }
}

/// The AI channel's sources from a channel-source breakdown, most visitors first.
pub fn ai_rows(sources: &[BreakdownRow]) -> Vec<AiRow> {
    let mut out: Vec<AiRow> = sources
        .iter()
        .filter_map(|r| {
            let host = r.key.as_deref()?.strip_prefix("AI|")?;
            (!host.is_empty() && r.visitors > 0).then(|| AiRow {
                host: host.to_string(),
                name: channel::source_name(host),
                visitors: r.visitors,
                revenue_usd: r.revenue_usd,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.visitors
            .cmp(&a.visitors)
            .then(b.revenue_usd.total_cmp(&a.revenue_usd))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

fn pct(n: f64, d: f64) -> String {
    if d > 0.0 {
        format!("{:.0}%", n / d * 100.0)
    } else {
        "0%".into()
    }
}

fn times(x: f64) -> String {
    if (1.95..2.05).contains(&x) {
        "twice".into()
    } else if (2.95..3.05).contains(&x) {
        "three times".into()
    } else {
        format!("{x:.1}x")
    }
}

fn count_word(n: usize) -> String {
    match n {
        2 => "two".into(),
        3 => "three".into(),
        4 => "four".into(),
        5 => "five".into(),
        n => n.to_string(),
    }
}

/// AI share of visitors and of revenue, as "27%".
pub fn ai_shares(t: &Totals, channels: &[BreakdownRow]) -> (String, String) {
    let ai = channels
        .iter()
        .find(|r| !r.unattributed && r.key.as_deref() == Some("AI"));
    let (v, r) = ai.map_or((0.0, 0.0), |a| (a.visitors as f64, a.revenue_usd));
    (pct(v, t.visitors as f64), pct(r, t.revenue_usd))
}

/// One line under the AI assistants list.
pub fn ai_line(rows: &[AiRow]) -> String {
    let Some(best) = rows
        .iter()
        .filter(|r| r.revenue_usd > 0.0)
        .max_by(|a, b| a.per_visitor().total_cmp(&b.per_visitor()))
    else {
        return if rows.is_empty() {
            String::new()
        } else {
            "No AI visitor has paid in this range yet.".into()
        };
    };
    let n = rows.len();
    if n == 1 {
        return format!(
            "{} is the only AI assistant sending visitors so far.",
            best.name
        );
    }
    if rows.last().is_some_and(|l| l.host == best.host) {
        format!(
            "{} sends the fewest of the {} but the highest value per visitor.",
            best.name,
            count_word(n)
        )
    } else if rows[0].host == best.host {
        format!(
            "{} sends the most AI visitors and the highest value per visitor.",
            best.name
        )
    } else {
        format!(
            "{} has the highest value per visitor, {}.",
            best.name,
            usd(best.per_visitor())
        )
    }
}

/// The insight card's lead sentence and its "cheapest fix" line against `avg` per visitor, or None when data is too thin.
pub fn summary(
    t: &Totals,
    avg: Option<f64>,
    channels: &[BreakdownRow],
    ai: &[AiRow],
    landing: &[BreakdownRow],
) -> Option<(String, String)> {
    let avg = avg.filter(|a| *a > 0.0)?;
    if t.visitors < MIN_VISITORS || t.matched_payments < MIN_MATCHED {
        return None;
    }
    let ai_channel = channels
        .iter()
        .find(|r| !r.unattributed && r.key.as_deref() == Some("AI"))
        .filter(|r| r.visitors >= MIN_ROW_VISITORS);
    let mut lead = match ai_channel {
        Some(a) => format!(
            "AI assistants bring {} of your visitors and {} of your revenue.",
            pct(a.visitors as f64, t.visitors as f64),
            pct(a.revenue_usd, t.revenue_usd)
        ),
        None => {
            let top = channels
                .iter()
                .filter(|r| !r.unattributed && r.key.is_some() && r.revenue_usd > 0.0)
                .max_by(|a, b| a.revenue_usd.total_cmp(&b.revenue_usd))?;
            format!(
                "{} brings {} of your visitors and {} of your revenue.",
                top.label,
                pct(top.visitors as f64, t.visitors as f64),
                pct(top.revenue_usd, t.revenue_usd)
            )
        }
    };
    let best_ai = ai
        .iter()
        .filter(|r| r.visitors >= MIN_ROW_VISITORS)
        .max_by(|a, b| a.per_visitor().total_cmp(&b.per_visitor()))
        .filter(|r| r.per_visitor() / avg >= 1.15);
    let best_channel = channels
        .iter()
        .filter(|r| !r.unattributed && r.key.is_some() && r.visitors >= MIN_ROW_VISITORS)
        .filter_map(|r| Some((r, r.revenue_per_visitor?)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|(_, pv)| pv / avg >= 1.15);
    if let Some(b) = best_ai.filter(|_| ai_channel.is_some()) {
        lead.push_str(&format!(
            " A {} visitor is worth {}, {} your average.",
            b.name,
            usd(b.per_visitor()),
            times(b.per_visitor() / avg)
        ));
    } else if let Some((r, pv)) = best_channel {
        lead.push_str(&format!(
            " {} visitors are worth {}, {} your average.",
            r.label,
            usd(pv),
            times(pv / avg)
        ));
    }
    let busy = (t.visitors / 20).max(MIN_ROW_VISITORS * 2);
    let fix = landing
        .iter()
        .filter(|r| !r.unattributed && r.key.is_some() && r.visitors >= busy)
        .filter_map(|r| Some((r, r.revenue_per_visitor? / avg)))
        .min_by(|a, b| a.1.total_cmp(&b.1).then(b.0.visitors.cmp(&a.0.visitors)))
        .filter(|(_, x)| *x <= 0.85)
        .map(|(r, x)| {
            format!(
                "{} gets {} visitors but earns {x:.1}x average. That page is your cheapest fix.",
                r.label,
                group(r.visitors)
            )
        })
        .unwrap_or_default();
    Some((lead, fix))
}

/// "$608 came from payments with no matched visit", or empty when every payment matched.
pub fn unmatched(channels: &[BreakdownRow]) -> String {
    let usd: f64 = channels
        .iter()
        .filter(|r| r.unattributed)
        .map(|r| r.revenue_usd)
        .sum();
    if usd > 0.0 {
        format!("{} came from payments with no matched visit", money(usd))
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, visitors: u64, revenue_usd: f64) -> BreakdownRow {
        BreakdownRow {
            key: Some(key.into()),
            label: key.into(),
            unattributed: false,
            visitors,
            payments: 1,
            customers: 1,
            revenue_usd,
            revenue_per_visitor: Some(revenue_usd / visitors as f64),
        }
    }

    fn totals(visitors: u64, revenue: f64) -> Totals {
        Totals {
            visitors,
            revenue_usd: revenue,
            matched_payments: 10,
            revenue_per_visitor: Some(revenue / visitors as f64),
            ..Totals::default()
        }
    }

    #[test]
    fn ai_sentences_compare_shares_and_value() {
        let sources = [
            row("AI|chatgpt.com", 544, 1174.0),
            row("AI|claude.ai", 148, 447.0),
            row("AI|perplexity.ai", 164, 357.0),
            row("Organic search|google.com", 900, 1200.0),
        ];
        let ai = ai_rows(&sources);
        let names: Vec<_> = ai.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["ChatGPT", "Perplexity", "Claude"]);
        assert_eq!(
            ai_line(&ai),
            "Claude sends the fewest of the three but the highest value per visitor."
        );
        let t = totals(3167, 6121.0);
        let channels = [row("AI", 844, 1978.0), row("Organic search", 1228, 1717.0)];
        assert_eq!(ai_shares(&t, &channels), ("27%".into(), "32%".into()));
        let landing = [row("/pricing", 619, 1200.0), row("/compare", 479, 570.0)];
        let (lead, fix) = summary(&t, t.revenue_per_visitor, &channels, &ai, &landing).unwrap();
        assert_eq!(
            lead,
            "AI assistants bring 27% of your visitors and 32% of your revenue. A Claude visitor is worth $3.02, 1.6x your average."
        );
        assert_eq!(
            fix,
            "/compare gets 479 visitors but earns 0.6x average. That page is your cheapest fix."
        );
    }

    #[test]
    fn thin_data_hides_the_summary() {
        let channels = [row("Direct", 20, 100.0)];
        assert!(summary(&totals(20, 100.0), Some(5.0), &channels, &[], &[]).is_none());
        assert_eq!(ai_line(&[]), "");
        let mut lost = row("x", 0, 50.0);
        lost.unattributed = true;
        assert_eq!(
            unmatched(&[lost]),
            "$50 came from payments with no matched visit"
        );
    }
}
