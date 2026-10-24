//! Generated sample data a site owner can try before real traffic arrives.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use time::OffsetDateTime;

use crate::app::AppState;
use crate::auth::Account;
use crate::sites::{self, Site};

/// Sites whose sample data is being removed right now.
static CLEARING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Mutex::default);

async fn owned(state: &AppState, account: &Account, domain: &str) -> Result<Site, StatusCode> {
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    sites::for_account(&state.db, &account.id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .into_iter()
        .find(|s| s.domain == domain)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Removes the site's sample data and its label; nothing happens for a site without sample data.
pub async fn clear(state: &AppState, site: &Site) {
    if !site.sample_data || !CLEARING.lock().unwrap().insert(site.id.clone()) {
        return;
    }
    let (store, id) = (state.events.clone(), site.id.clone());
    let removed = tokio::task::spawn_blocking(move || store.remove_sample(&id)).await;
    if matches!(removed, Ok(Ok(()))) {
        let off = sqlx::query("UPDATE sites SET sample_data = 0 WHERE id = ?")
            .bind(&site.id)
            .execute(&state.db)
            .await;
        if let Err(err) = off {
            tracing::error!(%err, "clearing the sample data flag failed");
        }
        state.sites.invalidate(&site.domain);
    } else {
        tracing::error!(?removed, "removing sample data failed");
    }
    CLEARING.lock().unwrap().remove(&site.id);
}

/// Fills a site that has no events yet with sample data.
pub async fn add(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let site = match owned(&state, &account, &domain).await {
        Ok(site) => site,
        Err(code) => return code.into_response(),
    };
    let home = Redirect::to(&sites::app_path(&site.domain, ""));
    let store = state.events.clone();
    let id = site.id.clone();
    let empty = tokio::task::spawn_blocking(move || store.latest_pageview(&id))
        .await
        .is_ok_and(|r| r.is_ok_and(|e| e.is_none()));
    let linked =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM integrations WHERE site_id = ?")
            .bind(&site.id)
            .fetch_one(&state.db)
            .await
            .is_ok_and(|n| n > 0);
    if site.sample_data || !empty || linked {
        return home.into_response();
    }
    let claimed = sqlx::query("UPDATE sites SET sample_data = 1 WHERE id = ? AND sample_data = 0")
        .bind(&site.id)
        .execute(&state.db)
        .await
        .is_ok_and(|r| r.rows_affected() == 1);
    if !claimed {
        return home.into_response();
    }
    state.sites.invalidate(&site.domain);
    let (store, id) = (state.events.clone(), site.id.clone());
    let loaded = tokio::task::spawn_blocking(move || {
        let (events, payments, subscriptions) = crate::demo::sample(&id, OffsetDateTime::now_utc());
        store.insert_events(&events)?;
        store.upsert_payments(&payments)?;
        store.replace_subscriptions(&id, crate::events::SAMPLE, &subscriptions)
    })
    .await;
    if !matches!(loaded, Ok(Ok(()))) {
        tracing::error!(?loaded, "loading sample data failed");
        clear(
            &state,
            &Site {
                sample_data: true,
                ..site
            },
        )
        .await;
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    crate::sync::rebuild_attribution(&state, Some(&site.id)).await;
    home.into_response()
}

/// Removes every sample row of the site.
pub async fn remove(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    match owned(&state, &account, &domain).await {
        Ok(site) => {
            clear(&state, &site).await;
            Redirect::to(&sites::app_path(&site.domain, "")).into_response()
        }
        Err(code) => code.into_response(),
    }
}
