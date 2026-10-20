//! The white-label weekly client report, shared by the app page, the public link and the email.

use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};

use super::cards::tone;
use super::insight::times;
use super::schedule::{self, Schedule};
use super::{Ctx, Opt, blocking, group, load, money, plural, usd};
use crate::app::AppState;
use crate::auth::Account;
use crate::channel::source_name;
use crate::chart::esc;
use crate::icons;
use crate::onboarding::render;
use crate::settings::{self, Brand};
use crate::sites::{self, Site};
use crate::stats::{BreakdownRow, DayPoint, Dimension, Period, Report, Totals, sort_rows};

/// Sources listed before the rest fold into "Other sites".
const SOURCE_ROWS: usize = 7;
const PAGE_ROWS: usize = 6;
/// Under this many visitors a row is a small sample and stays out of the insights.
const SMALL: u64 = 30;
const DEFAULT_ACCENT: &str = "#E8471C";
const WEEKS_LISTED: i64 = 12;
const NOTE_MAX: usize = 2000;

/// Monday of the week holding `d`.
pub(crate) fn week_start(d: Date) -> Date {
    d - Duration::days(d.weekday().number_days_from_monday().into())
}

/// Monday of the last full week before the one holding `today`.
pub(crate) fn last_week(today: Date) -> Date {
    week_start(today) - Duration::days(7)
}

/// The Monday asked for in `?week=`, else last week; unfinished weeks fall back to last week.
fn pick_week(today: Date, q: Option<&str>) -> Date {
    let last = last_week(today);
    q.and_then(|s| Date::parse(s, format_description!("[year]-[month]-[day]")).ok())
        .map(week_start)
        .filter(|w| *w <= last)
        .unwrap_or(last)
}

pub(crate) fn week_period(monday: Date, tz: &str) -> Period {
    Period {
        from: monday,
        to: monday + Duration::days(6),
        tz: tz.to_string(),
    }
}

/// "Mon Oct 5 to Sun Oct 11, 2026"
pub(crate) fn week_label(monday: Date) -> String {
    let sunday = monday + Duration::days(6);
    let f = format_description!("[weekday repr:short] [month repr:short] [day padding:none]");
    let (a, b) = (
        monday.format(f).unwrap_or_default(),
        sunday.format(f).unwrap_or_default(),
    );
    if monday.year() == sunday.year() {
        format!("{a} to {b}, {}", sunday.year())
    } else {
        format!("{a}, {} to {b}, {}", monday.year(), sunday.year())
    }
}

/// "Oct 5 to Oct 11"
pub(crate) fn week_short(monday: Date) -> String {
    format!(
        "{} to {}",
        super::day(monday),
        super::day(monday + Duration::days(6))
    )
}

fn week_opts(today: Date, current: Date, base: &str) -> Vec<Opt> {
    let last = last_week(today);
    (0..WEEKS_LISTED)
        .map(|i| {
            let w = last - Duration::days(7 * i);
            Opt {
                label: week_short(w),
                href: if i == 0 {
                    base.to_string()
                } else {
                    format!("{base}?week={w}")
                },
                on: w == current,
                count: String::new(),
            }
        })
        .collect()
}

/// Everything the report reads from the event store for one week.
pub(crate) struct Data {
    pub report: Report,
    /// Channel and referrer pairs ("AI|chatgpt.com").
    pub sources: Vec<BreakdownRow>,
    /// Landing pages of the week before, to tell new pages apart.
    pub prev_pages: Vec<BreakdownRow>,
}

pub(crate) async fn data(
    state: &AppState,
    period: Period,
    site_id: &str,
) -> Result<Data, StatusCode> {
    let store = state.events.clone();
    let id = site_id.to_string();
    blocking(move || {
        Ok(Data {
            report: store.report(&id, &period)?,
            sources: store.breakdown(&id, &period, Dimension::ChannelSource)?,
            prev_pages: store.breakdown(&id, &period.previous(), Dimension::LandingPage)?,
        })
    })
    .await
}

/// How a report is shown: branding, the agency note and the site's attribution settings.
pub(crate) struct Options<'a> {
    pub brand: &'a Brand,
    pub note: &'a str,
    pub model: &'a str,
    pub window_days: i64,
    /// In the app, where an empty note shows a placeholder and can be edited.
    pub editable: bool,
    /// A personal account's weekly email: Virling branded, without agency header or note.
    pub personal: bool,
}

pub struct Kpi {
    pub label: &'static str,
    pub value: String,
    pub change: String,
    /// up, down or quiet; empty without a change.
    pub tone: &'static str,
    pub note: String,
}

/// A source or page row with its share bars.
pub struct Row {
    pub icon: String,
    /// Image path of the icon for email; empty when there is none.
    pub src: String,
    pub name: String,
    pub sub: String,
    pub mono: bool,
    pub visitors: String,
    pub revenue: String,
    pub per_visitor: String,
    /// Per visitor against the average, "2.0x"; empty without an average.
    pub ratio: String,
    pub tone: &'static str,
    pub small: bool,
    /// Visitor and revenue shares, scaled so the largest share fills the bar.
    pub vw: f64,
    pub rw: f64,
}

pub struct Insight {
    pub kind: &'static str,
    /// good, bad or page.
    pub class: &'static str,
    pub icon: String,
    pub src: String,
    pub title: String,
    pub mono: bool,
    pub text: String,
}

/// One day of the revenue chart, for the email's table chart.
pub struct DayBar {
    pub name: &'static str,
    pub date: String,
    pub revenue: String,
    /// Bar height in pixels.
    pub height: u32,
    pub best: bool,
}

pub struct ReportView {
    pub agency: String,
    pub initials: String,
    pub logo: String,
    pub accent: String,
    /// The accent at 32% on white, for email bars.
    pub tint: String,
    pub site: String,
    pub week: String,
    pub week_short: String,
    pub headline: String,
    pub subline: String,
    pub kpis: Vec<Kpi>,
    pub chart: String,
    pub days: Vec<DayBar>,
    pub best_day: String,
    pub sources: Vec<Row>,
    pub avg_note: String,
    pub pages: Vec<Row>,
    pub insights: Vec<Insight>,
    pub note: String,
    pub editable: bool,
    pub footer: String,
    pub credit: String,
    pub powered_by: bool,
    pub personal: bool,
}

/// A credited source: a channel, with the referrer host when there is one.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Src {
    pub channel: String,
    pub host: Option<String>,
    pub visitors: u64,
    pub revenue: f64,
    /// How many sources an "Other sites" row holds; 0 for a single source.
    pub folded: usize,
}

impl Src {
    fn per_visitor(&self) -> Option<f64> {
        (self.visitors > 0).then(|| self.revenue / self.visitors as f64)
    }

    /// "ChatGPT", "Google", "Direct traffic"
    fn name(&self) -> String {
        match &self.host {
            Some(h) => source_name(h),
            None if self.channel == "Direct" => "Direct traffic".into(),
            None => self.channel.clone(),
        }
    }

    fn label(&self) -> String {
        match (&self.host, self.folded) {
            (_, n) if n > 0 => "Other sites".into(),
            (Some(h), _) => h.clone(),
            (None, _) => self.channel.clone(),
        }
    }

    fn kind(&self) -> String {
        if self.folded > 0 {
            return format!("{} smaller referrers", self.folded);
        }
        match self.channel.as_str() {
            "AI" => "AI assistant",
            "Organic search" => "Search",
            "Direct" => "Typed or bookmarked",
            "Paid" => "Paid ad",
            "Email" => "Newsletters",
            other => other,
        }
        .into()
    }

    /// Inline logo and its email image path.
    fn icon(&self, size: u32) -> (String, String) {
        if self.folded > 0 {
            return (
                icons::glyph("globe", size, "var(--ink-2)"),
                "/assets/icons/glyph-globe.svg".into(),
            );
        }
        match &self.host {
            Some(h) => (icons::source(h, size), icons::source_src(h)),
            None => (
                icons::channel_glyph(&self.channel)
                    .map(|g| icons::glyph(g, size, "var(--ink-2)"))
                    .unwrap_or_else(|| icons::source(&self.channel, size)),
                icons::source_src(&self.channel),
            ),
        }
    }

    /// The insight card title: "ChatGPT answers", "Posts on x.com".
    fn title(&self) -> String {
        let host = self.host.clone().unwrap_or_default();
        match self.channel.as_str() {
            _ if self.host.is_none() && self.channel == "Direct" => "Direct visits".into(),
            "AI" if !host.is_empty() => format!("{} answers", self.name()),
            "Organic search" if !host.is_empty() => format!("{} search", self.name()),
            "Social" if !host.is_empty() => format!("Posts on {host}"),
            "Communities" if !host.is_empty() => format!("Threads on {}", self.name()),
            "Paid" => "Paid ads".into(),
            "Email" => "Emails".into(),
            _ if !host.is_empty() => format!("Links on {host}"),
            other => other.to_string(),
        }
    }
}

/// Sources by revenue; past `SOURCE_ROWS` the rest fold into one "Other sites" row.
pub(crate) fn sources(rows: &[BreakdownRow]) -> Vec<Src> {
    let mut out: Vec<Src> = rows
        .iter()
        .filter(|r| !r.unattributed)
        .filter_map(|r| {
            let (channel, host) = r.key.as_deref()?.split_once('|')?;
            Some(Src {
                channel: channel.to_string(),
                host: (!host.is_empty()).then(|| host.to_string()),
                visitors: r.visitors,
                revenue: r.revenue_usd,
                folded: 0,
            })
        })
        .filter(|s| s.visitors > 0 || s.revenue > 0.0)
        .collect();
    let order = |a: &Src, b: &Src| {
        b.revenue
            .total_cmp(&a.revenue)
            .then(b.visitors.cmp(&a.visitors))
            .then_with(|| a.label().cmp(&b.label()))
    };
    if out.len() > SOURCE_ROWS + 1 {
        out.sort_by(|a, b| b.visitors.cmp(&a.visitors).then(order(a, b)));
        let rest = out.split_off(SOURCE_ROWS);
        out.push(Src {
            channel: "Referral".into(),
            host: None,
            visitors: rest.iter().map(|s| s.visitors).sum(),
            revenue: rest.iter().map(|s| s.revenue).sum(),
            folded: rest.len(),
        });
    }
    out.sort_by(order);
    out
}

/// Whole percent change, None when there is nothing to compare with.
fn change(cur: f64, prev: f64) -> Option<i64> {
    (prev > 0.0).then(|| ((cur - prev) / prev * 100.0).round() as i64)
}

fn pct(n: f64, d: f64) -> String {
    if d > 0.0 {
        format!("{:.0}%", n / d * 100.0)
    } else {
        "0%".into()
    }
}

/// "$1,467" without cents, for chart labels.
fn whole(v: f64) -> String {
    format!("${}", group(v.max(0.0).round() as u64))
}

fn kpi(
    label: &'static str,
    value: String,
    cur: Option<f64>,
    prev: Option<f64>,
    from: impl Fn(f64) -> String,
    falls: &'static str,
) -> Kpi {
    let (change, tone, note) = match (cur, prev) {
        (Some(c), Some(p)) => match change(c, p) {
            Some(x) => (
                if x == 0 {
                    "0%".into()
                } else {
                    format!("{x:+}%")
                },
                match x {
                    1.. => "up",
                    0 => "quiet",
                    _ => falls,
                },
                format!("from {}", from(p)),
            ),
            None => (String::new(), "", "none the week before".into()),
        },
        (None, _) => (String::new(), "", "no visitors this week".into()),
        (Some(_), None) => (String::new(), "", "none the week before".into()),
    };
    Kpi {
        label,
        value,
        change,
        tone,
        note,
    }
}

/// All revenue of the week over its visitors, the average the badges compare with.
fn per_visitor(t: &Totals) -> Option<f64> {
    (t.visitors > 0).then(|| t.revenue_usd / t.visitors as f64)
}

pub(crate) fn kpis(cur: &Totals, prev: &Totals) -> Vec<Kpi> {
    let mut out = vec![
        kpi(
            "Revenue",
            money(cur.revenue_usd),
            Some(cur.revenue_usd),
            Some(prev.revenue_usd),
            money,
            "down",
        ),
        kpi(
            "New customers",
            group(cur.new_customers),
            Some(cur.new_customers as f64),
            Some(prev.new_customers as f64),
            |v| group(v as u64),
            "down",
        ),
        kpi(
            "Revenue per visitor",
            per_visitor(cur).map_or_else(|| "n/a".into(), usd),
            per_visitor(cur),
            per_visitor(prev),
            usd,
            "down",
        ),
        kpi(
            "Visitors",
            group(cur.visitors),
            Some(cur.visitors as f64),
            Some(prev.visitors as f64),
            |v| group(v as u64),
            "quiet",
        ),
    ];
    let rev = change(cur.revenue_usd, prev.revenue_usd).unwrap_or(0);
    let vis = change(cur.visitors as f64, prev.visitors as f64).unwrap_or(0);
    if vis < 0 && rev > 0 {
        out[3].note = "fewer visitors, more money".into();
    } else if vis > 0 && rev < 0 {
        out[3].note = "more visitors, less money".into();
    }
    out
}

/// The bold line under "The week in one line".
pub(crate) fn headline(cur: &Totals, prev: &Totals, channels: &[BreakdownRow]) -> String {
    if cur.revenue_usd <= 0.0 {
        return if cur.visitors > 0 {
            format!(
                "No payments came in this week, from {}.",
                plural(cur.visitors, "visitor")
            )
        } else {
            "No visits or payments this week.".into()
        };
    }
    let total = money(cur.revenue_usd);
    let mut line = match change(cur.revenue_usd, prev.revenue_usd) {
        Some(x) if x >= 1 => format!("Revenue rose {x}% to {total}."),
        Some(x) if x <= -1 => format!("Revenue fell {}% to {total}.", -x),
        Some(_) => format!("Revenue held steady at {total}."),
        None => format!("Revenue was {total} this week."),
    };
    let named = channels
        .iter()
        .filter(|r| !r.unattributed && r.key.is_some() && r.revenue_usd > 0.0);
    let ai = named.clone().find(|r| r.key.as_deref() == Some("AI"));
    let top = ai.or_else(|| named.max_by(|a, b| a.revenue_usd.total_cmp(&b.revenue_usd)));
    if let Some(c) = top.filter(|_| cur.visitors > 0) {
        let shares = format!(
            "{} of visitors and {} of the money",
            pct(c.visitors as f64, cur.visitors as f64),
            pct(c.revenue_usd, cur.revenue_usd)
        );
        let who = match c.key.as_deref().unwrap_or_default() {
            "AI" => "AI assistants sent",
            "Organic search" => "Search engines sent",
            "Social" => "Social posts sent",
            "Communities" => "Community threads sent",
            "Referral" => "Links on other sites sent",
            "Email" => "Emails sent",
            "Paid" => "Paid ads sent",
            _ => "Direct visits were",
        };
        line.push_str(&format!(" {who} {shares}."));
    }
    line
}

/// The sentence under the headline: the top revenue source against the next one.
pub(crate) fn subline(srcs: &[Src]) -> String {
    let mut paying = srcs.iter().filter(|s| s.revenue > 0.0 && s.folded == 0);
    let Some(top) = paying.next() else {
        return String::new();
    };
    let Some(next) = paying.next() else {
        return format!("{} was your top revenue source this week.", top.name());
    };
    let r = top.visitors as f64 / next.visitors.max(1) as f64;
    let visitors = if next.visitors == 0 || r >= 0.95 {
        ""
    } else if r < 0.34 {
        ", with under a third of the visitors"
    } else if r < 0.45 {
        ", with under half the visitors"
    } else if r <= 0.55 {
        ", with half as many visitors"
    } else {
        ", with fewer visitors"
    };
    format!(
        "{} was your top revenue source this week, ahead of {}{visitors}.",
        top.name(),
        next.name()
    )
}

fn ratio(pv: Option<f64>, avg: Option<f64>) -> Option<f64> {
    Some(pv? / avg.filter(|a| *a > 0.0)?)
}

/// The source paying best per visitor, at 1.15x the average or more.
pub(crate) fn working(srcs: &[Src], avg: Option<f64>) -> Option<Insight> {
    let s = srcs
        .iter()
        .filter(|s| s.folded == 0 && s.visitors >= SMALL && s.revenue > 0.0)
        .filter_map(|s| Some((s, ratio(s.per_visitor(), avg)?)))
        .filter(|(_, x)| *x >= 1.15)
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let (icon, src) = s.0.icon(18);
    Some(Insight {
        kind: "Working",
        class: "good",
        icon,
        src,
        title: s.0.title(),
        mono: false,
        text: format!(
            "{} visitors brought {}. That's {} per visitor, {} your average.",
            group(s.0.visitors),
            money(s.0.revenue),
            usd(s.0.per_visitor().unwrap_or(0.0)),
            times(s.1)
        ),
    })
}

/// The source paying worst per visitor, at half the average or less; direct visits can't be acted on.
pub(crate) fn not_paying(srcs: &[Src], avg: Option<f64>) -> Option<Insight> {
    let s = srcs
        .iter()
        .filter(|s| s.folded == 0 && s.visitors >= SMALL && s.host.is_some())
        .filter_map(|s| Some((s, ratio(s.per_visitor(), avg)?)))
        .filter(|(_, x)| *x <= 0.5)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(b.0.visitors.cmp(&a.0.visitors)))?;
    let (s, x) = (s.0, s.1);
    let (icon, src) = s.icon(18);
    Some(Insight {
        kind: "Not paying off",
        class: "bad",
        icon,
        src,
        title: s.title(),
        mono: false,
        text: if s.revenue > 0.0 {
            format!(
                "{} visitors brought {}. That's {} per visitor, {} your average.",
                group(s.visitors),
                money(s.revenue),
                usd(s.per_visitor().unwrap_or(0.0)),
                times(x)
            )
        } else {
            format!("{} visitors and no payment yet.", group(s.visitors))
        },
    })
}

/// The landing page paying best per visitor, at 1.15x the average or more.
pub(crate) fn best_page(
    pages: &[BreakdownRow],
    prev_pages: &[BreakdownRow],
    avg: Option<f64>,
) -> Option<Insight> {
    let (p, x) = pages
        .iter()
        .filter(|r| !r.unattributed && r.key.is_some() && r.visitors >= SMALL)
        .filter_map(|r| Some((r, ratio(r.revenue_per_visitor, avg)?)))
        .filter(|(_, x)| *x >= 1.15)
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let pv = usd(p.revenue_per_visitor.unwrap_or(0.0));
    let new = !prev_pages
        .iter()
        .any(|r| r.key == p.key && !r.unattributed && r.visitors > 0);
    Some(Insight {
        kind: "Best page",
        class: "page",
        icon: String::new(),
        src: String::new(),
        title: p.label.clone(),
        mono: true,
        text: if new {
            format!("{pv} per visitor in its first week.")
        } else {
            format!("{pv} per visitor, {} your average.", times(x))
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn row(
    icon: (String, String),
    name: String,
    sub: String,
    mono: bool,
    visitors: u64,
    revenue: f64,
    avg: Option<f64>,
    totals: (f64, f64),
) -> Row {
    let pv = (visitors > 0).then(|| revenue / visitors as f64);
    let x = ratio(pv, avg);
    let share = |n: f64, t: f64| if t > 0.0 { n.max(0.0) / t } else { 0.0 };
    Row {
        icon: icon.0,
        src: icon.1,
        name,
        sub,
        mono,
        visitors: group(visitors),
        revenue: money(revenue),
        per_visitor: pv.map_or_else(|| "n/a".into(), usd),
        ratio: x.map(|x| format!("{x:.1}x")).unwrap_or_default(),
        tone: x.map_or("", tone),
        small: visitors < SMALL,
        vw: share(visitors as f64, totals.0),
        rw: share(revenue, totals.1),
    }
}

/// Days of the week with the revenue chart, its email bars and the best-day sentence.
fn chart(days: &[DayPoint], monday: Date, accent: &str) -> (String, Vec<DayBar>, String) {
    const NAMES: [(&str, &str); 7] = [
        ("Mon", "Monday"),
        ("Tue", "Tuesday"),
        ("Wed", "Wednesday"),
        ("Thu", "Thursday"),
        ("Fri", "Friday"),
        ("Sat", "Saturday"),
        ("Sun", "Sunday"),
    ];
    let week: Vec<(Date, f64, u64, u64)> = (0..7)
        .map(|i| {
            let date = monday + Duration::days(i);
            days.iter()
                .find(|d| d.date == date)
                .map_or((date, 0.0, 0, 0), |d| {
                    (date, d.revenue_usd.max(0.0), d.visitors, d.payments)
                })
        })
        .collect();
    let rmax = week.iter().map(|d| d.1).fold(0.0, f64::max);
    let vmax = week.iter().map(|d| d.2).max().unwrap_or(0);
    let vmin = week.iter().map(|d| d.2).min().unwrap_or(0);
    let best = (rmax > 0.0)
        .then(|| week.iter().position(|d| d.1 == rmax))
        .flatten();
    let col = 760.0 / 7.0;
    let cx = |i: usize| col / 2.0 + col * i as f64;
    let mut svg = String::from(
        r##"<path d="M0 206H760" style="stroke:var(--rule)" stroke-width="1"></path>"##,
    );
    let mut bars = Vec::new();
    for (i, d) in week.iter().enumerate() {
        let h = if rmax > 0.0 { d.1 / rmax * 148.3 } else { 0.0 };
        if h > 0.0 {
            svg.push_str(&format!(
                r#"<rect x="{:.1}" y="{:.1}" width="56" height="{h:.1}" rx="{:.1}" fill="{accent}" fill-opacity="{}"></rect>"#,
                cx(i) - 28.0,
                206.0 - h,
                h.min(12.0) / 2.0,
                if Some(i) == best { "1" } else { "0.32" }
            ));
        }
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="600" style="fill:var(--ink)" font-family="Schibsted Grotesk, sans-serif">{}</text>"##,
            cx(i),
            198.0 - h,
            whole(d.1)
        ));
        bars.push(DayBar {
            name: NAMES[i].0,
            date: super::day(d.0),
            revenue: whole(d.1),
            height: (d.1 / rmax.max(1.0) * 120.0).round() as u32,
            best: Some(i) == best,
        });
    }
    if vmax > 0 {
        let y = |v: u64| 206.0 - v as f64 / vmax as f64 * 145.1;
        let points: Vec<String> = week
            .iter()
            .enumerate()
            .map(|(i, d)| format!("{:.1} {:.1}", cx(i), y(d.2)))
            .collect();
        svg.push_str(&format!(
            r##"<path d="M{}" fill="none" style="stroke:var(--visitors)" stroke-width="1.6" stroke-dasharray="4 4"></path>"##,
            points.join(" L")
        ));
        for (i, d) in week.iter().enumerate() {
            svg.push_str(&format!(
                r##"<circle cx="{:.1}" cy="{:.1}" r="3" style="fill:var(--paper);stroke:var(--visitors)" stroke-width="1.6"></circle>"##,
                cx(i),
                y(d.2)
            ));
        }
    }
    for (i, d) in week.iter().enumerate() {
        svg.push_str(&format!(
            r##"<text x="{x:.1}" y="226" text-anchor="middle" font-size="12" style="fill:var(--ink-2)" font-family="Schibsted Grotesk, sans-serif">{}</text><text x="{x:.1}" y="242" text-anchor="middle" font-size="11" fill="#8A909A" font-family="Geist Mono, monospace">{}</text>"##,
            NAMES[i].0,
            super::day(d.0),
            x = cx(i)
        ));
    }
    let sentence = match best {
        Some(i) => {
            let d = &week[i];
            let crowd = if vmax == vmin {
                ""
            } else if d.2 == vmin {
                ", on the fewest visitors of the week"
            } else if d.2 == vmax {
                ", on the most visitors of the week"
            } else {
                ""
            };
            format!(
                "{} was the best day: {} from {}{crowd}.",
                NAMES[i].1,
                whole(d.1),
                plural(d.3, "payment")
            )
        }
        None => "No payments came in this week.".into(),
    };
    let label = format!(
        "Revenue per day as bars and visitors per day as a line, Monday {} to Sunday {}. {}",
        super::day(monday),
        super::day(monday + Duration::days(6)),
        esc(&sentence)
    );
    (
        format!(
            r#"<svg viewBox="0 0 760 250" width="100%" role="img" aria-label="{label}" class="r2-chart">{svg}</svg>"#
        ),
        bars,
        sentence,
    )
}

/// `accent` mixed with white, `alpha` of the way.
fn tint(accent: &str, alpha: f64) -> String {
    let c = |i: usize| u8::from_str_radix(accent.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    let mix = |v: u8| (255.0 - (255.0 - v as f64) * alpha).round() as u8;
    format!("#{:02X}{:02X}{:02X}", mix(c(1)), mix(c(3)), mix(c(5)))
}

/// Up to two initials of a name ("Fieldwork Studio" -> "FS").
fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

pub(crate) fn build(site: &str, monday: Date, d: &Data, o: &Options) -> ReportView {
    let r = &d.report;
    let (cur, prev) = (&r.overview.current, &r.overview.previous);
    let avg = per_visitor(cur);
    let accent = if o.brand.accent.is_empty() {
        DEFAULT_ACCENT.to_string()
    } else {
        o.brand.accent.to_ascii_uppercase()
    };
    let totals = (cur.visitors as f64, cur.revenue_usd);
    let srcs = sources(&d.sources);
    let mut source_rows: Vec<Row> = srcs
        .iter()
        .map(|s| {
            row(
                s.icon(18),
                s.label(),
                s.kind(),
                false,
                s.visitors,
                s.revenue,
                avg,
                totals,
            )
        })
        .collect();
    let mut landing: Vec<BreakdownRow> = r
        .landing_pages
        .iter()
        .filter(|p| !p.unattributed && p.key.is_some())
        .cloned()
        .collect();
    sort_rows(&mut landing);
    let mut page_rows: Vec<Row> = landing
        .iter()
        .take(PAGE_ROWS)
        .map(|p| {
            row(
                (String::new(), String::new()),
                p.label.clone(),
                format!(
                    "{} of visitors landed here",
                    pct(p.visitors as f64, cur.visitors as f64)
                ),
                true,
                p.visitors,
                p.revenue_usd,
                avg,
                totals,
            )
        })
        .collect();
    let scale = source_rows
        .iter()
        .chain(&page_rows)
        .map(|r| r.vw.max(r.rw))
        .fold(0.0, f64::max);
    for r in source_rows.iter_mut().chain(page_rows.iter_mut()) {
        if scale > 0.0 {
            r.vw = r.vw / scale * 100.0;
            r.rw = r.rw / scale * 100.0;
        }
    }
    let (svg, days, best_day) = chart(&r.days, monday, &accent);
    let (lost_usd, lost_payments) = r
        .channels
        .iter()
        .filter(|c| c.unattributed)
        .fold((0.0, 0), |(u, n), c| (u + c.revenue_usd, n + c.payments));
    let mut credit = format!(
        "Each payment is credited to the {} within {} days.",
        if o.model == "first_touch" {
            "first visit"
        } else {
            "last non-direct visit"
        },
        o.window_days
    );
    if lost_payments > 0 {
        credit.push_str(&format!(
            " {} from {} had no matched visit, so it counts in the total but in no source.",
            money(lost_usd),
            plural(lost_payments, "payment")
        ));
    }
    ReportView {
        agency: o.brand.agency_name.clone(),
        initials: initials(&o.brand.agency_name),
        logo: o.brand.logo_url.clone(),
        tint: tint(&accent, 0.32),
        accent,
        site: site.to_string(),
        week: week_label(monday),
        week_short: week_short(monday),
        headline: headline(cur, prev, &r.channels),
        subline: subline(&srcs),
        kpis: kpis(cur, prev),
        chart: svg,
        days,
        best_day,
        avg_note: avg.map_or_else(String::new, |a| {
            format!(
                "Per visitor is compared with your average of {}. Green means a source pays better than average.",
                usd(a)
            )
        }),
        insights: [
            working(&srcs, avg),
            not_paying(&srcs, avg),
            best_page(&landing, &d.prev_pages, avg),
        ]
        .into_iter()
        .flatten()
        .collect(),
        sources: source_rows,
        pages: page_rows,
        note: o.note.to_string(),
        editable: o.editable,
        personal: o.personal,
        footer: o.brand.footer.clone(),
        credit,
        powered_by: o.brand.powered_by,
    }
}

/// Attribution model and window of a site.
pub(crate) async fn attribution(
    state: &AppState,
    site_id: &str,
) -> Result<(String, i64), StatusCode> {
    sqlx::query_as("SELECT attribution_model, window_days FROM sites WHERE id = ?")
        .bind(site_id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

/// The report of one week of `site`, as the client sees it unless `editable`.
pub(crate) async fn view(
    state: &AppState,
    site: &Site,
    monday: Date,
    editable: bool,
) -> Result<ReportView, StatusCode> {
    let (agency, _) = schedule::owner(&state.db, &site.id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let (brand, note) = if agency {
        let brand = settings::brand(&state.db, &site.id)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let (_, note) = schedule::load(&state.db, &site.id)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        (brand, note)
    } else {
        let brand = Brand {
            powered_by: false,
            ..Brand::default()
        };
        (brand, String::new())
    };
    let (model, window_days) = attribution(state, &site.id).await?;
    let d = data(state, week_period(monday, &site.timezone), &site.id).await?;
    Ok(build(
        &site.domain,
        monday,
        &d,
        &Options {
            brand: &brand,
            note: &note,
            model: &model,
            window_days,
            editable: editable && agency,
            personal: !agency,
        },
    ))
}

#[derive(Deserialize, Default)]
pub struct WeekQuery {
    week: Option<String>,
    sent: Option<u32>,
    saved: Option<String>,
    error: Option<String>,
}

/// The schedule line and the fields of its popover.
pub struct SchedView {
    on: bool,
    day: &'static str,
    time: String,
    first: String,
    more: String,
    next: String,
    days: Vec<(u8, &'static str, bool)>,
    minute: u16,
    times: Vec<Opt>,
    recipients: String,
    tz: String,
}

#[derive(Template)]
#[template(path = "report.html")]
struct ReportPage {
    domain: String,
    report: ReportView,
    weeks: Vec<Opt>,
    week: String,
    week_param: String,
    sched: SchedView,
    notice: String,
    alert: &'static str,
    branded: bool,
    agency: bool,
}

#[derive(Template)]
#[template(path = "report_shared.html")]
struct SharedPage {
    report: ReportView,
    weeks: Vec<Opt>,
    week: String,
    week_param: String,
}

fn sched_view(s: &Schedule, ctx: &Ctx) -> SchedView {
    let now = OffsetDateTime::now_utc();
    SchedView {
        on: s.on && !s.recipients.is_empty(),
        day: schedule::DAYS[usize::from(s.weekday)].1,
        time: schedule::clock(s.minute),
        first: s.recipients.first().cloned().unwrap_or_default(),
        more: match s.recipients.len() {
            0 | 1 => String::new(),
            2 => " and 1 more".into(),
            n => format!(" and {} more", n - 1),
        },
        next: super::day(schedule::next(now, ctx.offset, s)),
        days: schedule::DAYS
            .iter()
            .enumerate()
            .map(|(i, (short, _))| (i as u8, *short, i == usize::from(s.weekday)))
            .collect(),
        minute: s.minute,
        times: (0..48u16)
            .map(|i| Opt {
                label: schedule::clock(i * 30),
                href: (i * 30).to_string(),
                on: i * 30 == s.minute,
                count: String::new(),
            })
            .collect(),
        recipients: s.recipients.join("\n"),
        tz: ctx.site.timezone.clone(),
    }
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<WeekQuery>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "report").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let monday = pick_week(ctx.today, q.week.as_deref());
    let report = match view(&state, &ctx.site, monday, true).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let Ok((sched, _)) = schedule::load(&state.db, &ctx.site.id).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let base = sites::app_path(&ctx.site.domain, "report");
    let notice = match (q.sent, q.saved.as_deref()) {
        (Some(1), _) => "Sent the report to 1 person.".into(),
        (Some(n), _) => format!("Sent the report to {n} people."),
        (_, Some("note")) => "Saved your note.".into(),
        (_, Some("schedule")) => "Saved the schedule.".into(),
        _ => String::new(),
    };
    render(ReportPage {
        alert: match q.error.as_deref() {
            Some("recipients") => "Add one or more valid email addresses, up to 10.",
            Some("none") => "Add who gets the report first: open Change schedule.",
            Some("send") => "The email could not be sent. Check the mail settings and try again.",
            _ => "",
        },
        notice,
        branded: !report.agency.is_empty() || !report.logo.is_empty(),
        agency: ctx.shell.agency,
        domain: ctx.site.domain.clone(),
        weeks: week_opts(ctx.today, monday, &base),
        week: week_short(monday),
        week_param: if monday == last_week(ctx.today) {
            String::new()
        } else {
            monday.to_string()
        },
        sched: sched_view(&sched, &ctx),
        report,
    })
}

/// The report behind a read-only link, without sign-in or Virling branding.
pub async fn shared(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<WeekQuery>,
) -> Response {
    let site: Result<Option<Site>, _> = sqlx::query_as(
        "SELECT s.id, s.domain, s.timezone, s.cookieless, s.same_visit, s.return_minutes FROM report_brands b JOIN sites s ON s.id = b.site_id WHERE b.share_token = ?",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await;
    let site = match site {
        Ok(Some(site)) => site,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let store = state.events.clone();
    let tz = site.timezone.clone();
    let offset = match blocking(move || store.utc_offset(&tz)).await {
        Ok(o) => o,
        Err(code) => return code.into_response(),
    };
    let today = OffsetDateTime::now_utc().to_offset(offset).date();
    let monday = pick_week(today, q.week.as_deref());
    let report = match view(&state, &site, monday, false).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    render(SharedPage {
        report,
        weeks: week_opts(today, monday, &format!("/r/{token}")),
        week: week_short(monday),
        week_param: String::new(),
    })
}

fn back(domain: &str, week: &str, flag: &str) -> Redirect {
    let base = sites::app_path(domain, "report");
    Redirect::to(&if week.is_empty() {
        format!("{base}?{flag}")
    } else {
        format!("{base}?week={week}&{flag}")
    })
}

/// A week param worth keeping in a redirect: a plain date.
fn week_param(week: Option<&str>) -> String {
    week.filter(|w| Date::parse(w, format_description!("[year]-[month]-[day]")).is_ok())
        .unwrap_or_default()
        .to_string()
}

#[derive(Deserialize)]
pub struct NoteInput {
    note: String,
    week: Option<String>,
}

pub async fn save_note(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<NoteInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "report").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let note: String = input.note.trim().chars().take(NOTE_MAX).collect();
    let note = note.replace("\r\n", "\n");
    if let Err(err) = sqlx::query(
        "INSERT INTO report_brands (site_id, note) VALUES (?, ?)
         ON CONFLICT(site_id) DO UPDATE SET note = excluded.note",
    )
    .bind(&ctx.site.id)
    .bind(&note)
    .execute(&state.db)
    .await
    {
        tracing::error!(%err, "saving the report note failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    back(
        &ctx.site.domain,
        &week_param(input.week.as_deref()),
        "saved=note",
    )
    .into_response()
}

#[derive(Deserialize)]
pub struct ScheduleInput {
    on: Option<String>,
    weekday: u8,
    minute: u16,
    recipients: String,
    week: Option<String>,
}

pub async fn save_schedule(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<ScheduleInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "report").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let week = week_param(input.week.as_deref());
    let on = input.on.is_some();
    let recipients = match schedule::recipients(&input.recipients) {
        _ if !ctx.shell.agency => vec![ctx.shell.email.clone()],
        Some(r) if !(on && r.is_empty()) => r,
        _ => return back(&ctx.site.domain, &week, "error=recipients").into_response(),
    };
    let s = Schedule {
        on,
        weekday: input.weekday.min(6),
        minute: input.minute.min(24 * 60 - 1),
        recipients,
        from: OffsetDateTime::now_utc().unix_timestamp(),
    };
    if let Err(err) = schedule::save(&state.db, &ctx.site.id, &s).await {
        tracing::error!(%err, "saving the report schedule failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    back(&ctx.site.domain, &week, "saved=schedule").into_response()
}

#[derive(Deserialize)]
pub struct SendInput {
    week: Option<String>,
}

/// Sends the shown week's report to the recipients now.
pub async fn send_now(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<SendInput>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "report").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let week = week_param(input.week.as_deref());
    let Ok((s, _)) = schedule::load(&state.db, &ctx.site.id).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let to = if ctx.shell.agency {
        s.recipients
    } else {
        vec![ctx.shell.email.clone()]
    };
    if to.is_empty() {
        return back(&ctx.site.domain, &week, "error=none").into_response();
    }
    let monday = pick_week(ctx.today, input.week.as_deref());
    match schedule::send(&state, &ctx.site, monday, &to).await {
        Ok(n) => {
            if monday == last_week(ctx.today)
                && let Err(err) = schedule::record(&state.db, &ctx.site.id, monday, n).await
            {
                tracing::error!(%err, "recording a report send failed");
            }
            back(&ctx.site.domain, &week, &format!("sent={n}")).into_response()
        }
        Err(err) => {
            tracing::error!(%err, "sending the report failed");
            back(&ctx.site.domain, &week, "error=send").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use time::macros::date;

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
            revenue_per_visitor: (visitors > 0).then(|| revenue / visitors as f64),
        }
    }

    fn design_sources() -> Vec<BreakdownRow> {
        vec![
            row("AI|chatgpt.com", 122, 412.0),
            row("Organic search|google.com", 284, 398.0),
            row("Direct|", 210, 205.0),
            row("AI|claude.ai", 46, 151.0),
            row("Organic search|bing.com", 22, 72.0),
            row("AI|perplexity.ai", 38, 64.0),
            row("Social|x.com", 61, 15.0),
            row("Referral|a.dev", 20, 30.0),
            row("Referral|b.dev", 15, 20.0),
            row("Referral|c.dev", 14, 14.0),
            row("Referral|d.dev", 12, 10.0),
            row("Referral|e.dev", 8, 8.0),
        ]
    }

    #[test]
    fn weeks_run_monday_to_sunday() {
        assert_eq!(week_start(date!(2026 - 10 - 16)), date!(2026 - 10 - 12));
        assert_eq!(week_start(date!(2026 - 10 - 12)), date!(2026 - 10 - 12));
        assert_eq!(week_start(date!(2026 - 10 - 18)), date!(2026 - 10 - 12));
        assert_eq!(last_week(date!(2026 - 10 - 16)), date!(2026 - 10 - 05));
        assert_eq!(last_week(date!(2026 - 10 - 12)), date!(2026 - 10 - 05));
        let today = date!(2026 - 10 - 16);
        assert_eq!(pick_week(today, None), date!(2026 - 10 - 05));
        assert_eq!(pick_week(today, Some("2026-09-30")), date!(2026 - 09 - 28));
        assert_eq!(pick_week(today, Some("2026-10-13")), date!(2026 - 10 - 05));
        assert_eq!(pick_week(today, Some("nope")), date!(2026 - 10 - 05));
        assert_eq!(
            week_label(date!(2026 - 10 - 05)),
            "Mon Oct 5 to Sun Oct 11, 2026"
        );
        assert_eq!(
            week_label(date!(2026 - 12 - 28)),
            "Mon Dec 28, 2026 to Sun Jan 3, 2027"
        );
        let p = week_period(date!(2026 - 10 - 05), "UTC");
        assert_eq!(
            (p.from, p.to),
            (date!(2026 - 10 - 05), date!(2026 - 10 - 11))
        );
        assert_eq!(p.previous().from, date!(2026 - 09 - 28));
        let opts = week_opts(today, date!(2026 - 09 - 28), "/r/t");
        assert_eq!(opts[0].href, "/r/t");
        assert_eq!(opts[1].href, "/r/t?week=2026-09-28");
        assert!(opts[1].on && !opts[0].on);
    }

    #[test]
    fn small_referrers_fold_into_other_sites() {
        let s = sources(&design_sources());
        let labels: Vec<String> = s.iter().map(|s| s.label()).collect();
        assert_eq!(
            labels,
            [
                "chatgpt.com",
                "google.com",
                "Direct",
                "claude.ai",
                "Other sites",
                "bing.com",
                "perplexity.ai",
                "x.com"
            ]
        );
        assert_eq!(s[4].kind(), "5 smaller referrers");
        assert_eq!((s[4].visitors, s[4].revenue), (69, 82.0));
        assert_eq!(s[2].kind(), "Typed or bookmarked");
        assert_eq!(s[0].kind(), "AI assistant");
        assert_eq!(
            sources(&design_sources()[..8]).len(),
            8,
            "one extra is not folded"
        );
    }

    #[test]
    fn insights_say_only_what_holds() {
        let s = sources(&design_sources());
        let avg = Some(1.72);
        let w = working(&s, avg).unwrap();
        assert_eq!(w.title, "ChatGPT answers");
        assert_eq!(
            w.text,
            "122 visitors brought $412. That's $3.38 per visitor, twice your average."
        );
        let n = not_paying(&s, avg).unwrap();
        assert_eq!(n.title, "Posts on x.com");
        assert_eq!(
            n.text,
            "61 visitors brought $15. That's $0.25 per visitor, 0.1x your average."
        );
        let pages = [
            row("/compare", 112, 268.0),
            row("/blog/late-payment-email-templates", 71, 186.0),
            row("/pricing", 150, 164.0),
        ];
        let b = best_page(&pages, &[row("/compare", 90, 100.0)], avg).unwrap();
        assert_eq!(b.title, "/blog/late-payment-email-templates");
        assert_eq!(b.text, "$2.62 per visitor in its first week.");
        let b = best_page(&pages, &pages, avg).unwrap();
        assert_eq!(b.text, "$2.62 per visitor, 1.5x your average.");

        let even = sources(&[
            row("AI|chatgpt.com", 100, 172.0),
            row("Social|x.com", 100, 150.0),
        ]);
        assert!(
            working(&even, avg).is_none(),
            "nobody pays 1.15x the average"
        );
        assert!(
            not_paying(&even, avg).is_none(),
            "nobody pays half the average"
        );
        assert!(working(&s, None).is_none() && best_page(&pages, &[], None).is_none());
        let small = sources(&[row("Organic search|bing.com", 22, 72.0)]);
        assert!(working(&small, avg).is_none(), "small samples stay out");
        let direct = sources(&[row("Direct|", 200, 10.0)]);
        assert!(
            not_paying(&direct, avg).is_none(),
            "direct can't be acted on"
        );
    }

    #[test]
    fn sentences_follow_the_numbers() {
        let cur = Totals {
            visitors: 852,
            revenue_usd: 1467.0,
            new_customers: 38,
            matched_payments: 30,
            revenue_per_visitor: Some(1.72),
            ..Totals::default()
        };
        let prev = Totals {
            visitors: 906,
            revenue_usd: 1310.0,
            new_customers: 35,
            revenue_per_visitor: Some(1.45),
            ..Totals::default()
        };
        let channels = [
            row("AI", 206, 627.0),
            row("Organic search", 306, 470.0),
            row("Direct", 210, 205.0),
        ];
        assert_eq!(
            headline(&cur, &prev, &channels),
            "Revenue rose 12% to $1,467. AI assistants sent 24% of visitors and 43% of the money."
        );
        assert_eq!(
            subline(&sources(&design_sources())),
            "ChatGPT was your top revenue source this week, ahead of Google, with under half the visitors."
        );
        let k = kpis(&cur, &prev);
        assert_eq!((k[0].change.as_str(), k[0].tone), ("+12%", "up"));
        assert_eq!(k[0].note, "from $1,310");
        assert_eq!(
            (k[1].change.as_str(), k[1].note.as_str()),
            ("+9%", "from 35")
        );
        assert_eq!(
            (k[2].value.as_str(), k[2].change.as_str()),
            ("$1.72", "+19%")
        );
        assert_eq!((k[3].change.as_str(), k[3].tone), ("-6%", "quiet"));
        assert_eq!(k[3].note, "fewer visitors, more money");
        let flat = Totals::default();
        assert_eq!(
            headline(&flat, &flat, &[]),
            "No visits or payments this week."
        );
        let k = kpis(&flat, &flat);
        assert_eq!(k[0].note, "none the week before");
        assert_eq!(k[2].note, "no visitors this week");
    }

    #[test]
    fn chart_names_the_best_day() {
        let monday = date!(2026 - 10 - 05);
        let day = |i: i64, visitors: u64, revenue: f64, payments: u64| DayPoint {
            date: monday + Duration::days(i),
            visitors,
            payments,
            customers: payments,
            revenue_usd: revenue,
            converted: 0,
            matched_revenue_usd: 0.0,
            conversion: None,
            revenue_per_visitor: None,
            hour: None,
            by_channel: Vec::new(),
        };
        let days = [
            day(0, 130, 73.0, 2),
            day(1, 140, 191.0, 4),
            day(2, 100, 0.0, 0),
            day(3, 70, 417.0, 9),
            day(4, 120, 0.0, 0),
            day(5, 110, 0.0, 0),
            day(6, 140, 217.0, 5),
        ];
        let (svg, bars, sentence) = chart(&days, monday, "#E8471C");
        assert_eq!(
            sentence,
            "Thursday was the best day: $417 from 9 payments, on the fewest visitors of the week."
        );
        assert!(svg.contains(r#"fill-opacity="1""#) && svg.contains(">$0<"));
        assert_eq!(bars.len(), 7);
        assert!(bars[3].best && bars[3].height == 120 && bars[2].height == 0);
        assert_eq!(tint("#E8471C", 0.32), "#F8C4B6");
        assert_eq!(initials("Fieldwork Studio"), "FS");
    }
}
