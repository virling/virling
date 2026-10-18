//! The date range every app page shares: presets, a custom span and compare, kept in the URL.

use time::macros::format_description;
use time::{Date, Duration};

use super::{Opt, day, encode, href, link, resolve_range};
use crate::stats::Period;

pub(crate) const PRESETS: [(&str, &str); 8] = [
    ("today", "Today"),
    ("yesterday", "Yesterday"),
    ("7d", "Last 7 days"),
    ("30d", "Last 30 days"),
    ("90d", "Last 90 days"),
    ("month", "This month"),
    ("12m", "Last 12 months"),
    ("all", "Since tracking started"),
];

/// Params that make up the range; every other param of a page is kept as is.
pub(crate) const KEYS: [&str; 4] = ["range", "from", "to", "compare"];

/// "Oct 16", "Oct 9 to 15", "Sep 16 to Oct 15", "Oct 2025 on".
pub(crate) fn span(from: Date, to: Date) -> String {
    if from == to {
        day(from)
    } else if from.year() != to.year() {
        from.format(format_description!("[month repr:short] [year] on"))
            .unwrap_or_default()
    } else if from.month() == to.month() {
        format!("{} to {}", day(from), to.day())
    } else {
        format!("{} to {}", day(from), day(to))
    }
}

/// The preset's local dates, or None when it does not apply yet.
pub(crate) fn preset(key: &str, today: Date, first: Option<Date>) -> Option<(Date, Date)> {
    let back = |n: i64| (today - Duration::days(n - 1), today);
    Some(match key {
        "today" => (today, today),
        "yesterday" => (today - Duration::days(1), today - Duration::days(1)),
        "7d" => back(7),
        "30d" => back(30),
        "90d" => back(90),
        "month" => (today.replace_day(1).ok()?, today),
        "12m" => back(365),
        "all" => (first?, today),
        _ => return None,
    })
}

fn ymd(s: Option<&str>) -> Option<Date> {
    Date::parse(s?, format_description!("[year]-[month]-[day]")).ok()
}

/// The range a page shows, read from ?range=, ?from=&to= and ?compare=.
pub(crate) struct Picked {
    /// A preset key, or "custom".
    pub key: &'static str,
    /// The preset a page shows without ?range=.
    pub default: &'static str,
    pub label: String,
    pub from: Date,
    pub to: Date,
    pub compare: bool,
}

/// The range control in a page's top bar.
pub struct RangeMenu {
    pub label: String,
    /// Shows "vs previous" on the button.
    pub vs: bool,
    pub compare: bool,
    pub compare_href: String,
    pub ranges: Vec<Opt>,
    pub custom: bool,
    pub custom_from: String,
    pub custom_to: String,
    pub today: String,
    pub base: String,
    /// Params a custom range keeps, as raw values.
    pub hidden: Vec<(&'static str, String)>,
}

impl Picked {
    pub(crate) fn read(
        today: Date,
        first: Option<Date>,
        range: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        compare: Option<&str>,
    ) -> Self {
        let default = resolve_range(today, first, None).3;
        let custom = match (ymd(from), ymd(to)) {
            (Some(f), Some(t)) if f <= t && f <= today => Some((f, t.min(today))),
            _ => None,
        };
        let key = range
            .and_then(|k| PRESETS.iter().map(|p| p.0).find(|p| *p == k))
            .filter(|k| preset(k, today, first).is_some())
            .unwrap_or(default);
        let (key, (from, to)) = match custom {
            Some(c) => ("custom", c),
            None => match preset(key, today, first) {
                Some(r) => (key, r),
                None => ("30d", preset("30d", today, None).unwrap_or((today, today))),
            },
        };
        Picked {
            key,
            default,
            label: match PRESETS.iter().find(|p| p.0 == key) {
                Some(p) => p.1.to_string(),
                None => span(from, to),
            },
            from,
            to,
            compare: compare != Some("off"),
        }
    }

    pub(crate) fn period(&self, tz: &str) -> Period {
        Period {
            from: self.from,
            to: self.to,
            tz: tz.to_string(),
        }
    }

    /// The URL params that carry this range, as raw values.
    pub(crate) fn params(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if self.key == "custom" {
            out.push(("from", self.from.to_string()));
            out.push(("to", self.to.to_string()));
        } else if self.key != self.default {
            out.push(("range", self.key.to_string()));
        }
        if !self.compare {
            out.push(("compare", "off".into()));
        }
        out
    }

    /// "?range=7d" style query that keeps this range on another page, or "".
    pub(crate) fn query(&self) -> String {
        let params = self.params();
        let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        href("", &params)
    }

    /// The sidebar footer: "1,204 visitors in the last 7 days".
    pub(crate) fn usage(&self, visitors: u64) -> String {
        let when = match self.key {
            "today" | "yesterday" => self.key.to_string(),
            "all" => "since tracking started".into(),
            "month" => "this month".into(),
            "custom" => span(self.from, self.to),
            _ => self.label.to_lowercase().replace("last", "in the last"),
        };
        format!("{} {when}", super::plural(visitors, "visitor"))
    }

    /// The range control for a page at `base` whose other params are `raw`.
    pub(crate) fn menu(
        &self,
        today: Date,
        first: Option<Date>,
        base: &str,
        raw: &[(&'static str, String)],
    ) -> RangeMenu {
        let others: Vec<(&'static str, String)> = raw
            .iter()
            .filter(|(k, _)| !KEYS.contains(k))
            .cloned()
            .collect();
        let mut all = self.params();
        all.extend(others.iter().cloned());
        let enc: Vec<(&str, String)> = all.iter().map(|(k, v)| (*k, encode(v))).collect();
        let params: Vec<(&str, &str)> = enc.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let go = |set: &[(&str, &str)]| link(base, &params, set);
        let mut hidden = others;
        if !self.compare {
            hidden.push(("compare", "off".into()));
        }
        RangeMenu {
            label: self.label.clone(),
            vs: self.compare && self.key != "all",
            compare: self.compare,
            compare_href: go(&[("compare", if self.compare { "off" } else { "" })]),
            ranges: PRESETS
                .iter()
                .filter_map(|(k, label)| {
                    let (f, t) = preset(k, today, first)?;
                    Some(Opt {
                        label: label.to_string(),
                        href: go(&[
                            ("range", if *k == self.default { "" } else { k }),
                            ("from", ""),
                            ("to", ""),
                        ]),
                        on: *k == self.key,
                        count: span(f, t),
                    })
                })
                .collect(),
            custom: self.key == "custom",
            custom_from: self.from.to_string(),
            custom_to: self.to.to_string(),
            today: today.to_string(),
            base: base.to_string(),
            hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;

    #[test]
    fn spans_and_presets() {
        let t = date!(2026 - 10 - 16);
        assert_eq!(span(t, t), "Oct 16");
        assert_eq!(
            span(date!(2026 - 10 - 09), date!(2026 - 10 - 15)),
            "Oct 9 to 15"
        );
        assert_eq!(span(date!(2026 - 09 - 16), t), "Sep 16 to Oct 16");
        assert_eq!(preset("month", t, None), Some((date!(2026 - 10 - 01), t)));
        assert_eq!(
            preset("yesterday", t, None),
            Some((date!(2026 - 10 - 15), date!(2026 - 10 - 15)))
        );
        assert_eq!(preset("all", t, None), None);
    }

    #[test]
    fn a_range_round_trips_through_its_params() {
        let t = date!(2026 - 10 - 16);
        let first = Some(date!(2026 - 01 - 01));
        let p = Picked::read(t, first, Some("month"), None, None, Some("off"));
        assert_eq!((p.key, p.from, p.to), ("month", date!(2026 - 10 - 01), t));
        assert_eq!(p.query(), "?range=month&compare=off");
        assert_eq!(p.usage(1), "1 visitor this month");
        let c = Picked::read(t, first, None, Some("2026-10-02"), Some("2026-10-09"), None);
        assert_eq!(c.key, "custom");
        assert_eq!(c.query(), "?from=2026-10-02&to=2026-10-09");
        assert_eq!(c.usage(3), "3 visitors Oct 2 to 9");
        let d = Picked::read(t, first, Some("bogus"), None, None, None);
        assert_eq!((d.key, d.query()), ("30d", String::new()));
        assert_eq!(d.usage(2), "2 visitors in the last 30 days");
        let m = c.menu(
            t,
            first,
            "/app/x.com/visitors",
            &[("channel", "A B".into()), ("range", "7d".into())],
        );
        assert!(m.custom && m.hidden == vec![("channel", "A B".to_string())]);
        let week = m.ranges.iter().find(|r| r.label == "Last 7 days").unwrap();
        assert_eq!(week.href, "/app/x.com/visitors?channel=A+B&range=7d");
        assert_eq!(
            m.compare_href,
            "/app/x.com/visitors?from=2026-10-02&to=2026-10-09&channel=A+B&compare=off"
        );
    }
}
