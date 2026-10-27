//! Visitors who reached pricing or checkout and did not pay, by channel and by the page they left from.

use std::collections::BTreeMap;

use duckdb::Connection;
use serde::Serialize;

use crate::stats::{BreakdownRow, Period};
use crate::visitors::{VisitorFilter, visitor_list};

/// Pages listed on the card.
const SHOWN: usize = 6;

/// A visitor who reached pricing or checkout in the range.
pub struct Reached {
    pub channel: String,
    pub paid: bool,
    pub exit_page: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AlmostPaid {
    /// (channel, reached pricing or checkout, of them paid), most reached first.
    pub channels: Vec<(String, u64, u64)>,
    /// (last page seen in the range, visitors who did not pay), most first.
    pub pages: Vec<(String, u64)>,
}

impl AlmostPaid {
    pub fn collect(reached: impl Iterator<Item = Reached>) -> Self {
        let mut channels: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        let mut pages: BTreeMap<String, u64> = BTreeMap::new();
        for r in reached {
            let c = channels.entry(r.channel).or_default();
            c.0 += 1;
            c.1 += r.paid as u64;
            if let (false, Some(page)) = (r.paid, r.exit_page) {
                *pages.entry(page).or_default() += 1;
            }
        }
        let mut channels: Vec<_> = channels.into_iter().map(|(c, (n, p))| (c, n, p)).collect();
        channels.sort_by_key(|c| std::cmp::Reverse(c.1));
        let mut pages: Vec<_> = pages.into_iter().collect();
        pages.sort_by_key(|p| std::cmp::Reverse(p.1));
        Self { channels, pages }
    }
}

pub(crate) fn almost_paid(
    conn: &Connection,
    site_id: &str,
    period: &Period,
) -> anyhow::Result<AlmostPaid> {
    let rows = visitor_list(
        conn,
        site_id,
        period,
        &VisitorFilter::default(),
        u32::MAX,
        0,
    )?
    .rows;
    Ok(AlmostPaid::collect(
        rows.into_iter()
            .filter(|r| r.pricing || r.checkout)
            .map(|r| Reached {
                channel: r.channel,
                paid: r.payments > 0,
                exit_page: r.exit_page,
            }),
    ))
}

pub struct AlmostCard {
    pub rows: Vec<AlmostRow>,
    /// Page and visitors who left from it.
    pub pages: Vec<(String, String)>,
    pub on_pages: bool,
    /// Links to the two tabs.
    pub tabs: (String, String),
}

pub struct AlmostRow {
    pub name: String,
    pub reached: String,
    pub paid: String,
    pub left: String,
}

/// The card from the counts, valuing each non-buyer at their channel's revenue per visitor.
pub fn card(
    data: &AlmostPaid,
    channels: &[BreakdownRow],
    on_pages: bool,
    tabs: (String, String),
) -> Option<AlmostCard> {
    if data.channels.is_empty() {
        return None;
    }
    let earns = channels.iter().any(|c| c.revenue_usd > 0.0);
    let rows = data
        .channels
        .iter()
        .map(|(name, reached, paid)| {
            let rpv = channels
                .iter()
                .find(|c| c.key.as_deref() == Some(name))
                .and_then(|c| c.revenue_per_visitor)
                .unwrap_or(0.0);
            AlmostRow {
                name: name.clone(),
                reached: crate::dashboard::group(*reached),
                paid: crate::dashboard::group(*paid),
                left: if earns {
                    crate::dashboard::money((reached - paid) as f64 * rpv)
                } else {
                    "n/a".into()
                },
            }
        })
        .collect();
    Some(AlmostCard {
        rows,
        pages: data
            .pages
            .iter()
            .take(SHOWN)
            .map(|(p, n)| (p.clone(), crate::dashboard::group(*n)))
            .collect(),
        on_pages,
        tabs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reached(channel: &str, paid: bool, page: &str) -> Reached {
        Reached {
            channel: channel.into(),
            paid,
            exit_page: Some(page.into()),
        }
    }

    #[test]
    fn counts_channels_and_pages_of_those_who_did_not_pay() {
        let a = AlmostPaid::collect(
            [
                reached("Social", false, "/pricing"),
                reached("Social", true, "/checkout"),
                reached("Social", false, "/pricing"),
                reached("Email", false, "/docs"),
            ]
            .into_iter(),
        );
        assert_eq!(a.channels[0], ("Social".to_string(), 3, 1));
        assert_eq!(a.channels[1], ("Email".to_string(), 1, 0));
        assert_eq!(a.pages[0], ("/pricing".to_string(), 2));
        assert_eq!(a.pages.len(), 2);
    }

    #[test]
    fn visitors_only_shows_counts_without_money() {
        let a = AlmostPaid::collect([reached("Social", false, "/pricing")].into_iter());
        let c = card(&a, &[], false, Default::default()).unwrap();
        assert_eq!(
            (c.rows[0].reached.as_str(), c.rows[0].left.as_str()),
            ("1", "n/a")
        );
        assert!(card(&AlmostPaid::default(), &[], false, Default::default()).is_none());
    }
}
