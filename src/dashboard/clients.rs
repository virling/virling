//! Every site of the account, grouped by client, with the last 30 days of each.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use time::Duration;

use super::{Shell, blocking, group, load, money};
use crate::app::AppState;
use crate::auth::Account;
use crate::onboarding::render;
use crate::sites::{self, app_path};
use crate::stats::Period;

pub struct ClientRow {
    pub domain: String,
    pub href: String,
    pub visitors: String,
    pub revenue: String,
}

pub struct ClientGroup {
    /// Empty for sites without a client.
    pub name: String,
    pub sites: Vec<ClientRow>,
    pub visitors: String,
    pub revenue: String,
}

#[derive(Template)]
#[template(path = "clients.html")]
struct ClientsPage {
    shell: Shell,
    title: &'static str,
    groups: Vec<ClientGroup>,
}

/// Sites ordered by client name, sites without a client last, keeping their order within a client.
pub fn by_client(mut all: Vec<sites::Site>) -> Vec<sites::Site> {
    all.sort_by_key(|s| {
        (
            s.client.is_none(),
            s.client.as_deref().map(str::to_lowercase),
        )
    });
    all
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "clients").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let all = match sites::for_account(&state.db, &account.id).await {
        Ok(all) => by_client(all),
        Err(_) => return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let today = ctx.today;
    let store = state.events.clone();
    let list = all.clone();
    let totals = match blocking(move || {
        list.iter()
            .map(|s| {
                let p = Period {
                    from: today - Duration::days(29),
                    to: today,
                    tz: s.timezone.clone(),
                };
                let t = store.overview(&s.id, &p)?.current;
                Ok((t.visitors, t.revenue_usd))
            })
            .collect::<anyhow::Result<Vec<_>>>()
    })
    .await
    {
        Ok(t) => t,
        Err(code) => return code.into_response(),
    };
    let mut groups: Vec<(ClientGroup, u64, f64)> = Vec::new();
    for (s, (visitors, revenue)) in all.iter().zip(totals) {
        let name = s.client.clone().unwrap_or_default();
        if groups.last().is_none_or(|g| g.0.name != name) {
            groups.push((
                ClientGroup {
                    name,
                    sites: Vec::new(),
                    visitors: String::new(),
                    revenue: String::new(),
                },
                0,
                0.0,
            ));
        }
        let g = groups.last_mut().expect("pushed above");
        g.1 += visitors;
        g.2 += revenue;
        g.0.sites.push(ClientRow {
            domain: s.domain.clone(),
            href: app_path(&s.domain, "dashboard"),
            visitors: group(visitors),
            revenue: money(revenue),
        });
    }
    render(ClientsPage {
        shell: ctx.shell,
        title: "All clients",
        groups: groups
            .into_iter()
            .map(|(mut g, visitors, revenue)| {
                g.visitors = group(visitors);
                g.revenue = money(revenue);
                g
            })
            .collect(),
    })
}
