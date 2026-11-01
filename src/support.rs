//! Help page: links to the docs and, on hosted Virling, a form that emails support.

use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::Account;
use crate::dashboard::{Shell, load};
use crate::onboarding::render;
use crate::sites::app_path;

/// Where hosted support mail goes; self-hosted servers show the issue tracker instead.
pub const SUPPORT_EMAIL: &str = "help@virling.com";

const TOPICS: &[&str] = &["Setup", "Payments", "Billing", "Bug", "Other"];

#[derive(Template)]
#[template(path = "help.html")]
struct HelpPage {
    shell: Shell,
    title: &'static str,
    hosted: bool,
    support: &'static str,
    topics: &'static [&'static str],
    sent: bool,
    error: &'static str,
}

#[derive(Deserialize, Default)]
pub struct HelpQuery {
    sent: Option<String>,
}

pub async fn page(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Query(q): Query<HelpQuery>,
) -> Response {
    show(&state, &account, &domain, q.sent.is_some(), "").await
}

async fn show(
    state: &AppState,
    account: &Account,
    domain: &str,
    sent: bool,
    error: &'static str,
) -> Response {
    let ctx = match load(state, account, domain, "help").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    render(HelpPage {
        shell: ctx.shell,
        title: "Help",
        hosted: state.config.hosted,
        support: SUPPORT_EMAIL,
        topics: TOPICS,
        sent,
        error,
    })
}

#[derive(Deserialize)]
pub struct HelpInput {
    topic: String,
    message: String,
}

pub async fn send(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    headers: HeaderMap,
    Form(input): Form<HelpInput>,
) -> Response {
    if !state.config.hosted {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }
    let ctx = match load(&state, &account, &domain, "help").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    let message = input.message.trim();
    if message.is_empty() || message.len() > 5000 {
        return show(
            &state,
            &account,
            &domain,
            false,
            "Write a message of up to 5,000 characters.",
        )
        .await;
    }
    let topic = TOPICS
        .iter()
        .find(|t| **t == input.topic)
        .copied()
        .unwrap_or("Other");
    let shell = &ctx.shell;
    let agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let text = format!(
        "{message}\n\n--\nFrom: {}\nAccount: {}\nPlan: {}\nSite: {}\nBrowser: {agent}\n",
        shell.email, account.id, shell.plan, ctx.site.domain
    );
    let subject = format!("[{topic}] {}", ctx.site.domain);
    if let Err(err) = state
        .mailer
        .send_named(
            "Virling help form",
            &shell.email,
            SUPPORT_EMAIL,
            &subject,
            text.clone(),
            format!("<pre>{}</pre>", html_escape(&text)),
        )
        .await
    {
        tracing::error!(%err, "sending a support message failed");
        return show(
            &state,
            &account,
            &domain,
            false,
            "Your message could not be sent. Email help@virling.com instead.",
        )
        .await;
    }
    let ack = format!(
        "Thanks, we got your message about {}. We reply within one business day, from {SUPPORT_EMAIL}.\n\nYour message:\n\n{message}\n",
        ctx.site.domain
    );
    if let Err(err) = state
        .mailer
        .send_named(
            "Virling support",
            SUPPORT_EMAIL,
            &shell.email,
            "We got your message",
            ack.clone(),
            format!("<pre>{}</pre>", html_escape(&ack)),
        )
        .await
    {
        tracing::error!(%err, "sending a support receipt failed");
    }
    Redirect::to(&format!("{}?sent=1", app_path(&ctx.site.domain, "help"))).into_response()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn signed_in(state: &AppState) -> String {
        let account: String =
            sqlx::query_scalar("SELECT id FROM accounts WHERE email = 'me@example.com'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let token = crate::auth::random_token();
        sqlx::query("INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, 0, ?)")
            .bind(crate::auth::hash_token(&token))
            .bind(&account)
            .bind(i64::MAX / 2)
            .execute(&state.db)
            .await
            .unwrap();
        format!("{}={token}", crate::auth::SESSION_COOKIE)
    }

    fn hosted(state: &AppState) -> AppState {
        let mut state = state.clone();
        let mut config = (*state.config).clone();
        config.hosted = true;
        state.config = Arc::new(config);
        state
    }

    async fn call(state: &AppState, req: Request<Body>) -> (StatusCode, String, String) {
        let res = crate::app::router(state.clone())
            .oneshot(req)
            .await
            .unwrap();
        let status = res.status();
        let location = res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, location, String::from_utf8(body.to_vec()).unwrap())
    }

    fn mails(state: &AppState) -> Vec<(String, String, String)> {
        match &*state.mailer {
            crate::mail::Mailer::Capture(m) => m.lock().unwrap().clone(),
            _ => unreachable!(),
        }
    }

    fn post(cookie: &str, body: &str) -> Request<Body> {
        Request::post("/app/example.com/help")
            .header("cookie", cookie)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn hosted_help_form_emails_support_and_the_user() {
        let f = crate::api::tests::fixture().await;
        let state = hosted(&f.state);
        let cookie = signed_in(&state).await;
        let get = Request::get("/app/example.com/help")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let (status, _, html) = call(&state, get).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Contact support"));
        let (status, location, _) = call(
            &state,
            post(&cookie, "topic=Payments&message=Stripe+shows+Unattributed"),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(location, "/app/example.com/help?sent=1");
        let sent = mails(&state);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].0, SUPPORT_EMAIL);
        assert!(sent[0].1.contains("[Payments] example.com"));
        assert!(sent[0].2.contains("Stripe shows Unattributed"));
        assert!(sent[0].2.contains("me@example.com"));
        assert_eq!(sent[1].0, "me@example.com");
    }

    #[tokio::test]
    async fn empty_messages_are_refused() {
        let f = crate::api::tests::fixture().await;
        let state = hosted(&f.state);
        let cookie = signed_in(&state).await;
        let (status, _, html) = call(&state, post(&cookie, "topic=Bug&message=+++")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Write a message"));
        assert!(mails(&state).is_empty());
    }

    #[tokio::test]
    async fn self_hosted_help_has_no_form() {
        let f = crate::api::tests::fixture().await;
        let cookie = signed_in(&f.state).await;
        let get = Request::get("/app/example.com/help")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let (status, _, html) = call(&f.state, get).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!html.contains("Contact support"));
        assert!(html.contains("github.com/virling/virling/issues"));
        let (status, _, _) = call(&f.state, post(&cookie, "topic=Bug&message=hi")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
