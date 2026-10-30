//! Server-side SVG charts. The plot is a stretched 1000x1000 SVG; labels, markers and tooltips are HTML on top.
use std::fmt::Write;

use time::Date;
use time::macros::format_description;

use crate::dashboard::{group, money};
use crate::stats::DayPoint;

const BLUE: &str = "var(--visitors)";
const ORANGE: &str = "var(--revenue)";
const NS: &str = r#" vector-effect="non-scaling-stroke""#;

/// "$1,200" on whole-dollar steps, else "$0.50".
fn tick(t: f64, step: f64) -> String {
    if step >= 1.0 {
        format!("${}", group(t.round() as u64))
    } else {
        format!("${t:.2}")
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
    if !s.contains(['&', '<', '>', '"']) {
        return s.to_string();
    }
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

fn tip_date(p: &DayPoint, weekly: bool) -> String {
    let d = p.date;
    if let Some(h) = p.hour {
        format!("{}, {h:02}:00", short(d))
    } else if weekly {
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
        let _ = write!(
            s,
            r##"<path d="M0,{y:.1}H1000" style="stroke:var(--grid)"{NS}/>"##
        );
    }
    let _ = write!(
        s,
        r##"<path d="M0,1000H1000" style="stroke:var(--axis)"{NS}/>"##
    );
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
    let at = if n == 24 && points[0].hour.is_some() {
        vec![0, 6, 12, 18, 23]
    } else {
        label_indexes(n)
    };
    at.into_iter()
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
                match points[i].hour {
                    Some(h) => format!("{h:02}:00"),
                    None => short(points[i].date),
                }
            )
        })
        .collect()
}

fn swatch(color: &str, kind: &str) -> String {
    format!(r#"<i class="sw {kind}" style="--c:{color}"></i>"#)
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

/// How the dashboard's revenue chart draws a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Revenue bars with a visitors dot each.
    Bars,
    /// Revenue line against the previous period.
    Line,
    /// Revenue bars stacked by credited channel.
    Channel,
    /// Running total against the previous period.
    Total,
}

/// Color of a credited channel; None is Unattributed.
pub fn channel_color(channel: Option<&str>) -> &'static str {
    match channel {
        Some("AI") => BLUE,
        Some("Organic search") => "#F2A541",
        Some("Direct") => "var(--ink-2)",
        Some("Email") => "#1E7A4C",
        Some("Referral") => "#9B6BDB",
        Some("Social") => "#E36F9C",
        Some("Communities") => "#B5622E",
        Some("Paid") => "#0E9AA7",
        Some(_) => "var(--mute)",
        None => "var(--axis)",
    }
}

fn running(points: &[DayPoint]) -> Vec<f64> {
    points
        .iter()
        .scan(0.0, |t, p| {
            *t += p.revenue_usd;
            Some(*t)
        })
        .collect()
}

/// The dashboard's revenue chart and its legend. `prev` lines up the previous period by position,
/// `pre` hatches the first fraction of the plot with a note, `only` names an active filter,
/// `hidden` shows shares of the period's revenue instead of amounts, and `day` links each day or week.
pub fn revenue(
    cur: &Series,
    prev: Option<&Series>,
    kind: Kind,
    pre: Option<(f64, &str)>,
    only: Option<&str>,
    hidden: bool,
    day: Option<&(dyn Fn(Date) -> String + Sync)>,
) -> String {
    let pts = &cur.points;
    let sum: f64 = pts.iter().map(|p| p.revenue_usd).sum();
    let amount = |v: f64| {
        if !hidden {
            money(v)
        } else if sum > 0.0 {
            format!("{:.0}%", v / sum * 100.0)
        } else {
            "0%".into()
        }
    };
    let n = pts.len().max(1);
    let w = 1000.0 / n as f64;
    let total = kind == Kind::Total;
    let vals: Vec<f64> = if total {
        running(pts)
    } else {
        pts.iter().map(|p| p.revenue_usd).collect()
    };
    let prev_vals: Option<Vec<f64>> = prev
        .filter(|_| matches!(kind, Kind::Line | Kind::Total))
        .map(|s| {
            if total {
                running(&s.points)
            } else {
                s.points.iter().map(|p| p.revenue_usd).collect()
            }
        });
    let top_val = vals
        .iter()
        .chain(prev_vals.iter().flatten())
        .copied()
        .fold(0.0, f64::max);
    let (step, top) = nice_ticks(top_val * 1.05, true);
    let vmax = pts.iter().map(|p| p.visitors).max().unwrap_or(0);
    let dual = kind == Kind::Bars && vmax > 0;
    let (vstep, vtop) = nice_ticks(vmax as f64 * 1.05, true);
    let best = vals
        .iter()
        .enumerate()
        .filter(|(_, v)| **v > 0.0)
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i);
    let y = |v: f64| 1000.0 - v / top * 1000.0;
    let mut svg = grid(top, step);
    let bw = w * 0.62;
    let x0 = |i: usize| w * i as f64 + (w - bw) / 2.0;
    match kind {
        Kind::Bars => {
            for (i, v) in vals.iter().enumerate() {
                let h = v / top * 1000.0;
                let fill = if Some(i) == best {
                    ORANGE
                } else {
                    "var(--revenue-tint)"
                };
                let _ = write!(
                    svg,
                    r#"<rect x="{:.1}" y="{:.1}" width="{bw:.1}" height="{h:.1}" style="fill:{fill}"/>"#,
                    x0(i),
                    1000.0 - h
                );
            }
        }
        Kind::Channel => {
            for (i, p) in pts.iter().enumerate() {
                let mut base = 1000.0;
                for (c, v) in &p.by_channel {
                    let h = v.max(0.0) / top * 1000.0;
                    base -= h;
                    let fill = if only.is_some() {
                        ORANGE
                    } else {
                        channel_color(c.as_deref())
                    };
                    let _ = write!(
                        svg,
                        r#"<rect x="{:.1}" y="{base:.1}" width="{bw:.1}" height="{h:.1}" style="fill:{fill}"/>"#,
                        x0(i)
                    );
                }
            }
        }
        Kind::Line | Kind::Total => {
            let path = |v: &[f64]| {
                v.iter()
                    .enumerate()
                    .map(|(i, v)| {
                        format!(
                            "{}{:.1},{:.1}",
                            if i == 0 { "M" } else { "L" },
                            w * (i as f64 + 0.5),
                            y(*v)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let d = path(&vals);
            if top_val > 0.0 {
                let _ = write!(
                    svg,
                    r##"<path d="{d} L{:.1},1000 L{:.1},1000 Z" style="fill:var(--revenue-soft)"/>"##,
                    w * (n as f64 - 0.5),
                    w * 0.5
                );
            }
            if let Some(pv) = &prev_vals {
                let _ = write!(
                    svg,
                    r##"<path d="{}" fill="none" style="stroke:var(--ink-4)" stroke-width="2" stroke-dasharray="5 5"{NS}/>"##,
                    path(&pv[..pv.len().min(n)])
                );
            }
            let _ = write!(
                svg,
                r#"<path d="{d}" fill="none" style="stroke:{ORANGE}" stroke-width="2.5" stroke-linejoin="round"{NS}/>"#
            );
        }
    }
    let unit = if pts.first().is_some_and(|p| p.hour.is_some()) {
        "hour"
    } else if cur.weekly {
        "week"
    } else {
        "day"
    };
    let mut cols = String::with_capacity(pts.len() * 600);
    if let Some((f, label)) = pre {
        let _ = write!(
            cols,
            r#"<div class="pre" style="width:{:.2}%"><span>{}</span></div>"#,
            f * 100.0,
            esc(label)
        );
    }
    let row = |s: &mut String, swatch: &str, name: &str, value: &str| {
        let _ = write!(
            s,
            r#"<div class="row"><span>{swatch}{name}</span><b>{}</b></div>"#,
            esc(value)
        );
    };
    for (i, p) in pts.iter().enumerate() {
        let xf = (i as f64 + 0.5) / n as f64;
        let link = day.filter(|_| p.hour.is_none()).map(|f| f(p.date));
        let (tag, attr) = match &link {
            Some(h) => (
                "a",
                format!(
                    r#" href="{}" aria-label="Show {}""#,
                    esc(h),
                    esc(&tip_date(p, cur.weekly))
                ),
            ),
            None => ("div", String::new()),
        };
        let _ = write!(
            cols,
            r#"<{tag} class="hov bar"{attr} style="left:{:.3}%;width:{:.3}%"><div class="tip{}"><span class="d">{}</span>"#,
            i as f64 / n as f64 * 100.0,
            1.0 / n as f64 * 100.0,
            if xf < 0.6 { "" } else { " l" },
            esc(&tip_date(p, cur.weekly))
        );
        row(
            &mut cols,
            &swatch(ORANGE, if total { "ln" } else { "" }),
            if total { "So far" } else { "Revenue" },
            &amount(vals[i]),
        );
        if let Some(pv) = prev_vals.as_ref().and_then(|v| v.get(i)) {
            row(
                &mut cols,
                &swatch("var(--ink-4)", "ln"),
                "Previous period",
                &amount(*pv),
            );
        }
        if kind == Kind::Channel && only.is_none() {
            for (c, v) in p.by_channel.iter().take(3) {
                row(
                    &mut cols,
                    &swatch(channel_color(c.as_deref()), ""),
                    &esc(c.as_deref().unwrap_or("Unmatched")),
                    &amount(*v),
                );
            }
        }
        if kind == Kind::Bars {
            row(
                &mut cols,
                r#"<i class="sw vd"></i>"#,
                "Visitors",
                &group(p.visitors),
            );
        }
        let foot = top_channel(p);
        if !foot.is_empty() {
            let _ = write!(cols, r#"<div class="foot">{}</div>"#, esc(&foot));
        }
        cols.push_str("</div>");
        match kind {
            Kind::Bars if p.visitors > 0 => {
                let _ = write!(
                    cols,
                    r#"<span class="vd" style="bottom:{:.1}%"></span>"#,
                    p.visitors as f64 / vtop * 100.0
                );
            }
            Kind::Line | Kind::Total => {
                cols.push_str(&marker(vals[i] / top, ORANGE));
            }
            _ => {}
        }
        let _ = write!(cols, "</{tag}>");
    }
    if matches!(kind, Kind::Line | Kind::Total) && top_val > 0.0 {
        let last = *vals.last().unwrap_or(&0.0);
        let _ = write!(
            cols,
            r#"<span class="end" style="left:{:.2}%;bottom:{:.2}%"></span>"#,
            (n as f64 - 0.5) / n as f64 * 100.0,
            last / top * 100.0
        );
        if total {
            let _ = write!(
                cols,
                r#"<span class="end-l" style="bottom:calc({:.2}% + 12px)">{} so far</span>"#,
                last / top * 100.0,
                amount(last)
            );
        }
    }
    let mut legend: Vec<String> = match kind {
        Kind::Bars => vec![
            format!("{}Revenue", swatch(ORANGE, "")),
            r#"<i class="sw vd"></i>Visitors"#.to_string(),
        ],
        Kind::Channel => match only {
            Some(name) => vec![format!("{}{}", swatch(ORANGE, ""), esc(name))],
            None => {
                let mut seen: Vec<(Option<String>, f64)> = Vec::new();
                for (c, v) in pts.iter().flat_map(|p| &p.by_channel) {
                    match seen.iter_mut().find(|s| s.0 == *c) {
                        Some(s) => s.1 += v,
                        None => seen.push((c.clone(), *v)),
                    }
                }
                seen.sort_by(|a, b| b.1.total_cmp(&a.1));
                seen.iter()
                    .map(|(c, _)| {
                        format!(
                            "{}{}",
                            swatch(channel_color(c.as_deref()), ""),
                            esc(c.as_deref().unwrap_or("Unmatched"))
                        )
                    })
                    .collect()
            }
        },
        Kind::Line | Kind::Total => vec![format!("{}This period", swatch(ORANGE, "ln"))],
    };
    if prev_vals.is_some() {
        legend.push(format!(
            r#"<i class="sw ln dash"></i>Previous {}"#,
            if pts.len() == 1 {
                unit.to_string()
            } else {
                format!("{} {unit}s", pts.len())
            }
        ));
    }
    let chart = frame(
        &format!(
            "rev {}{}",
            if kind == Kind::Bars { "bars" } else { "" },
            if dual { " dual" } else { "" }
        ),
        &format!("Revenue per {unit}"),
        &axis(top, step, |t| {
            if hidden { String::new() } else { tick(t, step) }
        }),
        &if dual {
            axis(vtop, vstep, |t| group(t as u64))
        } else {
            String::new()
        },
        &svg,
        &cols,
        &x_axis(pts, true),
    );
    format!(
        r#"<div class="legend">{}</div>{chart}"#,
        legend
            .iter()
            .map(|l| format!("<span>{l}</span>"))
            .collect::<String>()
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
            hour: None,
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
    fn hourly_ticks_are_even() {
        let pts: Vec<DayPoint> = (0..24)
            .map(|h| DayPoint {
                hour: Some(h),
                ..day(0, 1, 0.0)
            })
            .collect();
        let axis = x_axis(&pts, true);
        for want in ["00:00", "06:00", "12:00", "18:00", "23:00"] {
            assert!(axis.contains(want), "{want}");
        }
        assert!(!axis.contains("17:00"));
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
    fn visitors_have_their_own_axis() {
        let s = Series {
            points: vec![day(0, 3, 0.0), day(1, 1, 0.0), day(2, 0, 0.0)],
            weekly: false,
        };
        let bars = revenue(&s, None, Kind::Bars, None, None, false, None);
        assert!(bars.contains(r#"class="chart rev bars dual""#));
        assert!(bars.contains(r#"<div class="ya r">"#));
        // Three visitors on a 0..4 count axis, not on the dollar axis.
        assert!(bars.contains(r#"<span class="vd" style="bottom:75.0%">"#));
        assert!(bars.contains(r#"<span style="top:25.00%">3</span>"#));
        assert_eq!(count(&bars, r#"<i class="sw vd"></i>Visitors"#), 3 + 1);
        let line = revenue(&s, None, Kind::Line, None, None, false, None);
        assert!(!line.contains("ya r") && !line.contains("dual"));
        let empty = Series {
            points: vec![day(0, 0, 0.0)],
            weekly: false,
        };
        assert!(!revenue(&empty, None, Kind::Bars, None, None, false, None).contains("dual"));
    }

    #[test]
    fn revenue_kinds() {
        let d = days(7);
        let s = series(&d);
        let bars = revenue(&s, None, Kind::Bars, None, None, false, None);
        assert_eq!(count(&bars, r#"class="vd""#), 7);
        assert_eq!(
            count(&bars, ORANGE),
            1 + 7 + 1,
            "best bar, tooltips, legend"
        );
        let line = revenue(&s, Some(&s), Kind::Line, None, None, false, None);
        assert!(line.contains("stroke-dasharray") && line.contains("Previous 7 days"));
        let total = revenue(
            &s,
            None,
            Kind::Total,
            Some((0.5, "Not tracking yet")),
            None,
            false,
            None,
        );
        assert!(total.contains("$210 so far") && total.contains(r#"class="pre""#));
        let split = revenue(&s, None, Kind::Channel, None, None, false, None);
        assert!(split.contains("Referral") && split.contains("Unmatched"));
        let hidden = revenue(&s, None, Kind::Total, None, None, true, None);
        assert!(hidden.contains("100% so far") && !hidden.contains('$'));
        let link = |d: Date| format!("?from={d}");
        let linked = revenue(&s, None, Kind::Bars, None, None, false, Some(&link));
        assert_eq!(count(&linked, r#"<a class="hov bar" href="?from="#), 7);
        assert_eq!(count(&bars, "<a "), 0);
    }
}
