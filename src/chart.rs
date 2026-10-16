//! Server-side SVG charts. The plot is a stretched 1000x1000 SVG; labels, markers and tooltips are HTML on top.
use std::fmt::Write;

use time::Date;
use time::macros::format_description;

use crate::dashboard::{group, money};
use crate::stats::DayPoint;

const BLUE: &str = "#2F5BEA";
const ORANGE: &str = "#E8471C";
const INK: &str = "#0E1116";
const NS: &str = r#" vector-effect="non-scaling-stroke""#;

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
            Metric::Revenue => ORANGE,
            Metric::Visitors => BLUE,
            _ => INK,
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
                p.converted += d.converted;
                p.matched_revenue_usd += d.matched_revenue_usd;
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
            p.conversion = visitors.map(|v| p.converted as f64 / v);
            p.revenue_per_visitor = visitors.map(|v| p.matched_revenue_usd / v);
            p
        })
        .collect();
    Series {
        points,
        weekly: true,
    }
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

fn column(left: f64, width: f64, inner: &str) -> String {
    format!(
        r#"<div class="hov bar" style="left:{:.3}%;width:{:.3}%">{inner}</div>"#,
        left * 100.0,
        width * 100.0
    )
}

fn marker(yf: f64, color: &str) -> String {
    format!(
        r#"<span class="mk" style="top:{:.2}%;--c:{color}"></span>"#,
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
            r#"<rect x="{:.1}" y="{:.1}" width="{bw:.1}" height="{h:.1}" fill="{BLUE}"/>"#,
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
                swatch: swatch(BLUE, ""),
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
        let inner = marker(p.revenue_usd / rtop, ORANGE)
            + &tip(xf, &tip_date(p.date, cur.weekly), &rows, &top_channel(p));
        cols.push_str(&column(i as f64 / n as f64, 1.0 / n as f64, &inner));
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
        swatch(BLUE, ""),
        swatch(ORANGE, "ln")
    )
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
            converted: (revenue > 0.0) as u64,
            matched_revenue_usd: revenue,
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
    fn dual_has_two_axes() {
        let html = dual(&series(&days(7)));
        assert!(html.contains(r#"class="chart dual""#));
        assert!(html.contains(r#"<div class="ya r">"#));
        assert_eq!(count(&html, BLUE), 7 + 7 + 1, "bars, tooltips, legend");
    }
}
