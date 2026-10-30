//! Daily traffic imported from another tool's export, kept apart from tracked events so it never reaches attribution.

use std::collections::HashMap;
use std::path::Path;

use duckdb::Connection;
use time::Date;

use crate::channel;
use crate::stats::{BreakdownRow, DayPoint, Dimension, Overview, sort_rows};

/// One day of one row of a breakdown, or of the site total when `kind` is "total".
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub day: Date,
    pub kind: &'static str,
    pub label: Option<String>,
    pub visitors: u64,
    pub pageviews: u64,
    pub visits: u64,
}

/// The imported kind a breakdown merges; None for breakdowns imports do not cover.
pub fn kind(dimension: Dimension) -> Option<&'static str> {
    Some(match dimension {
        Dimension::Channel => "channel",
        Dimension::Referrer => "referrer",
        Dimension::Page => "page",
        Dimension::Country => "country",
        Dimension::Device => "device",
        _ => return None,
    })
}

/// Reads the CSV files of a Plausible export folder: visitors, sources, pages, locations and devices.
pub fn plausible(dir: &Path) -> anyhow::Result<Vec<Imported>> {
    let conn = Connection::open_in_memory()?;
    let mut out = Vec::new();
    let files = |table: &str| -> anyhow::Result<Option<String>> {
        let prefix = format!("imported_{table}_");
        let mut found = false;
        for entry in std::fs::read_dir(dir)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            found |= name.starts_with(&prefix) && name.ends_with(".csv");
        }
        let glob = dir.join(format!("{prefix}*.csv"));
        Ok(found.then(|| format!("'{}'", glob.to_string_lossy().replace('\'', "''"))))
    };
    let num = |c: &str| format!("coalesce(try_cast({c} AS BIGINT), 0)");
    let read = |table: &str, label: &str, extra: &str| -> anyhow::Result<Vec<Row>> {
        let Some(glob) = files(table)? else {
            return Ok(Vec::new());
        };
        let sql = format!(
            "SELECT CAST(CAST(date AS DATE) AS VARCHAR), {label}, {}, {}, {}{extra}
            FROM read_csv({glob}, header = true, all_varchar = true)",
            num("visitors"),
            num("pageviews"),
            num("visits"),
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(Row {
                day: date(&r.get::<_, String>(0)?).unwrap_or(Date::MIN),
                label: r
                    .get::<_, Option<String>>(1)?
                    .filter(|s| !s.trim().is_empty()),
                visitors: r.get::<_, i64>(2)?.max(0) as u64,
                pageviews: r.get::<_, i64>(3)?.max(0) as u64,
                visits: r.get::<_, i64>(4)?.max(0) as u64,
                medium: if extra.is_empty() { None } else { r.get(5)? },
                utm_source: if extra.is_empty() { None } else { r.get(6)? },
                referrer: if extra.is_empty() { None } else { r.get(7)? },
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    };
    for r in read("visitors", "NULL", "")? {
        out.push(r.to("total", None));
    }
    for r in read("pages", "page", "")? {
        let label = r.label.clone();
        out.push(r.to("page", label));
    }
    for r in read("locations", "country", "")? {
        let label = r.label.clone().map(|c| c.to_ascii_uppercase());
        out.push(r.to("country", label));
    }
    for r in read("devices", "device", "")? {
        let label = r.label.clone();
        out.push(r.to("device", label));
    }
    for r in read("sources", "source", ", utm_medium, utm_source, referrer")? {
        let host = source_host(&r);
        let channel = channel::classify(
            r.medium.as_deref(),
            r.utm_source.as_deref(),
            host.as_deref(),
        );
        out.push(r.clone().to("channel", Some(channel.as_str().to_string())));
        out.push(r.to("referrer", host.map(|h| channel::source_domain(&h))));
    }
    Ok(merge(out))
}

/// Rows of one key on one day summed, as an export may split them by UTM tags or other columns.
fn merge(out: Vec<Imported>) -> Vec<Imported> {
    let mut merged: HashMap<(Date, &'static str, Option<String>), Imported> = HashMap::new();
    for i in out {
        merged
            .entry((i.day, i.kind, i.label.clone()))
            .and_modify(|m| {
                m.visitors += i.visitors;
                m.pageviews += i.pageviews;
                m.visits += i.visits;
            })
            .or_insert(i);
    }
    let mut rows: Vec<Imported> = merged.into_values().collect();
    rows.sort_by(|a, b| (a.day, a.kind, &a.label).cmp(&(b.day, b.kind, &b.label)));
    rows
}

/// A Google Analytics column, by the names GA4 reports and the BigQuery query in the docs use.
fn ga_column(names: &[String], wanted: &[&str]) -> Option<usize> {
    names
        .iter()
        .position(|n| wanted.iter().any(|w| n.trim().eq_ignore_ascii_case(w)))
}

/// A GA4 day, as 20241001 or 2024-10-01.
fn ga_date(s: &str) -> Option<Date> {
    let s = s.trim();
    if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
        return date(&format!("{}-{}-{}", &s[..4], &s[4..6], &s[6..]));
    }
    date(s)
}

/// Reads every CSV file in `dir` exported from Google Analytics 4: each has a date column, at most one
/// dimension (source, page, country or device) and users, views and sessions.
pub fn google_analytics(dir: &Path) -> anyhow::Result<Vec<Imported>> {
    let conn = Connection::open_in_memory()?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("csv") {
            continue;
        }
        // GA4 report exports start with "#" comment lines before the header.
        let text = std::fs::read_to_string(&path)?;
        let body: String = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| format!("{l}\n"))
            .collect();
        let clean = std::env::temp_dir().join(format!("virling-ga-{}.csv", std::process::id()));
        std::fs::write(&clean, body)?;
        let file = format!("'{}'", clean.to_string_lossy().replace('\'', "''"));
        let mut stmt = conn.prepare(&format!(
            "SELECT * FROM read_csv({file}, header = true, all_varchar = true)"
        ))?;
        let mut rows = stmt.query([])?;
        let names: Vec<String> = rows.as_ref().map(|s| s.column_names()).unwrap_or_default();
        let Some(day) = ga_column(&names, &["Date", "date"]) else {
            continue;
        };
        let users = ga_column(&names, &["Active users", "Total users", "Users", "users"]);
        let views = ga_column(
            &names,
            &["Views", "Screen page views", "pageviews", "views"],
        );
        let sessions = ga_column(&names, &["Sessions", "sessions"]);
        let source = ga_column(
            &names,
            &[
                "Session source / medium",
                "Session source",
                "First user source",
                "Source",
                "source",
            ],
        );
        let medium = ga_column(
            &names,
            &["Session medium", "First user medium", "Medium", "medium"],
        );
        let page = ga_column(
            &names,
            &[
                "Page path and screen class",
                "Page path",
                "Landing page",
                "page",
            ],
        );
        let country = ga_column(&names, &["Country", "country", "country_code"]);
        let device = ga_column(&names, &["Device category", "device"]);
        let num = |v: Option<String>| -> u64 {
            v.and_then(|s| s.replace(',', "").trim().parse::<f64>().ok())
                .map_or(0, |n| n.max(0.0) as u64)
        };
        while let Some(r) = rows.next()? {
            let get = |i: Option<usize>| -> Option<String> {
                r.get::<_, Option<String>>(i?).ok().flatten()
            };
            let Some(day) = get(Some(day)).as_deref().and_then(ga_date) else {
                continue;
            };
            let row = Row {
                day,
                label: None,
                visitors: num(get(users)),
                pageviews: num(get(views)),
                visits: num(get(sessions)),
                medium: None,
                utm_source: None,
                referrer: None,
            };
            if let Some(raw) = get(source) {
                let (src, med) = match raw.split_once(" / ") {
                    Some((s, m)) => (s.trim().to_string(), Some(m.trim().to_string())),
                    None => (raw.trim().to_string(), get(medium)),
                };
                let src = src.to_ascii_lowercase();
                let direct = src.is_empty() || src.starts_with('(');
                let host = (!direct && src.contains('.')).then(|| src.clone());
                let med = med.filter(|m| !m.starts_with('('));
                let channel = channel::classify(
                    med.as_deref(),
                    (!direct).then_some(src.as_str()),
                    host.as_deref(),
                );
                let referrer = if direct {
                    None
                } else {
                    Some(host.as_deref().map_or(src.clone(), channel::source_domain))
                };
                out.push(
                    row.clone()
                        .to("channel", Some(channel.as_str().to_string())),
                );
                out.push(row.to("referrer", referrer));
            } else if let Some(p) = get(page) {
                out.push(row.to("page", Some(p.trim().to_string())));
            } else if let Some(c) = get(country) {
                let c = c.trim();
                let code = if c.len() == 2 {
                    Some(c.to_ascii_uppercase())
                } else {
                    crate::flags::FLAGS
                        .iter()
                        .find(|f| f.1.eq_ignore_ascii_case(c))
                        .map(|f| f.0.to_string())
                };
                out.push(row.to("country", code));
            } else if let Some(d) = get(device) {
                let mut d = d.trim().to_ascii_lowercase();
                if let Some(first) = d.get_mut(..1) {
                    first.make_ascii_uppercase();
                }
                out.push(row.to("device", Some(d)));
            } else {
                out.push(row.to("total", None));
            }
        }
    }
    Ok(merge(out))
}

/// A YYYY-MM-DD date.
pub fn date(s: &str) -> Option<Date> {
    Date::parse(
        s.trim(),
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .ok()
}

#[derive(Clone)]
struct Row {
    day: Date,
    label: Option<String>,
    visitors: u64,
    pageviews: u64,
    visits: u64,
    medium: Option<String>,
    utm_source: Option<String>,
    referrer: Option<String>,
}

impl Row {
    fn to(self, kind: &'static str, label: Option<String>) -> Imported {
        Imported {
            day: self.day,
            kind,
            label,
            visitors: self.visitors,
            pageviews: self.pageviews,
            visits: self.visits,
        }
    }
}

/// The referrer's host, else a source name that stands for a known host; None for direct traffic.
fn source_host(r: &Row) -> Option<String> {
    let referrer = r
        .referrer
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(host) = referrer.and_then(|s| {
        let url = if s.contains("://") {
            s.to_string()
        } else {
            format!("https://{s}")
        };
        url::Url::parse(&url)
            .ok()?
            .host_str()
            .map(str::to_ascii_lowercase)
    }) {
        return Some(host);
    }
    let source = r.label.as_deref()?.trim().to_ascii_lowercase();
    if source.is_empty() || source.starts_with("direct") {
        return None;
    }
    Some(source)
}

/// Imported sums of a period and the one before it.
#[derive(Default)]
pub struct Sums {
    /// Visitors per day of both periods.
    pub days: HashMap<Date, u64>,
    /// Visitors and visits of the period and of the one before.
    current: (u64, u64),
    previous: (u64, u64),
    /// Visitors per (kind, label) in the period.
    rows: HashMap<(String, Option<String>), u64>,
}

impl Sums {
    /// Takes a (kind, label, day of a total, in the period, visitors, visits) row.
    pub fn add(&mut self, row: (String, Option<String>, Option<String>, bool, u64, u64)) {
        let (kind, label, day, current, visitors, visits) = row;
        if kind == "total" {
            let t = if current {
                &mut self.current
            } else {
                &mut self.previous
            };
            t.0 += visitors;
            t.1 += visits;
            if let Some(day) = day.as_deref().and_then(date) {
                *self.days.entry(day).or_default() += visitors;
            }
        } else if current {
            *self.rows.entry((kind, label)).or_default() += visitors;
        }
    }

    /// Adds imported visitors and visits; revenue per visitor and conversion stay on tracked visitors.
    pub fn apply_totals(&self, o: &mut Overview) {
        o.current.visitors += self.current.0;
        o.current.sessions += self.current.1;
        o.previous.visitors += self.previous.0;
        o.previous.sessions += self.previous.1;
    }

    /// Adds imported visitors of `kind` to the matching rows, or as rows without revenue.
    pub fn apply_rows(&self, kind: &str, rows: &mut Vec<BreakdownRow>) {
        for ((k, key), visitors) in &self.rows {
            if k != kind {
                continue;
            }
            match rows.iter_mut().find(|r| !r.unattributed && r.key == *key) {
                Some(r) => r.visitors += visitors,
                None => rows.push(BreakdownRow {
                    label: key.clone().unwrap_or_else(|| "(none)".into()),
                    key: key.clone(),
                    unattributed: false,
                    visitors: *visitors,
                    payments: 0,
                    customers: 0,
                    refunded: 0,
                    revenue_usd: 0.0,
                    revenue_per_visitor: None,
                    avg_visit_secs: None,
                }),
            }
        }
        sort_rows(rows);
    }
}

pub fn add_days(days: &mut [DayPoint], imported: &HashMap<Date, u64>) {
    for d in days.iter_mut().filter(|d| d.hour.is_none()) {
        d.visitors += imported.get(&d.date).copied().unwrap_or(0);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use time::macros::{date, datetime};

    use super::*;
    use crate::events::{DuckStore, Event, EventStore, Payment};
    use crate::stats::{Filter, PaymentFilter, Period};

    fn export() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/plausible")
    }

    #[test]
    fn reads_a_plausible_export() {
        let rows = plausible(&export()).unwrap();
        let sum = |kind: &str, label: Option<&str>| -> u64 {
            rows.iter()
                .filter(|r| r.kind == kind && r.label.as_deref() == label)
                .map(|r| r.visitors)
                .sum()
        };
        assert_eq!(sum("total", None), 30);
        assert_eq!(sum("channel", Some("Organic search")), 6);
        assert_eq!(sum("channel", Some("Direct")), 4);
        assert_eq!(sum("channel", Some("Communities")), 15);
        assert_eq!(sum("channel", Some("Email")), 5);
        assert_eq!(sum("referrer", Some("google.com")), 6);
        assert_eq!(sum("referrer", None), 4);
        assert_eq!(sum("country", Some("DE")), 20);
        assert_eq!(sum("page", Some("/pricing")), 7);
        assert_eq!(sum("device", Some("Mobile")), 20);
    }

    #[test]
    fn reads_google_analytics_exports() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/ga4");
        let rows = google_analytics(&dir).unwrap();
        let sum = |kind: &str, label: Option<&str>| -> u64 {
            rows.iter()
                .filter(|r| r.kind == kind && r.label.as_deref() == label)
                .map(|r| r.visitors)
                .sum()
        };
        assert_eq!(sum("total", None), 30);
        assert_eq!(sum("channel", Some("Organic search")), 6);
        assert_eq!(sum("channel", Some("Direct")), 4);
        assert_eq!(sum("channel", Some("Communities")), 15);
        assert_eq!(sum("channel", Some("Email")), 5);
        assert_eq!(sum("referrer", Some("news.ycombinator.com")), 15);
        assert_eq!(sum("referrer", None), 4);
        assert_eq!(sum("page", Some("/pricing")), 7);
        assert_eq!(sum("country", Some("US")), 10);
        assert_eq!(sum("country", Some("DE")), 20);
        assert_eq!(sum("device", Some("Mobile")), 20);
        let day1 = rows
            .iter()
            .find(|r| r.kind == "total" && r.day == time::macros::date!(2024 - 01 - 01))
            .unwrap();
        assert_eq!((day1.pageviews, day1.visits), (25, 12));
    }

    #[test]
    fn imported_days_show_on_the_dashboard_but_never_earn_credit() {
        let dir = tempfile::tempdir().unwrap();
        let store = DuckStore::open(&dir.path().join("e.duckdb")).unwrap();
        let rows = plausible(&export()).unwrap();
        store.replace_imported("s", &rows).unwrap();
        let p = Period {
            from: date!(2024 - 01 - 01),
            to: date!(2024 - 01 - 31),
            tz: "UTC".into(),
        };
        let o = store.overview("s", &p).unwrap();
        assert_eq!((o.current.visitors, o.current.sessions), (30, 34));
        let r = store.report("s", &p).unwrap();
        let day = r
            .days
            .iter()
            .find(|d| d.date == date!(2024 - 01 - 02))
            .unwrap();
        assert_eq!(day.visitors, 20);
        let hn = r
            .channels
            .iter()
            .find(|c| c.label == "Communities")
            .unwrap();
        assert_eq!((hn.visitors, hn.payments, hn.revenue_usd), (15, 0, 0.0));
        let day2 = Period {
            from: date!(2024 - 01 - 02),
            to: date!(2024 - 01 - 02),
            tz: "UTC".into(),
        };
        let o = store.overview("s", &day2).unwrap();
        assert_eq!((o.current.visitors, o.previous.visitors), (20, 10));
        let later = Period {
            from: date!(2025 - 01 - 01),
            to: date!(2025 - 01 - 31),
            tz: "UTC".into(),
        };
        assert_eq!(store.overview("s", &later).unwrap().current.visitors, 0);
        let filtered = Filter {
            first: vec![("channel", vec![Some("Direct".into())])],
            viewed: None,
        };
        assert_eq!(
            store
                .overview_in("s", &p, &filtered)
                .unwrap()
                .current
                .visitors,
            0
        );

        store
            .upsert_payments(&[Payment {
                site_id: "s".into(),
                provider: "stripe".into(),
                provider_payment_id: "p1".into(),
                ts: datetime!(2024-01-02 12:00 UTC),
                amount: 900,
                currency: "usd".into(),
                amount_usd: Some(9.0),
                refunded: 0,
                status: "paid".into(),
                visitor_id: Some("v1".into()),
                customer_id: None,
                is_renewal: false,
            }])
            .unwrap();
        store
            .rebuild_attribution("s", "last_non_direct", 90)
            .unwrap();
        let pay = &store
            .payment_list("s", &p, PaymentFilter::All, 10, 0)
            .unwrap()
            .rows[0];
        assert!(pay.touch.is_none(), "imported traffic is never a touch");

        // Days once tracking began are left to tracked events.
        store
            .insert_events(&[Event {
                site_id: "s".into(),
                ts: datetime!(2024-01-02 09:00 UTC),
                visitor_id: "v2".into(),
                session_id: "s2".into(),
                kind: "pageview".into(),
                name: None,
                path: "/".into(),
                referrer_host: None,
                utm_source: None,
                utm_medium: None,
                utm_campaign: None,
                utm_content: None,
                utm_term: None,
                channel: "Direct".into(),
                country: None,
                device: "Desktop".into(),
                browser: "Safari".into(),
                os: "macOS".into(),
                region: None,
                city: None,
                host: None,
                language: None,
                direct: None,
            }])
            .unwrap();
        store.replace_imported("s", &rows).unwrap();
        let o = store.overview("s", &p).unwrap();
        assert_eq!(o.current.visitors, 10 + 1);
    }
}
