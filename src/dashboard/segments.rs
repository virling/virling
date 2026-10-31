//! Saved segments: named dashboard filters, reused by every view and the client report.

use std::collections::HashMap;

use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use super::home::FILTERS;
use super::{encode, load};
use crate::app::AppState;
use crate::auth::Account;
use crate::sites;

const MAX_SEGMENTS: i64 = 50;
/// Params besides the filters that the redirect after saving keeps.
const KEEP: [&str; 5] = ["range", "from", "to", "compare", "view"];

pub struct Segment {
    pub id: String,
    pub name: String,
    pub filters: Vec<(&'static str, String)>,
}

/// Filter params from `get`, in dashboard order, skipping unknown, empty and overlong ones.
pub(super) fn pick(get: impl Fn(&str) -> Option<String>) -> Vec<(&'static str, String)> {
    FILTERS
        .iter()
        .filter_map(|(k, _)| {
            get(k)
                .filter(|v| !v.is_empty() && v.len() <= 300)
                .map(|v| (*k, v))
        })
        .collect()
}

fn parse(lines: &str) -> Vec<(&'static str, String)> {
    let map: HashMap<&str, &str> = lines.lines().filter_map(|l| l.split_once('=')).collect();
    pick(|k| map.get(k).map(|v| v.to_string()))
}

pub async fn list(db: &SqlitePool, site_id: &str) -> sqlx::Result<Vec<Segment>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, name, filters FROM segments WHERE site_id = ? ORDER BY created_at, id",
    )
    .bind(site_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, f)| Segment {
            id,
            name,
            filters: parse(&f),
        })
        .collect())
}

/// The segment the client report is limited to, if any.
pub async fn of_report(db: &SqlitePool, site_id: &str) -> sqlx::Result<Option<Segment>> {
    let id: Option<Option<String>> =
        sqlx::query_scalar("SELECT segment_id FROM report_brands WHERE site_id = ?")
            .bind(site_id)
            .fetch_optional(db)
            .await?;
    let Some(id) = id.flatten() else {
        return Ok(None);
    };
    Ok(list(db, site_id).await?.into_iter().find(|s| s.id == id))
}

/// The dashboard with `filters` and the form's range params.
fn back(domain: &str, form: &HashMap<String, String>, filters: &[(&str, String)]) -> String {
    let mut q: Vec<String> = KEEP
        .iter()
        .filter_map(|k| {
            form.get(*k)
                .filter(|v| !v.is_empty() && v.len() <= 40)
                .map(|v| format!("{k}={}", encode(v)))
        })
        .collect();
    q.extend(filters.iter().map(|(k, v)| format!("{k}={}", encode(v))));
    let base = sites::app_path(domain, "");
    if q.is_empty() {
        base
    } else {
        format!("{base}?{}", q.join("&"))
    }
}

/// Saves the form's filters under a name.
pub async fn save(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "dashboard").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let filters = pick(|k| form.get(k).cloned());
    let name: String = form
        .get("name")
        .map_or("", |n| n.trim())
        .chars()
        .take(60)
        .collect();
    if filters.is_empty() || name.is_empty() {
        return Redirect::to(&back(&ctx.site.domain, &form, &filters)).into_response();
    }
    let lines: Vec<String> = filters.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let res = sqlx::query(
        "INSERT INTO segments (id, site_id, name, filters, created_at)
         SELECT ?, ?, ?, ?, ? WHERE (SELECT count(*) FROM segments WHERE site_id = ?) < ?",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&ctx.site.id)
    .bind(&name)
    .bind(lines.join("\n"))
    .bind(OffsetDateTime::now_utc().unix_timestamp())
    .bind(&ctx.site.id)
    .bind(MAX_SEGMENTS)
    .execute(&state.db)
    .await;
    if let Err(err) = res {
        tracing::error!(%err, "saving a segment failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&back(&ctx.site.domain, &form, &filters)).into_response()
}

pub async fn delete(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "dashboard").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let id = form.get("segment").map_or("", String::as_str);
    let res = sqlx::query("DELETE FROM segments WHERE id = ? AND site_id = ?")
        .bind(id)
        .bind(&ctx.site.id)
        .execute(&state.db)
        .await;
    if let Err(err) = res {
        tracing::error!(%err, "deleting a segment failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let filters = pick(|k| form.get(k).cloned());
    Redirect::to(&back(&ctx.site.domain, &form, &filters)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keeps_known_filters_in_order() {
        let f = parse("country=US\nbogus=1\nchannel=AI\npage=\nviewed=/a=b");
        assert_eq!(
            f,
            vec![
                ("channel", "AI".to_string()),
                ("viewed", "/a=b".to_string()),
                ("country", "US".to_string()),
            ]
        );
    }
}
