//! The dashboard's tabbed cards: Channels, Pages, Location, Devices and Timing.

use std::collections::HashMap;

use time::OffsetDateTime;

use super::{FilterHref, Opt, group, link, money, usd, visit_time};
use crate::channel::source_name;
use crate::crawlers::{BotHits, by_name};
use crate::icons;
use crate::stats::{BreakdownRow, DayPoint, Dimension};

/// Rows shown per card, and nested rows per opened row.
const SHOWN: usize = 6;
const KIDS: usize = 4;
/// Below this many visitors a row's per-visitor value is marked as a small sample.
const SMALL: u64 = 40;

/// What a tab lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Channel,
    Source,
    Campaign,
    Keyword,
    Landing,
    Before,
    Country,
    City,
    Language,
    Device,
    Browser,
    Os,
    Weekday,
    Hour,
}

impl Tab {
    /// The breakdown of the rows; None reads the pages viewed last before paying.
    pub fn dimension(self) -> Option<Dimension> {
        Some(match self {
            Tab::Channel => Dimension::Channel,
            Tab::Source => Dimension::ChannelSource,
            Tab::Campaign => Dimension::Campaign,
            Tab::Keyword => Dimension::Keyword,
            Tab::Landing => Dimension::LandingPage,
            Tab::Before => return None,
            Tab::Country => Dimension::Country,
            Tab::City => Dimension::CountryCity,
            Tab::Language => Dimension::Language,
            Tab::Device => Dimension::Device,
            Tab::Browser => Dimension::Browser,
            Tab::Os => Dimension::Os,
            Tab::Weekday => Dimension::Weekday,
            Tab::Hour => Dimension::WeekdayHour,
        })
    }

    /// The breakdown nested under each row, keyed "row|...".
    pub fn kids(self) -> Option<Dimension> {
        match self {
            Tab::Channel => Some(Dimension::ChannelSource),
            Tab::Landing => Some(Dimension::PageSource),
            Tab::Country => Some(Dimension::CountryCity),
            Tab::Device => Some(Dimension::DeviceOs),
            _ => None,
        }
    }

    /// Header of the first column.
    fn head(self) -> &'static str {
        match self {
            Tab::Channel => "Channel",
            Tab::Source => "Source",
            Tab::Campaign => "Campaign or tracked link",
            Tab::Keyword => "Search term",
            Tab::Landing => "First page of the visit",
            Tab::Before => "Last page before checkout",
            Tab::Country => "Country",
            Tab::City => "City",
            Tab::Language => "Browser language",
            Tab::Device => "Device",
            Tab::Browser => "Browser",
            Tab::Os => "Operating system",
            Tab::Weekday => "Weekday",
            Tab::Hour => "Hour",
        }
    }

    fn foot(self) -> &'static str {
        match self {
            Tab::Channel => "Click a channel to filter. Open it to see its sites.",
            Tab::Source => "Click a source to filter.",
            Tab::Campaign => "Tracked links and utm_campaign tags.",
            Tab::Keyword => "From utm_term on your links.",
            Tab::Landing => "Click a page to filter. Open it to see where its visitors came from.",
            Tab::Before => "Visitors here means everyone who viewed the page.",
            Tab::Country => "Click a country to filter. Open it to see its cities.",
            Tab::City => "Located from IP, approximate.",
            Tab::Language => "Click a language to filter. From the browser setting.",
            Tab::Device => "Open a device to see its systems.",
            Tab::Browser | Tab::Os => "From the user agent.",
            Tab::Weekday | Tab::Hour => "When visits began, in your site's time zone.",
        }
    }
}

pub struct Spec {
    /// Query param for the tab, also the card's anchor.
    pub key: &'static str,
    title: &'static str,
    label: &'static str,
    pub(super) tabs: &'static [(&'static str, &'static str, Tab)],
    empty: (&'static str, &'static str),
}

pub const SPECS: [Spec; 5] = [
    Spec {
        key: "channels",
        title: "Channels",
        label: "Break channels down by",
        tabs: &[
            ("channel", "Channel", Tab::Channel),
            ("source", "Source", Tab::Source),
            ("campaign", "Campaign", Tab::Campaign),
            ("keyword", "Keyword", Tab::Keyword),
        ],
        empty: (
            "No visits yet",
            "Channels, sources and campaigns show up after the first visit.",
        ),
    },
    Spec {
        key: "pages",
        title: "Pages",
        label: "Break pages down by",
        tabs: &[
            ("landing", "Landing page", Tab::Landing),
            ("before", "Before paying", Tab::Before),
        ],
        empty: (
            "No pages yet",
            "You will see which pages people land on and which ones sell.",
        ),
    },
    Spec {
        key: "location",
        title: "Location",
        label: "Break location down by",
        tabs: &[
            ("country", "Country", Tab::Country),
            ("city", "City", Tab::City),
            ("language", "Language", Tab::Language),
        ],
        empty: (
            "No locations yet",
            "Countries and cities show up after the first visit.",
        ),
    },
    Spec {
        key: "devices",
        title: "Devices",
        label: "Break devices down by",
        tabs: &[
            ("device", "Device", Tab::Device),
            ("browser", "Browser", Tab::Browser),
            ("os", "OS", Tab::Os),
        ],
        empty: (
            "No devices yet",
            "Devices, browsers and systems show up after the first visit.",
        ),
    },
    Spec {
        key: "timing",
        title: "Timing",
        label: "Break timing down by",
        tabs: &[
            ("weekday", "Weekday", Tab::Weekday),
            ("hour", "Hour", Tab::Hour),
        ],
        empty: (
            "No visits yet",
            "You will see which days and hours bring visitors and sales.",
        ),
    },
];

/// The tab one card shows, read from the query string.
pub struct Pick {
    spec: &'static Spec,
    tab: usize,
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
    pub(super) id: String,
    pub(super) icon: String,
    pub(super) name: String,
    pub(super) sub: String,
    pub(super) mono: bool,
    pub(super) kid: bool,
    pub(super) visitors: String,
    pub(super) revenue: String,
    pub(super) per_visitor: String,
    /// Average visit time, empty on rows under a channel.
    pub(super) visit: String,
    /// Share of payments refunded; empty when the table has no such column.
    pub(super) refund: String,
    pub(super) small: bool,
    /// Revenue in range from a visit before it, so no visitors to show.
    pub(super) earlier: bool,
    /// Per visitor against the site average, "1.4x"; empty without an average.
    pub(super) ratio: String,
    pub(super) tone: &'static str,
    /// Filters the dashboard to this row; empty when it cannot.
    pub(super) href: String,
    pub(super) kids: Vec<BarRow>,
    /// A tracked link's short URL, with a copy button.
    pub(super) copy: String,
}

/// One tab's rows; every tab is rendered so switching needs no reload.
pub struct Panel {
    pub(super) on: bool,
    /// Shows the average visit time column.
    pub(super) timed: bool,
    pub(super) head: &'static str,
    pub(super) rows: Vec<BarRow>,
    /// Shows the refunded column.
    pub(super) refunds: bool,
    pub(super) openable: bool,
    pub(super) foot: String,
    /// Where the "Tracked link" form posts; empty on tabs without one.
    pub(super) new_link: String,
    /// Title and text shown when the panel has no rows.
    pub(super) empty: (&'static str, &'static str),
    /// Draws the Hour tab's grid in place of rows.
    pub(super) grid: Option<Heat>,
}

pub struct TabCard {
    pub(super) key: &'static str,
    pub(super) title: &'static str,
    pub(super) label: &'static str,
    pub(super) tabs: Vec<Opt>,
    pub(super) panels: Vec<Panel>,
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

/// Site-wide numbers every row is measured against.
pub struct Base {
    pub visitors: u64,
    pub revenue: f64,
    /// Revenue per visitor; None until a payment matched.
    pub avg: Option<f64>,
    /// Rows under `SMALL` visitors get a small-sample note.
    pub warn_small: bool,
    /// Shows revenue as a share and per visitor as a ratio, for the public page.
    pub hide: bool,
}

/// One row before formatting: what it shows and which filter params it sets.
struct Item {
    icon: String,
    name: String,
    sub: String,
    mono: bool,
    filter: Vec<(&'static str, String)>,
    row: BreakdownRow,
}

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// "English (US)", "German" or "pt-BR" for a tag like "en-US"; unknown languages keep their tag.
pub fn language_name(tag: &str) -> String {
    let (lang, region) = tag.split_once('-').unwrap_or((tag, ""));
    let Some(name) = LANGUAGES.iter().find(|l| l.0 == lang).map(|l| l.1) else {
        return tag.to_string();
    };
    match region {
        "" => name.to_string(),
        "US" => format!("{name} (US)"),
        "GB" => format!("{name} (UK)"),
        r => format!("{name} ({})", icons::country(r)),
    }
}

/// The region's flag, else a language glyph.
pub fn language_icon(tag: &str) -> String {
    tag.split_once('-')
        .map(|(_, region)| icons::flag(region))
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| icons::glyph("language", 16, "var(--ink-2)"))
}

/// ISO 639-1 codes of common browser languages and their English names.
const LANGUAGES: &[(&str, &str)] = &[
    ("ar", "Arabic"),
    ("bg", "Bulgarian"),
    ("bn", "Bengali"),
    ("ca", "Catalan"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("de", "German"),
    ("el", "Greek"),
    ("en", "English"),
    ("es", "Spanish"),
    ("et", "Estonian"),
    ("fa", "Persian"),
    ("fi", "Finnish"),
    ("fil", "Filipino"),
    ("fr", "French"),
    ("he", "Hebrew"),
    ("hi", "Hindi"),
    ("hr", "Croatian"),
    ("hu", "Hungarian"),
    ("id", "Indonesian"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("lt", "Lithuanian"),
    ("lv", "Latvian"),
    ("ms", "Malay"),
    ("nb", "Norwegian"),
    ("nl", "Dutch"),
    ("no", "Norwegian"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("ro", "Romanian"),
    ("ru", "Russian"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("sr", "Serbian"),
    ("sv", "Swedish"),
    ("ta", "Tamil"),
    ("th", "Thai"),
    ("tr", "Turkish"),
    ("uk", "Ukrainian"),
    ("ur", "Urdu"),
    ("vi", "Vietnamese"),
    ("zh", "Chinese"),
];

fn none_label(tab: Tab) -> (&'static str, &'static str) {
    match tab {
        Tab::Campaign => ("No campaign", "Visits without UTM tags"),
        Tab::Keyword => ("No search term", "Visits without utm_term"),
        Tab::City => ("Unknown city", ""),
        _ => ("Unknown", ""),
    }
}

fn item(tab: Tab, r: &BreakdownRow) -> Item {
    let key = r.key.clone();
    let (none, none_sub) = none_label(tab);
    let v = |k: &str| key.clone().unwrap_or_else(|| k.to_string());
    let (icon, name, sub, mono, filter): (String, String, String, bool, Vec<(&str, String)>) =
        match tab {
            Tab::Channel => (
                icons::source(&r.label, 16),
                r.label.clone(),
                String::new(),
                false,
                vec![("channel", v(""))],
            ),
            Tab::Source => {
                let k = key.clone().unwrap_or_default();
                let (channel, host) = k.rsplit_once('|').unwrap_or((&k, ""));
                if host.is_empty() {
                    (
                        icons::source(channel, 16),
                        channel.to_string(),
                        "No referrer".into(),
                        false,
                        vec![("channel", channel.to_string())],
                    )
                } else {
                    (
                        icons::source(host, 16),
                        host.to_string(),
                        channel.to_string(),
                        false,
                        vec![("source", host.to_string())],
                    )
                }
            }
            Tab::Campaign | Tab::Keyword => {
                let param = if tab == Tab::Campaign {
                    "campaign"
                } else {
                    "keyword"
                };
                match &key {
                    Some(k) => (
                        icons::glyph(
                            if tab == Tab::Campaign {
                                "paid"
                            } else {
                                "search"
                            },
                            16,
                            "var(--ink-2)",
                        ),
                        k.clone(),
                        String::new(),
                        false,
                        vec![(param, k.clone())],
                    ),
                    None => (
                        icons::glyph("globe", 16, "var(--ink-2)"),
                        none.into(),
                        none_sub.into(),
                        false,
                        vec![(param, "(none)".into())],
                    ),
                }
            }
            Tab::Landing | Tab::Before => {
                let path = v("");
                let sub = if tab == Tab::Before {
                    format!("{} before paying", super::plural(r.payments, "payment"))
                } else {
                    String::new()
                };
                let param = if tab == Tab::Before { "viewed" } else { "page" };
                (
                    icons::glyph("page", 16, "var(--ink-2)"),
                    path.clone(),
                    sub,
                    true,
                    vec![(param, path)],
                )
            }
            Tab::Country => {
                let code = v("");
                (
                    icons::flag(&code),
                    icons::country(&code),
                    String::new(),
                    false,
                    vec![("country", code)],
                )
            }
            Tab::City => {
                let k = key.clone().unwrap_or_default();
                let (code, city) = k.split_once('|').unwrap_or(("", &k));
                (
                    icons::flag(code),
                    city.to_string(),
                    icons::country(code),
                    false,
                    vec![("city", city.to_string())],
                )
            }
            Tab::Language => {
                let tag = v("");
                (
                    language_icon(&tag),
                    language_name(&tag),
                    tag.clone(),
                    false,
                    vec![("language", tag)],
                )
            }
            Tab::Device => {
                let d = v("");
                (
                    icons::device(&d),
                    d.clone(),
                    String::new(),
                    false,
                    vec![("device", d)],
                )
            }
            Tab::Weekday => {
                let n: usize = v("").parse().unwrap_or(1);
                (
                    icons::glyph("calendar", 16, "var(--ink-2)"),
                    WEEKDAYS[n.clamp(1, 7) - 1].to_string(),
                    String::new(),
                    false,
                    Vec::new(),
                )
            }
            Tab::Hour => unreachable!("the Hour tab is a grid"),
            Tab::Browser | Tab::Os => {
                let n = v("");
                let param = if tab == Tab::Browser { "browser" } else { "os" };
                (
                    icons::ua(&n),
                    n.clone(),
                    String::new(),
                    false,
                    vec![(param, n)],
                )
            }
        };
    Item {
        icon,
        name,
        sub,
        mono,
        filter,
        row: r.clone(),
    }
}

/// Nested rows of `parent` from a "parent|rest" breakdown.
fn kid_items(tab: Tab, parent: &Item, kids: &[BreakdownRow]) -> Vec<Item> {
    let Some(pk) = parent.row.key.as_deref() else {
        return Vec::new();
    };
    let prefix = format!("{pk}|");
    let mut out: Vec<Item> = kids
        .iter()
        .filter(|r| !r.unattributed)
        .filter_map(|r| {
            let rest = r.key.as_deref()?.strip_prefix(&prefix)?;
            let (icon, name, filter) = match tab {
                Tab::Channel if !rest.is_empty() => (
                    icons::source(rest, 16),
                    rest.to_string(),
                    vec![("source", rest.to_string())],
                ),
                Tab::Landing => {
                    let (channel, host) = rest.rsplit_once('|').unwrap_or((rest, ""));
                    let mut f = parent.filter.clone();
                    if host.is_empty() {
                        f.push(("channel", channel.to_string()));
                        (icons::source(channel, 16), channel.to_string(), f)
                    } else {
                        f.push(("source", host.to_string()));
                        (icons::source(host, 16), host.to_string(), f)
                    }
                }
                Tab::Country if !rest.is_empty() => (
                    icons::glyph("pin", 16, "var(--ink-2)"),
                    rest.to_string(),
                    vec![("city", rest.to_string())],
                ),
                Tab::Device if !rest.is_empty() => (
                    icons::ua(rest),
                    rest.to_string(),
                    vec![("os", rest.to_string())],
                ),
                _ => return None,
            };
            Some(Item {
                icon,
                name,
                sub: String::new(),
                mono: false,
                filter,
                row: r.clone(),
            })
        })
        .collect();
    if tab == Tab::Landing {
        let mut merged: Vec<Item> = Vec::new();
        for i in out {
            match merged.iter_mut().find(|m| m.name == i.name) {
                Some(m) => {
                    m.row.visitors += i.row.visitors;
                    m.row.payments += i.row.payments;
                    m.row.revenue_usd += i.row.revenue_usd;
                }
                None => merged.push(i),
            }
        }
        out = merged;
    }
    out.sort_by(|a, b| {
        b.row
            .revenue_usd
            .total_cmp(&a.row.revenue_usd)
            .then(b.row.visitors.cmp(&a.row.visitors))
    });
    out.truncate(KIDS);
    out
}

fn bar(i: &Item, base: &Base, kid: bool, href: String, id: String) -> BarRow {
    let r = &i.row;
    let pv = if r.visitors > 0 {
        r.revenue_usd / r.visitors as f64
    } else {
        0.0
    };
    let x = base
        .avg
        .filter(|a| *a > 0.0 && r.visitors > 0 && r.revenue_usd > 0.0)
        .map(|a| pv / a);
    let (_, sr) = shares(r, base);
    BarRow {
        id,
        icon: i.icon.clone(),
        name: i.name.clone(),
        sub: i.sub.clone(),
        mono: i.mono,
        kid,
        visitors: group(r.visitors),
        revenue: if base.hide {
            format!("{:.0}%", sr * 100.0)
        } else {
            money(r.revenue_usd)
        },
        per_visitor: if base.hide {
            x.map_or_else(|| "–".into(), |x| format!("{x:.1}x"))
        } else if r.visitors == 0 {
            "–".into()
        } else if r.revenue_usd > 0.0 {
            usd(pv)
        } else {
            "$0".into()
        },
        visit: if kid {
            String::new()
        } else {
            r.avg_visit_secs.map_or_else(|| "–".into(), visit_time)
        },
        earlier: r.visitors == 0 && r.revenue_usd > 0.0,
        small: base.warn_small
            && !kid
            && r.visitors > 0
            && r.visitors < SMALL
            && r.revenue_usd > 0.0,
        ratio: x
            .filter(|_| !base.hide)
            .map(|x| format!("{x:.1}x"))
            .unwrap_or_default(),
        tone: x.map_or("", tone),
        refund: String::new(),
        href,
        kids: Vec::new(),
        copy: String::new(),
    }
}

/// Every weekday in order, empty ones included.
fn whole_week(mut items: Vec<Item>) -> Vec<Item> {
    if items.is_empty() {
        return items;
    }
    (1..=7)
        .map(|d| d.to_string())
        .map(
            |k| match items.iter().position(|i| i.row.key.as_deref() == Some(&k)) {
                Some(n) => items.swap_remove(n),
                None => item(
                    Tab::Weekday,
                    &BreakdownRow {
                        key: Some(k),
                        label: String::new(),
                        unattributed: false,
                        visitors: 0,
                        payments: 0,
                        customers: 0,
                        refunded: 0,
                        revenue_usd: 0.0,
                        revenue_per_visitor: None,
                        avg_visit_secs: None,
                    },
                ),
            },
        )
        .collect()
}

/// The Hour tab's grid: a row per weekday, a cell per hour.
pub struct Heat {
    /// Hour labels over the columns; empty for the hours left unlabeled.
    pub(super) labels: Vec<String>,
    pub(super) rows: Vec<HeatRow>,
    /// Runs of the busiest hours as (first hour, hours), outlined.
    pub(super) busiest: Vec<(usize, usize)>,
}

pub struct HeatRow {
    pub(super) day: &'static str,
    pub(super) cells: Vec<HeatCell>,
}

pub struct HeatCell {
    pub(super) class: String,
    pub(super) tip: String,
}

type Cells<T> = [[T; 24]; 7];

/// Visitors and revenue per weekday and hour, from "weekday|hour" rows.
fn cells_of(rows: &[BreakdownRow]) -> (Cells<u64>, Cells<f64>) {
    let (mut visitors, mut revenue) = ([[0; 24]; 7], [[0.0; 24]; 7]);
    for r in rows.iter().filter(|r| !r.unattributed) {
        let Some((d, h)) = r.key.as_deref().and_then(|k| k.split_once('|')) else {
            continue;
        };
        if let (Ok(d), Ok(h)) = (d.parse::<usize>(), h.parse::<usize>())
            && (1..=7).contains(&d)
            && h < 24
        {
            visitors[d - 1][h] += r.visitors;
            revenue[d - 1][h] += r.revenue_usd;
        }
    }
    (visitors, revenue)
}

/// The three hours with the most visitors over the week, in clock order.
fn busiest(visitors: &Cells<u64>) -> Vec<usize> {
    let total = |h: usize| visitors.iter().map(|d| d[h]).sum::<u64>();
    let mut hours: Vec<usize> = (0..24).filter(|h| total(*h) > 0).collect();
    hours.sort_by(|a, b| total(*b).cmp(&total(*a)).then(a.cmp(b)));
    let mut top: Vec<usize> = hours.into_iter().take(3).collect();
    top.sort();
    top
}

fn heat(visitors: &Cells<u64>, revenue: &Cells<f64>, base: &Base) -> Heat {
    let max = visitors.iter().flatten().copied().max().unwrap_or(1).max(1);
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for h in busiest(visitors) {
        match runs.last_mut() {
            Some(r) if r.0 + r.1 == h => r.1 += 1,
            _ => runs.push((h, 1)),
        }
    }
    Heat {
        labels: (0..24)
            .map(|h| {
                if h % 3 == 0 {
                    format!("{h:02}")
                } else {
                    String::new()
                }
            })
            .collect(),
        rows: WEEKDAYS
            .iter()
            .enumerate()
            .map(|(d, name)| HeatRow {
                day: &name[..3],
                cells: (0..24)
                    .map(|h| {
                        let (v, r) = (visitors[d][h], revenue[d][h]);
                        let class = format!("c s{}", (v * 4).div_ceil(max));
                        let when = format!("{} {h:02}:00", &name[..3]);
                        let tip = if v == 0 {
                            format!("{when}: no visitors")
                        } else if base.hide {
                            format!("{when}: {}", super::plural(v, "visitor"))
                        } else {
                            format!(
                                "{when}: {}, {} revenue, {} per visitor",
                                super::plural(v, "visitor"),
                                money(r),
                                if r > 0.0 {
                                    usd(r / v as f64)
                                } else {
                                    "$0".into()
                                }
                            )
                        };
                        HeatCell { class, tip }
                    })
                    .collect(),
            })
            .collect(),
        busiest: runs,
    }
}

/// "Best time to post: 09:00, 14:00 and 15:00", from the three hours with the most visitors.
fn best_hours(rows: &[BreakdownRow]) -> String {
    let hours: Vec<String> = busiest(&cells_of(rows).0)
        .iter()
        .map(|h| format!("{h:02}:00"))
        .collect();
    match hours.as_slice() {
        [] => String::new(),
        [only] => format!("Best time to post: {only}"),
        [head @ .., last] => format!("Best time to post: {} and {last}", head.join(", ")),
    }
}

/// Refunded share of a row's payments, "–" with none refunded.
fn refund_rate(r: &BreakdownRow) -> String {
    if r.refunded == 0 || r.payments == 0 {
        return "–".into();
    }
    format!("{:.1}%", r.refunded as f64 / r.payments as f64 * 100.0)
}

fn shares(r: &BreakdownRow, base: &Base) -> (f64, f64) {
    let share = |n: f64, t: f64| if t > 0.0 { n / t } else { 0.0 };
    (
        share(r.visitors as f64, base.visitors as f64),
        share(r.revenue_usd.max(0.0), base.revenue),
    )
}

#[allow(clippy::too_many_arguments)]
fn panel(
    key: &str,
    tab: Tab,
    rows: &[BreakdownRow],
    kids: &[BreakdownRow],
    base: &Base,
    on: bool,
    filter_href: &FilterHref,
    empty: (&'static str, &'static str),
) -> Panel {
    if tab == Tab::Hour {
        let (visitors, revenue) = cells_of(rows);
        let any = visitors.iter().flatten().any(|n| *n > 0);
        return Panel {
            on,
            timed: false,
            head: tab.head(),
            rows: Vec::new(),
            refunds: false,
            openable: false,
            foot: tab.foot().into(),
            new_link: String::new(),
            empty,
            grid: any.then(|| heat(&visitors, &revenue, base)),
        };
    }
    let mut items: Vec<Item> = rows
        .iter()
        .filter(|r| {
            !r.unattributed && (r.key.is_some() || matches!(tab, Tab::Campaign | Tab::Keyword))
        })
        .filter(|r| r.visitors > 0 || r.revenue_usd > 0.0)
        .map(|r| item(tab, r))
        .collect();
    let total = items.len();
    if tab == Tab::Weekday {
        items = whole_week(items);
    } else {
        // The "none" row of campaigns and keywords goes last.
        items.sort_by_key(|i| i.row.key.is_none());
        items.truncate(SHOWN);
    }
    let refunds = !base.hide
        && matches!(
            tab,
            Tab::Channel | Tab::Source | Tab::Campaign | Tab::Keyword
        )
        && items.iter().any(|i| i.row.payments > 0);
    let nested: Vec<Vec<Item>> = items
        .iter()
        .map(|i| {
            if tab.kids().is_some() {
                kid_items(tab, i, kids)
            } else {
                Vec::new()
            }
        })
        .collect();
    let rows: Vec<BarRow> = items
        .iter()
        .zip(nested)
        .enumerate()
        .map(|(n, (i, ks))| {
            let mut row = bar(i, base, false, filter_href(&i.filter), format!("{key}-{n}"));
            if refunds {
                row.refund = refund_rate(&i.row);
            }
            if row.sub.is_empty() && !ks.is_empty() {
                row.sub = match tab {
                    Tab::Country if ks.len() > 1 => format!("{} cities", ks.len()),
                    _ => ks
                        .iter()
                        .take(3)
                        .map(|k| match tab {
                            Tab::Channel | Tab::Landing => source_name(&k.name),
                            _ => k.name.clone(),
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                };
            }
            if ks.len() > 1 || (ks.len() == 1 && ks[0].name != i.name) {
                row.kids = ks
                    .iter()
                    .map(|k| {
                        let mut kid = bar(k, base, true, filter_href(&k.filter), String::new());
                        if refunds {
                            kid.refund = refund_rate(&k.row);
                        }
                        kid
                    })
                    .collect();
            }
            row
        })
        .collect();
    let foot = match tab {
        Tab::Source | Tab::City | Tab::Language if total > SHOWN => {
            format!("Top {SHOWN} of {total}. {}", tab.foot())
        }
        _ if rows.is_empty() => String::new(),
        _ if base.visitors < 100 => "Early numbers. They settle after about 100 visitors.".into(),
        _ => tab.foot().into(),
    };
    Panel {
        on,
        timed: tab == Tab::Channel,
        head: tab.head(),
        openable: rows.iter().any(|r| !r.kids.is_empty()),
        refunds,
        rows,
        foot,
        new_link: String::new(),
        empty,
        grid: None,
    }
}

/// Builds one card with a panel per tab; `data` holds each tab's rows and nested rows, in tab order.
pub fn card(
    pick: &Pick,
    data: &[(Vec<BreakdownRow>, Vec<BreakdownRow>)],
    base: &Base,
    tabs_base: &str,
    params: &[(&str, &str)],
    filter_href: &FilterHref,
) -> TabCard {
    let spec = pick.spec;
    let mut card = TabCard {
        key: spec.key,
        title: spec.title,
        label: spec.label,
        tabs: spec
            .tabs
            .iter()
            .enumerate()
            .map(|(i, (k, name, _))| Opt {
                label: name.to_string(),
                href: format!(
                    "{}#{}",
                    link(
                        tabs_base,
                        params,
                        &[(spec.key, if i == 0 { "" } else { k })]
                    ),
                    spec.key
                ),
                on: i == pick.tab,
                count: String::new(),
            })
            .collect(),
        panels: spec
            .tabs
            .iter()
            .zip(data)
            .enumerate()
            .map(|(i, ((k, _, tab), (rows, kids)))| {
                let empty = match tab {
                    Tab::Country if base.visitors > 0 => (
                        "No countries for these visits",
                        "Countries come from your proxy's country header, such as cf-ipcountry from Cloudflare, or the browser's time zone.",
                    ),
                    Tab::City if base.visitors > 0 => (
                        "No cities for these visits",
                        "Cities come from your proxy's location headers. On Cloudflare, turn on the Add visitor location headers Managed Transform.",
                    ),
                    Tab::Before => (
                        "No payments matched to visits yet",
                        "Pages people view before paying will show here.",
                    ),
                    _ => spec.empty,
                };
                panel(
                    &format!("{}-{k}", spec.key),
                    *tab,
                    rows,
                    kids,
                    base,
                    i == pick.tab,
                    filter_href,
                    empty,
                )
            })
            .collect(),
    };
    let hours = spec.tabs.iter().position(|t| t.2 == Tab::Hour);
    if let Some((rows, _)) = hours.and_then(|i| data.get(i)) {
        let best = best_hours(rows);
        if !best.is_empty() {
            for p in &mut card.panels {
                p.foot = format!("{best} (site time)");
            }
        }
    }
    card
}

/// Shows tracked links by name in the Campaign tab, adds the new-link form and notes a link just made.
pub fn with_links(
    cards: &mut [TabCard],
    links: &[crate::links::SiteLink],
    base_url: &str,
    action: &str,
    new_link: Option<&str>,
    error: Option<&str>,
) {
    let Some(card) = cards.iter_mut().find(|c| c.key == "channels") else {
        return;
    };
    let Some(panel) = SPECS[0]
        .tabs
        .iter()
        .position(|t| t.2 == Tab::Campaign)
        .and_then(|i| card.panels.get_mut(i))
    else {
        return;
    };
    let short = |slug: &str| format!("{base_url}/l/{slug}");
    for row in &mut panel.rows {
        if let Some(l) = links.iter().find(|l| l.slug == row.name) {
            row.icon = icons::source(&l.source, 16);
            let place = crate::links::PLACES
                .iter()
                .find(|p| p.2 == l.source)
                .map_or_else(|| source_name(&l.source), |p| p.1.to_string());
            row.sub = format!("{place} · {} clicks", group(l.clicks.max(0) as u64));
            row.name = l.name.clone();
            row.copy = short(&l.slug);
        }
    }
    panel.new_link = action.to_string();
    panel.foot = match (new_link, error) {
        (_, Some("destination")) => {
            "That page is not on this site. Use a path like /pricing.".into()
        }
        (_, Some(_)) => "Pick where you will share the link.".into(),
        (Some(slug), _) if links.iter().any(|l| l.slug == slug) => {
            format!("Link ready: {}", short(slug))
        }
        _ => panel.foot.clone(),
    };
}

/// Every tab of every card, for loading their rows.
pub fn all_tabs() -> Vec<Tab> {
    SPECS
        .iter()
        .flat_map(|s| s.tabs.iter().map(|t| t.2))
        .collect()
}

/// The AI card: share of visitors against share of revenue, and one row per AI company.
pub struct AiCard {
    pub(super) visitors_share: f64,
    pub(super) revenue_share: f64,
    pub(super) rows: Vec<AiRow>,
    /// "AI revenue +40% vs previous period", empty without a trend.
    pub(super) trend: String,
    /// SVG path of AI's share of revenue per point, in a 100 by 24 box; empty without revenue.
    pub(super) spark: String,
    /// Greyed logos for the empty state.
    pub(super) logos: Vec<String>,
    pub(super) docs: &'static str,
    /// Some AI crawler read a page in the range.
    pub(super) crawled: bool,
    /// The Answers tab, signed in only.
    pub(super) answers: Option<crate::dashboard::answers::AnswersTab>,
}

/// One AI company: visitors and revenue from its assistants, pages its crawlers read.
pub struct AiRow {
    pub(super) bar: BarRow,
    pub(super) pages: String,
    /// Each crawler's kind, hits, pages, most read page and last visit.
    pub(super) pages_title: String,
}

/// The company behind an assistant source and the name its logo goes by.
pub(super) fn ai_company(source: &str) -> (String, String) {
    let s = source.to_ascii_lowercase();
    let known: [(&[&str], &str, &str); 6] = [
        (&["openai", "chatgpt"], "OpenAI", "openai"),
        (&["claude", "anthropic"], "Anthropic", "anthropic"),
        (&["perplexity"], "Perplexity", "perplexity"),
        (&["gemini", "google"], "Google", "google"),
        (&["copilot", "microsoft"], "Microsoft", "copilot"),
        (&["deepseek"], "DeepSeek", "deepseek"),
    ];
    known
        .iter()
        .find(|(keys, _, _)| keys.iter().any(|k| s.contains(k)))
        .map(|(_, c, l)| (c.to_string(), l.to_string()))
        .unwrap_or_else(|| (source_name(source), source.to_string()))
}

/// One AI company on the AI to cash card.
pub struct CashRow {
    pub(super) icon: String,
    pub(super) name: String,
    pub(super) pages: String,
    /// "7 of 20" answers that name the site, or a dash.
    pub(super) named: String,
    pub(super) visitors: String,
    pub(super) revenue: String,
}

/// Per AI company: pages its crawlers read, answers naming the site, visitors it sent and revenue.
pub fn ai_cash(ai: &AiCard) -> Vec<CashRow> {
    let answers = ai.answers.as_ref().map_or(&[][..], |a| &a.rows);
    let mut rows: Vec<CashRow> = ai
        .rows
        .iter()
        .map(|r| CashRow {
            icon: r.bar.icon.clone(),
            name: r.bar.name.clone(),
            pages: r.pages.clone(),
            named: answers
                .iter()
                .find(|a| a.company == r.bar.name)
                .map_or_else(|| "\u{2013}".into(), |a| a.mentioned.clone()),
            visitors: r.bar.visitors.clone(),
            revenue: r.bar.revenue.clone(),
        })
        .collect();
    for a in answers {
        if a.mentioned != "\u{2013}" && !rows.iter().any(|r| r.name == a.company) {
            rows.push(CashRow {
                icon: icons::source(&ai_company(a.company).1, 16),
                name: a.company.into(),
                pages: "0".into(),
                named: a.mentioned.clone(),
                visitors: a.visitors.clone(),
                revenue: a.revenue.clone(),
            });
        }
    }
    rows.into_iter().take(SHOWN).collect()
}

fn ai_revenue(days: &[DayPoint]) -> f64 {
    days.iter()
        .flat_map(|d| &d.by_channel)
        .filter(|(c, _)| c.as_deref() == Some("AI"))
        .map(|(_, v)| v)
        .sum()
}

/// A company's assistant hosts and crawlers, summed.
struct Company {
    name: String,
    logo: String,
    row: BreakdownRow,
    hosts: Vec<(String, u64)>,
    bots: Vec<&'static str>,
    pages: u64,
    detail: Vec<String>,
}

/// Builds the AI card from channel-source rows, crawler hits and the daily series.
#[allow(clippy::too_many_arguments)]
pub fn ai_card(
    sources: &[BreakdownRow],
    channels: &[BreakdownRow],
    bots: &[BotHits],
    base: &Base,
    days: &[DayPoint],
    prev: Option<&[DayPoint]>,
    filter_href: &FilterHref,
    now: OffsetDateTime,
) -> AiCard {
    let mut companies: Vec<Company> = Vec::new();
    let mut entry = |name: String, logo: String| -> usize {
        if let Some(i) = companies.iter().position(|c| c.name == name) {
            return i;
        }
        companies.push(Company {
            row: BreakdownRow {
                key: None,
                label: name.clone(),
                unattributed: false,
                visitors: 0,
                payments: 0,
                customers: 0,
                refunded: 0,
                revenue_usd: 0.0,
                revenue_per_visitor: None,
                avg_visit_secs: None,
            },
            name,
            logo,
            hosts: Vec::new(),
            bots: Vec::new(),
            pages: 0,
            detail: Vec::new(),
        });
        companies.len() - 1
    };
    let mut placed = Vec::new();
    for r in sources
        .iter()
        .filter(|r| !r.unattributed && (r.visitors > 0 || r.revenue_usd > 0.0))
    {
        if let Some(host) = r.key.as_deref().and_then(|k| k.strip_prefix("AI|"))
            && !host.is_empty()
        {
            let (name, logo) = ai_company(host);
            placed.push((entry(name, logo), r, host));
        }
    }
    let mut found = Vec::new();
    for b in bots {
        if let Some(c) = by_name(&b.bot) {
            let i = entry(c.company.into(), c.logo.into());
            let seen = OffsetDateTime::from_unix_timestamp(b.last_seen)
                .map(|t| super::ago(t, now))
                .unwrap_or_default();
            let detail = format!(
                "{} ({}): {} on {}, most read {}, last {seen}",
                c.name,
                c.kind.label(),
                super::plural(b.hits.max(0) as u64, "hit"),
                super::plural(b.pages.max(0) as u64, "page"),
                b.top_page
            );
            found.push((i, c.name, b.hits.max(0) as u64, detail));
        }
    }
    for (i, r, host) in placed {
        let c = &mut companies[i];
        c.row.visitors += r.visitors;
        c.row.payments += r.payments;
        c.row.customers += r.customers;
        c.row.revenue_usd += r.revenue_usd;
        c.hosts.push((host.to_string(), r.visitors));
    }
    for (i, name, hits, detail) in found {
        companies[i].pages += hits;
        companies[i].bots.push(name);
        companies[i].detail.push(detail);
    }
    companies.sort_by(|a, b| {
        (b.row.visitors, b.row.revenue_usd, b.pages)
            .partial_cmp(&(a.row.visitors, a.row.revenue_usd, a.pages))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let items: Vec<(Item, u64, String)> = companies
        .into_iter()
        .take(SHOWN)
        .map(|c| {
            let top = c.hosts.iter().max_by_key(|h| h.1).map(|h| h.0.clone());
            let sub = if c.hosts.is_empty() {
                c.bots.join(", ")
            } else {
                let hosts: Vec<&str> = c.hosts.iter().map(|h| h.0.as_str()).collect();
                hosts.join(", ")
            };
            let item = Item {
                icon: icons::source(&c.logo, 16),
                name: c.name,
                sub,
                mono: false,
                filter: top.map(|h| vec![("source", h)]).unwrap_or_default(),
                row: c.row,
            };
            (item, c.pages, c.detail.join("; "))
        })
        .collect();
    // The AI channel row counts each visitor once, even one who came from two assistants.
    let (v, r) = channels
        .iter()
        .find(|c| c.key.as_deref() == Some("AI"))
        .map_or((0, 0.0), |c| (c.visitors, c.revenue_usd));
    let share = |n: f64, t: f64| if t > 0.0 { n / t * 100.0 } else { 0.0 };
    let rows: Vec<AiRow> = items
        .iter()
        .map(|(i, pages, title)| {
            let href = if i.filter.is_empty() {
                String::new()
            } else {
                filter_href(&i.filter)
            };
            AiRow {
                bar: bar(i, base, false, href, String::new()),
                pages: group(*pages),
                pages_title: title.clone(),
            }
        })
        .collect();
    let now = ai_revenue(days);
    let trend = match prev.map(ai_revenue) {
        Some(p) if p > 0.0 && now > 0.0 && ((now - p) / p).abs() >= 0.1 => {
            format!(
                "AI revenue {:+.0}% vs previous period",
                (now - p) / p * 100.0
            )
        }
        _ => String::new(),
    };
    let points: Vec<f64> = days
        .iter()
        .map(|d| {
            let ai: f64 = d
                .by_channel
                .iter()
                .filter(|(c, _)| c.as_deref() == Some("AI"))
                .map(|(_, v)| v)
                .sum();
            share(ai, d.revenue_usd) / 100.0
        })
        .collect();
    let spark = if now > 0.0 && points.len() > 1 {
        let n = (points.len() - 1) as f64;
        points
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    "{}{:.1},{:.1}",
                    if i == 0 { "M" } else { "L" },
                    i as f64 / n * 100.0,
                    22.0 - s * 20.0
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        String::new()
    };
    AiCard {
        visitors_share: share(v as f64, base.visitors as f64),
        revenue_share: share(r, base.revenue),
        rows,
        trend,
        spark,
        logos: ["openai", "anthropic", "perplexity", "google", "copilot"]
            .iter()
            .map(|l| icons::source(l, 18))
            .collect(),
        docs: "https://docs.virling.com/track/ai-crawlers.html",
        crawled: !bots.is_empty(),
        answers: None,
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
            refunded: 0,
            revenue_usd: revenue,
            revenue_per_visitor: Some(revenue / visitors as f64),
            avg_visit_secs: None,
        }
    }

    #[test]
    fn refund_rate_is_a_dash_without_refunds() {
        let mut r = row("Social", 10, 50.0);
        assert_eq!(refund_rate(&r), "–");
        r.payments = 8;
        r.refunded = 1;
        assert_eq!(refund_rate(&r), "12.5%");
        r.payments = 0;
        assert_eq!(refund_rate(&r), "–");
    }

    #[test]
    fn languages_read_as_names_with_flags() {
        assert_eq!(language_name("en-US"), "English (US)");
        assert_eq!(language_name("en-GB"), "English (UK)");
        assert_eq!(language_name("de-AT"), "German (Austria)");
        assert_eq!(language_name("de"), "German");
        assert_eq!(language_name("xx-YY"), "xx-YY");
        let us = item(Tab::Language, &row("en-US", 10, 5.0));
        assert!(us.icon.contains("/assets/flags/us.svg"));
        assert_eq!(
            (us.sub.as_str(), us.filter[0].1.as_str()),
            ("en-US", "en-US")
        );
        assert!(
            item(Tab::Language, &row("de", 10, 5.0))
                .icon
                .contains("M4 5h9")
        );
    }

    fn base() -> Base {
        Base {
            visitors: 400,
            revenue: 400.0,
            avg: Some(1.0),
            warn_small: true,
            hide: false,
        }
    }

    fn href(f: &[(&'static str, String)]) -> String {
        f.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    #[test]
    fn bars_scale_to_site_shares_and_badge_against_the_average() {
        let q = HashMap::new();
        let picks = picks(&q);
        let rows = [row("Social", 100, 300.0), row("AI", 300, 100.0)];
        let kids = [
            row("AI|chatgpt.com", 200, 80.0),
            row("AI|claude.ai", 100, 20.0),
        ];
        let data = vec![(rows.to_vec(), kids.to_vec()); 4];
        let c = card(&picks[0], &data, &base(), "/app/x.com", &[], &href);
        let r = &c.panels[0].rows;
        assert_eq!(r[0].name, "Social", "most revenue first");
        assert_eq!((r[0].ratio.as_str(), r[0].tone), ("3.0x", "up"));
        assert_eq!((r[1].ratio.as_str(), r[1].tone), ("0.3x", "down"));
        assert_eq!(r[1].href, "channel=AI");
        assert_eq!(r[1].sub, "ChatGPT, Claude");
        assert_eq!(r[1].kids[0].href, "source=chatgpt.com");
        assert!(c.panels[0].openable && r[0].kids.is_empty());
        assert!(c.panels[0].on && !c.panels[1].on);
    }

    #[test]
    fn ai_to_cash_joins_crawls_answers_visits_and_revenue() {
        let sources = [row("AI|chatgpt.com", 15, 80.0)];
        let now = OffsetDateTime::from_unix_timestamp(3600).unwrap();
        let mut ai = ai_card(&sources, &[], &[], &base(), &[], None, &href, now);
        assert!(ai_cash(&ai).iter().all(|r| r.named == "\u{2013}"));
        let answer =
            |company: &'static str, mentioned: &str| crate::dashboard::answers::AnswerRow {
                icon: String::new(),
                name: "x",
                company,
                sub: String::new(),
                href: String::new(),
                reads: "0".into(),
                reads_title: String::new(),
                mentioned: mentioned.into(),
                was: String::new(),
                tone: "",
                visitors: "0".into(),
                revenue: "$0".into(),
            };
        ai.answers = Some(crate::dashboard::answers::AnswersTab {
            rows: vec![
                answer("OpenAI", "7 of 20"),
                answer("Google", "3 of 20"),
                answer("Anthropic", "\u{2013}"),
            ],
            foot: "",
            link: "",
            note: "",
            settings: String::new(),
            on: false,
            tabs: (String::new(), String::new()),
        });
        let rows = ai_cash(&ai);
        let seen: Vec<_> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.named.as_str(), r.visitors.as_str()))
            .collect();
        assert_eq!(
            seen,
            [("OpenAI", "7 of 20", "15"), ("Google", "3 of 20", "0")]
        );
    }

    #[test]
    fn ai_card_has_one_row_per_company() {
        let sources = [
            row("AI|chatgpt.com", 10, 50.0),
            row("AI|chat.openai.com", 5, 0.0),
            row("AI|claude.ai", 3, 30.0),
        ];
        let hit = |bot: &str, hits| BotHits {
            bot: bot.into(),
            hits,
            pages: 1,
            top_page: "/".into(),
            last_seen: 0,
        };
        let bots = [
            hit("GPTBot", 100),
            hit("OAI-SearchBot", 20),
            hit("ClaudeBot", 20),
            hit("PerplexityBot", 7),
        ];
        let channels = [row("AI", 18, 80.0)];
        let now = OffsetDateTime::from_unix_timestamp(3600).unwrap();
        let c = ai_card(&sources, &channels, &bots, &base(), &[], None, &href, now);
        let names: Vec<_> = c.rows.iter().map(|r| r.bar.name.as_str()).collect();
        assert_eq!(names, ["OpenAI", "Anthropic", "Perplexity"]);
        let openai = &c.rows[0];
        assert_eq!(
            (openai.bar.visitors.as_str(), openai.pages.as_str()),
            ("15", "120")
        );
        assert_eq!(openai.bar.sub, "chatgpt.com, chat.openai.com");
        assert_eq!(openai.bar.href, "source=chatgpt.com");
        let perplexity = &c.rows[2];
        assert_eq!(
            (
                perplexity.bar.visitors.as_str(),
                perplexity.bar.revenue.as_str(),
                perplexity.bar.per_visitor.as_str(),
                perplexity.pages.as_str(),
            ),
            ("0", "$0", "–", "7")
        );
        assert!(perplexity.bar.href.is_empty() && perplexity.bar.sub == "PerplexityBot");
        assert!(
            c.rows.iter().all(|r| r.bar.icon.contains("<svg")),
            "real logos"
        );
        assert!(
            perplexity
                .pages_title
                .starts_with("PerplexityBot (Search): 7 hits on 1 page, most read /")
        );
        let empty = ai_card(&[], &[], &[], &base(), &[], None, &href, now);
        assert!(empty.rows.is_empty() && empty.logos.len() == 5);
    }

    #[test]
    fn hidden_amounts_show_shares_and_ratios() {
        let picks = picks(&HashMap::new());
        let data = vec![(vec![row("Social", 100, 300.0)], Vec::new()); 4];
        let hidden = Base {
            hide: true,
            ..base()
        };
        let r = &card(&picks[0], &data, &hidden, "/x", &[], &href).panels[0].rows[0];
        assert_eq!(
            (r.revenue.as_str(), r.per_visitor.as_str(), r.ratio.as_str()),
            ("75%", "3.0x", "")
        );
    }

    #[test]
    fn tabs_keep_the_other_cards_and_jump_back_to_the_card() {
        let q: HashMap<String, String> = [("pages".to_string(), "before".to_string())].into();
        let picks = picks(&q);
        assert_eq!(picks[1].tab, 1);
        let params = params(&picks);
        assert_eq!(params, [("pages", "before")]);
        let c = card(&picks[0], &[], &base(), "/app/x.com", &params, &href);
        assert_eq!(
            c.tabs[1].href,
            "/app/x.com?pages=before&channels=source#channels"
        );
        assert_eq!(c.tabs[0].href, "/app/x.com?pages=before#channels");
        assert!(c.panels.is_empty());
    }

    #[test]
    fn location_tabs_explain_missing_headers_once_visits_exist() {
        let picks = picks(&HashMap::new());
        let loc = picks.iter().find(|p| p.spec.key == "location").unwrap();
        let data = vec![(Vec::new(), Vec::new()); 3];
        let c = card(loc, &data, &base(), "/x", &[], &href);
        assert_eq!(c.panels[1].empty.0, "No cities for these visits");
        assert_eq!(c.panels[2].empty.0, "No locations yet");
        let none = Base {
            visitors: 0,
            ..base()
        };
        let c = card(loc, &data, &none, "/x", &[], &href);
        assert_eq!(c.panels[1].empty.0, "No locations yet");
    }

    #[test]
    fn the_best_time_is_the_three_busiest_hours() {
        let rows = [
            row("1|09", 5, 0.0),
            row("2|09", 4, 0.0),
            row("3|14", 7, 0.0),
            row("3|15", 2, 0.0),
            row("4|20", 3, 0.0),
            row("5|03", 1, 0.0),
        ];
        assert_eq!(
            best_hours(&rows),
            "Best time to post: 09:00, 14:00 and 20:00"
        );
        assert_eq!(best_hours(&[]), "");
        let (visitors, _) = cells_of(&rows);
        assert_eq!((visitors[0][9], visitors[2][14]), (5, 7));
    }
}
