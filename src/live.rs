//! Who has a site open right now, and the long poll that tells an open dashboard when to refresh.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::app::AppState;
use crate::auth::Account;
use crate::sites;

/// A visitor stays live this long after their last event; the tracker pings every 30 s while the tab is visible.
const FRESH: Duration = Duration::from_secs(75);
/// How long a poll waits for a change before answering anyway.
const WAIT: Duration = Duration::from_secs(25);

/// Visitors per site with a visible tab open, and when each was last heard from.
#[derive(Default)]
pub struct Presence(Mutex<HashMap<String, HashMap<String, Instant>>>);

impl Presence {
    pub fn seen(&self, site_id: &str, visitor_id: &str) {
        self.0
            .lock()
            .unwrap()
            .entry(site_id.to_string())
            .or_default()
            .insert(visitor_id.to_string(), Instant::now());
    }

    /// The visitor closed their last tab of the site.
    pub fn gone(&self, site_id: &str, visitor_id: &str) {
        if let Some(v) = self.0.lock().unwrap().get_mut(site_id) {
            v.remove(visitor_id);
        }
    }

    pub fn ids(&self, site_id: &str) -> HashSet<String> {
        let mut all = self.0.lock().unwrap();
        let Some(v) = all.get_mut(site_id) else {
            return HashSet::new();
        };
        v.retain(|_, t| t.elapsed() < FRESH);
        v.keys().cloned().collect()
    }

    pub fn count(&self, site_id: &str) -> u64 {
        let mut all = self.0.lock().unwrap();
        let Some(v) = all.get_mut(site_id) else {
            return 0;
        };
        v.retain(|_, t| t.elapsed() < FRESH);
        v.len() as u64
    }
}

#[derive(Deserialize)]
pub struct Since {
    since: Option<String>,
}

/// Answers with the site's version once it moves past `since`, or after 25 s.
pub async fn poll(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<Since>,
) -> Response {
    let Some(site) = sites::for_account(&state.db, &account.id)
        .await
        .ok()
        .and_then(|all| all.into_iter().find(|s| s.domain == domain))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let version = || {
        format!(
            "{}-{}",
            state.events.generation(&site.id),
            state.presence.count(&site.id)
        )
    };
    let changes = state.events.subscribe();
    if q.since.as_deref() == Some(version().as_str()) {
        let changed = async {
            match changes {
                Some(mut rx) => while rx.recv().await.is_ok_and(|id| id != site.id) {},
                None => std::future::pending().await,
            }
        };
        tokio::time::timeout(WAIT, changed).await.ok();
    }
    Json(json!({ "v": version() })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_counts_each_visitor_once_until_they_leave() {
        let p = Presence::default();
        p.seen("s1", "a");
        p.seen("s1", "a");
        p.seen("s1", "b");
        p.seen("s2", "c");
        assert_eq!(p.count("s1"), 2);
        p.gone("s1", "a");
        assert_eq!(p.ids("s1"), HashSet::from(["b".to_string()]));
        assert_eq!(p.count("nope"), 0);
    }
}
