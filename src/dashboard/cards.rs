//! The dashboard's tabbed breakdown cards: Acquisition, Location, Pages and Devices.

use std::collections::HashMap;

use super::{Opt, group, link, money, usd};
use crate::stats::{BreakdownRow, Dimension};
use crate::{channel, chart, icons, world};

pub struct Spec {
    /// Query param for the tab; `{key}_by` holds the metric.
    pub key: &'static str,
    by: &'static str,
    title: &'static str,
    tabs: &'static [(&'static str, &'static str, Dimension)],
    default: usize,
}

pub const SPECS: [Spec; 4] = [
    Spec {
        key: "acq",
        by: "acq_by",
        title: "Acquisition",
        tabs: &[
            ("channel", "Channel", Dimension::Channel),
            ("referrer", "Referrer", Dimension::Referrer),
            ("campaign", "Campaign", Dimension::Campaign),
            ("keyword", "Keyword", Dimension::Keyword),
        ],
        default: 0,
    },
    Spec {
        key: "loc",
        by: "loc_by",
        title: "Location",
        tabs: &[
            ("map", "Map", Dimension::Country),
            ("country", "Country", Dimension::Country),
            ("region", "Region", Dimension::Region),
            ("city", "City", Dimension::City),
        ],
        default: 0,
    },
    Spec {
        key: "pg",
        by: "pg_by",
        title: "Pages",
        tabs: &[
            ("hostname", "Hostname", Dimension::Hostname),
            ("page", "Page", Dimension::Page),
            ("entry", "Entry page", Dimension::LandingPage),
            ("exit", "Exit link", Dimension::ExitLink),
        ],
        default: 2,
    },
    Spec {
        key: "dev",
        by: "dev_by",
        title: "Devices",
        tabs: &[
            ("browser", "Browser", Dimension::Browser),
            ("os", "OS", Dimension::Os),
            ("device", "Device", Dimension::Device),
        ],
        default: 0,
    },
];

#[derive(Clone, Copy, PartialEq)]
pub enum By {
    Revenue,
    Visitors,
    Conversion,
    PerVisitor,
}

const BYS: [(By, &str, &str); 4] = [
    (By::Revenue, "revenue", "Revenue"),
    (By::Visitors, "visitors", "Visitors"),
    (By::Conversion, "conversion", "Conversion"),
    (By::PerVisitor, "rpv", "Per visitor"),
];

impl By {
    fn value(self, r: &BreakdownRow) -> f64 {
        match self {
            By::Revenue => r.revenue_usd,
            By::Visitors => r.visitors as f64,
            By::Conversion => conversion(r),
            By::PerVisitor => r.revenue_per_visitor.unwrap_or(0.0),
        }
    }

    fn format(self, r: &BreakdownRow) -> String {
        match self {
            By::Revenue => money(r.revenue_usd),
            By::Visitors => group(r.visitors),
            By::Conversion => format!("{:.1}%", conversion(r) * 100.0),
            By::PerVisitor => usd(r.revenue_per_visitor.unwrap_or(0.0)),
        }
    }
}

fn conversion(r: &BreakdownRow) -> f64 {
    if r.visitors > 0 {
        (r.customers as f64 / r.visitors as f64).min(1.0)
    } else {
        0.0
    }
}

/// What one card shows, read from the query string.
pub struct Pick {
    spec: &'static Spec,
    tab: usize,
    by: By,
    all: bool,
}

impl Pick {
    pub fn dimension(&self) -> Dimension {
        self.spec.tabs[self.tab].2
    }

    /// Channel rows open into their sources, which need one more query.
    pub fn needs_sources(&self) -> bool {
        self.dimension() == Dimension::Channel
    }
}

pub fn picks(q: &HashMap<String, String>) -> Vec<Pick> {
    SPECS
        .iter()
        .map(|spec| {
            let tab = q
                .get(spec.key)
                .and_then(|t| spec.tabs.iter().position(|(k, _, _)| k == t))
                .unwrap_or(spec.default);
            let by = q
                .get(spec.by)
                .and_then(|b| BYS.iter().find(|(_, k, _)| k == b))
                .map_or(By::Revenue, |b| b.0);
            Pick {
                spec,
                tab,
                by,
                all: q.get("all").is_some_and(|a| a == spec.key),
            }
        })
        .collect()
}

/// The non-default card params of `picks`, for links that keep the other cards as they are.
pub fn params(picks: &[Pick]) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for p in picks {
        if p.tab != p.spec.default {
            out.push((p.spec.key, p.spec.tabs[p.tab].0));
        }
        if p.by != By::Revenue {
            out.push((
                p.spec.by,
                BYS.iter().find(|b| b.0 == p.by).map_or("", |b| b.1),
            ));
        }
        if p.all {
            out.push(("all", p.spec.key));
        }
    }
    out
}

pub struct BarRow {
    pub(super) icon: String,
    pub(super) label: String,
    pub(super) mono: bool,
    pub(super) value: String,
    pub(super) second: String,
    /// Visitors, as a share of the busiest row.
    pub(super) blue: f64,
    /// Revenue per visitor relative to the best row, drawn inside the blue bar.
    pub(super) orange: f64,
    pub(super) zero: bool,
    /// A channel's sources, shown when the row is opened.
    pub(super) sources: Vec<BarRow>,
}

pub struct TabCard {
    pub(super) key: &'static str,
    pub(super) title: &'static str,
    pub(super) tabs: Vec<Opt>,
    pub(super) metrics: Vec<Opt>,
    pub(super) metric: &'static str,
    pub(super) head: &'static str,
    pub(super) rows: Vec<BarRow>,
    pub(super) map: String,
    pub(super) empty: &'static str,
    pub(super) foot: String,
    pub(super) foot_href: String,
    pub(super) foot_link: &'static str,
}

const SHOWN: usize = 8;

fn label(dim: Dimension, r: &BreakdownRow) -> (String, String, bool) {
    let key = r.key.as_deref().unwrap_or_default();
    let placed = || match key.split_once('|') {
        Some((c, name)) => (icons::flag(c), name.to_string()),
        None => (String::new(), key.to_string()),
    };
    match dim {
        _ if r.unattributed => (
            icons::glyph("unattributed", 16, "#B9BCC2"),
            "Unattributed".into(),
            false,
        ),
        Dimension::Channel => (
            icons::glyph(
                icons::channel_glyph(key).unwrap_or("link"),
                16,
                chart::channel_color(r.key.as_deref()),
            ),
            r.label.clone(),
            false,
        ),
        Dimension::Referrer => (icons::source(&r.label, 16), r.label.clone(), false),
        Dimension::Country if r.key.is_some() => (icons::flag(key), icons::country(key), false),
        Dimension::Country => (String::new(), "Unknown".into(), false),
        Dimension::Region | Dimension::City => {
            let (icon, name) = placed();
            (icon, name, false)
        }
        Dimension::ChannelSource => {
            let host = key.split_once('|').map_or("", |(_, h)| h);
            (icons::source(host, 16), channel::source_name(host), false)
        }
        Dimension::ExitLink => {
            let host = key.split('/').next().unwrap_or_default();
            (icons::source(host, 16), r.label.clone(), false)
        }
        Dimension::Browser | Dimension::Os => (icons::ua(key), r.label.clone(), false),
        Dimension::Device => (icons::device(key), r.label.clone(), false),
        Dimension::Hostname | Dimension::Page | Dimension::LandingPage => {
            (String::new(), r.label.clone(), true)
        }
        Dimension::Campaign | Dimension::Keyword => (String::new(), r.label.clone(), false),
    }
}

fn bar_rows(dim: Dimension, rows: &[&BreakdownRow], by: By) -> Vec<BarRow> {
    let max_visitors = rows.iter().map(|r| r.visitors).max().unwrap_or(0) as f64;
    let max_rpv = rows
        .iter()
        .filter_map(|r| r.revenue_per_visitor)
        .fold(0.0, f64::max);
    rows.iter()
        .map(|r| {
            let (icon, label, mono) = label(dim, r);
            let blue = if max_visitors > 0.0 {
                r.visitors as f64 / max_visitors * 100.0
            } else {
                0.0
            };
            let orange = match r.revenue_per_visitor {
                Some(v) if max_rpv > 0.0 => v / max_rpv * 100.0,
                _ => 0.0,
            };
            BarRow {
                icon,
                label,
                mono,
                value: if r.unattributed && by != By::Revenue {
                    "n/a".into()
                } else {
                    by.format(r)
                },
                second: match by {
                    _ if r.unattributed => String::new(),
                    By::Visitors => money(r.revenue_usd),
                    _ => format!(
                        "{} visitor{}",
                        group(r.visitors),
                        if r.visitors == 1 { "" } else { "s" }
                    ),
                },
                blue,
                orange: if r.unattributed { 100.0 } else { orange },
                zero: r.unattributed,
                sources: Vec::new(),
            }
        })
        .collect()
}

fn empty_text(dim: Dimension) -> &'static str {
    match dim {
        Dimension::Region | Dimension::City => {
            "Region and city show up when your proxy sends visitor location headers, such as Cloudflare or Vercel."
        }
        Dimension::Campaign => "No visits with utm_campaign in this range.",
        Dimension::Keyword => "No visits with utm_term in this range.",
        Dimension::ExitLink => "No outbound link clicks in this range.",
        _ => "Nothing in this range yet.",
    }
}

/// Builds one card from its breakdown and, for channels, the per-channel sources.
pub fn card(
    pick: &Pick,
    rows: &[BreakdownRow],
    sources: &[BreakdownRow],
    base: &str,
    params: &[(&str, &str)],
    foot: Option<String>,
) -> TabCard {
    let spec = pick.spec;
    let dim = pick.dimension();
    let by = pick.by;
    let anchor = |href: String| format!("{href}#card-{}", spec.key);
    let mut shown: Vec<&BreakdownRow> = rows
        .iter()
        .filter(|r| !r.unattributed && (r.key.is_some() || dim == Dimension::Country))
        .collect();
    shown.sort_by(|a, b| {
        by.value(b)
            .total_cmp(&by.value(a))
            .then(b.visitors.cmp(&a.visitors))
            .then_with(|| a.label.cmp(&b.label))
    });
    let total = shown.len();
    shown.truncate(if pick.all { 50 } else { SHOWN });
    if dim == Dimension::Channel
        && let Some(u) = rows.iter().find(|r| r.unattributed && r.payments > 0)
    {
        shown.push(u);
    }
    let mut bars = bar_rows(dim, &shown, by);
    if dim == Dimension::Channel {
        for (bar, r) in bars.iter_mut().zip(&shown) {
            let Some(c) = r.key.as_deref() else { continue };
            let prefix = format!("{c}|");
            let mut own: Vec<&BreakdownRow> = sources
                .iter()
                .filter(|s| {
                    s.key
                        .as_deref()
                        .and_then(|k| k.strip_prefix(&prefix))
                        .is_some_and(|h| !h.is_empty())
                })
                .collect();
            own.sort_by(|a, b| by.value(b).total_cmp(&by.value(a)));
            own.truncate(SHOWN);
            bar.sources = bar_rows(Dimension::ChannelSource, &own, by);
        }
    }
    let map = if spec.tabs[pick.tab].0 == "map" {
        let values: Vec<(String, f64, String)> = rows
            .iter()
            .filter_map(|r| {
                let code = r.key.clone()?;
                let name = icons::country(&code);
                Some((code, by.value(r), format!("{name}: {}", by.format(r))))
            })
            .collect();
        world::map(&values, by == By::Visitors)
    } else {
        String::new()
    };
    let (foot, foot_href, foot_link) = match foot {
        Some(f) => (f, String::new(), ""),
        None if pick.all => (
            format!("All {total}"),
            anchor(link(base, params, &[("all", "")])),
            "Show fewer",
        ),
        None if total > SHOWN && map.is_empty() => (
            format!("Top {SHOWN} of {}", group(total as u64)),
            anchor(link(base, params, &[("all", spec.key)])),
            "View all",
        ),
        None => (String::new(), String::new(), ""),
    };
    TabCard {
        key: spec.key,
        title: spec.title,
        tabs: spec
            .tabs
            .iter()
            .enumerate()
            .map(|(i, (k, name, _))| Opt {
                label: name.to_string(),
                href: anchor(link(
                    base,
                    params,
                    &[
                        (spec.key, if i == spec.default { "" } else { k }),
                        ("all", ""),
                    ],
                )),
                on: i == pick.tab,
                count: String::new(),
            })
            .collect(),
        metrics: BYS
            .iter()
            .map(|(b, k, name)| Opt {
                label: name.to_string(),
                href: anchor(link(
                    base,
                    params,
                    &[(spec.by, if *b == By::Revenue { "" } else { k })],
                )),
                on: *b == by,
                count: String::new(),
            })
            .collect(),
        metric: BYS.iter().find(|b| b.0 == by).map_or("", |b| b.2),
        head: spec.tabs[pick.tab].1,
        rows: if map.is_empty() { bars } else { Vec::new() },
        map,
        empty: empty_text(dim),
        foot,
        foot_href,
        foot_link,
    }
}
