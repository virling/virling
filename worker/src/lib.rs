//! Waitlist-phase site for virling.com on Cloudflare Workers: static landing pages plus the waitlist API.
use askama::Template;
use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::*;

#[path = "../../src/waitlist_copy.rs"]
mod waitlist_copy;

use waitlist_copy::{confirm_text, ConfirmEmail, AGENCY, SELFHOST};

#[derive(Template)]
#[template(path = "waitlist_left.html")]
struct LeftPage {
    removed: bool,
}

#[derive(Deserialize)]
struct Signup {
    email: String,
    wedge: String,
    clients: Option<String>,
    referrer: Option<String>,
}

#[derive(Serialize)]
struct Entry {
    email: String,
    wedge: String,
    clients: Option<String>,
    referrer: Option<String>,
    created_at: String,
    leave_token: String,
}

fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else { return false };
    email.len() <= 254
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(char::is_whitespace)
}

fn clip(s: Option<String>, n: usize) -> Option<String> {
    s.map(|s| s.chars().take(n).collect())
}

fn json_status(value: serde_json::Value, status: u16) -> Result<Response> {
    Ok(Response::from_json(&value)?.with_status(status))
}

async fn join(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let Ok(input) = req.json::<Signup>().await else {
        return json_status(json!({ "error": "Enter a valid email" }), 400);
    };
    let email = input.email.trim().to_ascii_lowercase();
    if !valid_email(&email) || !matches!(input.wedge.as_str(), "selfhost" | "agency") {
        return json_status(json!({ "error": "Enter a valid email" }), 400);
    }
    let kv = ctx.kv("WAITLIST")?;
    let key = format!("signup:{}:{email}", input.wedge);
    if kv.get(&key).text().await?.is_some() {
        return json_status(json!({ "ok": true }), 200);
    }
    let leave_token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let entry = Entry {
        email: email.clone(),
        wedge: input.wedge.clone(),
        clients: clip(input.clients, 40),
        referrer: clip(input.referrer, 500),
        created_at: Date::now().to_string(),
        leave_token: leave_token.clone(),
    };
    kv.put(&key, serde_json::to_string(&entry)?)?.execute().await?;
    kv.put(&format!("leave:{leave_token}"), &key)?.execute().await?;

    let copy = if input.wedge == "agency" { &AGENCY } else { &SELFHOST };
    let leave_link = format!("{}/waitlist/leave?token={leave_token}", ctx.var("BASE_URL")?.to_string());
    let html = ConfirmEmail { copy, leave_link: &leave_link }.render().map_err(|e| Error::RustError(e.to_string()))?;
    let from = ctx.var("WAITLIST_FROM")?.to_string();
    let body = json!({
        "from": from,
        "reply_to": from,
        "to": [email],
        "subject": copy.subject,
        "html": html,
        "text": confirm_text(copy, &leave_link),
    });
    let sent = match ctx.secret("RESEND_API_KEY") {
        Ok(key) => send_email(&key.to_string(), &body).await,
        Err(err) => Err(err),
    };
    if let Err(err) = sent {
        console_error!("waitlist confirmation email failed: {err}");
    }
    json_status(json!({ "ok": true }), 200)
}

async fn send_email(api_key: &str, body: &serde_json::Value) -> Result<()> {
    let headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {api_key}"))?;
    headers.set("Content-Type", "application/json")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_headers(headers).with_body(Some(body.to_string().into()));
    let mut res = Fetch::Request(Request::new_with_init("https://api.resend.com/emails", &init)?).send().await?;
    if res.status_code() >= 300 {
        return Err(Error::RustError(format!("resend {}: {}", res.status_code(), res.text().await?)));
    }
    Ok(())
}

async fn leave(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let token = req.url()?.query_pairs().find(|(k, _)| k == "token").map(|(_, v)| v.into_owned()).unwrap_or_default();
    let kv = ctx.kv("WAITLIST")?;
    let mut removed = false;
    if !token.is_empty() {
        if let Some(key) = kv.get(&format!("leave:{token}")).text().await? {
            kv.delete(&key).await?;
            kv.delete(&format!("leave:{token}")).await?;
            removed = true;
        }
    }
    let html = LeftPage { removed }.render().map_err(|e| Error::RustError(e.to_string()))?;
    Response::from_html(html)
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    Router::new()
        .post_async("/api/waitlist", join)
        .get_async("/waitlist/leave", leave)
        .run(req, env)
        .await
}
