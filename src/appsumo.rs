//! AppSumo lifetime licenses on the hosted service: OAuth redemption at /appsumo and the license webhooks.

use askama::Template;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use crate::app::AppState;
use crate::auth::{Account, hash_token, random_token};

pub const PATH: &str = "/appsumo";
/// Where buyers manage their AppSumo products and change tier.
const PRODUCTS_URL: &str = "https://appsumo.com/account/products/";
const CLAIM_COOKIE: &str = "virling_appsumo";
const CLAIM_TTL_SECS: i64 = 3600;

const INVALID: &str = "AppSumo didn't confirm this license. Start again from your AppSumo account.";
const USED: &str = "This license is already redeemed on another Virling account.";
const REFUNDED: &str =
    "This license was refunded or deactivated on AppSumo, so it can't be redeemed.";
const STACKED: &str = "Your account already has a lifetime license. Change tier on AppSumo instead of adding another code.";
const SUBSCRIBED: &str = "Your account has a monthly plan. Cancel it in Account settings first, or redeem with another email.";
const MEMBER: &str = "Team members can't redeem a license. Sign in with your own account.";
const EXPIRED: &str = "This redemption expired. Start again from your AppSumo account.";

/// AppSumo licensing for the hosted service, set with --appsumo-api-key.
#[derive(Debug, Clone)]
pub struct AppSumo {
    pub client_id: String,
    pub client_secret: String,
    /// The partner API key: it signs webhooks and reads the Licensing API.
    pub api_key: String,
    /// The AppSumo tier numbers sold as Personal and Agency.
    pub personal_tier: i64,
    pub agency_tier: i64,
    /// https://appsumo.com, where OAuth runs.
    pub oauth_url: String,
    /// https://api.licensing.appsumo.com
    pub api_url: String,
}

impl AppSumo {
    /// The account type an AppSumo tier unlocks.
    pub fn kind(&self, tier: i64) -> Option<&'static str> {
        match tier {
            t if t == self.agency_tier => Some("agency"),
            t if t == self.personal_tier => Some("personal"),
            _ => None,
        }
    }
}

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Checks X-Appsumo-Signature: hex HMAC-SHA256 of the X-Appsumo-Timestamp value followed by the body, keyed with the API key.
pub fn verify(api_key: &str, timestamp: &str, body: &[u8], signature: &str) -> bool {
    let mut msg = timestamp.as_bytes().to_vec();
    msg.extend_from_slice(body);
    let want = hex(&crate::crypto::hmac_sha256(api_key.as_bytes(), &msg));
    let got = signature.trim().to_ascii_lowercase();
    !timestamp.is_empty()
        && want.len() == got.len()
        && want
            .bytes()
            .zip(got.bytes())
            .fold(0, |acc, (a, b)| acc | (a ^ b))
            == 0
}

#[derive(Deserialize)]
struct Hook {
    license_key: String,
    prev_license_key: Option<String>,
    event: String,
    tier: Option<i64>,
    #[serde(default)]
    test: bool,
    #[serde(default)]
    event_timestamp: i64,
    parent_license_key: Option<String>,
}

pub async fn webhook(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(sumo) = &state.config.appsumo else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };
    if !verify(
        &sumo.api_key,
        header("x-appsumo-timestamp"),
        &body,
        header("x-appsumo-signature"),
    ) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(hook) = serde_json::from_slice::<Hook>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    // Test requests and deal add-ons are acknowledged without changing anything.
    if !hook.test
        && hook.parent_license_key.is_none()
        && let Err(err) = apply(&state.db, sumo, &hook).await
    {
        tracing::error!(%err, "handling an AppSumo webhook failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    axum::Json(json!({ "event": hook.event, "success": true })).into_response()
}

/// Records a license event once, ignoring events older than the last one applied to that license.
async fn apply(db: &SqlitePool, sumo: &AppSumo, h: &Hook) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    match h.event.as_str() {
        "purchase" => {
            sqlx::query(
                "INSERT INTO appsumo_licenses (license_key, tier, status, event_at) VALUES (?, ?, 'inactive', ?)
                 ON CONFLICT DO NOTHING",
            )
            .bind(&h.license_key)
            .bind(h.tier)
            .bind(h.event_timestamp)
            .execute(&mut *tx)
            .await?;
        }
        // An upgrade or downgrade issues a new key, which takes over the account of the previous one.
        "activate" | "upgrade" | "downgrade" => {
            sqlx::query(
                "INSERT INTO appsumo_licenses (license_key, tier, status, account_id, prev_license_key, event_at)
                 SELECT ?1, ?2, 'active', p.account_id, ?3, ?4
                 FROM (SELECT 1) LEFT JOIN appsumo_licenses p ON p.license_key = ?3 WHERE true
                 ON CONFLICT (license_key) DO UPDATE SET tier = coalesce(excluded.tier, tier), status = 'active',
                   account_id = coalesce(account_id, excluded.account_id),
                   prev_license_key = coalesce(excluded.prev_license_key, prev_license_key),
                   event_at = excluded.event_at
                 WHERE excluded.event_at >= appsumo_licenses.event_at",
            )
            .bind(&h.license_key)
            .bind(h.tier)
            .bind(&h.prev_license_key)
            .bind(h.event_timestamp)
            .execute(&mut *tx)
            .await?;
        }
        "deactivate" => {
            sqlx::query(
                "INSERT INTO appsumo_licenses (license_key, tier, status, event_at) VALUES (?, ?, 'deactivated', ?)
                 ON CONFLICT (license_key) DO UPDATE SET status = 'deactivated', event_at = excluded.event_at
                 WHERE excluded.event_at >= appsumo_licenses.event_at",
            )
            .bind(&h.license_key)
            .bind(h.tier)
            .bind(h.event_timestamp)
            .execute(&mut *tx)
            .await?;
        }
        _ => {}
    }
    let account: Option<String> =
        sqlx::query_scalar("SELECT account_id FROM appsumo_licenses WHERE license_key = ?")
            .bind(&h.license_key)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    if let Some(account) = account {
        sync(&mut tx, sumo, &account).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Sets the plan from the account's newest active license, or ends a lifetime plan that has none left.
async fn sync(tx: &mut SqliteConnection, sumo: &AppSumo, account: &str) -> sqlx::Result<()> {
    let tiers: Vec<i64> = sqlx::query_scalar(
        "SELECT tier FROM appsumo_licenses WHERE account_id = ? AND status = 'active' AND tier IS NOT NULL
         ORDER BY event_at DESC",
    )
    .bind(account)
    .fetch_all(&mut *tx)
    .await?;
    match tiers.into_iter().find_map(|t| sumo.kind(t)) {
        Some(kind) => {
            sqlx::query("UPDATE accounts SET plan = 'lifetime', account_type = ? WHERE id = ?")
                .bind(kind)
                .bind(account)
                .execute(&mut *tx)
                .await?;
        }
        None => {
            sqlx::query(
                "UPDATE accounts SET plan = 'self_hosted', account_type = 'personal' WHERE id = ? AND plan = 'lifetime'",
            )
            .bind(account)
            .execute(&mut *tx)
            .await?;
        }
    }
    Ok(())
}

/// Whether the account holds a lifetime plan, which only AppSumo changes.
pub async fn lifetime(db: &SqlitePool, account: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT plan = 'lifetime' FROM accounts WHERE id = ?")
        .bind(account)
        .fetch_optional(db)
        .await
        .map(|v| v.unwrap_or(false))
}

/// The active license's AppSumo tier of a lifetime account.
pub async fn tier(db: &SqlitePool, account: &str) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar(
        "SELECT l.tier FROM appsumo_licenses l JOIN accounts a ON a.id = l.account_id
         WHERE a.id = ? AND a.plan = 'lifetime' AND l.status = 'active' ORDER BY l.event_at DESC LIMIT 1",
    )
    .bind(account)
    .fetch_optional(db)
    .await
    .map(Option::flatten)
}

/// The license key and status behind an OAuth code, the documented way.
async fn license_from_code(
    state: &AppState,
    sumo: &AppSumo,
    code: &str,
) -> anyhow::Result<(String, String)> {
    #[derive(Deserialize)]
    struct Token {
        #[serde(default)]
        access_token: String,
    }
    #[derive(Deserialize)]
    struct License {
        license_key: String,
        status: String,
    }
    let token: Token = state
        .http
        .post(format!("{}/openid/token/", sumo.oauth_url))
        .json(&json!({
            "client_id": sumo.client_id,
            "client_secret": sumo.client_secret,
            "code": code,
            "redirect_uri": format!("{}{PATH}", state.config.base_url),
            "grant_type": "authorization_code",
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    anyhow::ensure!(!token.access_token.is_empty(), "no access token");
    let license: License = state
        .http
        .get(format!("{}/openid/license_key/", sumo.oauth_url))
        .query(&[("access_token", &token.access_token)])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    anyhow::ensure!(
        !license.license_key.is_empty()
            && license
                .license_key
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "not a license key"
    );
    Ok((license.license_key, license.status))
}

/// Stores a license OAuth confirmed, reading its tier from the Licensing API when no webhook brought it.
async fn record(state: &AppState, sumo: &AppSumo, key: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO appsumo_licenses (license_key, status) VALUES (?, 'inactive') ON CONFLICT DO NOTHING",
    )
    .bind(key)
    .execute(&state.db)
    .await?;
    let tier: Option<i64> =
        sqlx::query_scalar("SELECT tier FROM appsumo_licenses WHERE license_key = ?")
            .bind(key)
            .fetch_one(&state.db)
            .await?;
    if tier.is_some() {
        return Ok(());
    }
    #[derive(Deserialize)]
    struct License {
        tier: Option<i64>,
    }
    let license: License = state
        .http
        .get(format!("{}/v2/licenses/{key}", sumo.api_url))
        .header("X-AppSumo-Licensing-Key", &sumo.api_key)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    sqlx::query("UPDATE appsumo_licenses SET tier = coalesce(tier, ?) WHERE license_key = ?")
        .bind(license.tier)
        .bind(key)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Links a confirmed license to the owner's account once, then returns its tier.
async fn redeem(
    db: &SqlitePool,
    sumo: &AppSumo,
    account: &Account,
    key: &str,
) -> anyhow::Result<Result<i64, &'static str>> {
    if !account.owner {
        return Ok(Err(MEMBER));
    }
    let mut tx = db.begin().await?;
    let row: Option<(Option<i64>, String, Option<String>)> = sqlx::query_as(
        "SELECT tier, status, account_id FROM appsumo_licenses WHERE license_key = ?",
    )
    .bind(key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((tier, status, owner)) = row else {
        return Ok(Err(INVALID));
    };
    if status == "deactivated" {
        return Ok(Err(REFUNDED));
    }
    match (owner, tier) {
        (Some(o), Some(t)) if o == account.id => return Ok(Ok(t)),
        (Some(_), _) => return Ok(Err(USED)),
        _ => {}
    }
    let Some(tier) = tier.filter(|t| sumo.kind(*t).is_some()) else {
        return Ok(Err(INVALID));
    };
    let (stacked, subscribed): (bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM appsumo_licenses WHERE account_id = ?1 AND status = 'active'),
                EXISTS (SELECT 1 FROM billing WHERE account_id = ?1
                        AND status IN ('active', 'past_due', 'on_hold', 'paused'))",
    )
    .bind(&account.id)
    .fetch_one(&mut *tx)
    .await?;
    if stacked {
        return Ok(Err(STACKED));
    }
    if subscribed {
        return Ok(Err(SUBSCRIBED));
    }
    let linked = sqlx::query(
        "UPDATE appsumo_licenses SET account_id = ?, status = 'active', redeemed_at = ?,
           claim_hash = NULL, claim_expires_at = NULL
         WHERE license_key = ? AND account_id IS NULL",
    )
    .bind(&account.id)
    .bind(now())
    .bind(key)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if linked == 0 {
        return Ok(Err(USED));
    }
    sync(&mut tx, sumo, &account.id).await?;
    tx.commit().await?;
    Ok(Ok(tier))
}

#[derive(Template)]
#[template(path = "appsumo.html")]
struct Page {
    /// "start", "signin" or "done".
    view: &'static str,
    tier: i64,
    plan: &'static str,
    events: String,
    error: &'static str,
    products: &'static str,
}

fn page(sumo: &AppSumo, view: &'static str, tier: i64, error: &'static str) -> Page {
    let kind = sumo.kind(tier).unwrap_or("personal");
    Page {
        view,
        tier,
        plan: crate::dashboard::plan_label(kind),
        events: crate::dashboard::group(crate::usage::limit(kind).0),
        error,
        products: PRODUCTS_URL,
    }
}

fn respond(p: Page, cookies: &[String]) -> Response {
    let Ok(html) = p.render() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut res = Html(html).into_response();
    let h = res.headers_mut();
    h.insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    for c in cookies {
        if let Ok(v) = HeaderValue::from_str(c) {
            h.append(header::SET_COOKIE, v);
        }
    }
    res
}

#[derive(Deserialize)]
pub struct PageQuery {
    code: Option<String>,
}

/// The unlisted redemption page and the OAuth redirect URL: AppSumo validates it with a bare GET, so it never redirects.
pub async fn redeem_page(
    State(state): State<AppState>,
    account: Result<Account, Redirect>,
    headers: HeaderMap,
    Query(q): Query<PageQuery>,
) -> Response {
    let Some(sumo) = state.config.appsumo.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let account = account.ok();
    let secure = if state.config.base_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    let clear = format!("{CLAIM_COOKIE}=; Path={PATH}; HttpOnly; SameSite=Lax; Max-Age=0{secure}");
    let done =
        async |key: &str, account: &Account| match redeem(&state.db, sumo, account, key).await {
            Ok(Ok(tier)) => respond(page(sumo, "done", tier, ""), std::slice::from_ref(&clear)),
            Ok(Err(error)) => respond(page(sumo, "start", 0, error), std::slice::from_ref(&clear)),
            Err(err) => {
                tracing::error!(%err, "redeeming an AppSumo license failed");
                StatusCode::SERVICE_UNAVAILABLE.into_response()
            }
        };
    if let Some(code) = q.code.filter(|c| !c.is_empty()) {
        let confirmed = async {
            let (key, status) = license_from_code(&state, sumo, &code).await?;
            if status != "deactivated" {
                record(&state, sumo, &key).await?;
            }
            anyhow::Ok((key, status))
        }
        .await;
        let key = match confirmed {
            Ok((_, status)) if status == "deactivated" => {
                return respond(page(sumo, "start", 0, REFUNDED), &[]);
            }
            Ok((key, _)) => key,
            Err(err) => {
                tracing::warn!(%err, "AppSumo OAuth failed");
                return respond(page(sumo, "start", 0, INVALID), &[]);
            }
        };
        if let Some(account) = &account {
            return done(&key, account).await;
        }
        let token = random_token();
        let claimed: Option<(Option<i64>, String)> = match sqlx::query_as(
            "UPDATE appsumo_licenses SET claim_hash = ?, claim_expires_at = ?
             WHERE license_key = ? RETURNING tier, status",
        )
        .bind(hash_token(&token))
        .bind(now() + CLAIM_TTL_SECS)
        .bind(&key)
        .fetch_optional(&state.db)
        .await
        {
            Ok(c) => c,
            Err(err) => {
                tracing::error!(%err, "storing an AppSumo claim failed");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        };
        return match claimed {
            None => respond(page(sumo, "start", 0, INVALID), &[]),
            Some((_, status)) if status == "deactivated" => {
                respond(page(sumo, "start", 0, REFUNDED), &[])
            }
            Some((tier, _)) => respond(
                page(sumo, "signin", tier.unwrap_or(0), ""),
                &[
                    format!(
                        "{CLAIM_COOKIE}={token}; Path={PATH}; HttpOnly; SameSite=Lax; Max-Age={CLAIM_TTL_SECS}{secure}"
                    ),
                    format!(
                        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={CLAIM_TTL_SECS}{secure}",
                        crate::oauth::NEXT_COOKIE,
                        hex(PATH.as_bytes())
                    ),
                ],
            ),
        };
    }
    if let Some(token) = crate::auth::cookie(&headers, CLAIM_COOKIE) {
        let claimed: Option<(String, Option<i64>)> = match sqlx::query_as(
            "SELECT license_key, tier FROM appsumo_licenses WHERE claim_hash = ? AND claim_expires_at > ?",
        )
        .bind(hash_token(&token))
        .bind(now())
        .fetch_optional(&state.db)
        .await
        {
            Ok(c) => c,
            Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        };
        match (claimed, &account) {
            (None, None) => return respond(page(sumo, "start", 0, EXPIRED), &[clear]),
            (None, Some(_)) => {}
            (Some((_, tier)), None) => {
                return respond(page(sumo, "signin", tier.unwrap_or(0), ""), &[]);
            }
            (Some((key, _)), Some(account)) => return done(&key, account).await,
        }
    }
    if let Some(account) = &account
        && let Ok(Some(tier)) = tier(&state.db, &account.id).await
    {
        return respond(page(sumo, "done", tier, ""), &[clear]);
    }
    respond(page(sumo, "start", 0, ""), &[])
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::Request;
    use axum::routing::get;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    const KEY: &str = "c73455d8-0f64-4354-bd64-1f454a227104";
    const L1: &str = "3794577c-3dbc-11ec-9bbc-0242ac130002";
    const L2: &str = "c86ad3d7-3942-4d11-8814-b0bd81971691";
    const L3: &str = "c8e57fa3-ea5b-4c39-a2bf-74f7f51d01b0";

    /// A local stand-in for AppSumo's OAuth and Licensing API: code "c-<key>" grants license <key>.
    async fn mock() -> String {
        let app = axum::Router::new()
            .route(
                "/openid/token/",
                axum::routing::post(|axum::Json(b): axum::Json<Value>| async move {
                    assert_eq!(b["grant_type"], "authorization_code");
                    assert_eq!(b["client_id"], "cid");
                    assert_eq!(b["client_secret"], "csecret");
                    assert_eq!(b["redirect_uri"], "https://app.virling.com/appsumo");
                    let code = b["code"].as_str().unwrap();
                    match code.strip_prefix("c-") {
                        Some(key) => axum::Json(json!({ "access_token": format!("t-{key}"), "token_type": "bearer", "expires_in": 3600 })).into_response(),
                        None => StatusCode::FORBIDDEN.into_response(),
                    }
                }),
            )
            .route(
                "/openid/license_key/",
                get(|Query(q): Query<std::collections::HashMap<String, String>>| async move {
                    let key = q["access_token"].strip_prefix("t-").unwrap().to_string();
                    let status = if key.starts_with("dead") { "deactivated" } else { "active" };
                    axum::Json(json!({ "license_key": key.replace("dead", "aaaa"), "status": status, "scopes": ["read_license"] }))
                }),
            )
            .route(
                "/v2/licenses/{key}",
                get(|headers: HeaderMap| async move {
                    assert_eq!(headers["x-appsumo-licensing-key"], KEY);
                    axum::Json(json!({ "status": "active", "tier": 2 }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    struct T {
        f: crate::api::tests::Fixture,
        cookie: String,
    }

    async fn setup() -> T {
        let mut f = crate::api::tests::fixture().await;
        let url = mock().await;
        let mut config = (*f.state.config).clone();
        config.hosted = true;
        config.base_url = "https://app.virling.com".into();
        config.appsumo = Some(AppSumo {
            client_id: "cid".into(),
            client_secret: "csecret".into(),
            api_key: KEY.into(),
            personal_tier: 1,
            agency_tier: 2,
            oauth_url: url.clone(),
            api_url: url,
        });
        f.state.config = Arc::new(config);
        let token = random_token();
        sqlx::query("INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, 0, ?)")
            .bind(hash_token(&token))
            .bind(&f.account)
            .bind(now() + 3600)
            .execute(&f.state.db)
            .await
            .unwrap();
        T {
            f,
            cookie: format!("virling_session={token}"),
        }
    }

    /// A message as the page escapes it.
    fn shown(s: &str) -> String {
        s.replace('\'', "&#39;")
    }

    fn sign(body: &str, ts: &str) -> String {
        let mut msg = ts.as_bytes().to_vec();
        msg.extend_from_slice(body.as_bytes());
        hex(&crate::crypto::hmac_sha256(KEY.as_bytes(), &msg))
    }

    async fn hook(t: &T, body: Value) -> (StatusCode, String) {
        let body = body.to_string();
        let ts = "1671586387628";
        let res = crate::app::router(t.f.state.clone())
            .oneshot(
                Request::post("/api/webhooks/appsumo")
                    .header("content-type", "application/json")
                    .header("x-appsumo-timestamp", ts)
                    .header("x-appsumo-signature", sign(&body, ts))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    async fn get_page(t: &T, path: &str, cookie: &str) -> (StatusCode, HeaderMap, String) {
        let res = crate::app::router(t.f.state.clone())
            .layer(MockConnectInfo(SocketAddr::from(([203, 0, 113, 7], 5555))))
            .oneshot(
                Request::get(path)
                    .header("cookie", cookie)
                    .header("accept", "text/html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, headers) = (res.status(), res.headers().clone());
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
    }

    async fn plan_of(t: &T, account: &str) -> (String, String) {
        sqlx::query_as("SELECT plan, account_type FROM accounts WHERE id = ?")
            .bind(account)
            .fetch_one(&t.f.state.db)
            .await
            .unwrap()
    }

    fn event(event: &str, key: &str, tier: i64, at: i64) -> Value {
        json!({ "license_key": key, "event": event, "license_status": "inactive", "event_timestamp": at, "created_at": 1671586387624_i64, "tier": tier, "test": false, "extra": { "reason": "Purchased by the customer" } })
    }

    #[test]
    fn verifies_the_documented_signature() {
        let body = r#"{"license_key":"3794577c-3dbc-11ec-9bbc-0242ac130002","event":"purchase","status":"active","event_timestamp":1318781876406,"test":false}"#;
        // Computed independently with Python's hmac, as in AppSumo's example code.
        let want = "e8f1b04eab1a8775a92c402c871db838e2f3843757fa3f6437d22079cfa68d9f";
        assert!(verify(KEY, "1625165884119", body.as_bytes(), want));
        assert!(verify(
            KEY,
            "1625165884119",
            body.as_bytes(),
            &want.to_uppercase()
        ));
        assert!(!verify(KEY, "1625165884120", body.as_bytes(), want));
        assert!(!verify(KEY, "1625165884119", b"{}", want));
        assert!(!verify("other", "1625165884119", body.as_bytes(), want));
        assert!(!verify(KEY, "", body.as_bytes(), want));
        assert!(!verify(KEY, "1625165884119", body.as_bytes(), ""));
    }

    #[tokio::test]
    async fn webhook_rejects_a_bad_signature_and_answers_tests() {
        let t = setup().await;
        let body = event("activate", L1, 1, 1).to_string();
        let res = crate::app::router(t.f.state.clone())
            .oneshot(
                Request::post("/api/webhooks/appsumo")
                    .header("x-appsumo-timestamp", "1")
                    .header("x-appsumo-signature", sign(&body, "2"))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM appsumo_licenses")
            .fetch_one(&t.f.state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
        let mut test = event("purchase", "00000000-aaaa-1111-bbbb-abcdef012345", 1, 1);
        test["test"] = json!(true);
        let (status, body) = hook(&t, test).await;
        assert_eq!(status, StatusCode::OK);
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v, json!({ "event": "purchase", "success": true }));
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM appsumo_licenses")
            .fetch_one(&t.f.state.db)
            .await
            .unwrap();
        assert_eq!(n, 0, "test webhooks change nothing");
    }

    #[tokio::test]
    async fn bare_redirect_url_answers_200_unlisted() {
        let t = setup().await;
        let (status, headers, html) = get_page(&t, "/appsumo", "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["x-robots-tag"], "noindex, nofollow");
        assert!(html.contains(r#"<meta name="robots" content="noindex, nofollow">"#));
        assert!(html.contains("Redeem your AppSumo license"));
        let mut state = t.f.state.clone();
        let mut config = (*state.config).clone();
        config.appsumo = None;
        state.config = Arc::new(config);
        let res = crate::app::router(state)
            .oneshot(Request::get("/appsumo").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn redeems_once_for_a_signed_in_owner() {
        let t = setup().await;
        assert_eq!(
            hook(&t, event("purchase", L1, 1, 1)).await.0,
            StatusCode::OK
        );
        assert_eq!(
            hook(&t, event("activate", L1, 1, 2)).await.0,
            StatusCode::OK
        );
        assert_eq!(plan_of(&t, &t.f.account).await.0, "self_hosted");
        let (status, _, html) = get_page(&t, &format!("/appsumo?code=c-{L1}"), &t.cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Your lifetime plan is active"), "{html}");
        assert!(html.contains("100,000 events"));
        assert_eq!(
            plan_of(&t, &t.f.account).await,
            ("lifetime".into(), "personal".into())
        );
        let billing: i64 = sqlx::query_scalar("SELECT count(*) FROM billing")
            .fetch_one(&t.f.state.db)
            .await
            .unwrap();
        assert_eq!(billing, 0, "no subscription");
        // The same license on another account is refused.
        let other: String =
            sqlx::query_scalar("SELECT id FROM accounts WHERE email = 'other@example.com'")
                .fetch_one(&t.f.state.db)
                .await
                .unwrap();
        let token = random_token();
        sqlx::query("INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, 0, ?)")
            .bind(hash_token(&token))
            .bind(&other)
            .bind(now() + 3600)
            .execute(&t.f.state.db)
            .await
            .unwrap();
        let (_, _, html) = get_page(
            &t,
            &format!("/appsumo?code=c-{L1}"),
            &format!("virling_session={token}"),
        )
        .await;
        assert!(html.contains(USED), "{html}");
        assert_eq!(plan_of(&t, &other).await.0, "self_hosted");
        // A second code on the first account is not stacked.
        let (_, _, html) = get_page(&t, &format!("/appsumo?code=c-{L3}"), &t.cookie).await;
        assert!(html.contains(&shown(STACKED)), "{html}");
        // Bad codes and refunded licenses show clear errors.
        let (_, _, html) = get_page(&t, "/appsumo?code=bogus", &t.cookie).await;
        assert!(html.contains(&shown(INVALID)));
        let (_, _, html) = get_page(&t, "/appsumo?code=c-dead-1", &t.cookie).await;
        assert!(html.contains(&shown(REFUNDED)));
    }

    #[tokio::test]
    async fn new_users_finish_after_the_email_sign_in() {
        let t = setup().await;
        // No webhook yet: the tier comes from the Licensing API.
        let (status, headers, html) = get_page(&t, &format!("/appsumo?code=c-{L2}"), "").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            html.contains("One step left") && html.contains("tier 2"),
            "{html}"
        );
        let cookies: Vec<String> = headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
            .collect();
        assert!(
            cookies.iter().any(|c| c == "virling_next=2f61707073756d6f"),
            "{cookies:?}"
        );
        let claim = cookies
            .iter()
            .find(|c| c.starts_with("virling_appsumo="))
            .unwrap();
        // The magic link signs a new user in and returns here.
        let account = crate::sites::find_or_create_account(&t.f.state.db, "new@example.com")
            .await
            .unwrap();
        let res = crate::auth::start_session(&t.f.state, &account)
            .await
            .unwrap();
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("virling_next=2f61707073756d6f"),
        );
        let res = crate::oauth::resume(&h, res);
        assert_eq!(res.headers()[header::LOCATION], "/appsumo");
        let session = res.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let (_, headers, html) = get_page(&t, "/appsumo", &format!("{session}; {claim}")).await;
        assert!(
            html.contains("Your lifetime plan is active") && html.contains("1,000,000 events"),
            "{html}"
        );
        assert!(
            headers[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        assert_eq!(
            plan_of(&t, &account).await,
            ("lifetime".into(), "agency".into())
        );
        // The claim is spent.
        let (_, _, html) = get_page(&t, "/appsumo", &format!("{session}; {claim}")).await;
        assert!(html.contains("Your lifetime plan is active"));
        let (_, _, html) = get_page(&t, "/appsumo", claim).await;
        assert!(html.contains(&shown(EXPIRED)), "{html}");
    }

    #[tokio::test]
    async fn upgrade_downgrade_and_refund_follow_the_license() {
        let t = setup().await;
        hook(&t, event("activate", L1, 1, 10)).await;
        get_page(&t, &format!("/appsumo?code=c-{L1}"), &t.cookie).await;
        assert_eq!(
            plan_of(&t, &t.f.account).await,
            ("lifetime".into(), "personal".into())
        );
        let mut up = event("upgrade", L2, 2, 20);
        up["prev_license_key"] = json!(L1);
        assert_eq!(hook(&t, up.clone()).await.0, StatusCode::OK);
        let mut gone = event("deactivate", L1, 1, 21);
        gone["extra"]["reason"] = json!("Upgraded by the customer");
        hook(&t, gone).await;
        assert_eq!(
            plan_of(&t, &t.f.account).await,
            ("lifetime".into(), "agency".into())
        );
        // A retried upgrade changes nothing.
        hook(&t, up).await;
        assert_eq!(
            plan_of(&t, &t.f.account).await,
            ("lifetime".into(), "agency".into())
        );
        let mut down = event("downgrade", L3, 1, 30);
        down["prev_license_key"] = json!(L2);
        hook(&t, down).await;
        hook(&t, event("deactivate", L2, 2, 31)).await;
        assert_eq!(
            plan_of(&t, &t.f.account).await,
            ("lifetime".into(), "personal".into())
        );
        // A stale event is ignored.
        hook(&t, event("activate", L2, 2, 25)).await;
        assert_eq!(
            plan_of(&t, &t.f.account).await,
            ("lifetime".into(), "personal".into())
        );
        let page = crate::billing::view(&t.f.state.db, &t.f.account, now())
            .await
            .unwrap();
        assert_eq!(page.headline, "Lifetime (AppSumo), tier 1");
        // Refund: back to no plan, sites and data kept.
        let mut refund = event("deactivate", L3, 1, 40);
        refund["license_status"] = json!("active");
        refund["extra"]["reason"] = json!("Refunded by the user");
        assert_eq!(hook(&t, refund.clone()).await.0, StatusCode::OK);
        assert_eq!(hook(&t, refund).await.0, StatusCode::OK);
        assert_eq!(
            plan_of(&t, &t.f.account).await,
            ("self_hosted".into(), "personal".into())
        );
        let sites: i64 = sqlx::query_scalar("SELECT count(*) FROM sites WHERE account_id = ?")
            .bind(&t.f.account)
            .fetch_one(&t.f.state.db)
            .await
            .unwrap();
        assert_eq!(sites, 1);
        let (_, _, html) = get_page(&t, &format!("/appsumo?code=c-{L3}"), &t.cookie).await;
        assert!(html.contains(&shown(REFUNDED)), "{html}");
    }

    #[tokio::test]
    async fn lifetime_plans_get_limits_but_no_founding_spot() {
        let t = setup().await;
        let open = crate::billing::founding_open(&t.f.state.db, "agency")
            .await
            .unwrap();
        for i in 0..crate::billing::FOUNDING_SPOTS {
            let key = format!("00000000-0000-0000-0000-{i:012}");
            let account =
                crate::sites::find_or_create_account(&t.f.state.db, &format!("b{i}@example.com"))
                    .await
                    .unwrap();
            hook(&t, event("activate", &key, 2, 1)).await;
            let a = Account {
                id: account.clone(),
                user: account,
                owner: true,
            };
            let sumo = t.f.state.config.appsumo.clone().unwrap();
            assert_eq!(redeem(&t.f.state.db, &sumo, &a, &key).await.unwrap(), Ok(2));
        }
        assert!(open);
        assert!(
            crate::billing::founding_open(&t.f.state.db, "agency")
                .await
                .unwrap()
        );
        assert!(
            crate::billing::founding_open(&t.f.state.db, "personal")
                .await
                .unwrap()
        );
        // Limits follow the tier's plan and only warn.
        hook(&t, event("activate", L1, 1, 1)).await;
        get_page(&t, &format!("/appsumo?code=c-{L1}"), &t.cookie).await;
        let month = time::OffsetDateTime::now_utc();
        let m = format!("{}-{:02}", month.year(), u8::from(month.month()));
        sqlx::query("INSERT INTO usage_months (account_id, month, used) VALUES (?, ?, 150000)")
            .bind(&t.f.account)
            .bind(&m)
            .execute(&t.f.state.db)
            .await
            .unwrap();
        let kind = plan_of(&t, &t.f.account).await.1;
        let banner = crate::usage::banner(&t.f.state.db, &t.f.account, &kind)
            .await
            .unwrap();
        assert!(
            banner.starts_with("You have used all 100,000 events")
                && banner.contains("Nothing is dropped"),
            "{banner}"
        );
        // Members can't redeem, and a lifetime account can't switch plan or start checkout.
        let a = Account {
            id: t.f.account.clone(),
            user: "someone".into(),
            owner: false,
        };
        let sumo = t.f.state.config.appsumo.clone().unwrap();
        assert_eq!(
            redeem(&t.f.state.db, &sumo, &a, L2).await.unwrap(),
            Err(MEMBER)
        );
        for path in [
            "/app/example.com/account/plan",
            "/app/example.com/account/billing",
        ] {
            let res = crate::app::router(t.f.state.clone())
                .layer(MockConnectInfo(SocketAddr::from(([203, 0, 113, 7], 5555))))
                .oneshot(
                    Request::post(path)
                        .header("cookie", &t.cookie)
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("account_type=agency&plan=agency"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(
                res.status().is_redirection() || res.status() == StatusCode::NOT_FOUND,
                "{path}"
            );
        }
        assert_eq!(
            plan_of(&t, &t.f.account).await,
            ("lifetime".into(), "personal".into())
        );
    }
}
