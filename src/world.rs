//! A server-rendered world map, each country shaded by a value.

use std::fmt::Write;

use crate::chart::esc;

const SHAPES: &str = include_str!("../assets/world.tsv");

/// (country code, value, hover text) to an SVG; blue for visitors, orange for money.
pub fn map(values: &[(String, f64, String)], visitors: bool) -> String {
    let max = values.iter().map(|v| v.1).fold(0.0, f64::max);
    let (r, g, b) = if visitors {
        (47, 91, 234)
    } else {
        (232, 71, 28)
    };
    let mut svg = String::from(
        r#"<svg class="world" viewBox="0 0 1000 395" role="img" aria-label="World map">"#,
    );
    for line in SHAPES.lines().filter(|l| !l.starts_with('#')) {
        let Some((code, d)) = line.split_once('\t') else {
            continue;
        };
        let hit = values.iter().find(|v| v.0 == code);
        let fill = match hit {
            Some(v) if max > 0.0 && v.1 > 0.0 => {
                let t = 0.15 + 0.85 * (v.1 / max).sqrt();
                format!("rgba({r},{g},{b},{t:.2})")
            }
            _ => "#ECECE7".into(),
        };
        let _ = write!(svg, r#"<path d="{d}" fill="{fill}">"#);
        if let Some(v) = hit {
            let _ = write!(svg, "<title>{}</title>", esc(&v.2));
        }
        svg.push_str("</path>");
    }
    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shades_known_countries_and_titles_them() {
        let svg = map(&[("DE".into(), 10.0, "Germany: 10".into())], true);
        assert!(svg.contains("<title>Germany: 10</title>"));
        assert!(svg.contains("rgba(47,91,234,1.00)"));
        assert_eq!(svg.matches("<path").count(), SHAPES.lines().count() - 1);
    }
}
