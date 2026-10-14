//! Server-side SVG charts. The plot is a stretched 1000x1000 SVG; labels, markers and tooltips are HTML on top.
use std::fmt::Write;

use time::Date;
use time::macros::format_description;

use crate::dashboard::{group, money};
use crate::stats::DayPoint;

const BLUE: &str = "#2F5BEA";
const BLUE_TINT: &str = "#C9D6FA";
const ORANGE: &str = "#E8471C";
const GREY: &str = "#B9BCC2";
const NS: &str = r#" vector-effect="non-scaling-stroke""#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Line,
    Area,
    Channel,
    Dual,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Line, Kind::Area, Kind::Channel, Kind::Dual];

    pub fn parse(s: &str) -> Option<Self> {
        Kind::ALL.into_iter().find(|k| k.key() == s)
    }

    pub fn key(self) -> &'static str {
        match self {
            Kind::Line => "line",
            Kind::Area => "area",
            Kind::Channel => "channel",
            Kind::Dual => "dual",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Line => "Line",
            Kind::Area => "Area",
            Kind::Channel => "By channel",
            Kind::Dual => "Visitors vs revenue",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Revenue,
    Visitors,
    Conversion,
    PerVisitor,
}

impl Metric {
    pub const ALL: [Metric; 4] = [
        Metric::Revenue,
        Metric::Visitors,
        Metric::Conversion,
        Metric::PerVisitor,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        Metric::ALL.into_iter().find(|m| m.key() == s)
    }

    pub fn key(self) -> &'static str {
        match self {
            Metric::Revenue => "revenue",
            Metric::Visitors => "visitors",
            Metric::Conversion => "conversion",
            Metric::PerVisitor => "rpv",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Metric::Revenue => "Revenue",
            Metric::Visitors => "Visitors",
            Metric::Conversion => "Conversion rate",
            Metric::PerVisitor => "Revenue per visitor",
        }
    }

    pub fn color(self) -> &'static str {
        match self {
            Metric::Visitors => BLUE,
            _ => ORANGE,
        }
    }

    /// Conversion is in percent.
    fn value(self, p: &DayPoint) -> f64 {
        match self {
            Metric::Revenue => p.revenue_usd,
            Metric::Visitors => p.visitors as f64,
            Metric::Conversion => p.conversion.unwrap_or(0.0) * 100.0,
            Metric::PerVisitor => p.revenue_per_visitor.unwrap_or(0.0),
        }
    }

    fn format(self, v: f64) -> String {
        match self {
            Metric::Revenue => money(v),
            Metric::Visitors => group(v.round() as u64),
            Metric::Conversion => format!("{v:.2}%"),
            Metric::PerVisitor => format!("${v:.2}"),
        }
    }

    fn tick(self, t: f64, step: f64) -> String {
        match self {
            Metric::Revenue if step >= 1.0 => format!("${}", group(t.round() as u64)),
            Metric::Revenue | Metric::PerVisitor => format!("${t:.2}"),
            Metric::Visitors => group(t.round() as u64),
            Metric::Conversion => format!("{}%", trim(t)),
        }
    }

    fn integer(self) -> bool {
        matches!(self, Metric::Revenue | Metric::Visitors)
    }
}

/// Points to plot: days, or weeks when the range is longer than 120 days.
pub struct Series {
    pub points: Vec<DayPoint>,
    pub weekly: bool,
}

pub fn series(days: &[DayPoint]) -> Series {
    if days.len() <= 120 {
        return Series {
            points: days.to_vec(),
            weekly: false,
        };
    }
    let points = days
        .chunks(7)
        .map(|week| {
            let mut p = week[0].clone();
            for d in &week[1..] {
                p.visitors += d.visitors;
                p.payments += d.payments;
                p.customers += d.customers;
                p.revenue_usd += d.revenue_usd;
                for (channel, revenue) in &d.by_channel {
                    match p.by_channel.iter_mut().find(|c| c.0 == *channel) {
                        Some(c) => c.1 += revenue,
                        None => p.by_channel.push((channel.clone(), *revenue)),
                    }
                }
            }
            p.by_channel
                .sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let visitors = (p.visitors > 0).then_some(p.visitors as f64);
            p.conversion = visitors.map(|v| p.customers as f64 / v);
            p.revenue_per_visitor = visitors.map(|v| p.revenue_usd / v);
            p
        })
        .collect();
    Series {
        points,
        weekly: true,
    }
}

pub fn channel_color(channel: Option<&str>) -> &'static str {
    match channel {
        Some("Organic search") => BLUE,
        Some("Referral") => ORANGE,
        Some("Direct") => "#1BAF7A",
        Some("Social") => "#EDA100",
        Some("Email") => "#E87BA4",
        Some("Paid") => "#7A5AF8",
        Some("AI") => "#0E9AA7",
        _ => GREY,
    }
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// "$1,234.50"
fn cents(v: f64) -> String {
    let c = (v * 100.0).round() as u64;
    format!("${}.{:02}", group(c / 100), c % 100)
}

fn trim(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Step and top of four even gridlines that cover `vmax`.
pub fn nice_ticks(vmax: f64, integer: bool) -> (f64, f64) {
    let raw = vmax / 4.0;
    let mut mag = if raw > 0.0 {
        10f64.powf(raw.log10().floor())
    } else {
        1.0
    };
    if integer {
        mag = mag.max(1.0);
    }
    let mut step = 10.0 * mag;
    for m in [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0] {
        let s = m * mag;
        if integer && s.fract() != 0.0 {
            continue;
        }
        if s * 4.0 >= vmax {
            step = s;
            break;
        }
    }
    (step, step * 4.0)
}

fn short(d: Date) -> String {
    d.format(format_description!("[month repr:short] [day padding:none]"))
        .unwrap_or_default()
}

fn tip_date(d: Date, weekly: bool) -> String {
    if weekly {
        format!("Week of {}", short(d))
    } else {
        d.format(format_description!(
            "[month repr:short] [day padding:none], [weekday repr:short]"
        ))
        .unwrap_or_default()
    }
}

/// Up to five evenly spaced label indexes.
fn label_indexes(n: usize) -> Vec<usize> {
    let mut out: Vec<usize> = (0..5)
        .map(|k| ((k * n.saturating_sub(1)) as f64 / 4.0).round() as usize)
        .collect();
    out.dedup();
    out
}

fn grid(top: f64, step: f64) -> String {
    let mut s = String::new();
    for k in 1..=4 {
        let y = 1000.0 - k as f64 * step / top * 1000.0;
        let _ = write!(s, r##"<path d="M0,{y:.1}H1000" stroke="#EFEFEB"{NS}/>"##);
    }
    let _ = write!(s, r##"<path d="M0,1000H1000" stroke="#D6D7D1"{NS}/>"##);
    s
}

fn axis(top: f64, step: f64, label: impl Fn(f64) -> String) -> String {
    (0..=4)
        .map(|k| {
            let t = k as f64 * step;
            format!(
                r#"<span style="top:{:.2}%">{}</span>"#,
                (1.0 - t / top) * 100.0,
                esc(&label(t))
            )
        })
        .collect()
}

fn x_axis(points: &[DayPoint], centered: bool) -> String {
    let n = points.len();
    label_indexes(n)
        .into_iter()
        .map(|i| {
            let f = if centered {
                (i as f64 + 0.5) / n as f64
            } else if n > 1 {
                i as f64 / (n - 1) as f64
            } else {
                0.5
            };
            let edge = if f <= 0.001 {
                " class=\"s\""
            } else if f >= 0.999 {
                " class=\"e\""
            } else {
                ""
            };
            format!(
                r#"<span{edge} style="left:{:.2}%">{}</span>"#,
                f * 100.0,
                short(points[i].date)
            )
        })
        .collect()
}

struct Row {
    swatch: String,
    name: String,
    value: String,
}

fn swatch(color: &str, kind: &str) -> String {
    format!(r#"<i class="sw {kind}" style="--c:{color}"></i>"#)
}

fn tip(xf: f64, date: &str, rows: &[Row], foot: &str) -> String {
    let side = if xf < 0.6 { "" } else { " l" };
    let mut s = format!(
        r#"<div class="tip{side}"><span class="d">{}</span>"#,
        esc(date)
    );
    for r in rows {
        let _ = write!(
            s,
            r#"<div class="row"><span>{}{}</span><b>{}</b></div>"#,
            r.swatch,
            r.name,
            esc(&r.value)
        );
    }
    if !foot.is_empty() {
        let _ = write!(s, r#"<div class="foot">{}</div>"#, esc(foot));
    }
    s.push_str("</div>");
    s
}

fn column(left: f64, width: f64, bar: bool, inner: &str) -> String {
    format!(
        r#"<div class="hov{}" style="left:{:.3}%;width:{:.3}%">{inner}</div>"#,
        if bar { " bar" } else { "" },
        left * 100.0,
        width * 100.0
    )
}

fn marker(yf: f64, color: &str, open: bool) -> String {
    format!(
        r#"<span class="mk{}" style="top:{:.2}%;--c:{color}"></span>"#,
        if open { " o" } else { "" },
        (1.0 - yf) * 100.0
    )
}

fn frame(
    class: &str,
    label: &str,
    left: &str,
    right: &str,
    svg: &str,
    cols: &str,
    xa: &str,
) -> String {
    let right = if right.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="ya r">{right}</div>"#)
    };
    format!(
        r#"<div class="chart {class}" role="img" aria-label="{}"><div class="ya">{left}</div>{right}<div class="plot"><svg viewBox="0 0 1000 1000" preserveAspectRatio="none" aria-hidden="true">{svg}</svg>{cols}</div><div class="xa">{xa}</div></div>"#,
        esc(label)
    )
}

fn top_channel(p: &DayPoint) -> String {
    let n = p.payments;
    let count = format!("{n} payment{}", if n == 1 { "" } else { "s" });
    match p.by_channel.first() {
        Some((Some(c), _)) if n > 1 => format!("{count}, most from {c}"),
        Some((Some(c), _)) => format!("{count} from {c}"),
        _ if n > 0 => count,
        _ => String::new(),
    }
}

fn unit(s: &Series) -> &'static str {
    if s.weekly { "week" } else { "day" }
}

/// Line or area chart of one metric, with the previous period dashed on line charts.
pub fn line(cur: &Series, prev: Option<&Series>, metric: Metric, area: bool, span: &str) -> String {
    let pts = &cur.points;
    let n = pts.len();
    let vals: Vec<f64> = pts.iter().map(|p| metric.value(p)).collect();
    let prev_pts: &[DayPoint] = match prev {
        Some(s) if !area => &s.points,
        _ => &[],
    };
    let pvals: Vec<f64> = prev_pts.iter().take(n).map(|p| metric.value(p)).collect();
    let max = vals.iter().chain(&pvals).fold(0.0, |a: f64, b| a.max(*b));
    let (step, top) = nice_ticks(max * 1.05, metric.integer());
    let xf = |i: usize| {
        if n > 1 {
            i as f64 / (n - 1) as f64
        } else {
            0.5
        }
    };
    let path = |vs: &[f64]| {
        vs.iter()
            .enumerate()
            .map(|(i, v)| {
                format!(
                    "{}{:.1},{:.1}",
                    if i == 0 { "M" } else { "L" },
                    xf(i) * 1000.0,
                    1000.0 - v / top * 1000.0
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let color = metric.color();
    let mut svg = grid(top, step);
    let d = path(&vals);
    if area && n > 0 {
        let _ = write!(
            svg,
            r#"<path d="{d} L{:.1},1000 L{:.1},1000 Z" fill="{color}" fill-opacity="0.10"/>"#,
            xf(n - 1) * 1000.0,
            xf(0) * 1000.0
        );
    }
    if !pvals.is_empty() {
        let _ = write!(
            svg,
            r#"<path d="{}" fill="none" stroke="{GREY}" stroke-width="1.5" stroke-dasharray="4 4"{NS}/>"#,
            path(&pvals)
        );
    }
    let _ = write!(
        svg,
        r#"<path d="{d}" fill="none" stroke="{color}" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"{NS}/>"#
    );
    let width = if n > 1 { 1.0 / (n - 1) as f64 } else { 1.0 };
    let mut cols = String::new();
    for (i, p) in pts.iter().enumerate() {
        let x = xf(i);
        let mut inner = String::from(r#"<i class="cross"></i>"#);
        let mut rows = vec![Row {
            swatch: swatch(color, "ln"),
            name: "This period".into(),
            value: metric.format(vals[i]),
        }];
        if let Some(pv) = pvals.get(i) {
            inner.push_str(&marker(pv / top, "#8A909A", true));
            rows.push(Row {
                swatch: swatch(GREY, "dash"),
                name: esc(&tip_date(prev_pts[i].date, cur.weekly)),
                value: metric.format(*pv),
            });
        }
        inner.push_str(&marker(vals[i] / top, color, false));
        let foot = if metric == Metric::Revenue {
            top_channel(p)
        } else {
            String::new()
        };
        inner.push_str(&tip(x, &tip_date(p.date, cur.weekly), &rows, &foot));
        cols.push_str(&column(x - width / 2.0, width, false, &inner));
    }
    let chart = frame(
        "",
        &format!("{} per {}", metric.name(), unit(cur)),
        &axis(top, step, |t| metric.tick(t, step)),
        "",
        &svg,
        &cols,
        &x_axis(pts, false),
    );
    let mut legend = format!(
        r#"<div class="legend"><span>{}{}, {}</span>"#,
        swatch(color, "ln"),
        metric.name(),
        esc(span)
    );
    if !pvals.is_empty() {
        let _ = write!(
            legend,
            r#"<span>{}Previous {} {}s</span>"#,
            swatch(GREY, "dash"),
            prev_pts.len(),
            unit(cur)
        );
    }
    legend.push_str("</div>");
    chart + &legend
}

/// Channels in a stable order: largest total first, Unattributed last.
fn channel_order(pts: &[DayPoint]) -> Vec<(Option<String>, f64)> {
    let mut totals: Vec<(Option<String>, f64)> = Vec::new();
    for p in pts {
        for (c, r) in &p.by_channel {
            match totals.iter_mut().find(|t| t.0 == *c) {
                Some(t) => t.1 += r,
                None => totals.push((c.clone(), *r)),
            }
        }
    }
    totals.sort_by(|a, b| {
        a.0.is_none()
            .cmp(&b.0.is_none())
            .then(b.1.total_cmp(&a.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    totals
}

/// Revenue per day stacked by credited channel.
pub fn stacked(cur: &Series) -> String {
    let pts = &cur.points;
    let n = pts.len().max(1);
    let order = channel_order(pts);
    let max = pts.iter().map(|p| p.revenue_usd).fold(0.0, f64::max);
    let (step, top) = nice_ticks(max * 1.05, true);
    let w = 1000.0 / n as f64;
    let bw = w * 0.62;
    let gap = 7.0;
    let mut svg = grid(top, step);
    let mut cols = String::new();
    for (i, p) in pts.iter().enumerate() {
        let x = w * i as f64 + (w - bw) / 2.0;
        let mut base = 1000.0;
        let mut rows = Vec::new();
        for (c, _) in &order {
            let Some((_, r)) = p.by_channel.iter().find(|b| b.0 == *c) else {
                continue;
            };
            let color = channel_color(c.as_deref());
            let h = r / top * 1000.0;
            if h - gap > 0.5 {
                let _ = write!(
                    svg,
                    r#"<rect x="{x:.1}" y="{:.1}" width="{bw:.1}" height="{:.1}" fill="{color}"/>"#,
                    base - h + gap,
                    h - gap
                );
            }
            base -= h;
            rows.push((
                *r,
                Row {
                    swatch: swatch(color, ""),
                    name: esc(c.as_deref().unwrap_or("Unattributed")),
                    value: cents(*r),
                },
            ));
        }
        rows.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut rows: Vec<Row> = rows.into_iter().map(|r| r.1).collect();
        rows.push(Row {
            swatch: String::new(),
            name: "<b>Total</b>".into(),
            value: cents(p.revenue_usd),
        });
        let xf = (i as f64 + 0.5) / n as f64;
        let inner = tip(xf, &tip_date(p.date, cur.weekly), &rows, "");
        cols.push_str(&column(i as f64 / n as f64, 1.0 / n as f64, true, &inner));
    }
    let chart = frame(
        "",
        &format!("Revenue per {} by channel", unit(cur)),
        &axis(top, step, |t| Metric::Revenue.tick(t, step)),
        "",
        &svg,
        &cols,
        &x_axis(pts, true),
    );
    let legend: String = order
        .iter()
        .map(|(c, _)| {
            format!(
                "<span>{}{}</span>",
                swatch(channel_color(c.as_deref()), ""),
                esc(c.as_deref().unwrap_or("Unattributed"))
            )
        })
        .collect();
    format!(r#"{chart}<div class="legend">{legend}</div>"#)
}

/// Visitor bars on the left axis, revenue line on the right axis.
pub fn dual(cur: &Series) -> String {
    let pts = &cur.points;
    let n = pts.len().max(1);
    let vmax = pts.iter().map(|p| p.visitors as f64).fold(0.0, f64::max);
    let rmax = pts.iter().map(|p| p.revenue_usd).fold(0.0, f64::max);
    let (vstep, vtop) = nice_ticks(vmax * 1.05, true);
    let (rstep, rtop) = nice_ticks(rmax * 1.05, true);
    let w = 1000.0 / n as f64;
    let bw = w * 0.56;
    let mut svg = grid(vtop, vstep);
    for (i, p) in pts.iter().enumerate() {
        let h = p.visitors as f64 / vtop * 1000.0;
        let _ = write!(
            svg,
            r#"<rect x="{:.1}" y="{:.1}" width="{bw:.1}" height="{h:.1}" fill="{BLUE_TINT}"/>"#,
            w * i as f64 + (w - bw) / 2.0,
            1000.0 - h
        );
    }
    let d = pts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "{}{:.1},{:.1}",
                if i == 0 { "M" } else { "L" },
                w * (i as f64 + 0.5),
                1000.0 - p.revenue_usd / rtop * 1000.0
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let _ = write!(
        svg,
        r##"<path d="{d}" fill="none" stroke="#FFFFFF" stroke-width="5" stroke-linejoin="round"{NS}/><path d="{d}" fill="none" stroke="{ORANGE}" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"{NS}/>"##
    );
    let mut cols = String::new();
    for (i, p) in pts.iter().enumerate() {
        let xf = (i as f64 + 0.5) / n as f64;
        let rows = [
            Row {
                swatch: swatch(BLUE_TINT, ""),
                name: "Visitors".into(),
                value: group(p.visitors),
            },
            Row {
                swatch: swatch(ORANGE, "ln"),
                name: "Revenue".into(),
                value: money(p.revenue_usd),
            },
            Row {
                swatch: String::new(),
                name: "Per visitor".into(),
                value: format!("${:.2}", p.revenue_per_visitor.unwrap_or(0.0)),
            },
        ];
        let inner = marker(p.revenue_usd / rtop, ORANGE, false)
            + &tip(xf, &tip_date(p.date, cur.weekly), &rows, &top_channel(p));
        cols.push_str(&column(i as f64 / n as f64, 1.0 / n as f64, true, &inner));
    }
    let chart = frame(
        "dual",
        &format!("Visitors and revenue per {}", unit(cur)),
        &axis(vtop, vstep, |t| group(t.round() as u64)),
        &axis(rtop, rstep, |t| Metric::Revenue.tick(t, rstep)),
        &svg,
        &cols,
        &x_axis(pts, true),
    );
    format!(
        r#"{chart}<div class="legend"><span>{}Visitors, left axis</span><span>{}Revenue, right axis</span></div>"#,
        swatch(BLUE_TINT, ""),
        swatch(ORANGE, "ln")
    )
}

/// Visitor bars for a new site, with days before tracking started shaded and payment pills above their day.
pub fn sparse(cur: &Series, since: Option<Date>, today: Date, pills: &[(Date, String)]) -> String {
    let pts = &cur.points;
    let n = pts.len().max(1);
    let few = pts.len() <= 14;
    let vmax = pts.iter().map(|p| p.visitors as f64).fold(0.0, f64::max);
    let (step, top) = nice_ticks(vmax * 1.05, true);
    let w = 1000.0 / n as f64;
    let bw = w * 0.42;
    let before = since.map_or(pts.len(), |s| pts.iter().filter(|p| p.date < s).count());
    let mut svg = String::new();
    let mut over = String::new();
    if before > 0 {
        let _ = write!(
            svg,
            r##"<rect x="0" y="0" width="{:.1}" height="1000" fill="#F6F6F3"/>"##,
            w * before as f64
        );
    }
    svg.push_str(&grid(top, step));
    if before > 0 && before < pts.len() {
        let x = w * before as f64;
        let _ = write!(
            svg,
            r##"<path d="M{x:.1},-40V1000" stroke="#0E1116" stroke-opacity="0.35" stroke-dasharray="3 3"{NS}/>"##
        );
        let _ = write!(
            over,
            r#"<span class="first" style="left:{:.2}%">First visit</span>"#,
            x / 10.0
        );
    }
    if before >= 2 {
        let note = since.map_or_else(
            || "Waiting for the first visit".to_string(),
            |s| format!("Script added {}", short(s)),
        );
        let _ = write!(
            over,
            r#"<div class="before" style="width:{:.2}%"><b>Not tracking yet</b><span>{note}</span></div>"#,
            before as f64 / n as f64 * 100.0
        );
    }
    let mut cols = String::new();
    for (i, p) in pts.iter().enumerate() {
        let xf = (i as f64 + 0.5) / n as f64;
        if i >= before {
            let h = p.visitors as f64 / top * 1000.0;
            let fade = if p.date == today {
                r#" fill-opacity="0.6""#
            } else {
                ""
            };
            let _ = write!(
                svg,
                r#"<rect x="{:.1}" y="{:.1}" width="{bw:.1}" height="{h:.1}" fill="{BLUE_TINT}"{fade}/>"#,
                w * i as f64 + (w - bw) / 2.0,
                1000.0 - h
            );
            let yf = p.visitors as f64 / top * 100.0;
            if few {
                let _ = write!(
                    over,
                    r#"<span class="val" style="left:{:.2}%;bottom:calc({yf:.2}% + 6px)">{}</span>"#,
                    xf * 100.0,
                    group(p.visitors)
                );
                if let Some((_, pill)) = pills.iter().find(|(d, _)| *d == p.date) {
                    let _ = write!(
                        over,
                        r#"<span class="pay" style="left:{:.2}%;bottom:calc({yf:.2}% + 24px)">{pill}</span>"#,
                        xf * 100.0
                    );
                }
            }
            let rows = [
                Row {
                    swatch: swatch(BLUE_TINT, ""),
                    name: "Visitors".into(),
                    value: group(p.visitors),
                },
                Row {
                    swatch: swatch(ORANGE, ""),
                    name: "Revenue".into(),
                    value: money(p.revenue_usd),
                },
            ];
            let inner = tip(xf, &tip_date(p.date, cur.weekly), &rows, &top_channel(p));
            cols.push_str(&column(i as f64 / n as f64, 1.0 / n as f64, true, &inner));
        }
    }
    let xa = if few {
        pts.iter()
            .enumerate()
            .map(|(i, p)| {
                let text = if p.date == today {
                    "Today".to_string()
                } else {
                    short(p.date)
                };
                format!(
                    r#"<span style="left:{:.2}%">{text}</span>"#,
                    (i as f64 + 0.5) / n as f64 * 100.0
                )
            })
            .collect()
    } else {
        x_axis(pts, true)
    };
    let chart = frame(
        "sparse",
        &format!("Visitors and payments per {}", unit(cur)),
        &axis(top, step, |t| group(t.round() as u64)),
        "",
        &svg,
        &(over + &cols),
        &xa,
    );
    let mut legend = format!(
        r#"<div class="legend"><span>{}Visitors</span><span>{}Payment</span>"#,
        swatch(BLUE_TINT, ""),
        swatch(ORANGE, "")
    );
    if pts.last().is_some_and(|p| p.date == today) {
        legend.push_str(r#"<span class="quiet">Today is still counting</span>"#);
    }
    legend.push_str("</div>");
    chart + &legend
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;

    fn day(i: i64, visitors: u64, revenue: f64) -> DayPoint {
        DayPoint {
            date: date!(2026 - 09 - 15) + time::Duration::days(i),
            visitors,
            payments: (revenue > 0.0) as u64,
            customers: (revenue > 0.0) as u64,
            revenue_usd: revenue,
            conversion: (visitors > 0).then(|| (revenue > 0.0) as u64 as f64 / visitors as f64),
            revenue_per_visitor: (visitors > 0).then(|| revenue / visitors as f64),
            by_channel: if revenue > 0.0 {
                vec![
                    (Some("Referral".into()), revenue * 0.75),
                    (None, revenue * 0.25),
                ]
            } else {
                Vec::new()
            },
        }
    }

    fn days(n: i64) -> Vec<DayPoint> {
        (0..n)
            .map(|i| day(i, 100 + i as u64, 10.0 * i as f64))
            .collect()
    }

    fn count(html: &str, needle: &str) -> usize {
        html.matches(needle).count()
    }

    #[test]
    fn ticks_are_round() {
        assert_eq!(nice_ticks(1000.0, true), (250.0, 1000.0));
        assert_eq!(nice_ticks(1234.0, true), (400.0, 1600.0));
        assert_eq!(nice_ticks(0.0, true), (1.0, 4.0));
        assert_eq!(nice_ticks(5.0, true), (2.0, 8.0));
        let (step, top) = nice_ticks(2.1, false);
        assert!((step - 0.6).abs() < 1e-9 && (top - 2.4).abs() < 1e-9);
    }

    #[test]
    fn long_ranges_become_weeks() {
        let d = days(365);
        let s = series(&d);
        assert!(s.weekly);
        assert_eq!(s.points.len(), 53);
        let total: f64 = s.points.iter().map(|p| p.revenue_usd).sum();
        let want: f64 = d.iter().map(|p| p.revenue_usd).sum();
        assert!((total - want).abs() < 1e-6);
        let split: f64 = s.points[1].by_channel.iter().map(|c| c.1).sum();
        assert!((split - s.points[1].revenue_usd).abs() < 1e-6);
        assert!(!series(&days(90)).weekly);
    }

    #[test]
    fn line_compares_with_previous_period() {
        let cur = series(&days(30));
        let prev = series(&days(30));
        let html = line(
            &cur,
            Some(&prev),
            Metric::Revenue,
            false,
            "Sep 15 to Oct 14",
        );
        assert_eq!(count(&html, r#"class="hov""#), 30);
        assert_eq!(count(&html, "stroke-dasharray=\"4 4\""), 1);
        assert!(html.contains("Previous 30 days"));
        assert!(html.contains("Revenue, Sep 15 to Oct 14"));
        assert!(html.contains("1 payment from Referral"));
        assert!(html.contains("Sep 15, Tue"));
        let area = line(&cur, Some(&prev), Metric::Visitors, true, "x");
        assert_eq!(count(&area, "stroke-dasharray"), 0);
        assert!(area.contains(r#"fill-opacity="0.10""#));
        let conv = line(&cur, None, Metric::Conversion, false, "x");
        assert!(conv.contains("0.97%") && conv.contains('%'));
    }

    #[test]
    fn stacked_bars_split_revenue_by_channel() {
        let cur = series(&days(10));
        let html = stacked(&cur);
        assert_eq!(count(&html, "<rect"), 18);
        assert_eq!(count(&html, ORANGE), 9 + 9 + 1, "bars, tooltips, legend");
        let referral = html.find(">Referral</span>").unwrap();
        let unattributed = html.rfind(">Unattributed</span>").unwrap();
        assert!(referral < unattributed, "Unattributed is listed last");
        assert!(html.contains("<b>Total</b>") && html.contains("$67.50"));
        assert_eq!(cents(1234.5), "$1,234.50");
    }

    #[test]
    fn dual_has_two_axes() {
        let html = dual(&series(&days(7)));
        assert!(html.contains(r#"class="chart dual""#));
        assert!(html.contains(r#"<div class="ya r">"#));
        assert_eq!(count(&html, BLUE_TINT), 7 + 7 + 1);
    }

    #[test]
    fn sparse_shades_days_before_tracking() {
        let mut d = days(7);
        d[0].visitors = 0;
        d[1].visitors = 0;
        let today = d[6].date;
        let pills = [(d[3].date, "$49 from <X>".to_string())];
        let html = sparse(&series(&d), Some(d[2].date), today, &pills);
        assert!(html.contains("Not tracking yet") && html.contains("Script added Sep 17"));
        assert!(html.contains("First visit"));
        assert_eq!(count(&html, &format!(r#"fill="{BLUE_TINT}""#)), 5);
        assert!(html.contains(">Today<") && html.contains("Today is still counting"));
        assert_eq!(count(&html, r#"class="pay""#), 1);
    }

    #[test]
    fn labels_are_escaped() {
        let mut d = days(3);
        d[1].by_channel = vec![(Some("<script>".into()), 10.0)];
        let html = stacked(&series(&d));
        assert!(!html.contains("<script>") && html.contains("&lt;script&gt;"));
        assert_eq!(Kind::parse("channel"), Some(Kind::Channel));
        assert_eq!(Metric::parse("rpv"), Some(Metric::PerVisitor));
        assert_eq!(Kind::parse("pie"), None);
    }
}
