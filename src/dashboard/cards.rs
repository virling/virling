//! The dashboard's tabbed cards: Sources, Location and Pages.

use std::collections::HashMap;

use super::{Opt, group, link, money, usd};
use crate::icons;
use crate::stats::{BreakdownRow, Dimension};

/// Rows shown per card.
const SHOWN: usize = 6;

pub struct Spec {
    /// Query param for the tab, also the card's anchor.
    pub key: &'static str,
    label: &'static str,
    /// (param value, label, breakdown); None reads the pages viewed last before paying.
    tabs: &'static [(&'static str, &'static str, Option<Dimension>)],
}

pub const SPECS: [Spec; 3] = [
    Spec {
        key: "sources",
        label: "Source breakdown",
        tabs: &[
            ("channel", "Channel", Some(Dimension::Channel)),
            ("referrer", "Referrer", Some(Dimension::Referrer)),
            ("campaign", "Campaign", Some(Dimension::Campaign)),
        ],
    },
    Spec {
        key: "location",
        label: "Location breakdown",
        tabs: &[
            ("country", "Country", Some(Dimension::Country)),
            ("device", "Device", Some(Dimension::Device)),
        ],
    },
    Spec {
        key: "pages",
        label: "Page breakdown",
        tabs: &[
            ("landing", "Landing", Some(Dimension::LandingPage)),
            ("last", "Last before paying", None),
        ],
    },
];

/// The tab one card shows, read from the query string.
pub struct Pick {
    spec: &'static Spec,
    tab: usize,
}

impl Pick {
    pub fn dimension(&self) -> Option<Dimension> {
        self.spec.tabs[self.tab].2
    }
}

pub fn picks(q: &HashMap<String, String>) -> Vec<Pick> {
    SPECS
        .iter()
        .map(|spec| Pick {
            spec,
            tab: q
                .get(spec.key)
                .and_then(|t| spec.tabs.iter().position(|(k, _, _)| k == t))
                .unwrap_or(0),
        })
        .collect()
}

/// The non-default tab params of `picks`, for links that keep the other cards as they are.
pub fn params(picks: &[Pick]) -> Vec<(&'static str, &'static str)> {
    picks
        .iter()
        .filter(|p| p.tab != 0)
        .map(|p| (p.spec.key, p.spec.tabs[p.tab].0))
        .collect()
}

pub struct BarRow {
    pub(super) icon: String,
    pub(super) label: String,
    pub(super) mono: bool,
    pub(super) visitors: String,
    pub(super) revenue: String,
    pub(super) per_visitor: String,
    /// Per visitor against the site average, "1.4x"; empty without an average.
    pub(super) ratio: String,
    pub(super) tone: &'static str,
    /// Visitor and revenue shares, scaled so the largest share fills the bar.
    pub(super) vw: f64,
    pub(super) rw: f64,
}

pub struct TabCard {
    pub(super) key: &'static str,
    pub(super) label: &'static str,
    pub(super) tabs: Vec<Opt>,
    pub(super) rows: Vec<BarRow>,
    pub(super) note: &'static str,
    pub(super) empty: &'static str,
}

/// "up" at 1.15x the average or more, "down" at 0.85x or less.
pub fn tone(x: f64) -> &'static str {
    if x >= 1.15 {
        "up"
    } else if x <= 0.85 {
        "down"
    } else {
        "flat"
    }
}

fn label(dim: Option<Dimension>, r: &BreakdownRow) -> (String, String, bool) {
    let key = r.key.as_deref().unwrap_or_default();
    match dim {
        Some(Dimension::Channel | Dimension::Referrer) => {
            (icons::source(&r.label, 16), r.label.clone(), false)
        }
        Some(Dimension::Campaign) => (icons::glyph("paid", 16, "#6B717C"), r.label.clone(), false),
        Some(Dimension::Country) => (icons::flag(key), icons::country(key), false),
        Some(Dimension::Device) => (icons::device(key), r.label.clone(), false),
        _ => (String::new(), r.label.clone(), true),
    }
}

/// Bar rows sized like the design: each share of the shown rows, over the largest share.
pub fn bar_rows(
    rows: &[&BreakdownRow],
    avg: Option<f64>,
    label: impl Fn(&BreakdownRow) -> (String, String, bool),
) -> Vec<BarRow> {
    let sv: f64 = rows.iter().map(|r| r.visitors as f64).sum();
    let sr: f64 = rows.iter().map(|r| r.revenue_usd.max(0.0)).sum();
    let share = |n: f64, total: f64| if total > 0.0 { n / total } else { 0.0 };
    let m = rows
        .iter()
        .map(|r| share(r.visitors as f64, sv).max(share(r.revenue_usd.max(0.0), sr)))
        .fold(0.0, f64::max);
    let scale = |s: f64| if m > 0.0 { s / m * 100.0 } else { 0.0 };
    rows.iter()
        .map(|r| {
            let (icon, label, mono) = label(r);
            let pv = r.revenue_per_visitor.unwrap_or(0.0);
            let x = avg.filter(|a| *a > 0.0 && r.visitors > 0).map(|a| pv / a);
            BarRow {
                icon,
                label,
                mono,
                visitors: group(r.visitors),
                revenue: money(r.revenue_usd),
                per_visitor: usd(pv),
                ratio: x.map(|x| format!("{x:.1}x")).unwrap_or_default(),
                tone: x.map_or("", tone),
                vw: scale(share(r.visitors as f64, sv)),
                rw: scale(share(r.revenue_usd.max(0.0), sr)),
            }
        })
        .collect()
}

/// Builds one card from its breakdown; `avg` is the site's revenue per visitor.
pub fn card(
    pick: &Pick,
    rows: &[BreakdownRow],
    avg: Option<f64>,
    base: &str,
    params: &[(&str, &str)],
) -> TabCard {
    let spec = pick.spec;
    let dim = pick.dimension();
    let shown: Vec<&BreakdownRow> = rows
        .iter()
        .filter(|r| !r.unattributed && r.key.is_some())
        .take(SHOWN)
        .collect();
    TabCard {
        key: spec.key,
        label: spec.label,
        tabs: spec
            .tabs
            .iter()
            .enumerate()
            .map(|(i, (k, name, _))| Opt {
                label: name.to_string(),
                href: format!(
                    "{}#{}",
                    link(base, params, &[(spec.key, if i == 0 { "" } else { k })]),
                    spec.key
                ),
                on: i == pick.tab,
                count: String::new(),
            })
            .collect(),
        rows: bar_rows(&shown, avg, |r| label(dim, r)),
        note: match (spec.key, dim) {
            ("pages", None) => "Page read right before checkout",
            ("pages", _) => "First page of the visit",
            _ => "",
        },
        empty: match dim {
            Some(Dimension::Campaign) => "No visits with utm_campaign in this range.",
            None => "No payment matched to a page view in this range.",
            _ => "Nothing in this range yet.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, visitors: u64, revenue: f64) -> BreakdownRow {
        BreakdownRow {
            key: Some(key.into()),
            label: key.into(),
            unattributed: false,
            visitors,
            payments: 1,
            customers: 1,
            revenue_usd: revenue,
            revenue_per_visitor: Some(revenue / visitors as f64),
        }
    }

    #[test]
    fn bars_scale_to_the_largest_share_and_badge_against_the_average() {
        let rows = [row("/a", 300, 100.0), row("/b", 100, 300.0)];
        let refs: Vec<&BreakdownRow> = rows.iter().collect();
        let bars = bar_rows(&refs, Some(1.0), |r| (String::new(), r.label.clone(), true));
        let w = |b: &BarRow| (b.vw.round(), b.rw.round());
        assert_eq!(w(&bars[0]), (100.0, 33.0));
        assert_eq!(w(&bars[1]), (33.0, 100.0));
        assert_eq!((bars[0].ratio.as_str(), bars[0].tone), ("0.3x", "down"));
        assert_eq!((bars[1].ratio.as_str(), bars[1].tone), ("3.0x", "up"));
        assert_eq!(tone(1.0), "flat");
        let none = bar_rows(&refs, None, |r| (String::new(), r.label.clone(), true));
        assert!(none[0].ratio.is_empty());
    }

    #[test]
    fn tabs_keep_the_other_cards_and_jump_back_to_the_card() {
        let q: HashMap<String, String> = [("pages".to_string(), "last".to_string())].into();
        let picks = picks(&q);
        assert_eq!(picks[2].dimension(), None);
        let params = params(&picks);
        assert_eq!(params, [("pages", "last")]);
        let c = card(&picks[0], &[], None, "/app/x.com", &params);
        assert_eq!(
            c.tabs[1].href,
            "/app/x.com?pages=last&sources=referrer#sources"
        );
        assert_eq!(c.tabs[0].href, "/app/x.com?pages=last#sources");
    }
}
