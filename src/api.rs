use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};

use crate::app::AppState;
use crate::auth::{hash_token, random_token};
use crate::dashboard::{RANGES, blocking, resolve_range};
use crate::sites::{self, Site};
use crate::stats::{BreakdownRow, Dimension, PaymentFilter, PaymentRow, Period, Totals};
use crate::visitors::Timeline;

pub const KEY_PREFIX: &str = "vk_";
const MAX_DAYS: i64 = 3660;
pub const MAX_PAYMENTS: u32 = 200;
pub const MAX_ROWS: usize = 1000;

/// Breakdown dimensions by API name, with what each row is.
pub const DIMENSIONS: [(&str, Dimension, &str); 10] = [
    (
        "channel",
        Dimension::Channel,
        "Channel of the visit, such as Search, Social, AI, Email, Paid, Referral or Direct",
    ),
    (
        "referrer",
        Dimension::Referrer,
        "Referring domain of the visit",
    ),
    ("campaign", Dimension::Campaign, "utm_campaign of the visit"),
    ("keyword", Dimension::Keyword, "utm_term of the visit"),
    (
        "landing_page",
        Dimension::LandingPage,
        "First page of the visit",
    ),
    (
        "page",
        Dimension::Page,
        "Every page viewed; a payment counts for each page of its credited visit",
    ),
    ("country", Dimension::Country, "Two-letter country code"),
    (
        "device",
        Dimension::Device,
        "Desktop, Mobile, Tablet or Unknown",
    ),
    ("browser", Dimension::Browser, "Browser name"),
    ("os", Dimension::Os, "Operating system"),
];

pub struct ApiError(pub StatusCode, pub String);

impl ApiError {
    fn new(code: StatusCode, msg: impl Into<String>) -> Self {
        ApiError(code, msg.into())
    }

    fn bad(msg: impl Into<String>) -> Self {
        ApiError::new(StatusCode::BAD_REQUEST, msg)
    }
}

impl From<StatusCode> for ApiError {
    fn from(code: StatusCode) -> Self {
        ApiError(code, "try again".into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// The account of a valid, unrevoked API key.
pub async fn key_account(db: &SqlitePool, key: &str) -> Option<String> {
    let row: Option<(String, String, Option<i64>)> = sqlx::query_as(
        "SELECT id, account_id, last_used_at FROM api_keys WHERE key_hash = ? AND revoked_at IS NULL",
    )
    .bind(hash_token(key))
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    let (id, account, used) = row?;
    if used.is_none_or(|t| t < now() - 60) {
        let _ = sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
            .bind(now())
            .bind(&id)
            .execute(db)
            .await;
    }
    Some(account)
}

/// The key from an `Authorization: Bearer` header.
pub fn bearer(parts: &Parts) -> Option<&str> {
    let value = parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, key) = value.trim().split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| key.trim())
}

/// The account behind the request's API key; 401 without a valid one.
pub struct KeyAccount {
    pub id: String,
}

impl FromRequestParts<AppState> for KeyAccount {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let unauthorized = || {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "missing or invalid API key; send Authorization: Bearer <key>",
            )
        };
        let key = bearer(parts).ok_or_else(unauthorized)?;
        let id = key_account(&state.db, key).await.ok_or_else(unauthorized)?;
        Ok(KeyAccount { id })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct KeyRow {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// Stores a new key and returns it; only its hash is kept.
pub async fn create_key(db: &SqlitePool, account_id: &str, name: &str) -> anyhow::Result<String> {
    let key = format!("{KEY_PREFIX}{}", random_token());
    let name: String = name.trim().chars().take(60).collect();
    sqlx::query(
        "INSERT INTO api_keys (id, account_id, name, key_hash, prefix, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(account_id)
    .bind(if name.is_empty() { "API key" } else { &name })
    .bind(hash_token(&key))
    .bind(&key[..KEY_PREFIX.len() + 8])
    .bind(now())
    .execute(db)
    .await?;
    Ok(key)
}

pub async fn list_keys(db: &SqlitePool, account_id: &str) -> sqlx::Result<Vec<KeyRow>> {
    sqlx::query_as(
        "SELECT id, name, prefix, created_at, last_used_at FROM api_keys
         WHERE account_id = ? AND revoked_at IS NULL ORDER BY created_at, id",
    )
    .bind(account_id)
    .fetch_all(db)
    .await
}

pub async fn revoke_key(db: &SqlitePool, account_id: &str, id: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE api_keys SET revoked_at = ? WHERE id = ? AND account_id = ? AND revoked_at IS NULL",
    )
    .bind(now())
    .bind(id)
    .bind(account_id)
    .execute(db)
    .await?;
    Ok(())
}

/// A date range: a preset like the dashboard's, or explicit local dates.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RangeQuery {
    /// today, 7d, 30d, 90d, 12m or all
    pub range: Option<String>,
    /// YYYY-MM-DD, with `to`
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RangeOut {
    pub from: String,
    pub to: String,
    pub timezone: String,
}

impl RangeOut {
    fn of(p: &Period) -> Self {
        RangeOut {
            from: p.from.to_string(),
            to: p.to.to_string(),
            timezone: p.tz.clone(),
        }
    }
}

fn parse_date(s: &str) -> Result<Date, ApiError> {
    Date::parse(s.trim(), format_description!("[year]-[month]-[day]"))
        .map_err(|_| ApiError::bad(format!("{s:?} is not a date like 2026-09-30")))
}

/// The account's site with this domain; 404 for any other.
async fn site(state: &AppState, account: &str, domain: &str) -> Result<Site, ApiError> {
    let all = sites::for_account(&state.db, account)
        .await
        .map_err(|_| ApiError::from(StatusCode::SERVICE_UNAVAILABLE))?;
    let domain = sites::normalize_domain(domain).unwrap_or_default();
    all.into_iter()
        .find(|s| s.domain == domain)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "site not found"))
}

/// The site and the local-date period a query asks for.
async fn site_period(
    state: &AppState,
    account: &str,
    domain: &str,
    q: &RangeQuery,
) -> Result<(Site, Period), ApiError> {
    let site = site(state, account, domain).await?;
    let explicit = match (q.from.as_deref(), q.to.as_deref()) {
        (Some(f), Some(t)) => Some((parse_date(f)?, parse_date(t)?)),
        (None, None) => None,
        _ => return Err(ApiError::bad("from and to go together")),
    };
    if let Some(key) = q.range.as_deref()
        && !RANGES.iter().any(|r| r.0 == key)
    {
        return Err(ApiError::bad(
            "range must be one of today, 7d, 30d, 90d, 12m, all",
        ));
    }
    if let Some((from, to)) = explicit {
        if from > to || (to - from).whole_days() >= MAX_DAYS {
            return Err(ApiError::bad(
                "from must not be after to, and the range must be under 10 years",
            ));
        }
        let tz = site.timezone.clone();
        return Ok((site, Period { from, to, tz }));
    }
    let store = state.events.clone();
    let (id, tz) = (site.id.clone(), site.timezone.clone());
    let (today, first_day) = blocking(move || {
        let offset = store.utc_offset(&tz)?;
        Ok((
            OffsetDateTime::now_utc().to_offset(offset).date(),
            store.first_event(&id)?.map(|t| t.to_offset(offset).date()),
        ))
    })
    .await?;
    let (_, _, days, _) = resolve_range(today, first_day, q.range.as_deref());
    let period = Period {
        from: today - Duration::days(days - 1),
        to: today,
        tz: site.timezone.clone(),
    };
    Ok((site, period))
}

#[derive(Debug, Serialize)]
pub struct SiteOut {
    pub domain: String,
    pub timezone: String,
}

pub async fn sites_for(state: &AppState, account: &str) -> Result<Vec<SiteOut>, ApiError> {
    let all = sites::for_account(&state.db, account)
        .await
        .map_err(|_| ApiError::from(StatusCode::SERVICE_UNAVAILABLE))?;
    Ok(all
        .into_iter()
        .map(|s| SiteOut {
            domain: s.domain,
            timezone: s.timezone,
        })
        .collect())
}

/// Relative change from the previous period, e.g. 0.25 for +25%; None when there is nothing to compare.
#[derive(Debug, Serialize)]
pub struct Change {
    pub visitors: Option<f64>,
    pub revenue_usd: Option<f64>,
    pub payments: Option<f64>,
    pub conversion: Option<f64>,
    pub revenue_per_visitor: Option<f64>,
}

fn change(cur: Option<f64>, prev: Option<f64>) -> Option<f64> {
    match (cur, prev) {
        (Some(c), Some(p)) if p != 0.0 => Some((c - p) / p),
        _ => None,
    }
}

#[derive(Debug, Serialize)]
pub struct OverviewOut {
    pub site: String,
    pub range: RangeOut,
    pub previous_range: RangeOut,
    pub current: Totals,
    pub previous: Totals,
    pub change: Change,
}

pub async fn overview_for(
    state: &AppState,
    account: &str,
    domain: &str,
    q: &RangeQuery,
) -> Result<OverviewOut, ApiError> {
    let (site, period) = site_period(state, account, domain, q).await?;
    let store = state.events.clone();
    let (id, p) = (site.id.clone(), period.clone());
    let o = blocking(move || store.overview(&id, &p)).await?;
    let (c, p) = (&o.current, &o.previous);
    let change = Change {
        visitors: change(Some(c.visitors as f64), Some(p.visitors as f64)),
        revenue_usd: change(Some(c.revenue_usd), Some(p.revenue_usd)),
        payments: change(Some(c.payments as f64), Some(p.payments as f64)),
        conversion: change(c.conversion, p.conversion),
        revenue_per_visitor: change(c.revenue_per_visitor, p.revenue_per_visitor),
    };
    Ok(OverviewOut {
        site: site.domain,
        range: RangeOut::of(&period),
        previous_range: RangeOut::of(&period.previous()),
        current: o.current,
        previous: o.previous,
        change,
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct BreakdownQuery {
    pub range: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct BreakdownOut {
    pub site: String,
    pub range: RangeOut,
    pub dimension: String,
    /// Rows before `limit` was applied.
    pub total: usize,
    /// Most revenue first, then most visitors.
    pub rows: Vec<BreakdownRow>,
}

pub async fn breakdown_for(
    state: &AppState,
    account: &str,
    domain: &str,
    dimension: &str,
    q: &BreakdownQuery,
) -> Result<BreakdownOut, ApiError> {
    let Some((name, dim, _)) = DIMENSIONS.iter().find(|d| d.0 == dimension) else {
        let names: Vec<&str> = DIMENSIONS.iter().map(|d| d.0).collect();
        return Err(ApiError::bad(format!(
            "dimension must be one of {}",
            names.join(", ")
        )));
    };
    let range = RangeQuery {
        range: q.range.clone(),
        from: q.from.clone(),
        to: q.to.clone(),
    };
    let (site, period) = site_period(state, account, domain, &range).await?;
    let store = state.events.clone();
    let (id, p, d) = (site.id.clone(), period.clone(), *dim);
    let mut rows = blocking(move || store.breakdown(&id, &p, d)).await?;
    let total = rows.len();
    rows.truncate(q.limit.unwrap_or(100).clamp(1, MAX_ROWS));
    Ok(BreakdownOut {
        site: site.domain,
        range: RangeOut::of(&period),
        dimension: name.to_string(),
        total,
        rows,
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct PaymentsQuery {
    pub range: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// all, attributed, unattributed or refunded
    pub filter: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct PaymentsOut {
    pub site: String,
    pub range: RangeOut,
    pub filter: String,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
    /// Newest first.
    pub rows: Vec<PaymentRow>,
}

pub async fn payments_for(
    state: &AppState,
    account: &str,
    domain: &str,
    q: &PaymentsQuery,
) -> Result<PaymentsOut, ApiError> {
    let name = q.filter.as_deref().unwrap_or("all");
    let filter = PaymentFilter::parse(name).ok_or_else(|| {
        ApiError::bad("filter must be one of all, attributed, unattributed, refunded")
    })?;
    let range = RangeQuery {
        range: q.range.clone(),
        from: q.from.clone(),
        to: q.to.clone(),
    };
    let (site, period) = site_period(state, account, domain, &range).await?;
    let (limit, offset) = (
        q.limit.unwrap_or(20).clamp(1, MAX_PAYMENTS),
        q.offset.unwrap_or(0),
    );
    let store = state.events.clone();
    let (id, p) = (site.id.clone(), period.clone());
    let page = blocking(move || store.payment_list(&id, &p, filter, limit, offset)).await?;
    Ok(PaymentsOut {
        site: site.domain,
        range: RangeOut::of(&period),
        filter: name.to_string(),
        total: page.total,
        limit,
        offset,
        rows: page.rows,
    })
}

#[derive(Debug, Serialize)]
pub struct VisitorOut {
    pub site: String,
    #[serde(flatten)]
    pub timeline: Timeline,
}

pub async fn visitor_for(
    state: &AppState,
    account: &str,
    domain: &str,
    visitor_id: &str,
) -> Result<VisitorOut, ApiError> {
    let site = site(state, account, domain).await?;
    let store = state.events.clone();
    let (id, visitor) = (site.id.clone(), visitor_id.to_string());
    let timeline = blocking(move || store.timeline(&id, &visitor))
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "visitor not found"))?;
    Ok(VisitorOut {
        site: site.domain,
        timeline,
    })
}

#[derive(Debug, Serialize)]
pub struct SitesOut {
    pub sites: Vec<SiteOut>,
}

pub async fn sites(State(state): State<AppState>, key: KeyAccount) -> ApiResult<SitesOut> {
    Ok(Json(SitesOut {
        sites: sites_for(&state, &key.id).await?,
    }))
}

pub async fn overview(
    State(state): State<AppState>,
    key: KeyAccount,
    Path(domain): Path<String>,
    Query(q): Query<RangeQuery>,
) -> ApiResult<OverviewOut> {
    Ok(Json(overview_for(&state, &key.id, &domain, &q).await?))
}

pub async fn breakdown(
    State(state): State<AppState>,
    key: KeyAccount,
    Path((domain, dimension)): Path<(String, String)>,
    Query(q): Query<BreakdownQuery>,
) -> ApiResult<BreakdownOut> {
    Ok(Json(
        breakdown_for(&state, &key.id, &domain, &dimension, &q).await?,
    ))
}

pub async fn payments(
    State(state): State<AppState>,
    key: KeyAccount,
    Path(domain): Path<String>,
    Query(q): Query<PaymentsQuery>,
) -> ApiResult<PaymentsOut> {
    Ok(Json(payments_for(&state, &key.id, &domain, &q).await?))
}

pub async fn visitor(
    State(state): State<AppState>,
    key: KeyAccount,
    Path((domain, visitor_id)): Path<(String, String)>,
) -> ApiResult<VisitorOut> {
    Ok(Json(
        visitor_for(&state, &key.id, &domain, &visitor_id).await?,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::Request;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::app::{Config, router};
    use crate::events::{DuckStore, Event, EventStore, Payment};

    pub(crate) struct Fixture {
        _dir: tempfile::TempDir,
        pub state: AppState,
        pub key: String,
        pub account: String,
    }

    fn event(site: &str, visitor: &str, session: &str, ts: OffsetDateTime, path: &str) -> Event {
        Event {
            site_id: site.into(),
            ts,
            visitor_id: visitor.into(),
            session_id: session.into(),
            kind: "pageview".into(),
            name: None,
            path: path.into(),
            referrer_host: Some("chatgpt.com".into()),
            utm_source: None,
            utm_medium: None,
            utm_campaign: Some("launch".into()),
            utm_content: None,
            utm_term: None,
            channel: "AI".into(),
            country: Some("DE".into()),
            device: "Desktop".into(),
            browser: "Firefox".into(),
            os: "macOS".into(),
            region: None,
            city: None,
            host: None,
        }
    }

    /// example.com with one paying visitor from ChatGPT, plus another account's other.com.
    pub(crate) async fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::open_sqlite(dir.path()).await.unwrap();
        let account = sites::find_or_create_account(&db, "me@example.com")
            .await
            .unwrap();
        let site = sites::create_site(&db, &account, "example.com", "UTC")
            .await
            .unwrap();
        let other = sites::find_or_create_account(&db, "other@example.com")
            .await
            .unwrap();
        let other_site = sites::create_site(&db, &other, "other.com", "UTC")
            .await
            .unwrap();
        let store = Arc::new(DuckStore::open(&dir.path().join("events.duckdb")).unwrap());
        let t = OffsetDateTime::now_utc() - Duration::days(2);
        store
            .insert_events(&[
                event(&site, "v1", "s1", t, "/"),
                event(&site, "v1", "s1", t + Duration::minutes(1), "/pricing"),
                event(&site, "v2", "s2", t, "/"),
                event(&other_site, "v9", "s9", t, "/"),
            ])
            .unwrap();
        store
            .upsert_payments(&[Payment {
                site_id: site.clone(),
                provider: "stripe".into(),
                provider_payment_id: "pi_1".into(),
                ts: t + Duration::minutes(2),
                amount: 4900,
                currency: "usd".into(),
                amount_usd: Some(49.0),
                refunded: 0,
                status: "paid".into(),
                visitor_id: Some("v1".into()),
                customer_id: Some("cus_1".into()),
                is_renewal: false,
            }])
            .unwrap();
        store
            .rebuild_attribution(&site, "last_non_direct", 90)
            .unwrap();
        let config = Config {
            marketing: false,
            app: true,
            trust_proxy: false,
            events_per_second: 200,
            base_url: "http://localhost:8080".into(),
            waitlist_from: "Virling <hello@virling.com>".into(),
        };
        let state = AppState::new(
            config,
            db.clone(),
            store,
            crate::mail::Mailer::Capture(Default::default()),
            crate::crypto::Vault::load(dir.path(), None).unwrap(),
        );
        let key = create_key(&db, &account, "test").await.unwrap();
        Fixture {
            _dir: dir,
            state,
            key,
            account,
        }
    }

    async fn get(state: &AppState, uri: &str, key: Option<&str>) -> (StatusCode, Value) {
        let mut req = Request::get(uri);
        if let Some(k) = key {
            req = req.header("authorization", format!("Bearer {k}"));
        }
        let res = router(state.clone())
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn requires_a_valid_unrevoked_key() {
        let f = fixture().await;
        let (code, body) = get(&f.state, "/api/v1/sites", None).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        assert!(body["error"].as_str().unwrap().contains("Bearer"));
        let (code, _) = get(&f.state, "/api/v1/sites", Some("vk_nope")).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        let (code, _) = get(&f.state, "/api/v1/sites", Some(&f.key)).await;
        assert_eq!(code, StatusCode::OK);

        let keys = list_keys(&f.state.db, &f.account).await.unwrap();
        assert_eq!(keys.len(), 1);
        assert!(f.key.starts_with(&keys[0].prefix));
        let stored: String = sqlx::query_scalar("SELECT key_hash FROM api_keys")
            .fetch_one(&f.state.db)
            .await
            .unwrap();
        assert_eq!(stored, hash_token(&f.key));
        revoke_key(&f.state.db, "someone-else", &keys[0].id)
            .await
            .unwrap();
        assert_eq!(
            get(&f.state, "/api/v1/sites", Some(&f.key)).await.0,
            StatusCode::OK
        );
        revoke_key(&f.state.db, &f.account, &keys[0].id)
            .await
            .unwrap();
        let (code, _) = get(&f.state, "/api/v1/sites", Some(&f.key)).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        assert!(list_keys(&f.state.db, &f.account).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn other_accounts_sites_are_not_found() {
        let f = fixture().await;
        for path in ["overview", "breakdown/channel", "payments", "visitors/v9"] {
            let (code, body) = get(
                &f.state,
                &format!("/api/v1/sites/other.com/{path}"),
                Some(&f.key),
            )
            .await;
            assert_eq!(code, StatusCode::NOT_FOUND, "{path}");
            assert_eq!(body["error"], "site not found");
        }
    }

    #[tokio::test]
    async fn lists_the_accounts_sites() {
        let f = fixture().await;
        let (code, body) = get(&f.state, "/api/v1/sites", Some(&f.key)).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!({"sites": [{"domain": "example.com", "timezone": "UTC"}]})
        );
    }

    #[tokio::test]
    async fn overview_has_totals_and_change() {
        let f = fixture().await;
        let (code, body) = get(
            &f.state,
            "/api/v1/sites/example.com/overview?range=7d",
            Some(&f.key),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{body}");
        assert_eq!(body["site"], "example.com");
        assert_eq!(body["range"]["timezone"], "UTC");
        let c = &body["current"];
        assert_eq!(
            (c["visitors"].as_u64(), c["payments"].as_u64()),
            (Some(2), Some(1))
        );
        assert_eq!(c["revenue_usd"], 49.0);
        assert_eq!(c["conversion"], 0.5);
        assert_eq!(c["revenue_per_visitor"], 24.5);
        assert_eq!(body["previous"]["visitors"], 0);
        assert!(body["change"]["visitors"].is_null());
        assert!(body["change"].get("revenue_per_visitor").is_some());
    }

    #[tokio::test]
    async fn ranges_are_checked() {
        let f = fixture().await;
        let today = OffsetDateTime::now_utc().date();
        let uri = format!(
            "/api/v1/sites/example.com/overview?from={}&to={today}",
            today - Duration::days(9)
        );
        let (code, body) = get(&f.state, &uri, Some(&f.key)).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["range"]["to"], today.to_string());
        assert_eq!(body["current"]["visitors"], 2);
        assert_eq!(
            body["previous_range"]["to"],
            (today - Duration::days(10)).to_string()
        );
        for bad in [
            "range=5d",
            "from=2026-01-01",
            "from=2026-02-01&to=2026-01-01",
            "from=x&to=y",
        ] {
            let (code, body) = get(
                &f.state,
                &format!("/api/v1/sites/example.com/overview?{bad}"),
                Some(&f.key),
            )
            .await;
            assert_eq!(code, StatusCode::BAD_REQUEST, "{bad}");
            assert!(body["error"].is_string());
        }
    }

    #[tokio::test]
    async fn breakdowns_carry_revenue() {
        let f = fixture().await;
        let (code, body) = get(
            &f.state,
            "/api/v1/sites/example.com/breakdown/channel",
            Some(&f.key),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["dimension"], "channel");
        let row = &body["rows"][0];
        assert_eq!(row["key"], "AI");
        assert_eq!(
            (row["visitors"].as_u64(), row["payments"].as_u64()),
            (Some(2), Some(1))
        );
        assert_eq!(row["revenue_usd"], 49.0);
        for dim in DIMENSIONS.map(|d| d.0) {
            let (code, body) = get(
                &f.state,
                &format!("/api/v1/sites/example.com/breakdown/{dim}?limit=1"),
                Some(&f.key),
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{dim}");
            assert!(body["rows"].as_array().unwrap().len() <= 1, "{dim}");
            assert!(body["total"].as_u64().unwrap() >= 1, "{dim}");
        }
        let (_, body) = get(
            &f.state,
            "/api/v1/sites/example.com/breakdown/page",
            Some(&f.key),
        )
        .await;
        assert_eq!(body["total"], 2);
        let (code, body) = get(
            &f.state,
            "/api/v1/sites/example.com/breakdown/planet",
            Some(&f.key),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("landing_page"));
    }

    #[tokio::test]
    async fn payments_list_their_touch() {
        let f = fixture().await;
        let (code, body) = get(
            &f.state,
            "/api/v1/sites/example.com/payments?limit=5&filter=attributed",
            Some(&f.key),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(
            (body["total"].as_u64(), body["limit"].as_u64()),
            (Some(1), Some(5))
        );
        let p = &body["rows"][0];
        assert_eq!(p["provider_payment_id"], "pi_1");
        assert_eq!(p["net_usd"], 49.0);
        assert_eq!(p["visitor"], "v1");
        assert_eq!(p["touch"]["source"], "ChatGPT");
        assert_eq!(p["touch"]["campaign"], "launch");
        assert!(p["ts"].as_str().unwrap().ends_with('Z'));
        let (_, body) = get(
            &f.state,
            "/api/v1/sites/example.com/payments?filter=unattributed",
            Some(&f.key),
        )
        .await;
        assert_eq!(body["total"], 0);
        let (code, _) = get(
            &f.state,
            "/api/v1/sites/example.com/payments?filter=big",
            Some(&f.key),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn visitor_timeline_shows_visits_and_payments() {
        let f = fixture().await;
        let (code, body) = get(
            &f.state,
            "/api/v1/sites/example.com/visitors/v1",
            Some(&f.key),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["visitor_id"], "v1");
        let visit = &body["entries"][0];
        assert_eq!(visit["type"], "visit");
        assert_eq!(visit["source"], "ChatGPT");
        let steps: Vec<&str> = visit["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["type"].as_str().unwrap())
            .collect();
        assert_eq!(steps, ["page", "page", "exit", "payment"]);
        assert_eq!(body["payments"][0]["amount"], 4900);
        let (code, _) = get(
            &f.state,
            "/api/v1/sites/example.com/visitors/nobody",
            Some(&f.key),
        )
        .await;
        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn settings_create_show_once_and_revoke() {
        let f = fixture().await;
        let session = random_token();
        sqlx::query("INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, 0, ?)")
            .bind(hash_token(&session))
            .bind(&f.account)
            .bind(now() + 3600)
            .execute(&f.state.db)
            .await
            .unwrap();
        let send = |req: axum::http::request::Builder, body: String| {
            let app = router(f.state.clone());
            let req = req
                .header("cookie", format!("virling_session={session}"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap();
            async move {
                let res = app.oneshot(req).await.unwrap();
                let status = res.status();
                let bytes = axum::body::to_bytes(res.into_body(), 1 << 22)
                    .await
                    .unwrap();
                (status, String::from_utf8(bytes.to_vec()).unwrap())
            }
        };
        let (code, html) = send(
            Request::post("/app/example.com/settings/api-keys"),
            "name=Laptop".into(),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        let keys = list_keys(&f.state.db, &f.account).await.unwrap();
        let laptop = keys.iter().find(|k| k.name == "Laptop").unwrap();
        let start = html.find(&laptop.prefix).unwrap();
        let shown = &html[start..start + KEY_PREFIX.len() + 64];
        assert!(key_account(&f.state.db, shown).await.is_some());
        let (_, html) = send(Request::get("/app/example.com/settings"), String::new()).await;
        assert!(html.contains("Laptop") && !html.contains(shown));
        let (code, _) = send(
            Request::post("/app/example.com/settings/api-keys/revoke"),
            format!("id={}", laptop.id),
        )
        .await;
        assert_eq!(code, StatusCode::SEE_OTHER);
        assert!(key_account(&f.state.db, shown).await.is_none());
        assert!(key_account(&f.state.db, &f.key).await.is_some());
    }
}
