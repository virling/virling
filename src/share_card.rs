//! A share card: revenue by channel for a range, drawn as an SVG image the owner can post.

use std::fmt::Write;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::Account;
use crate::chart::esc;
use crate::dashboard::daterange::span;
use crate::dashboard::{RangeArgs, RangeMenu, Shell, blocking, load_ranged, money};
use crate::onboarding::render;
use crate::sites::app_path;
use crate::stats::{BreakdownRow, Dimension};

/// Channels drawn before the rest are summed as Other.
const SHOWN: usize = 5;

#[derive(Deserialize, Default)]
pub struct CardQuery {
    range: Option<String>,
    from: Option<String>,
    to: Option<String>,
    /// Present once the options form was sent, so an unticked box means off.
    set: Option<String>,
    amounts: Option<String>,
    site: Option<String>,
    dark: Option<String>,
}

/// What the owner chose to show.
struct Options {
    amounts: bool,
    site: bool,
    dark: bool,
}

impl CardQuery {
    fn options(&self) -> Options {
        let sent = self.set.is_some();
        Options {
            amounts: self.amounts.is_some(),
            site: !sent || self.site.is_some(),
            dark: self.dark.is_some(),
        }
    }

    fn args(&self) -> RangeArgs {
        RangeArgs {
            range: self.range.clone(),
            from: self.from.clone(),
            to: self.to.clone(),
            compare: Some("off".into()),
        }
    }
}

/// (channel, revenue) with the most revenue first, the tail summed as Other.
fn channels(rows: &[BreakdownRow]) -> Vec<(String, f64)> {
    let mut paid: Vec<(String, f64)> = rows
        .iter()
        .filter(|r| !r.unattributed && r.revenue_usd > 0.0)
        .map(|r| (r.label.clone(), r.revenue_usd))
        .collect();
    paid.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if paid.len() > SHOWN + 1 {
        let rest: f64 = paid[SHOWN..].iter().map(|c| c.1).sum();
        paid.truncate(SHOWN);
        paid.push(("Other".into(), rest));
    }
    paid
}

/// The 1200 by 630 card, sized for social link previews.
fn draw(rows: &[(String, f64)], when: &str, domain: &str, o: &Options) -> String {
    let (paper, ink, dim, rule, bar) = if o.dark {
        ("#191c20", "#eef0f2", "#8e949d", "#2a2e34", "#ff6a3d")
    } else {
        ("#ffffff", "#0e1116", "#6b717c", "#efefeb", "#e8471c")
    };
    let font = "font-family=\"-apple-system, 'Segoe UI', Helvetica, Arial, sans-serif\"";
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630" {font}><rect width="1200" height="630" fill="{paper}"/><text x="72" y="112" font-size="44" font-weight="700" fill="{ink}">Revenue by channel</text>"#
    );
    let sub = if o.site {
        format!("{} · {when}", esc(domain))
    } else {
        when.to_string()
    };
    let _ = write!(
        s,
        r#"<text x="72" y="160" font-size="26" fill="{dim}">{sub}</text>"#
    );
    let total: f64 = rows.iter().map(|r| r.1).sum();
    if o.amounts && total > 0.0 {
        let _ = write!(
            s,
            r#"<text x="1128" y="112" font-size="44" font-weight="700" fill="{bar}" text-anchor="end">{}</text><text x="1128" y="160" font-size="26" fill="{dim}" text-anchor="end">matched to a channel</text>"#,
            money(total)
        );
    }
    if rows.is_empty() {
        let _ = write!(
            s,
            r#"<text x="72" y="340" font-size="30" fill="{dim}">No revenue in this range yet.</text>"#
        );
    }
    let max = rows.first().map_or(1.0, |r| r.1);
    let step = 330.0 / rows.len().max(1) as f64;
    for (i, (label, revenue)) in rows.iter().enumerate() {
        let y = 222.0 + step * i as f64 + step / 2.0;
        let share = revenue / total * 100.0;
        let value = if o.amounts {
            format!("{} · {share:.0}%", money(*revenue))
        } else {
            format!("{share:.0}%")
        };
        let width = (revenue / max * 520.0).max(4.0);
        let _ = write!(
            s,
            r#"<text x="72" y="{ty:.0}" font-size="28" fill="{ink}">{label}</text><rect x="380" y="{ry:.0}" width="520" height="22" rx="4" fill="{rule}"/><rect x="380" y="{ry:.0}" width="{width:.0}" height="22" rx="4" fill="{bar}"/><text x="1128" y="{ty:.0}" font-size="28" font-weight="600" fill="{ink}" text-anchor="end">{value}</text>"#,
            ty = y + 10.0,
            ry = y - 11.0,
            label = esc(label),
        );
    }
    let _ = write!(
        s,
        r#"<text x="1128" y="596" font-size="20" fill="{dim}" text-anchor="end">Measured with Virling</text></svg>"#
    );
    s
}

#[derive(Template)]
#[template(path = "share_card.html")]
struct CardPage {
    shell: Shell,
    title: &'static str,
    range: RangeMenu,
    /// The image URL with the chosen range and options.
    image: String,
    file: String,
    hidden: Vec<(&'static str, String)>,
    amounts: bool,
    site: bool,
    dark: bool,
}

/// The query that keeps a range and options, without the leading "?".
fn keep(params: &[(&'static str, String)], o: &Options) -> String {
    let mut all: Vec<String> = params
        .iter()
        .map(|(k, v)| {
            let v: String = url::form_urlencoded::byte_serialize(v.as_bytes()).collect();
            format!("{k}={v}")
        })
        .collect();
    all.push("set=1".into());
    for (on, k) in [(o.amounts, "amounts"), (o.site, "site"), (o.dark, "dark")] {
        if on {
            all.push(format!("{k}=1"));
        }
    }
    all.join("&")
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<CardQuery>,
) -> Response {
    let (ctx, picked) = match load_ranged(&state, &account, &domain, "dashboard", q.args()).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let o = q.options();
    let params: Vec<(&'static str, String)> = picked
        .params()
        .into_iter()
        .filter(|(k, _)| *k != "compare")
        .collect();
    let path = app_path(&ctx.site.domain, "share-card");
    let mut menu = picked.menu(ctx.today, ctx.first_day, &path, &[]);
    menu.vs = false;
    render(CardPage {
        title: "Share card",
        image: format!("{path}.svg?{}", keep(&params, &o)),
        file: format!("{}-revenue-by-channel.png", ctx.site.domain),
        range: menu,
        hidden: params,
        amounts: o.amounts,
        site: o.site,
        dark: o.dark,
        shell: ctx.shell,
    })
}

pub async fn image(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<CardQuery>,
) -> Response {
    let (ctx, picked) = match load_ranged(&state, &account, &domain, "dashboard", q.args()).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let p = picked.period(&ctx.site.timezone);
    let (store, id) = (state.events.clone(), ctx.site.id.clone());
    let rows = match blocking(move || store.breakdown(&id, &p, Dimension::Channel)).await {
        Ok(r) => r,
        Err(code) => return code.into_response(),
    };
    let when = if picked.from.year() == picked.to.year() {
        format!("{}, {}", span(picked.from, picked.to), picked.to.year())
    } else {
        span(picked.from, picked.to)
    };
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        draw(&channels(&rows), &when, &ctx.site.domain, &q.options()),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: &str, revenue: f64) -> BreakdownRow {
        BreakdownRow {
            key: Some(label.to_lowercase()),
            label: label.into(),
            unattributed: false,
            visitors: 10,
            payments: 1,
            customers: 1,
            refunded: 0,
            revenue_usd: revenue,
            revenue_per_visitor: None,
            avg_visit_secs: None,
        }
    }

    #[test]
    fn sums_the_tail_as_other_and_skips_unpaid() {
        let mut rows: Vec<BreakdownRow> = (1..=8)
            .map(|i| row(&format!("C{i}"), i as f64 * 10.0))
            .collect();
        rows.push(row("Free", 0.0));
        let c = channels(&rows);
        assert_eq!(c.len(), SHOWN + 1);
        assert_eq!(c[0], ("C8".into(), 80.0));
        assert_eq!(c[SHOWN], ("Other".into(), 60.0));
    }

    #[test]
    fn hides_amounts_and_site_unless_chosen() {
        let rows = vec![("Organic search".into(), 750.0), ("AI".into(), 250.0)];
        let quiet = Options {
            amounts: false,
            site: false,
            dark: false,
        };
        let svg = draw(&rows, "Oct 1 to 31, 2026", "example.com", &quiet);
        assert!(svg.contains(">75%<") && svg.contains(">25%<"));
        assert!(!svg.contains('$') && !svg.contains("example.com"));
        let all = Options {
            amounts: true,
            site: true,
            dark: true,
        };
        let svg = draw(&rows, "Oct 1 to 31, 2026", "example.com", &all);
        assert!(svg.contains("$1,000") && svg.contains("$750 · 75%"));
        assert!(svg.contains("example.com · Oct 1 to 31, 2026"));
    }
}
