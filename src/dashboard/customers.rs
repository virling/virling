//! The Recent customers card: the last paying customers, how they found the site and how long they took.

use super::{Names, amount, payer_key, plural};
use crate::events::EventStore;
use crate::icons;
use crate::stats::{Journey, PaymentRow};
use crate::visitor_pages::visitor_href;

/// Customers listed.
const SHOWN: usize = 10;

/// First payments that went through, newest first.
fn firsts(rows: &[PaymentRow]) -> impl Iterator<Item = &PaymentRow> {
    rows.iter()
        .filter(|p| !p.is_renewal && matches!(p.status.as_str(), "paid" | "partially_refunded"))
        .take(SHOWN)
}

/// The journey of each listed customer, in order.
pub fn journeys(
    store: &dyn EventStore,
    site: &str,
    rows: &[PaymentRow],
    window_days: i64,
) -> anyhow::Result<Vec<Option<Journey>>> {
    firsts(rows)
        .map(|p| store.journey(site, &p.provider, &p.provider_payment_id, window_days))
        .collect()
}

pub fn keys(rows: &[PaymentRow]) -> Vec<String> {
    firsts(rows).map(payer_key).collect()
}

pub struct Step {
    pub(super) icon: String,
    pub(super) label: String,
}

pub struct Row {
    pub(super) avatar: String,
    pub(super) name: String,
    pub(super) href: String,
    pub(super) amount: String,
    pub(super) visits: String,
    pub(super) wait: String,
    pub(super) path: Vec<Step>,
}

pub struct Card {
    pub(super) rows: Vec<Row>,
    pub(super) payments: String,
    pub(super) foot: String,
}

pub fn card(
    rows: &[PaymentRow],
    journeys: &[Option<Journey>],
    names: &Names,
    domain: &str,
    visitors: u64,
) -> Card {
    let rows: Vec<Row> = firsts(rows)
        .zip(journeys)
        .map(|(p, j)| {
            let who = names.get(&payer_key(p));
            let mut path = Vec::new();
            let mut step = |icon: String, label: &str| {
                path.push(Step {
                    icon,
                    label: label.into(),
                })
            };
            let first = p.first_touch.as_ref().map(|f| f.source.as_str());
            if let Some(f) = first {
                step(icons::source(f, 14), f);
            }
            if let Some(t) = p
                .touch
                .as_ref()
                .filter(|t| Some(t.source.as_str()) != first)
            {
                step(icons::source(&t.source, 14), &t.source);
            }
            if let Some(page) = j.as_ref().and_then(|j| j.last_page.as_deref()) {
                step(String::new(), page);
            }
            step(String::new(), "Paid");
            Row {
                avatar: who.avatar,
                name: who.name,
                href: p
                    .visitor
                    .as_deref()
                    .map(|v| visitor_href(domain, v))
                    .unwrap_or_default(),
                amount: amount(p),
                visits: j.as_ref().map_or_else(
                    || "\u{2013}".into(),
                    |j| plural(j.sessions.len() as u64, "visit"),
                ),
                wait: p.first_touch.as_ref().map_or_else(
                    || "\u{2013}".into(),
                    |f| crate::topay::label((p.ts - f.ts).whole_seconds().max(0) as f64 / 86400.0),
                ),
                path,
            }
        })
        .collect();
    let foot = match rows.len() {
        0 => String::new(),
        _ if visitors < 100 => "Early numbers. They settle after about 100 visitors.".into(),
        _ => "Click a customer to see their visits.".into(),
    };
    Card {
        rows,
        payments: crate::sites::app_path(domain, "payments"),
        foot,
    }
}
