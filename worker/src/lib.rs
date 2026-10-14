//! Waitlist-phase site for virling.com on Cloudflare Workers: static landing pages plus the waitlist API.
use askama::Template;
use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::wasm_bindgen::JsValue;
use worker::*;

#[path = "../../src/channel.rs"]
mod channel;
#[path = "../../src/ua.rs"]
mod ua;
#[path = "../../src/waitlist_copy.rs"]
mod waitlist_copy;

mod analytics;
mod preorder;

use waitlist_copy::{AGENCY, ConfirmEmail, SELFHOST, confirm_text};

#[derive(Template)]
#[template(path = "waitlist_left.html")]
struct LeftPage {
    removed: bool,
}

#[derive(Template)]
#[template(path = "email/founding.html")]
struct FoundingHtml {
    number: u64,
}

#[derive(Template)]
#[template(path = "email/founding.txt")]
struct FoundingText {
    number: u64,
}

#[derive(Deserialize)]
struct Signup {
    email: String,
    wedge: String,
    clients: Option<String>,
    referrer: Option<String>,
    visitor_id: Option<String>,
}

#[derive(Serialize)]
struct Entry {
    email: String,
    wedge: String,
    clients: Option<String>,
    referrer: Option<String>,
    created_at: String,
    leave_token: String,
    visitor_id: Option<String>,
}

fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
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

fn visitor(v: Option<String>) -> Option<String> {
    v.filter(|v| analytics::valid_visitor_id(v))
}

async fn record_conversion(
    ctx: &RouteContext<()>,
    kind: &str,
    visitor_id: Option<&str>,
    amount_cents: Option<i64>,
    reference: Option<&str>,
) {
    let opt = |s: Option<&str>| s.map_or(JsValue::NULL, JsValue::from_str);
    let result = async {
        ctx.d1("DB")?
            .prepare(analytics::INSERT_CONVERSION)
            .bind(&[
                JsValue::from_f64(Date::now().as_millis() as f64),
                JsValue::from_str(kind),
                opt(visitor_id),
                amount_cents.map_or(JsValue::NULL, |a| JsValue::from_f64(a as f64)),
                opt(reference),
            ])?
            .run()
            .await
    }
    .await;
    if let Err(err) = result {
        console_error!("conversion {kind} not recorded: {err}");
    }
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
    let visitor_id = visitor(input.visitor_id);
    if kv.get(&key).text().await?.is_some() {
        return json_status(json!({ "ok": true }), 200);
    }
    let leave_token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let entry = Entry {
        email: email.clone(),
        wedge: input.wedge.clone(),
        clients: clip(input.clients, 40),
        referrer: clip(input.referrer, 500),
        created_at: Date::now().to_string(),
        leave_token: leave_token.clone(),
        visitor_id: visitor_id.clone(),
    };
    kv.put(&key, serde_json::to_string(&entry)?)?
        .execute()
        .await?;
    kv.put(&format!("leave:{leave_token}"), &key)?
        .execute()
        .await?;
    if let Some(kind) = analytics::waitlist_kind(&input.wedge) {
        record_conversion(&ctx, kind, visitor_id.as_deref(), None, None).await;
    }

    let copy = if input.wedge == "agency" {
        &AGENCY
    } else {
        &SELFHOST
    };
    let leave_link = format!(
        "{}/waitlist/leave?token={leave_token}",
        ctx.var("BASE_URL")?
    );
    let html = ConfirmEmail {
        copy,
        leave_link: &leave_link,
    }
    .render()
    .map_err(|e| Error::RustError(e.to_string()))?;
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
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.to_string().into()));
    let mut res = Fetch::Request(Request::new_with_init(
        "https://api.resend.com/emails",
        &init,
    )?)
    .send()
    .await?;
    if res.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "resend {}: {}",
            res.status_code(),
            res.text().await?
        )));
    }
    Ok(())
}

async fn leave(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let token = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "token")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();
    let kv = ctx.kv("WAITLIST")?;
    let mut removed = false;
    if !token.is_empty()
        && let Some(key) = kv.get(&format!("leave:{token}")).text().await?
    {
        kv.delete(&key).await?;
        kv.delete(&format!("leave:{token}")).await?;
        removed = true;
    }
    let html = LeftPage { removed }
        .render()
        .map_err(|e| Error::RustError(e.to_string()))?;
    Response::from_html(html)
}

#[derive(Deserialize, Default)]
struct SessionInput {
    email: Option<String>,
    visitor_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct FounderRecord {
    number: u64,
    email: String,
    name: Option<String>,
    created_at: String,
    emailed: bool,
}

const FOUNDER_COUNT: &str = "founders:count";

async fn founder_count(kv: &kv::KvStore) -> Result<u64> {
    Ok(kv
        .get(FOUNDER_COUNT)
        .text()
        .await?
        .and_then(|n| n.parse().ok())
        .unwrap_or(0))
}

async fn preorder_session(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let ip = req.headers().get("CF-Connecting-IP")?.unwrap_or_default();
    if !ctx.rate_limiter("PREORDER_LIMIT")?.limit(ip).await?.success {
        return json_status(
            json!({ "error": "Too many attempts. Try again in a minute." }),
            429,
        );
    }
    let raw = req.text().await?;
    let input = if raw.trim().is_empty() {
        SessionInput::default()
    } else {
        match serde_json::from_str::<SessionInput>(&raw) {
            Ok(input) => input,
            Err(_) => return json_status(json!({ "error": "Invalid request" }), 400),
        }
    };
    let email = input.email.map(|e| e.trim().to_ascii_lowercase());
    if email.as_deref().is_some_and(|e| !valid_email(e)) {
        return json_status(json!({ "error": "Enter a valid email" }), 400);
    }
    let headers = Headers::new();
    headers.set(
        "Authorization",
        &format!("Bearer {}", ctx.secret("DODO_API_KEY")?),
    )?;
    headers.set("Content-Type", "application/json")?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(
            preorder::session_body(email.as_deref(), visitor(input.visitor_id).as_deref())
                .to_string()
                .into(),
        ));
    let mut res = Fetch::Request(Request::new_with_init(
        "https://live.dodopayments.com/checkouts",
        &init,
    )?)
    .send()
    .await?;
    let status = res.status_code();
    let body: serde_json::Value = res.json().await.unwrap_or_default();
    match body["checkout_url"].as_str() {
        Some(url) if status < 300 => json_status(json!({ "checkout_url": url }), 200),
        _ => {
            console_error!("dodo checkout session {status}: {body}");
            json_status(json!({ "error": "Checkout couldn't load." }), 502)
        }
    }
}

async fn preorder_status(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let kv = ctx.kv("WAITLIST")?;
    let founders = founder_count(&kv).await?;
    let subscription = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "subscription_id")
        .map(|(_, v)| v.into_owned())
        .filter(|s| s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    let number = match subscription {
        Some(id) => kv
            .get(&format!("founder:{id}"))
            .json::<FounderRecord>()
            .await?
            .map(|r| r.number),
        None => None,
    };
    let mut res = Response::from_json(&json!({
        "founders": founders,
        "spots_left": preorder::spots_left(founders),
        "number": number,
    }))?;
    res.headers_mut().set("Cache-Control", "no-store")?;
    Ok(res)
}

async fn dodo_webhook(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body = req.text().await?;
    let header = |name: &str| req.headers().get(name).map(Option::unwrap_or_default);
    let (id, timestamp, signature) = (
        header("webhook-id")?,
        header("webhook-timestamp")?,
        header("webhook-signature")?,
    );
    let secret = ctx.secret("DODO_WEBHOOK_SECRET")?.to_string();
    let now = (Date::now().as_millis() / 1000) as i64;
    if let Err(err) = preorder::verify_signature(&secret, &id, &timestamp, &body, &signature, now) {
        console_error!("dodo webhook rejected: {err:?}");
        return json_status(json!({ "error": "invalid signature" }), 401);
    }
    let Some(founder) = preorder::founder_from_event(&body) else {
        return json_status(json!({ "received": true }), 200);
    };
    record_conversion(
        &ctx,
        "preorder",
        visitor(founder.visitor_id.clone()).as_deref(),
        founder.amount_cents,
        Some(&founder.subscription_id),
    )
    .await;
    let kv = ctx.kv("WAITLIST")?;
    let key = format!("founder:{}", founder.subscription_id);
    let mut record = match kv.get(&key).json::<FounderRecord>().await? {
        Some(record) => record,
        None => {
            let number = founder_count(&kv).await? + 1;
            kv.put(FOUNDER_COUNT, number.to_string())?.execute().await?;
            let record = FounderRecord {
                number,
                email: founder.email,
                name: founder.name,
                created_at: Date::now().to_string(),
                emailed: false,
            };
            kv.put(&key, serde_json::to_string(&record)?)?
                .execute()
                .await?;
            record
        }
    };
    if !record.emailed {
        let render = |e: askama::Error| Error::RustError(e.to_string());
        let from = ctx.var("WAITLIST_FROM")?.to_string();
        let email = json!({
            "from": from,
            "reply_to": from,
            "to": [record.email],
            "subject": format!("You\u{2019}re founding agency #{}", record.number),
            "html": FoundingHtml { number: record.number }.render().map_err(render)?,
            "text": FoundingText { number: record.number }.render().map_err(render)?,
        });
        send_email(&ctx.secret("RESEND_API_KEY")?.to_string(), &email).await?;
        record.emailed = true;
        kv.put(&key, serde_json::to_string(&record)?)?
            .execute()
            .await?;
    }
    json_status(json!({ "received": true }), 200)
}

fn cors(mut res: Response) -> Result<Response> {
    res.headers_mut().set("Access-Control-Allow-Origin", "*")?;
    Ok(res)
}

async fn collect(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let headers = req.headers();
    let user_agent = headers.get("User-Agent")?.unwrap_or_default();
    let country = analytics::country(headers.get("CF-IPCountry")?.as_deref());
    let ip = headers.get("CF-Connecting-IP")?.unwrap_or_default();
    if ua::is_bot(&user_agent) {
        return cors(Response::empty()?.with_status(202));
    }
    if !ctx.rate_limiter("EVENT_LIMIT")?.limit(ip).await?.success {
        return cors(Response::empty()?.with_status(429));
    }
    let Some(batch) = analytics::parse_batch(&req.text().await?) else {
        return cors(Response::error("invalid event", 400)?);
    };
    let now = Date::now().as_millis() as i64;
    let db = ctx.d1("DB")?;
    let insert = db.prepare(analytics::INSERT_EVENT);
    let text = |s: &Option<String>| s.as_deref().map_or(JsValue::NULL, JsValue::from_str);
    let mut statements = Vec::new();
    for e in batch {
        let Some(row) = analytics::to_row(e, &user_agent, country.as_deref(), now) else {
            continue;
        };
        statements.push(insert.clone().bind(&[
            JsValue::from_f64(row.ts as f64),
            JsValue::from_str(&row.visitor_id),
            JsValue::from_str(row.kind),
            text(&row.name),
            JsValue::from_str(&row.path),
            text(&row.referrer_host),
            text(&row.utm_source),
            text(&row.utm_medium),
            text(&row.utm_campaign),
            text(&row.utm_content),
            text(&row.utm_term),
            JsValue::from_str(row.channel),
            text(&row.country),
            JsValue::from_str(row.device),
            JsValue::from_str(row.browser),
        ])?);
    }
    if !statements.is_empty() {
        db.batch(statements).await?;
    }
    cors(Response::empty()?.with_status(202))
}

fn event_preflight(_: Request, _: RouteContext<()>) -> Result<Response> {
    let mut res = Response::empty()?.with_status(204);
    let headers = res.headers_mut();
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set("Access-Control-Allow-Methods", "POST")?;
    headers.set("Access-Control-Allow-Headers", "content-type")?;
    headers.set("Access-Control-Max-Age", "86400")?;
    Ok(res)
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    Router::new()
        .post_async("/api/event", collect)
        .options("/api/event", event_preflight)
        .post_async("/api/waitlist", join)
        .get_async("/waitlist/leave", leave)
        .post_async("/api/preorder/session", preorder_session)
        .get_async("/api/preorder/status", preorder_status)
        .post_async("/api/dodo/webhook", dodo_webhook)
        .run(req, env)
        .await
}
