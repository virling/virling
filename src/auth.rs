use askama::Template;
use axum::Form;
use axum::extract::{ConnectInfo, FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use time::OffsetDateTime;

use crate::app::AppState;
use crate::sites;
use crate::waitlist::valid_email;

const LINK_TTL_SECS: i64 = 15 * 60;
const SESSION_TTL_SECS: i64 = 30 * 24 * 3600;
const LINKS_PER_HOUR: i64 = 5;
const LINKS_PER_IP_HOUR: i64 = 20;
pub const TURNSTILE_VERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
pub const SESSION_COOKIE: &str = "virling_session";

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("system randomness");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginPage {
    pub view: &'static str,
    pub email: String,
    pub token: String,
    pub error: Option<&'static str>,
    pub turnstile: Option<String>,
}

#[derive(Template)]
#[template(path = "email/login.html")]
struct LoginEmailHtml<'a> {
    email: &'a str,
    link: &'a str,
}

pub fn page(p: LoginPage) -> Response {
    match p.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!(%err, "template render failed");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong",
            )
                .into_response()
        }
    }
}

pub async fn login_form(State(state): State<AppState>) -> Response {
    page(LoginPage {
        view: "enter",
        turnstile: site_key(&state),
        email: String::new(),
        token: String::new(),
        error: None,
    })
}

#[derive(Deserialize)]
pub struct LoginInput {
    email: String,
    #[serde(rename = "cf-turnstile-response")]
    turnstile: Option<String>,
}

fn site_key(state: &AppState) -> Option<String> {
    state.config.turnstile.as_ref().map(|t| t.site_key.clone())
}

/// True when Turnstile is off, or Cloudflare confirms the token for the login action and base URL host; any failure counts as a bot.
async fn human(state: &AppState, token: Option<&str>, ip: &str) -> bool {
    let Some(t) = &state.config.turnstile else {
        return true;
    };
    let Some(token) = token.filter(|t| !t.is_empty()) else {
        return false;
    };
    let Some(host) = url::Url::parse(&state.config.base_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
    else {
        tracing::warn!("turnstile cannot check the hostname: base url has no host");
        return false;
    };
    let sent = state
        .http
        .post(&t.verify_url)
        .json(&serde_json::json!({ "secret": t.secret, "response": token, "remoteip": ip }))
        .send()
        .await;
    let verdict = match sent {
        Ok(res) => res.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };
    let ok = verdict.is_some_and(|v| {
        v["success"] == true && v["action"] == "login" && v["hostname"] == host.as_str()
    });
    if !ok {
        tracing::warn!(%ip, "turnstile check failed");
    }
    ok
}

pub async fn send_link(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(input): Form<LoginInput>,
) -> Response {
    let trust = state.config.trust_proxy;
    let ip = crate::ingest::client_ip(
        &headers,
        peer,
        trust,
        crate::ingest::via_cloudflare(peer.ip(), trust),
    )
    .to_string();
    let key = site_key(&state);
    let email = input.email.trim().to_ascii_lowercase();
    if !valid_email(&email) {
        return page(LoginPage {
            view: "enter",
            turnstile: key.clone(),
            email,
            token: String::new(),
            error: Some("Enter a valid email."),
        });
    }
    if !human(&state, input.turnstile.as_deref(), &ip).await {
        return page(LoginPage {
            view: "enter",
            turnstile: key,
            email,
            token: String::new(),
            error: Some("Confirm you are human and try again."),
        });
    }
    let recent: i64 =
        sqlx::query_scalar("SELECT count(*) FROM login_tokens WHERE email = ? AND created_at > ?")
            .bind(&email)
            .bind(now() - 3600)
            .fetch_one(&state.db)
            .await
            .unwrap_or(LINKS_PER_HOUR);
    let from_ip: i64 =
        sqlx::query_scalar("SELECT count(*) FROM login_tokens WHERE ip = ? AND created_at > ?")
            .bind(&ip)
            .bind(now() - 3600)
            .fetch_one(&state.db)
            .await
            .unwrap_or(LINKS_PER_IP_HOUR);
    if recent < LINKS_PER_HOUR && from_ip < LINKS_PER_IP_HOUR {
        let token = random_token();
        let inserted = sqlx::query("INSERT INTO login_tokens (token_hash, email, created_at, expires_at, ip) VALUES (?, ?, ?, ?, ?)")
            .bind(hash_token(&token))
            .bind(&email)
            .bind(now())
            .bind(now() + LINK_TTL_SECS)
            .bind(&ip)
            .execute(&state.db)
            .await;
        if let Err(err) = inserted {
            tracing::error!(%err, "storing login token failed");
            return page(LoginPage {
                view: "enter",
                turnstile: key.clone(),
                email,
                token: String::new(),
                error: Some("Something went wrong. Try again."),
            });
        }
        let link = format!("{}/auth/verify?token={token}", state.config.base_url);
        let text = format!(
            "Sign in to Virling as {email}:\n\n{link}\n\nThe link works once and expires in 15 minutes. If you didn't ask for this, ignore this email."
        );
        let html = LoginEmailHtml {
            email: &email,
            link: &link,
        }
        .render()
        .unwrap_or_default();
        if let Err(err) = state
            .mailer
            .send(&email, "Your Virling sign-in link", text, html)
            .await
        {
            tracing::error!(%err, "sending login email failed");
            return page(LoginPage {
                view: "enter",
                turnstile: key.clone(),
                email,
                token: String::new(),
                error: Some("We couldn't send the email. Try again in a minute."),
            });
        }
    } else {
        tracing::warn!(%email, %ip, "login link rate limit reached");
    }
    page(LoginPage {
        view: "sent",
        turnstile: key.clone(),
        email,
        token: String::new(),
        error: None,
    })
}

#[derive(Deserialize)]
pub struct TokenInput {
    token: String,
}

async fn pending_email(state: &AppState, token: &str) -> Option<String> {
    sqlx::query_scalar("SELECT email FROM login_tokens WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?")
        .bind(hash_token(token))
        .bind(now())
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
}

const EXPIRED: &str = "This link has expired or was already used. Request a new one.";

/// Shows a confirm button instead of signing in, so email scanners that open links can't use them up.
pub async fn confirm(State(state): State<AppState>, Query(input): Query<TokenInput>) -> Response {
    match pending_email(&state, &input.token).await {
        Some(email) => page(LoginPage {
            view: "confirm",
            turnstile: None,
            email,
            token: input.token,
            error: None,
        }),
        None => page(LoginPage {
            view: "enter",
            turnstile: None,
            email: String::new(),
            token: String::new(),
            error: Some(EXPIRED),
        }),
    }
}

pub async fn verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<TokenInput>,
) -> Response {
    let email: Option<String> = sqlx::query_scalar(
        "UPDATE login_tokens SET used_at = ? WHERE token_hash = ? AND used_at IS NULL AND expires_at > ? RETURNING email",
    )
    .bind(now())
    .bind(hash_token(&input.token))
    .bind(now())
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let Some(email) = email else {
        return page(LoginPage {
            view: "enter",
            turnstile: None,
            email: String::new(),
            token: String::new(),
            error: Some(EXPIRED),
        });
    };
    let started = async {
        let account = sites::find_or_create_account(&state.db, &email).await?;
        let suspended: bool =
            sqlx::query_scalar("SELECT suspended_at IS NOT NULL FROM accounts WHERE id = ?")
                .bind(&account)
                .fetch_one(&state.db)
                .await?;
        anyhow::ensure!(!suspended, "account suspended");
        start_session(&state, &account).await
    }
    .await;
    match started {
        Ok(res) => crate::oauth::resume(&headers, res),
        Err(err) => {
            tracing::error!(%err, "creating session failed");
            page(LoginPage {
                view: "enter",
                turnstile: None,
                email,
                token: String::new(),
                error: Some("Something went wrong. Try again."),
            })
        }
    }
}

/// Signs `account_id` in and sends them to the app.
pub async fn start_session(state: &AppState, account_id: &str) -> anyhow::Result<Response> {
    let session = random_token();
    sqlx::query(
        "INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(hash_token(&session))
    .bind(account_id)
    .bind(now())
    .bind(now() + SESSION_TTL_SECS)
    .execute(&state.db)
    .await?;
    let secure = if state.config.base_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{SESSION_COOKIE}={session}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_TTL_SECS}{secure}"
    );
    let mut res = Redirect::to("/app").into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("ascii cookie"),
    );
    Ok(res)
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie(&headers, SESSION_COOKIE) {
        let _ = sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash_token(&token))
            .execute(&state.db)
            .await;
    }
    let mut res = Redirect::to("/login").into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("virling_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    res
}

pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

/// The signed-in user's account; redirects to /login when there is no valid session.
#[derive(Debug, Clone)]
pub struct Account {
    /// The account that owns the sites: the user's own, or the team they are a member of.
    pub id: String,
    /// The signed-in user.
    pub user: String,
    /// False for team members, who can't change the plan, API keys or the team.
    pub owner: bool,
}

impl FromRequestParts<AppState> for Account {
    type Rejection = Redirect;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = cookie(&parts.headers, SESSION_COOKIE).ok_or(Redirect::to("/login"))?;
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT s.account_id, m.org_id FROM sessions s JOIN accounts u ON u.id = s.account_id LEFT JOIN members m ON m.account_id = s.account_id LEFT JOIN accounts o ON o.id = m.org_id WHERE s.token_hash = ? AND s.expires_at > ? AND u.suspended_at IS NULL AND o.suspended_at IS NULL",
        )
        .bind(hash_token(&token))
        .bind(now())
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
        let (user, org) = row.ok_or(Redirect::to("/login"))?;
        Ok(Account {
            id: org.clone().unwrap_or_else(|| user.clone()),
            owner: org.is_none(),
            user,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_hashed() {
        let a = random_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, random_token());
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn reads_cookies() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; virling_session=xyz; b=2"),
        );
        assert_eq!(cookie(&h, SESSION_COOKIE).as_deref(), Some("xyz"));
        assert_eq!(cookie(&h, "missing"), None);
    }
}
