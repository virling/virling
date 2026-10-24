use std::net::SocketAddr;
use std::time::Duration;

use askama::Template;
use axum::extract::{ConnectInfo, FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Form, Json};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use time::OffsetDateTime;
use url::Url;

use crate::api;
use crate::app::{AppState, Config};
use crate::auth::{self, Account, hash_token, random_token};
use crate::onboarding::render;

const CODE_TTL_SECS: i64 = 5 * 60;
const ACCESS_TTL_SECS: i64 = 3600;
const REFRESH_TTL_SECS: i64 = 30 * 24 * 3600;
pub const ACCESS_PREFIX: &str = "vo_";
const REFRESH_PREFIX: &str = "vr_";
/// The one scope: read every site of the account, like an API key.
const SCOPE: &str = "read";
const NEXT_COOKIE: &str = "virling_next";
const DOCS: &str = "https://docs.virling.com/reference/mcp.html";
const REGISTER_PER_SECOND: u32 = 3;
const TOKEN_PER_SECOND: u32 = 20;
const METADATA_MAX_BYTES: usize = 5 * 1024;
const METADATA_MAX_AGE_SECS: i64 = 86_400;

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// OAuth sign-in needs a public https base URL at the root of its host, where clients look for the metadata.
pub fn enabled(config: &Config) -> bool {
    Url::parse(&config.base_url).is_ok_and(|u| u.scheme() == "https" && u.path() == "/")
}

/// The MCP endpoint's canonical URI on the base URL, which tokens are bound to by default.
fn resource(config: &Config) -> String {
    format!("{}/mcp", config.base_url)
}

/// Whether the request was sent to the dedicated MCP host.
pub fn on_mcp_host(config: &Config, headers: &HeaderMap) -> bool {
    let Some(mcp) = config.mcp_url.as_deref().and_then(|u| Url::parse(u).ok()) else {
        return false;
    };
    let sent = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| Url::parse(&format!("https://{h}")).ok());
    sent.is_some_and(|s| s.host_str().is_some() && s.host_str() == mcp.host_str())
}

/// The MCP endpoint URI of the host a request was sent to.
fn resource_for(config: &Config, headers: &HeaderMap) -> String {
    match &config.mcp_url {
        Some(mcp) if on_mcp_host(config, headers) => format!("{mcp}/mcp"),
        _ => resource(config),
    }
}

/// The canonical resource a client asked for, when it is one this server issues tokens for.
fn asked_resource(config: &Config, asked: &str) -> Option<String> {
    let asked = asked.trim_end_matches('/');
    std::iter::once(resource(config))
        .chain(config.mcp_url.iter().map(|u| format!("{u}/mcp")))
        .find(|r| r == asked)
}

fn same_resource(config: &Config, asked: &str) -> bool {
    asked_resource(config, asked).is_some()
}

/// RFC 9728 metadata of the MCP endpoint, at the root and path-suffixed well-known URIs.
pub async fn protected_resource(State(state): State<AppState>, headers: HeaderMap) -> Json<Value> {
    Json(json!({
        "resource": resource_for(&state.config, &headers),
        "authorization_servers": [state.config.base_url],
        "scopes_supported": [SCOPE],
        "bearer_methods_supported": ["header"],
        "resource_name": "Virling",
        "resource_documentation": DOCS,
    }))
}

/// RFC 8414 metadata; this server is its own authorization server.
pub async fn authorization_server(State(state): State<AppState>) -> Json<Value> {
    let base = &state.config.base_url;
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "scopes_supported": [SCOPE],
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "client_id_metadata_document_supported": true,
        "authorization_response_iss_parameter_supported": true,
        "service_documentation": DOCS,
    }))
}

fn loopback(u: &Url) -> bool {
    matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

/// https, or http on a loopback host for apps on the user's machine; never a fragment.
fn valid_redirect(uri: &str) -> bool {
    let Ok(u) = Url::parse(uri) else {
        return false;
    };
    uri.len() <= 2000
        && u.fragment().is_none()
        && match u.scheme() {
            "https" => u.host().is_some(),
            "http" => loopback(&u),
            _ => false,
        }
}

/// Exact match, except that a loopback redirect may use any port (RFC 8252).
fn redirect_matches(registered: &str, asked: &str) -> bool {
    if registered == asked {
        return true;
    }
    let (Ok(mut r), Ok(mut a)) = (Url::parse(registered), Url::parse(asked)) else {
        return false;
    };
    if r.scheme() != "http" || !loopback(&r) {
        return false;
    }
    let _ = (r.set_port(None), a.set_port(None));
    r == a
}

fn client_error(code: &str, description: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": code, "error_description": description })),
    )
        .into_response()
}

/// Per-IP requests per second, with the ingest limiter; a 429 when over.
fn limited(
    state: &AppState,
    route: &str,
    peer: SocketAddr,
    headers: &HeaderMap,
    limit: u32,
) -> Option<Response> {
    let trust = state.config.trust_proxy;
    let ip = crate::ingest::client_ip(
        headers,
        peer,
        trust,
        crate::ingest::via_cloudflare(peer.ip(), trust),
    );
    (!state.limiter.allow(&format!("oauth-{route}:{ip}"), limit)).then(|| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            Json(json!({ "error": "too_many_requests" })),
        )
            .into_response()
    })
}

/// RFC 7591 registration of a public client; it gets no secret and uses PKCE.
pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if let Some(res) = limited(&state, "register", peer, &headers, REGISTER_PER_SECOND) {
        return res;
    }
    let Ok(meta) = serde_json::from_slice::<Value>(&body) else {
        return client_error("invalid_client_metadata", "send the metadata as JSON");
    };
    let uris: Vec<String> = meta["redirect_uris"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|u| u.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if uris.is_empty() || uris.len() > 10 || !uris.iter().all(|u| valid_redirect(u)) {
        return client_error(
            "invalid_redirect_uri",
            "redirect_uris must be https, or http on localhost",
        );
    }
    let mut name: String = meta["client_name"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(60)
        .collect();
    if name.is_empty() {
        name = host(&uris[0]);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let issued = now();
    let saved = sqlx::query(
        "INSERT INTO oauth_clients (id, name, redirect_uris, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(uris.join("\n"))
    .bind(issued)
    .execute(&state.db)
    .await;
    if let Err(err) = saved {
        tracing::error!(%err, "registering an OAuth client failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    (
        StatusCode::CREATED,
        Json(json!({
            "client_id": id,
            "client_id_issued_at": issued,
            "client_name": name,
            "redirect_uris": uris,
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
            "scope": SCOPE,
        })),
    )
        .into_response()
}

/// A Client ID Metadata Document URL: https on a domain, with a path and nothing to normalize.
fn metadata_url(id: &str) -> bool {
    let Ok(u) = Url::parse(id) else {
        return false;
    };
    id.len() <= 2000
        && u.scheme() == "https"
        && u.as_str() == id
        && matches!(u.host(), Some(url::Host::Domain(_)))
        && u.path() != "/"
        && u.fragment().is_none()
        && u.username().is_empty()
        && u.password().is_none()
}

/// The name and redirect URIs of a metadata document whose client_id is `id`.
fn from_document(id: &str, doc: &Value) -> Option<(String, Vec<String>)> {
    let uris: Vec<String> = doc["redirect_uris"]
        .as_array()?
        .iter()
        .map(|u| u.as_str().map(String::from))
        .collect::<Option<_>>()?;
    let name: String = doc["client_name"]
        .as_str()?
        .trim()
        .chars()
        .take(60)
        .collect();
    (doc["client_id"] == id
        && !name.is_empty()
        && (1..=10).contains(&uris.len())
        && uris.iter().all(|u| valid_redirect(u))
        && doc
            .get("token_endpoint_auth_method")
            .is_none_or(|m| m == "none"))
    .then_some((name, uris))
}

/// Seconds to keep a fetched document, from its Cache-Control; an hour when it has none.
fn max_age(cache_control: &str) -> i64 {
    let mut age = 3600;
    for d in cache_control.to_ascii_lowercase().split(',').map(str::trim) {
        match d.strip_prefix("max-age=") {
            Some(s) => age = s.parse().unwrap_or(0),
            None if d == "no-store" || d == "no-cache" => return 0,
            None => {}
        }
    }
    age.clamp(0, METADATA_MAX_AGE_SECS)
}

/// Fetches a client's metadata document from a public host and saves it as the client's row.
async fn fetch_client(state: &AppState, id: &str) -> anyhow::Result<()> {
    let res = state
        .favicons
        .get(id)
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(Duration::from_secs(3))
        .send()
        .await?;
    anyhow::ensure!(
        res.status() == StatusCode::OK && res.url().as_str() == id,
        "not served at its own URL"
    );
    let age = max_age(
        res.headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default(),
    );
    let doc: Value = serde_json::from_slice(&crate::favicon::body(res, METADATA_MAX_BYTES).await?)?;
    let (name, uris) = from_document(id, &doc).ok_or(anyhow::anyhow!("invalid metadata"))?;
    sqlx::query(
        "INSERT INTO oauth_clients (id, name, redirect_uris, created_at, refresh_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name, redirect_uris = excluded.redirect_uris, refresh_at = excluded.refresh_at",
    )
    .bind(id)
    .bind(name)
    .bind(uris.join("\n"))
    .bind(now())
    .bind(now() + age)
    .execute(&state.db)
    .await?;
    Ok(())
}

fn host(uri: &str) -> String {
    Url::parse(uri)
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_default()
}

#[derive(Deserialize, Default)]
pub struct AuthorizeParams {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    state: Option<String>,
    scope: Option<String>,
    resource: Option<String>,
    decision: Option<String>,
}

impl AuthorizeParams {
    /// The request's parameters, carried through the consent form.
    fn fields(&self) -> Vec<(&'static str, String)> {
        [
            ("response_type", &self.response_type),
            ("client_id", &self.client_id),
            ("redirect_uri", &self.redirect_uri),
            ("code_challenge", &self.code_challenge),
            ("code_challenge_method", &self.code_challenge_method),
            ("state", &self.state),
            ("scope", &self.scope),
            ("resource", &self.resource),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.clone().map(|v| (k, v)))
        .collect()
    }
}

#[derive(Template)]
#[template(path = "consent.html")]
struct ConsentPage {
    /// ask, member or invalid
    view: &'static str,
    client: String,
    host: String,
    email: String,
    message: &'static str,
    fields: Vec<(&'static str, String)>,
}

fn invalid(message: &'static str) -> Response {
    let mut res = render(ConsentPage {
        view: "invalid",
        client: String::new(),
        host: String::new(),
        email: String::new(),
        message,
        fields: Vec::new(),
    });
    *res.status_mut() = StatusCode::BAD_REQUEST;
    res
}

/// Back to the app's redirect URI with `pairs`, the state and the issuer (RFC 9207).
fn back(config: &Config, redirect: &str, p: &AuthorizeParams, pairs: &[(&str, &str)]) -> Response {
    let mut url = Url::parse(redirect).expect("redirect URIs are checked at registration");
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in pairs {
            q.append_pair(k, v);
        }
        if let Some(s) = &p.state {
            q.append_pair("state", s);
        }
        q.append_pair("iss", &config.base_url);
    }
    Redirect::to(url.as_str()).into_response()
}

fn valid_challenge(c: &str) -> bool {
    c.len() == 43
        && c.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The client's name and redirect URI; an error page when either is wrong, else an error sent back to the app.
async fn check(state: &AppState, p: &AuthorizeParams) -> Result<(String, String), Box<Response>> {
    let id = p.client_id.as_deref().unwrap_or_default();
    if metadata_url(id) {
        let refresh_at: Option<Option<i64>> =
            sqlx::query_scalar("SELECT refresh_at FROM oauth_clients WHERE id = ?")
                .bind(id)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| Box::new(StatusCode::SERVICE_UNAVAILABLE.into_response()))?;
        if !refresh_at.flatten().is_some_and(|t| t > now()) {
            let fetched = match state
                .limiter
                .allow(&format!("oauth-metadata:{}", host(id)), REGISTER_PER_SECOND)
            {
                true => fetch_client(state, id).await,
                false => Err(anyhow::anyhow!("too many fetches")),
            };
            if let Err(err) = fetched {
                tracing::info!(%err, client = id, "reading client metadata failed");
                return Err(Box::new(invalid(
                    "Virling could not read this app's client metadata.",
                )));
            }
        }
    }
    let client: Option<(String, String)> =
        sqlx::query_as("SELECT name, redirect_uris FROM oauth_clients WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| Box::new(StatusCode::SERVICE_UNAVAILABLE.into_response()))?;
    let Some((name, uris)) = client else {
        return Err(Box::new(invalid(
            "This app is not registered with Virling.",
        )));
    };
    let redirect = match &p.redirect_uri {
        Some(r) => uris
            .lines()
            .any(|u| redirect_matches(u, r))
            .then(|| r.clone()),
        None => match uris.lines().collect::<Vec<_>>()[..] {
            [one] => Some(one.to_string()),
            _ => None,
        },
    };
    let Some(redirect) = redirect else {
        return Err(Box::new(invalid(
            "The return address does not match the app.",
        )));
    };
    let error = |code| {
        Err(Box::new(back(
            &state.config,
            &redirect,
            p,
            &[("error", code)],
        )))
    };
    if p.response_type.as_deref() != Some("code") {
        return error("unsupported_response_type");
    }
    if p.code_challenge_method.as_deref() != Some("S256")
        || !p.code_challenge.as_deref().is_some_and(valid_challenge)
    {
        return error("invalid_request");
    }
    if !p
        .resource
        .as_deref()
        .is_none_or(|r| same_resource(&state.config, r))
    {
        return error("invalid_target");
    }
    Ok((name, redirect))
}

/// Sends a visitor who is not signed in to the login, and back here after it.
fn to_login(uri: &Uri) -> Response {
    let next: String = uri
        .to_string()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    let cookie =
        format!("{NEXT_COOKIE}={next}; Path=/; HttpOnly; SameSite=Lax; Max-Age=3600; Secure");
    ([(header::SET_COOKIE, cookie)], Redirect::to("/login")).into_response()
}

/// After sign-in, returns to the consent page that sent the user to log in.
pub fn resume(headers: &HeaderMap, mut res: Response) -> Response {
    let Some(next) = auth::cookie(headers, NEXT_COOKIE) else {
        return res;
    };
    let bytes: Option<Vec<u8>> = (0..next.len())
        .step_by(2)
        .map(|i| {
            next.get(i..i + 2)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        })
        .collect();
    if let Some(location) = bytes
        .and_then(|b| String::from_utf8(b).ok())
        .filter(|n| n.starts_with("/oauth/authorize?"))
        .and_then(|n| HeaderValue::from_str(&n).ok())
    {
        res.headers_mut().insert(header::LOCATION, location);
    }
    res.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_static("virling_next=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    res
}

/// The consent page for a signed-in owner.
pub async fn authorize(
    State(state): State<AppState>,
    account: Result<Account, Redirect>,
    uri: Uri,
    Query(p): Query<AuthorizeParams>,
) -> Response {
    let (client, redirect) = match check(&state, &p).await {
        Ok(c) => c,
        Err(res) => return *res,
    };
    let Ok(account) = account else {
        return to_login(&uri);
    };
    let email: String = sqlx::query_scalar("SELECT email FROM accounts WHERE id = ?")
        .bind(&account.user)
        .fetch_one(&state.db)
        .await
        .unwrap_or_default();
    render(ConsentPage {
        view: if account.owner { "ask" } else { "member" },
        client,
        host: host(&redirect),
        email,
        message: "",
        fields: p.fields(),
    })
}

/// Allow or Cancel on the consent page.
pub async fn consent(
    State(state): State<AppState>,
    account: Result<Account, Redirect>,
    Form(p): Form<AuthorizeParams>,
) -> Response {
    let (_, redirect) = match check(&state, &p).await {
        Ok(c) => c,
        Err(res) => return *res,
    };
    let account = match account {
        Ok(a) if a.owner && p.decision.as_deref() == Some("allow") => a,
        Ok(_) => return back(&state.config, &redirect, &p, &[("error", "access_denied")]),
        Err(r) => return r.into_response(),
    };
    let code = random_token();
    let _ = sqlx::query("DELETE FROM oauth_codes WHERE expires_at < ?")
        .bind(now())
        .execute(&state.db)
        .await;
    let saved = sqlx::query(
        "INSERT INTO oauth_codes (code_hash, client_id, account_id, redirect_uri, challenge, resource, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(hash_token(&code))
    .bind(p.client_id.as_deref().unwrap_or_default())
    .bind(&account.id)
    .bind(&redirect)
    .bind(p.code_challenge.as_deref().unwrap_or_default())
    .bind(
        p.resource
            .as_deref()
            .and_then(|r| asked_resource(&state.config, r))
            .unwrap_or_else(|| resource(&state.config)),
    )
    .bind(now() + CODE_TTL_SECS)
    .execute(&state.db)
    .await;
    if let Err(err) = saved {
        tracing::error!(%err, "storing an OAuth code failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    back(&state.config, &redirect, &p, &[("code", &code)])
}

/// Unpadded base64url, as PKCE uses.
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// RFC 7636 S256: the challenge is base64url(sha256(verifier)).
fn pkce_ok(verifier: &str, challenge: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        && base64url(&Sha256::digest(verifier.as_bytes())) == challenge
}

#[derive(Deserialize)]
pub struct TokenParams {
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    resource: Option<String>,
}

fn token_error(code: &'static str) -> Response {
    let status = match code {
        "invalid_client" => StatusCode::UNAUTHORIZED,
        "server_error" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_REQUEST,
    };
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "error": code })),
    )
        .into_response()
}

fn new_tokens() -> (String, String) {
    (
        format!("{ACCESS_PREFIX}{}", random_token()),
        format!("{REFRESH_PREFIX}{}", random_token()),
    )
}

/// Code exchange and refresh; refresh tokens rotate, and reusing an old one revokes the grant.
pub async fn token(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(p): Form<TokenParams>,
) -> Response {
    if let Some(res) = limited(&state, "token", peer, &headers, TOKEN_PER_SECOND) {
        return res;
    }
    if !p
        .resource
        .as_deref()
        .is_none_or(|r| same_resource(&state.config, r))
    {
        return token_error("invalid_target");
    }
    let client = p.client_id.as_deref().unwrap_or_default();
    let issued = match p.grant_type.as_deref() {
        Some("authorization_code") => exchange(&state.db, client, &p).await,
        Some("refresh_token") => refresh(&state.db, client, &p).await,
        _ => Err("unsupported_grant_type"),
    };
    match issued {
        Ok((access, refresh)) => (
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({
                "access_token": access,
                "token_type": "Bearer",
                "expires_in": ACCESS_TTL_SECS,
                "refresh_token": refresh,
                "scope": SCOPE,
            })),
        )
            .into_response(),
        Err(code) => token_error(code),
    }
}

type Issued = Result<(String, String), &'static str>;

async fn exchange(db: &SqlitePool, client: &str, p: &TokenParams) -> Issued {
    let row: Option<(String, String, String, String, String)> = sqlx::query_as(
        "DELETE FROM oauth_codes WHERE code_hash = ? AND expires_at > ? RETURNING client_id, account_id, redirect_uri, challenge, resource",
    )
    .bind(hash_token(p.code.as_deref().unwrap_or_default()))
    .bind(now())
    .fetch_optional(db)
    .await
    .map_err(|_| "server_error")?;
    let Some((code_client, account, redirect, challenge, resource)) = row else {
        return Err("invalid_grant");
    };
    if code_client != client
        || p.resource
            .as_deref()
            .is_some_and(|r| r.trim_end_matches('/') != resource)
        || p.redirect_uri.as_ref().is_some_and(|r| *r != redirect)
        || !pkce_ok(p.code_verifier.as_deref().unwrap_or_default(), &challenge)
    {
        return Err("invalid_grant");
    }
    let (access, refresh) = new_tokens();
    sqlx::query(
        "INSERT INTO oauth_grants (id, account_id, client_id, resource, access_hash, access_expires_at, refresh_hash, refresh_expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&account)
    .bind(client)
    .bind(&resource)
    .bind(hash_token(&access))
    .bind(now() + ACCESS_TTL_SECS)
    .bind(hash_token(&refresh))
    .bind(now() + REFRESH_TTL_SECS)
    .bind(now())
    .execute(db)
    .await
    .map_err(|_| "server_error")?;
    Ok((access, refresh))
}

async fn refresh(db: &SqlitePool, client: &str, p: &TokenParams) -> Issued {
    let old = hash_token(p.refresh_token.as_deref().unwrap_or_default());
    let (access, refresh) = new_tokens();
    let rotated: Option<String> = sqlx::query_scalar(
        "UPDATE oauth_grants SET access_hash = ?, access_expires_at = ?, refresh_hash = ?, refresh_expires_at = ?, old_refresh_hash = refresh_hash
         WHERE refresh_hash = ? AND client_id = ? AND revoked_at IS NULL AND refresh_expires_at > ? RETURNING id",
    )
    .bind(hash_token(&access))
    .bind(now() + ACCESS_TTL_SECS)
    .bind(hash_token(&refresh))
    .bind(now() + REFRESH_TTL_SECS)
    .bind(&old)
    .bind(client)
    .bind(now())
    .fetch_optional(db)
    .await
    .map_err(|_| "server_error")?;
    if rotated.is_none() {
        let _ = sqlx::query(
            "UPDATE oauth_grants SET revoked_at = ? WHERE old_refresh_hash = ? AND revoked_at IS NULL",
        )
        .bind(now())
        .bind(&old)
        .execute(db)
        .await;
        return Err("invalid_grant");
    }
    Ok((access, refresh))
}

/// The account of a live access token issued for this server's MCP endpoint.
async fn token_account(state: &AppState, headers: &HeaderMap, token: &str) -> Option<String> {
    let row: Option<(String, String, String, Option<i64>)> = sqlx::query_as(
        "SELECT g.id, g.account_id, g.resource, g.last_used_at FROM oauth_grants g JOIN accounts a ON a.id = g.account_id WHERE g.access_hash = ? AND g.access_expires_at > ? AND g.revoked_at IS NULL AND a.suspended_at IS NULL",
    )
    .bind(hash_token(token))
    .bind(now())
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let (id, account, aud, used) = row?;
    if aud != resource_for(&state.config, headers) && aud != resource(&state.config) {
        return None;
    }
    if used.is_none_or(|t| t < now() - 60) {
        let _ = sqlx::query("UPDATE oauth_grants SET last_used_at = ? WHERE id = ?")
            .bind(now())
            .bind(&id)
            .execute(&state.db)
            .await;
    }
    Some(account)
}

/// The account behind an MCP request's API key or OAuth access token.
pub struct McpAccount {
    pub id: String,
}

impl FromRequestParts<AppState> for McpAccount {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = api::bearer(parts);
        let id = match token {
            Some(t) if t.starts_with(ACCESS_PREFIX) => {
                token_account(state, &parts.headers, t).await
            }
            Some(t) => api::key_account(&state.db, t).await,
            None => None,
        };
        id.map(|id| McpAccount { id })
            .ok_or_else(|| challenge(&state.config, &parts.headers, token.is_some()))
    }
}

/// 401 that points clients at the resource metadata, so they can start OAuth sign-in.
fn challenge(config: &Config, headers: &HeaderMap, sent_token: bool) -> Response {
    let mut value = "Bearer".to_string();
    if enabled(config) {
        let origin = match &config.mcp_url {
            Some(mcp) if on_mcp_host(config, headers) => mcp,
            _ => &config.base_url,
        };
        value = format!(
            r#"Bearer resource_metadata="{origin}/.well-known/oauth-protected-resource/mcp", scope="{SCOPE}""#
        );
        if sent_token {
            value.push_str(r#", error="invalid_token""#);
        }
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, value)],
        Json(json!({ "error": "missing or invalid API key or token; send Authorization: Bearer <key>" })),
    )
        .into_response()
}

#[derive(Debug, sqlx::FromRow)]
pub struct GrantRow {
    pub id: String,
    pub name: String,
    pub redirect_uris: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

impl GrantRow {
    /// Where the app sends users back to, such as claude.ai.
    pub fn host(&self) -> String {
        host(self.redirect_uris.lines().next().unwrap_or_default())
    }
}

/// Apps the account allowed that can still refresh their access.
pub async fn list_grants(db: &SqlitePool, account_id: &str) -> sqlx::Result<Vec<GrantRow>> {
    sqlx::query_as(
        "SELECT g.id, c.name, c.redirect_uris, g.created_at, g.last_used_at FROM oauth_grants g
         JOIN oauth_clients c ON c.id = g.client_id
         WHERE g.account_id = ? AND g.revoked_at IS NULL AND g.refresh_expires_at > ? ORDER BY g.created_at, g.id",
    )
    .bind(account_id)
    .bind(now())
    .fetch_all(db)
    .await
}

pub async fn revoke_grant(db: &SqlitePool, account_id: &str, id: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE oauth_grants SET revoked_at = ? WHERE id = ? AND account_id = ? AND revoked_at IS NULL",
    )
    .bind(now())
    .bind(id)
    .bind(account_id)
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::api::tests::{Fixture, fixture};
    use crate::app::router;

    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
    const CALLBACK: &str = "https://claude.ai/api/mcp/auth_callback";
    const RESOURCE: &str = "https://stats.example.com/mcp";
    const METADATA: &str = "https://stats.example.com/.well-known/oauth-protected-resource/mcp";

    async fn https() -> Fixture {
        let mut f = fixture().await;
        f.state.config = Arc::new(Config {
            base_url: "https://stats.example.com".into(),
            ..(*f.state.config).clone()
        });
        f
    }

    async fn send(state: &AppState, req: Request<Body>) -> Response {
        send_from(state, req, 7).await
    }

    async fn send_from(state: &AppState, req: Request<Body>, ip: u8) -> Response {
        router(state.clone())
            .layer(MockConnectInfo(SocketAddr::from(([203, 0, 113, ip], 5555))))
            .oneshot(req)
            .await
            .unwrap()
    }

    async fn body(res: Response) -> String {
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    async fn json_of(res: Response) -> (StatusCode, Value) {
        let status = res.status();
        (status, serde_json::from_str(&body(res).await).unwrap())
    }

    fn form(uri: &str, body: String, cookie: Option<&str>) -> Request<Body> {
        let mut b = Request::post(uri).header("content-type", "application/x-www-form-urlencoded");
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        b.body(Body::from(body)).unwrap()
    }

    fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
        let mut b = Request::get(uri);
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        b.body(Body::empty()).unwrap()
    }

    fn enc(v: &str) -> String {
        url::form_urlencoded::byte_serialize(v.as_bytes()).collect()
    }

    fn location(res: &Response) -> String {
        res.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .to_string()
    }

    fn param(location: &str, name: &str) -> Option<String> {
        Url::parse(location)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    }

    async fn register_client(state: &AppState, redirects: Value) -> (StatusCode, Value) {
        register_from(state, redirects, 7).await
    }

    async fn register_from(state: &AppState, redirects: Value, ip: u8) -> (StatusCode, Value) {
        let req = Request::post("/oauth/register")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "client_name": "Claude", "redirect_uris": redirects, "token_endpoint_auth_method": "none" })
                    .to_string(),
            ))
            .unwrap();
        json_of(send_from(state, req, ip).await).await
    }

    async fn client(state: &AppState) -> String {
        let (code, c) = register_client(state, json!([CALLBACK])).await;
        assert_eq!(code, StatusCode::CREATED);
        c["client_id"].as_str().unwrap().to_string()
    }

    async fn session(state: &AppState, account: &str) -> String {
        let res = auth::start_session(state, account).await.unwrap();
        let c = res.headers()[header::SET_COOKIE].to_str().unwrap();
        c.split(';').next().unwrap().to_string()
    }

    fn query(client: &str, extra: &str) -> String {
        format!(
            "response_type=code&client_id={client}&redirect_uri={}&code_challenge={CHALLENGE}&code_challenge_method=S256&state=xyz&scope=read&resource={}{extra}",
            enc(CALLBACK),
            enc(RESOURCE)
        )
    }

    /// Allows the app on the consent form and returns the code.
    async fn allow(state: &AppState, cookie: &str, client: &str) -> String {
        let res = send(
            state,
            form(
                "/oauth/authorize",
                query(client, "&decision=allow"),
                Some(cookie),
            ),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let to = location(&res);
        assert!(to.starts_with(CALLBACK), "{to}");
        assert_eq!(param(&to, "state").as_deref(), Some("xyz"));
        assert_eq!(
            param(&to, "iss").as_deref(),
            Some("https://stats.example.com")
        );
        param(&to, "code").unwrap()
    }

    async fn token_req(state: &AppState, body: String) -> (StatusCode, Value) {
        json_of(send(state, form("/oauth/token", body, None)).await).await
    }

    fn exchange_body(client: &str, code: &str, verifier: &str) -> String {
        format!(
            "grant_type=authorization_code&client_id={client}&code={code}&code_verifier={verifier}&redirect_uri={}&resource={}",
            enc(CALLBACK),
            enc(RESOURCE)
        )
    }

    fn refresh_body(client: &str, refresh: &str) -> String {
        format!(
            "grant_type=refresh_token&client_id={client}&refresh_token={refresh}&resource={}",
            enc(RESOURCE)
        )
    }

    /// Tokens for a new grant: (client, access, refresh).
    async fn tokens(f: &Fixture) -> (String, String, String) {
        let client = client(&f.state).await;
        let cookie = session(&f.state, &f.account).await;
        let code = allow(&f.state, &cookie, &client).await;
        let (status, t) = token_req(&f.state, exchange_body(&client, &code, VERIFIER)).await;
        assert_eq!(status, StatusCode::OK, "{t}");
        let s = |k: &str| t[k].as_str().unwrap().to_string();
        (client, s("access_token"), s("refresh_token"))
    }

    async fn mcp(state: &AppState, token: Option<&str>) -> Response {
        let mut req = Request::post("/mcp").header("content-type", "application/json");
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let call = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "list_sites", "arguments": {} } });
        send(state, req.body(Body::from(call.to_string())).unwrap()).await
    }

    fn www_authenticate(res: &Response) -> &str {
        res.headers()[header::WWW_AUTHENTICATE].to_str().unwrap()
    }

    #[test]
    fn pkce_uses_s256_and_base64url() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert!(pkce_ok(VERIFIER, CHALLENGE), "RFC 7636 appendix B");
        assert!(!pkce_ok(VERIFIER, VERIFIER), "plain is not accepted");
        assert!(!pkce_ok("short", &base64url(&Sha256::digest(b"short"))));
        assert!(valid_challenge(CHALLENGE) && !valid_challenge("abc"));
    }

    #[test]
    fn redirect_uris_match_exactly_except_loopback_ports() {
        assert!(redirect_matches(CALLBACK, CALLBACK));
        for other in [
            "https://claude.ai/api/mcp/auth_callback/",
            "https://claude.ai/api/mcp/auth_callback?x=1",
            "https://claude.ai:8443/api/mcp/auth_callback",
            "https://evil.example/api/mcp/auth_callback",
            "HTTPS://claude.ai/api/mcp/auth_callback",
        ] {
            assert!(!redirect_matches(CALLBACK, other), "{other}");
        }
        assert!(redirect_matches(
            "http://localhost:3118/callback",
            "http://localhost:50123/callback"
        ));
        assert!(redirect_matches(
            "http://127.0.0.1/callback",
            "http://127.0.0.1:50123/callback"
        ));
        assert!(!redirect_matches(
            "http://localhost:3118/callback",
            "http://localhost:3118/other"
        ));
        assert!(valid_redirect(CALLBACK) && valid_redirect("http://localhost:1/cb"));
        for bad in [
            "http://claude.ai/cb",
            "https://claude.ai/cb#frag",
            "myapp://cb",
            "javascript:alert(1)",
            "not a url",
        ] {
            assert!(!valid_redirect(bad), "{bad}");
        }
    }

    #[tokio::test]
    async fn serves_metadata_only_on_https() {
        let f = https().await;
        for path in [
            "/.well-known/oauth-protected-resource",
            "/.well-known/oauth-protected-resource/mcp",
        ] {
            let (code, m) = json_of(send(&f.state, get(path, None)).await).await;
            assert_eq!(code, StatusCode::OK);
            assert_eq!(m["resource"], RESOURCE);
            assert_eq!(
                m["authorization_servers"],
                json!(["https://stats.example.com"])
            );
            assert_eq!(m["scopes_supported"], json!(["read"]));
        }
        let (_, m) = json_of(
            send(
                &f.state,
                get("/.well-known/oauth-authorization-server", None),
            )
            .await,
        )
        .await;
        assert_eq!(m["issuer"], "https://stats.example.com");
        assert_eq!(
            m["authorization_endpoint"],
            "https://stats.example.com/oauth/authorize"
        );
        assert_eq!(m["token_endpoint"], "https://stats.example.com/oauth/token");
        assert_eq!(
            m["registration_endpoint"],
            "https://stats.example.com/oauth/register"
        );
        assert_eq!(m["code_challenge_methods_supported"], json!(["S256"]));
        assert_eq!(m["token_endpoint_auth_methods_supported"], json!(["none"]));
        assert_eq!(m["authorization_response_iss_parameter_supported"], true);
        assert_eq!(m["client_id_metadata_document_supported"], true);

        let plain = fixture().await;
        for path in [
            "/.well-known/oauth-protected-resource/mcp",
            "/.well-known/oauth-authorization-server",
            "/oauth/authorize",
        ] {
            let res = send(&plain.state, get(path, None)).await;
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
        }
        let res = mcp(&plain.state, None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(www_authenticate(&res), "Bearer");

        let mut path = fixture().await;
        path.state.config = Arc::new(Config {
            base_url: "https://example.com/virling".into(),
            ..(*path.state.config).clone()
        });
        let res = send(
            &path.state,
            get("/.well-known/oauth-authorization-server", None),
        )
        .await;
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "no sign-in under a path"
        );
        assert_eq!(www_authenticate(&mcp(&path.state, None).await), "Bearer");
    }

    #[tokio::test]
    async fn registration_checks_redirect_uris() {
        let f = https().await;
        let (code, c) = register_client(
            &f.state,
            json!([CALLBACK, "http://localhost:4000/callback"]),
        )
        .await;
        assert_eq!(code, StatusCode::CREATED);
        assert_eq!(c["client_name"], "Claude");
        assert_eq!(c["token_endpoint_auth_method"], "none");
        assert!(
            c.get("client_secret").is_none(),
            "public clients get no secret"
        );
        for (ip, bad) in [
            json!([]),
            json!(["http://claude.ai/cb"]),
            json!(["myapp://cb"]),
            json!([CALLBACK, "https://claude.ai/cb#x"]),
            json!("https://claude.ai/cb"),
        ]
        .into_iter()
        .enumerate()
        {
            let (code, e) = register_from(&f.state, bad.clone(), 100 + ip as u8).await;
            assert_eq!(code, StatusCode::BAD_REQUEST, "{bad}");
            assert_eq!(e["error"], "invalid_redirect_uri");
        }
        let req = Request::post("/oauth/register")
            .body(Body::from("not json"))
            .unwrap();
        let (code, e) = json_of(send_from(&f.state, req, 99).await).await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert_eq!(e["error"], "invalid_client_metadata");
    }

    #[tokio::test]
    async fn registration_and_tokens_are_rate_limited_per_ip() {
        let f = https().await;
        let mut codes = Vec::new();
        for _ in 0..20 {
            codes.push(register_from(&f.state, json!([CALLBACK]), 50).await.0);
        }
        assert_eq!(codes[0], StatusCode::CREATED);
        assert!(codes.contains(&StatusCode::TOO_MANY_REQUESTS), "{codes:?}");
        let (code, _) = register_from(&f.state, json!([CALLBACK]), 51).await;
        assert_eq!(code, StatusCode::CREATED, "another IP is not limited");

        let mut codes = Vec::new();
        for _ in 0..60 {
            let req = form(
                "/oauth/token",
                "grant_type=refresh_token&refresh_token=x".into(),
                None,
            );
            codes.push(send_from(&f.state, req, 52).await.status());
        }
        assert_eq!(codes[0], StatusCode::BAD_REQUEST);
        let limited = codes
            .iter()
            .filter(|c| **c == StatusCode::TOO_MANY_REQUESTS)
            .count();
        assert!(limited > 0, "{codes:?}");
    }

    #[test]
    fn client_metadata_documents_are_checked() {
        const DOC: &str = "https://app.example.com/oauth/client.json";
        assert!(metadata_url(DOC));
        for bad in [
            "http://app.example.com/client.json",
            "https://app.example.com",
            "https://app.example.com/",
            "https://203.0.113.9/client.json",
            "https://[2001:db8::1]/client.json",
            "https://user@app.example.com/client.json",
            "https://app.example.com/client.json#x",
            "https://app.example.com/a/../client.json",
            "https://APP.example.com/client.json",
            "9d6c0b1e-0000-4000-8000-000000000000",
        ] {
            assert!(!metadata_url(bad), "{bad}");
        }

        let doc = json!({ "client_id": DOC, "client_name": " Example ", "redirect_uris": [CALLBACK, "http://127.0.0.1:3000/cb"], "token_endpoint_auth_method": "none" });
        assert_eq!(
            from_document(DOC, &doc),
            Some((
                "Example".into(),
                vec![CALLBACK.into(), "http://127.0.0.1:3000/cb".into()]
            ))
        );
        let with = |k: &str, v: Value| {
            let mut d = doc.clone();
            d[k] = v;
            from_document(DOC, &d)
        };
        assert_eq!(
            with("client_id", json!("https://app.example.com/other.json")),
            None
        );
        assert_eq!(with("client_name", json!("")), None);
        assert_eq!(with("client_name", Value::Null), None);
        assert_eq!(with("redirect_uris", json!([])), None);
        assert_eq!(
            with("redirect_uris", json!(["http://evil.example/cb"])),
            None
        );
        assert_eq!(with("redirect_uris", json!([CALLBACK, 7])), None);
        assert_eq!(
            with("token_endpoint_auth_method", json!("private_key_jwt")),
            None
        );
        assert_eq!(from_document(DOC, &json!("not an object")), None);

        assert_eq!(max_age(""), 3600);
        assert_eq!(max_age("public, max-age=600"), 600);
        assert_eq!(max_age("Max-Age=999999999"), 86_400);
        assert_eq!(max_age("max-age=600, no-store"), 0);
        assert_eq!(max_age("no-cache"), 0);
        assert_eq!(max_age("max-age=x"), 0);
    }

    #[tokio::test]
    async fn metadata_document_clients_sign_in() {
        const DOC: &str = "https://app.example.com/oauth/client.json";
        let f = https().await;
        let cookie = session(&f.state, &f.account).await;
        let page = |client: &str| {
            let (state, cookie) = (f.state.clone(), cookie.clone());
            let q = query(&enc(client), "");
            async move { send(&state, get(&format!("/oauth/authorize?{q}"), Some(&cookie))).await }
        };

        for (client, message) in [
            (
                "https://localhost/oauth/client.json",
                "could not read this app",
            ),
            ("https://127.0.0.1/oauth/client.json", "not registered"),
        ] {
            let res = page(client).await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{client}");
            assert!(body(res).await.contains(message), "{client}");
        }

        sqlx::query("INSERT INTO oauth_clients (id, name, redirect_uris, created_at, refresh_at) VALUES (?, 'Example', ?, ?, ?)")
            .bind(DOC)
            .bind(CALLBACK)
            .bind(now())
            .bind(now() + 3600)
            .execute(&f.state.db)
            .await
            .unwrap();
        let res = page(DOC).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            body(res)
                .await
                .contains("Example wants read&#8209;only access")
        );
        let code = allow(&f.state, &cookie, &enc(DOC)).await;
        let (status, t) = token_req(&f.state, exchange_body(&enc(DOC), &code, VERIFIER)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            t["access_token"]
                .as_str()
                .unwrap()
                .starts_with(ACCESS_PREFIX)
        );
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_clients")
            .fetch_one(&f.state.db)
            .await
            .unwrap();
        assert_eq!(n, 1, "one row per client, not per connection");
    }

    #[tokio::test]
    async fn authorize_rejects_bad_requests() {
        let f = https().await;
        let client = client(&f.state).await;
        let cookie = session(&f.state, &f.account).await;
        let page = |q: String| {
            let state = f.state.clone();
            let cookie = cookie.clone();
            async move { send(&state, get(&format!("/oauth/authorize?{q}"), Some(&cookie))).await }
        };

        let res = page(query("unknown", "")).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "unknown client");
        assert!(body(res).await.contains("not registered"));
        let wrong = query(&client, "").replace(&enc(CALLBACK), &enc("https://evil.example/cb"));
        let res = page(wrong).await;
        assert_eq!(
            res.status(),
            StatusCode::BAD_REQUEST,
            "wrong redirect is never followed"
        );
        assert!(res.headers().get(header::LOCATION).is_none());
        assert!(body(res).await.contains("return address"));

        for (q, error) in [
            (
                query(&client, "").replace("response_type=code", "response_type=token"),
                "unsupported_response_type",
            ),
            (
                query(&client, "").replace("S256", "plain"),
                "invalid_request",
            ),
            (query(&client, "").replace(CHALLENGE, ""), "invalid_request"),
            (
                query(&client, "").replace(&enc(RESOURCE), &enc("https://other.example/mcp")),
                "invalid_target",
            ),
        ] {
            let res = page(q).await;
            assert_eq!(res.status(), StatusCode::SEE_OTHER);
            let to = location(&res);
            assert!(to.starts_with(CALLBACK));
            assert_eq!(param(&to, "error").as_deref(), Some(error));
            assert_eq!(param(&to, "state").as_deref(), Some("xyz"));
        }

        let res = page(query(&client, "")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let html = body(res).await;
        assert!(html.contains("Claude wants read&#8209;only access to your Virling sites"));
        assert!(
            html.contains("<b>claude.ai</b>"),
            "the redirect host is shown"
        );
        assert!(html.contains("me@example.com"));
        assert!(html.contains(r#"value="allow">Allow</button>"#));
        assert!(html.contains(r#"value="cancel">Cancel</button>"#));

        let res = send(
            &f.state,
            form(
                "/oauth/authorize",
                query(&client, "&decision=cancel"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            param(&location(&res), "error").as_deref(),
            Some("access_denied")
        );
        assert!(param(&location(&res), "code").is_none());
    }

    #[tokio::test]
    async fn team_members_cannot_allow() {
        let f = https().await;
        let client = client(&f.state).await;
        let member = crate::sites::find_or_create_account(&f.state.db, "member@example.com")
            .await
            .unwrap();
        sqlx::query("INSERT INTO members (account_id, org_id, created_at) VALUES (?, ?, 0)")
            .bind(&member)
            .bind(&f.account)
            .execute(&f.state.db)
            .await
            .unwrap();
        let cookie = session(&f.state, &member).await;
        let uri = format!("/oauth/authorize?{}", query(&client, ""));
        let html = body(send(&f.state, get(&uri, Some(&cookie))).await).await;
        assert!(html.contains("Ask the account owner") && !html.contains(">Allow<"));
        let res = send(
            &f.state,
            form(
                "/oauth/authorize",
                query(&client, "&decision=allow"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            param(&location(&res), "error").as_deref(),
            Some("access_denied")
        );
    }

    #[tokio::test]
    async fn code_exchange_needs_the_verifier_client_and_redirect() {
        let f = https().await;
        let client = client(&f.state).await;
        let other = self::client(&f.state).await;
        let cookie = session(&f.state, &f.account).await;
        let wrong_verifier = "x".repeat(43);
        for body in [
            exchange_body(&client, "CODE", &wrong_verifier),
            exchange_body(&other, "CODE", VERIFIER),
            exchange_body(&client, "CODE", VERIFIER)
                .replace(&enc(CALLBACK), &enc("https://claude.ai/other")),
        ] {
            let code = allow(&f.state, &cookie, &client).await;
            let (status, e) = token_req(&f.state, body.replace("CODE", &code)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
            assert_eq!(e["error"], "invalid_grant");
            let (status, _) = token_req(&f.state, exchange_body(&client, &code, VERIFIER)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "a failed code is used up");
        }

        let code = allow(&f.state, &cookie, &client).await;
        sqlx::query("UPDATE oauth_codes SET expires_at = 0")
            .execute(&f.state.db)
            .await
            .unwrap();
        let (_, e) = token_req(&f.state, exchange_body(&client, &code, VERIFIER)).await;
        assert_eq!(e["error"], "invalid_grant", "expired code");

        let code = allow(&f.state, &cookie, &client).await;
        let body = exchange_body(&client, &code, VERIFIER);
        let (_, e) = token_req(
            &f.state,
            body.replace(&enc(RESOURCE), &enc("https://other.example/mcp")),
        )
        .await;
        assert_eq!(e["error"], "invalid_target");
        let (_, e) = token_req(&f.state, body.replace("authorization_code", "password")).await;
        assert_eq!(e["error"], "unsupported_grant_type");
        let (status, t) = token_req(&f.state, body.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(t["token_type"], "Bearer");
        assert_eq!(t["expires_in"], 3600);
        assert_eq!(t["scope"], "read");
        assert!(t["access_token"].as_str().unwrap().starts_with("vo_"));
        assert!(t["refresh_token"].as_str().unwrap().starts_with("vr_"));
        let (_, e) = token_req(&f.state, body).await;
        assert_eq!(e["error"], "invalid_grant", "a code works once");

        let stored: Vec<(String, String)> =
            sqlx::query_as("SELECT access_hash, refresh_hash FROM oauth_grants")
                .fetch_all(&f.state.db)
                .await
                .unwrap();
        assert_eq!(
            stored,
            [(
                hash_token(t["access_token"].as_str().unwrap()),
                hash_token(t["refresh_token"].as_str().unwrap())
            )],
            "only hashes are stored"
        );
    }

    #[tokio::test]
    async fn refresh_tokens_rotate_and_reuse_revokes() {
        let f = https().await;
        let (client, access, refresh) = tokens(&f).await;
        let (status, t) = token_req(&f.state, refresh_body(&client, &refresh)).await;
        assert_eq!(status, StatusCode::OK);
        let (access2, refresh2) = (
            t["access_token"].as_str().unwrap().to_string(),
            t["refresh_token"].as_str().unwrap().to_string(),
        );
        assert_ne!(refresh2, refresh);
        assert_eq!(mcp(&f.state, Some(&access2)).await.status(), StatusCode::OK);
        assert_eq!(
            mcp(&f.state, Some(&access)).await.status(),
            StatusCode::UNAUTHORIZED,
            "the old access token is replaced"
        );
        let (_, e) = token_req(&f.state, refresh_body("someone-else", &refresh2)).await;
        assert_eq!(
            e["error"], "invalid_grant",
            "another client's refresh token"
        );

        let (status, e) = token_req(&f.state, refresh_body(&client, &refresh)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            e["error"], "invalid_grant",
            "a rotated refresh token is dead"
        );
        assert_eq!(
            mcp(&f.state, Some(&access2)).await.status(),
            StatusCode::UNAUTHORIZED,
            "reusing it revokes the grant"
        );
        let (_, e) = token_req(&f.state, refresh_body(&client, &refresh2)).await;
        assert_eq!(e["error"], "invalid_grant");

        let (client, _, refresh) = tokens(&f).await;
        sqlx::query("UPDATE oauth_grants SET refresh_expires_at = 0 WHERE revoked_at IS NULL")
            .execute(&f.state.db)
            .await
            .unwrap();
        let (_, e) = token_req(&f.state, refresh_body(&client, &refresh)).await;
        assert_eq!(e["error"], "invalid_grant", "expired refresh token");
    }

    const MCP_HOST: &str = "mcp.example.com";
    const MCP_RESOURCE: &str = "https://mcp.example.com/mcp";

    async fn with_mcp_host() -> Fixture {
        let mut f = https().await;
        f.state.config = Arc::new(Config {
            mcp_url: Some("https://mcp.example.com".into()),
            ..(*f.state.config).clone()
        });
        f
    }

    fn on_host(host: &str, req: Request<Body>) -> Request<Body> {
        let (mut parts, body) = req.into_parts();
        parts.headers.insert(header::HOST, host.parse().unwrap());
        Request::from_parts(parts, body)
    }

    async fn mcp_on(state: &AppState, host: &str, token: Option<&str>) -> Response {
        let mut req = Request::post("/mcp").header("content-type", "application/json");
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let call = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "list_sites", "arguments": {} } });
        let req = req.body(Body::from(call.to_string())).unwrap();
        send(state, on_host(host, req)).await
    }

    #[tokio::test]
    async fn mcp_host_serves_only_the_endpoint_and_its_metadata() {
        let f = with_mcp_host().await;
        for path in [
            "/",
            "/app",
            "/auth/login",
            "/oauth/authorize",
            "/oauth/token",
            "/.well-known/oauth-authorization-server",
            "/api/v1/sites",
        ] {
            let res = send(&f.state, on_host(MCP_HOST, get(path, None))).await;
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
            assert!(res.headers().get(header::SET_COOKIE).is_none(), "{path}");
        }
        for path in [
            "/.well-known/oauth-protected-resource",
            "/.well-known/oauth-protected-resource/mcp",
        ] {
            let (code, m) = json_of(send(&f.state, on_host(MCP_HOST, get(path, None))).await).await;
            assert_eq!(code, StatusCode::OK);
            assert_eq!(m["resource"], MCP_RESOURCE);
            assert_eq!(
                m["authorization_servers"],
                json!(["https://stats.example.com"])
            );
        }
        let (_, m) = json_of(
            send(
                &f.state,
                get("/.well-known/oauth-protected-resource/mcp", None),
            )
            .await,
        )
        .await;
        assert_eq!(m["resource"], RESOURCE, "the base host is unchanged");

        let res = mcp_on(&f.state, MCP_HOST, None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            www_authenticate(&res),
            r#"Bearer resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource/mcp", scope="read""#
        );
        let res = mcp_on(&f.state, "stats.example.com", None).await;
        assert_eq!(
            www_authenticate(&res),
            format!(r#"Bearer resource_metadata="{METADATA}", scope="read""#)
        );
    }

    #[tokio::test]
    async fn tokens_are_bound_to_the_requested_audience() {
        let f = with_mcp_host().await;
        let client = client(&f.state).await;
        let cookie = session(&f.state, &f.account).await;
        let swap = |s: String| s.replace(&enc(RESOURCE), &enc(MCP_RESOURCE));
        let res = send(
            &f.state,
            form(
                "/oauth/authorize",
                swap(query(&client, "&decision=allow")),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let code = param(&location(&res), "code").unwrap();
        let (status, t) = token_req(&f.state, swap(exchange_body(&client, &code, VERIFIER))).await;
        assert_eq!(status, StatusCode::OK, "{t}");
        let access = t["access_token"].as_str().unwrap();
        assert_eq!(
            mcp_on(&f.state, MCP_HOST, Some(access)).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            mcp_on(&f.state, "stats.example.com", Some(access))
                .await
                .status(),
            StatusCode::UNAUTHORIZED,
            "an mcp audience token is not valid on the base host"
        );

        let (_, base_access, _) = tokens(&f).await;
        assert_eq!(
            mcp_on(&f.state, "stats.example.com", Some(&base_access))
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            mcp_on(&f.state, MCP_HOST, Some(&base_access))
                .await
                .status(),
            StatusCode::OK
        );

        let other = exchange_body(&client, "x", VERIFIER)
            .replace(&enc(RESOURCE), &enc("https://other.example/mcp"));
        let (_, e) = token_req(&f.state, other).await;
        assert_eq!(e["error"], "invalid_target");
    }

    #[tokio::test]
    async fn mcp_answers_401_with_the_metadata_pointer() {
        let f = https().await;
        let res = mcp(&f.state, None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            www_authenticate(&res),
            format!(r#"Bearer resource_metadata="{METADATA}", scope="read""#)
        );
        for bad in ["vo_nope", "vk_nope", "garbage"] {
            let res = mcp(&f.state, Some(bad)).await;
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(
                www_authenticate(&res),
                format!(
                    r#"Bearer resource_metadata="{METADATA}", scope="read", error="invalid_token""#
                )
            );
        }
        assert_eq!(
            mcp(&f.state, Some(&f.key)).await.status(),
            StatusCode::OK,
            "API keys still work"
        );

        let (_, access, _) = tokens(&f).await;
        assert_eq!(mcp(&f.state, Some(&access)).await.status(), StatusCode::OK);
        let req = Request::get("/api/v1/sites")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            send(&f.state, req).await.status(),
            StatusCode::UNAUTHORIZED,
            "OAuth tokens are only for the MCP endpoint"
        );

        sqlx::query("UPDATE oauth_grants SET resource = 'https://other.example/mcp'")
            .execute(&f.state.db)
            .await
            .unwrap();
        assert_eq!(
            mcp(&f.state, Some(&access)).await.status(),
            StatusCode::UNAUTHORIZED,
            "wrong audience"
        );
        sqlx::query("UPDATE oauth_grants SET resource = ?, access_expires_at = 0")
            .bind(RESOURCE)
            .execute(&f.state.db)
            .await
            .unwrap();
        assert_eq!(
            mcp(&f.state, Some(&access)).await.status(),
            StatusCode::UNAUTHORIZED,
            "expired access token"
        );
    }

    #[tokio::test]
    async fn full_flow_from_sign_in_to_revoke() {
        let f = https().await;
        let client = client(&f.state).await;
        let authorize = format!("/oauth/authorize?{}", query(&client, ""));

        let res = send(&f.state, get(&authorize, None)).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&res), "/login");
        let next = res.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(next.contains("HttpOnly") && next.contains("Secure"));
        let next = next.split(';').next().unwrap().to_string();

        send(
            &f.state,
            form("/login", "email=me%40example.com".into(), None),
        )
        .await;
        let mail = match &*f.state.mailer {
            crate::mail::Mailer::Capture(m) => m.lock().unwrap().last().unwrap().2.clone(),
            _ => unreachable!(),
        };
        let token = mail
            .split("https://stats.example.com/auth/verify?token=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();
        let res = send(
            &f.state,
            form("/auth/verify", format!("token={token}"), Some(&next)),
        )
        .await;
        assert_eq!(location(&res), authorize, "back to the consent page");
        let cookies: Vec<&str> = res
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert!(cookies.iter().any(|c| c.starts_with("virling_next=;")));
        let cookie = cookies
            .iter()
            .find(|c| c.starts_with(auth::SESSION_COOKIE))
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();

        let res = send(&f.state, get(&authorize, Some(&cookie))).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            body(res)
                .await
                .contains("Claude wants read&#8209;only access")
        );
        let code = allow(&f.state, &cookie, &client).await;
        let (_, t) = token_req(&f.state, exchange_body(&client, &code, VERIFIER)).await;
        let access = t["access_token"].as_str().unwrap();

        let res = mcp(&f.state, Some(access)).await;
        assert_eq!(res.status(), StatusCode::OK);
        let (_, r) = json_of(res).await;
        assert_eq!(
            r["result"]["structuredContent"]["sites"][0]["domain"],
            "example.com"
        );

        let page = body(send(&f.state, get("/app/example.com/account", Some(&cookie))).await).await;
        assert!(page.contains("Connected apps") && page.contains("1 connected"));
        assert!(page.contains("<b>Claude</b> <span class=\"mono small\">claude.ai</span>"));
        assert!(page.contains("Connected just now. Used just now."));
        let id: String = sqlx::query_scalar("SELECT id FROM oauth_grants")
            .fetch_one(&f.state.db)
            .await
            .unwrap();
        let res = send(
            &f.state,
            form(
                "/app/example.com/account/apps/revoke",
                format!("id={id}"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(location(&res), "/app/example.com/account#apps");
        let res = mcp(&f.state, Some(access)).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "revoked");
        assert!(www_authenticate(&res).contains(r#"error="invalid_token""#));
        let page = body(send(&f.state, get("/app/example.com/account", Some(&cookie))).await).await;
        assert!(page.contains("No apps yet"));
    }
}
