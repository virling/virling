use askama::Template;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use time::OffsetDateTime;

use crate::app::AppState;
use crate::auth::random_token;
use crate::waitlist_copy::{confirm_text, ConfirmEmail, AGENCY, SELFHOST};

#[derive(Template)]
#[template(path = "waitlist_left.html")]
struct LeftPage {
    removed: bool,
}

#[derive(Deserialize)]
pub struct Signup {
    email: String,
    wedge: String,
    clients: Option<String>,
    referrer: Option<String>,
}

pub fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else { return false };
    email.len() <= 254
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(|c| c.is_whitespace())
}

pub async fn join(State(state): State<AppState>, Json(input): Json<Signup>) -> (StatusCode, Json<Value>) {
    let email = input.email.trim().to_ascii_lowercase();
    if !valid_email(&email) || !matches!(input.wedge.as_str(), "selfhost" | "agency") {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Enter a valid email" })));
    }
    let clip = |s: Option<String>, n: usize| s.map(|s| s.chars().take(n).collect::<String>());
    let leave_token = random_token();
    let result = sqlx::query(
        "INSERT INTO waitlist (email, wedge, clients, referrer, created_at, leave_token) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(&email)
    .bind(&input.wedge)
    .bind(clip(input.clients, 40))
    .bind(clip(input.referrer, 500))
    .bind(OffsetDateTime::now_utc().unix_timestamp().to_string())
    .bind(&leave_token)
    .execute(&state.db)
    .await;
    match result {
        Ok(done) => {
            if done.rows_affected() == 1 {
                let copy = if input.wedge == "agency" { &AGENCY } else { &SELFHOST };
                let leave_link = format!("{}/waitlist/leave?token={leave_token}", state.config.base_url);
                let html = ConfirmEmail { copy, leave_link: &leave_link }.render().unwrap_or_default();
                let text = confirm_text(copy, &leave_link);
                let mailer = state.mailer.clone();
                let from = state.config.waitlist_from.clone();
                tokio::spawn(async move {
                    if let Err(err) = mailer.send_as(Some(&from), &email, copy.subject, text, html).await {
                        tracing::error!(%err, "waitlist confirmation email failed");
                    }
                });
            }
            (StatusCode::OK, Json(json!({ "ok": true })))
        }
        Err(err) => {
            tracing::error!(%err, "waitlist insert failed");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Something went wrong. Try again." })))
        }
    }
}

#[derive(Deserialize)]
pub struct LeaveInput {
    token: String,
}

pub async fn leave(State(state): State<AppState>, Query(input): Query<LeaveInput>) -> Response {
    let removed = sqlx::query("DELETE FROM waitlist WHERE leave_token = ?")
        .bind(&input.token)
        .execute(&state.db)
        .await
        .map(|r| r.rows_affected() > 0)
        .unwrap_or(false);
    match (LeftPage { removed }).render() {
        Ok(html) => Html(html).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::valid_email;

    #[test]
    fn emails() {
        assert!(valid_email("a@b.co"));
        assert!(!valid_email("a@b"));
        assert!(!valid_email("a b@c.com"));
        assert!(!valid_email("@c.com"));
    }
}
