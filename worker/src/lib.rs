//! Waitlist-phase site for virling.com on Cloudflare Workers: static landing pages plus the waitlist API.
use askama::Template;
use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::wasm_bindgen::JsValue;
use worker::*;

// Shared with the server, which also uses source_name.
#[path = "../../src/assets.rs"]
mod assets;
#[allow(dead_code)]
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

#[derive(Template)]
#[template(path = "email/founding_personal.html")]
struct FoundingPersonalHtml {
    number: u64,
}

#[derive(Template)]
#[template(path = "email/founding_personal.txt")]
struct FoundingPersonalText {
    number: u64,
}

#[derive(Serialize, Deserialize)]
struct ConversionTotal {
    kind: String,
    count: i64,
    amount_cents: i64,
}

impl ConversionTotal {
    fn dollars(&self) -> String {
        format!(
            "${}.{:02}",
            self.amount_cents / 100,
            self.amount_cents % 100
        )
    }
}

#[derive(Template, Serialize)]
#[template(path = "stats.html")]
struct Stats {
    days: i64,
    visitors: i64,
    pageviews: i64,
    sessions: i64,
    conversions: Vec<ConversionTotal>,
    channels: Vec<analytics::StatsRow>,
    landing_pages: Vec<analytics::StatsRow>,
    exit_pages: Vec<analytics::LabelCount>,
    exit_links: Vec<analytics::LabelCount>,
}

#[derive(Deserialize)]
struct Signup {
    email: String,
    wedge: String,
    clients: Option<String>,
    referrer: Option<String>,
    visitor_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
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
    if let Some(entry) = kv.get(&key).json::<Entry>().await? {
        return json_status(json!({ "ok": true, "token": entry.leave_token }), 200);
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
    let added = async {
        ctx.d1("DB")?
            .prepare(
                "INSERT OR IGNORE INTO applications (email, wedge, created_at) VALUES (?1, ?2, ?3)",
            )
            .bind(&[
                JsValue::from_str(&email),
                JsValue::from_str(&input.wedge),
                JsValue::from_f64(Date::now().as_millis() as f64),
            ])?
            .run()
            .await
    }
    .await;
    if let Err(err) = added {
        console_error!("application row not stored: {err}");
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
    json_status(json!({ "ok": true, "token": leave_token }), 200)
}

/// Wedge and email of a stored signup key, "signup:{wedge}:{email}".
fn signup_of(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix("signup:")?.split_once(':')
}

/// Beta applications for the operator panel, behind a bearer ADMIN_TOKEN.
async fn applications(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let given = req
        .headers()
        .get("Authorization")?
        .and_then(|h| h.strip_prefix("Bearer ").map(str::to_string))
        .unwrap_or_default();
    let expected = ctx
        .secret("ADMIN_TOKEN")
        .map(|s| s.to_string())
        .unwrap_or_default();
    if !analytics::token_matches(&given, &expected) {
        return json_status(json!({ "error": "unauthorized" }), 401);
    }
    let rows = ctx
        .d1("DB")?
        .prepare("SELECT email, wedge, created_at, site, payments, revenue, audience FROM applications ORDER BY created_at DESC LIMIT 500")
        .all()
        .await?
        .results::<serde_json::Value>()?;
    private(Response::from_json(&rows)?)
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
        if let Some((wedge, email)) = signup_of(&key) {
            ctx.d1("DB")?
                .prepare("DELETE FROM applications WHERE email = ?1 AND wedge = ?2")
                .bind(&[JsValue::from_str(email), JsValue::from_str(wedge)])?
                .run()
                .await?;
        }
        removed = true;
    }
    let html = LeftPage { removed }
        .render()
        .map_err(|e| Error::RustError(e.to_string()))?;
    Response::from_html(html)
}

#[derive(Deserialize, Default)]
struct SessionInput {
    #[serde(default)]
    plan: preorder::Plan,
    email: Option<String>,
    visitor_id: Option<String>,
    code: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct FounderRecord {
    number: u64,
    email: String,
    name: Option<String>,
    created_at: String,
    emailed: bool,
}

async fn founder_count(kv: &kv::KvStore, plan: preorder::Plan) -> Result<u64> {
    Ok(kv
        .get(plan.count_key())
        .text()
        .await?
        .and_then(|n| n.parse().ok())
        .unwrap_or(0))
}

fn discount_code(code: Option<String>) -> Option<String> {
    let code = code?.trim().to_ascii_uppercase();
    (!code.is_empty() && code.len() <= 32 && code.chars().all(|c| c.is_ascii_alphanumeric()))
        .then_some(code)
}

async fn preorder_session(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let ip = req.headers().get("CF-Connecting-IP")?.unwrap_or_default();
    if !ctx.rate_limiter("PREORDER_LIMIT")?.limit(ip).await?.success {
        return json_status(
            json!({ "error": "Too many attempts. Try again in a minute." }),
            429,
        );
    }
    let cookie = req.headers().get("Cookie")?.unwrap_or_default();
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
            preorder::session_body(
                input.plan,
                email.as_deref(),
                visitor(input.visitor_id)
                    .or_else(|| visitor(preorder::cookie_visitor(&cookie)))
                    .as_deref(),
                discount_code(input.code).as_deref(),
            )
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
    let url = req.url()?;
    let query = |key: &str| {
        url.query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    };
    let plan = match query("plan") {
        None => preorder::Plan::Agency,
        Some(p) => match preorder::Plan::parse(&p) {
            Some(plan) => plan,
            None => return json_status(json!({ "error": "Unknown plan" }), 400),
        },
    };
    let kv = ctx.kv("WAITLIST")?;
    let founders = founder_count(&kv, plan).await?;
    let payment = query("payment_id")
        .filter(|s| s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    let number = match payment {
        Some(id) => kv
            .get(&plan.founder_key(&id))
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
        founder.plan.conversion(),
        visitor(founder.visitor_id.clone()).as_deref(),
        founder.amount_cents,
        Some(&founder.payment_id),
    )
    .await;
    let kv = ctx.kv("WAITLIST")?;
    let key = founder.plan.founder_key(&founder.payment_id);
    let mut record = match kv.get(&key).json::<FounderRecord>().await? {
        Some(record) => record,
        None => {
            let number = founder_count(&kv, founder.plan).await? + 1;
            kv.put(founder.plan.count_key(), number.to_string())?
                .execute()
                .await?;
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
        let number = record.number;
        let (subject, html, text) = match founder.plan {
            preorder::Plan::Agency => (
                format!("You\u{2019}re founding agency #{number}"),
                FoundingHtml { number }.render().map_err(render)?,
                FoundingText { number }.render().map_err(render)?,
            ),
            preorder::Plan::Personal => (
                format!("You\u{2019}re founding customer #{number}"),
                FoundingPersonalHtml { number }.render().map_err(render)?,
                FoundingPersonalText { number }.render().map_err(render)?,
            ),
        };
        let email = json!({
            "from": from,
            "reply_to": from,
            "to": [record.email],
            "subject": subject,
            "html": html,
            "text": text,
        });
        send_email(&ctx.secret("RESEND_API_KEY")?.to_string(), &email).await?;
        record.emailed = true;
        kv.put(&key, serde_json::to_string(&record)?)?
            .execute()
            .await?;
    }
    json_status(json!({ "received": true }), 200)
}

/// Founding spots bought with an email, for the hosted app to link them at sign-up.
async fn founder_lookup(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let given = req.headers().get("Authorization")?.unwrap_or_default();
    let expected = ctx
        .secret("FOUNDERS_TOKEN")
        .map(|s| s.to_string())
        .unwrap_or_default();
    if !analytics::token_matches(given.trim_start_matches("Bearer "), &expected) {
        return json_status(json!({ "error": "unauthorized" }), 401);
    }
    let email = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "email")
        .map(|(_, v)| v.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let kv = ctx.kv("WAITLIST")?;
    let mut founders = Vec::new();
    for key in kv.list().prefix("founder:".into()).execute().await?.keys {
        let Some(plan) = preorder::plan_of_key(&key.name) else {
            continue;
        };
        if let Some(record) = kv.get(&key.name).json::<FounderRecord>().await?
            && !email.is_empty()
            && record.email.eq_ignore_ascii_case(&email)
        {
            founders.push(json!({ "plan": plan, "number": record.number }));
        }
    }
    private(Response::from_json(&json!({ "founders": founders }))?)
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

async fn load_stats(req: &Request, ctx: &RouteContext<()>) -> Result<Option<Stats>> {
    let token = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "token")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();
    let expected = ctx
        .secret("STATS_TOKEN")
        .map(|s| s.to_string())
        .unwrap_or_default();
    if !analytics::token_matches(&token, &expected) {
        return Ok(None);
    }
    let days = 30;
    let now = Date::now().as_millis() as i64;
    let since = JsValue::from_f64((now - days * analytics::DAY_MS) as f64);
    let lookback =
        JsValue::from_f64((now - days * analytics::DAY_MS - analytics::ATTRIBUTION_MS) as f64);
    let db = ctx.d1("DB")?;
    let query = |sql: &str| db.prepare(sql).bind(std::slice::from_ref(&since));
    let mut results = db
        .batch(vec![
            query(analytics::STATS_TOTALS)?,
            query(analytics::STATS_SESSIONS)?,
            query(analytics::STATS_CONVERSIONS)?,
            query(analytics::STATS_CHANNELS)?,
            query(analytics::STATS_CONVERSIONS_BY_CHANNEL)?,
            query(analytics::STATS_LANDING)?,
            db.prepare(analytics::STATS_CONVERSIONS_BY_LANDING)
                .bind(&[since.clone(), lookback])?,
            query(analytics::STATS_EXIT_PAGES)?,
            query(analytics::STATS_EXIT_LINKS)?,
        ])
        .await?
        .into_iter();
    let mut next = || {
        results
            .next()
            .ok_or_else(|| Error::RustError("missing result".into()))
    };
    #[derive(Deserialize)]
    struct Totals {
        visitors: i64,
        pageviews: i64,
    }
    #[derive(Deserialize)]
    struct Sessions {
        sessions: i64,
    }
    let totals = next()?.results::<Totals>()?.into_iter().next();
    let sessions = next()?.results::<Sessions>()?.into_iter().next();
    let conversions = next()?.results()?;
    let channels = analytics::pivot(next()?.results()?, next()?.results()?);
    let landing_pages = analytics::pivot(next()?.results()?, next()?.results()?);
    let exit_pages = next()?.results()?;
    let exit_links = next()?.results()?;
    Ok(Some(Stats {
        days,
        visitors: totals.as_ref().map_or(0, |t| t.visitors),
        pageviews: totals.as_ref().map_or(0, |t| t.pageviews),
        sessions: sessions.map_or(0, |s| s.sessions),
        conversions,
        channels,
        landing_pages,
        exit_pages,
        exit_links,
    }))
}

fn private(mut res: Response) -> Result<Response> {
    res.headers_mut().set("Cache-Control", "no-store")?;
    res.headers_mut().set("X-Robots-Tag", "noindex")?;
    Ok(res)
}

async fn stats_json(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    match load_stats(&req, &ctx).await? {
        Some(stats) => private(Response::from_json(&stats)?),
        None => json_status(json!({ "error": "unauthorized" }), 401),
    }
}

async fn stats_page(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    match load_stats(&req, &ctx).await? {
        Some(stats) => private(Response::from_html(
            stats
                .render()
                .map_err(|e| Error::RustError(e.to_string()))?,
        )?),
        None => Response::error("Not found", 404),
    }
}

/// Markdown copy of a page, when the browser or agent asks for text/markdown.
fn markdown_path(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    if path.ends_with(".md") {
        return None;
    }
    Some(if path.is_empty() {
        "/index.md".into()
    } else {
        format!("{path}.md")
    })
}

/// Serves the page's Markdown copy to `Accept: text/markdown`, and every other request from the static assets.
async fn pages(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let assets = ctx.env.assets("ASSETS")?;
    let wants = req.method() == Method::Get
        && req
            .headers()
            .get("Accept")?
            .is_some_and(|a| a.contains("text/markdown"));
    if wants && let Some(path) = markdown_path(req.url()?.path()) {
        let mut url = req.url()?;
        url.set_path(&path);
        let mut copy = assets
            .fetch_request(Request::new(url.as_str(), Method::Get)?)
            .await?;
        if copy.status_code() == 200 {
            let text = copy.text().await?;
            let mut res = Response::ok(&text)?;
            let h = res.headers_mut();
            h.set("Content-Type", "text/markdown; charset=utf-8")?;
            h.set("Vary", "Accept")?;
            h.set("x-markdown-tokens", &(text.len() / 4).to_string())?;
            return Ok(res);
        }
    }
    assets.fetch_request(req).await
}

/// Tells the Virling server about a bot reading a page, since bots never run the tracking script.
fn forward_ai(req: &Request, ctx: &Context) {
    let ua = req
        .headers()
        .get("User-Agent")
        .ok()
        .flatten()
        .unwrap_or_default();
    if req.method() != Method::Get || ua.is_empty() || !ua::is_bot(&ua) {
        return;
    }
    let Ok(url) = req.url() else {
        return;
    };
    let body = serde_json::json!({ "s": "virling.com", "u": url.as_str(), "ua": ua }).to_string();
    ctx.wait_until(async move {
        let headers = Headers::new();
        let _ = headers.set("content-type", "application/json");
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(worker::wasm_bindgen::JsValue::from_str(&body)));
        if let Ok(post) = Request::new_with_init("https://beta.virling.com/api/crawl", &init) {
            let _ = Fetch::Request(post).send().await;
        }
    });
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    forward_ai(&req, &ctx);
    Router::new()
        .get_async("/api/stats", stats_json)
        .get_async("/stats", stats_page)
        .post_async("/api/event", collect)
        .options("/api/event", event_preflight)
        .post_async("/api/waitlist", join)
        .get_async("/api/applications", applications)
        .get_async("/waitlist/leave", leave)
        .post_async("/api/preorder/session", preorder_session)
        .get_async("/api/preorder/status", preorder_status)
        .post_async("/api/dodo/webhook", dodo_webhook)
        .get_async("/api/founders", founder_lookup)
        .or_else_any_method_async("/", pages)
        .or_else_any_method_async("/*path", pages)
        .run(req, env)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_applications_from_signup_keys() {
        assert_eq!(
            signup_of("signup:agency:a@b.co"),
            Some(("agency", "a@b.co"))
        );
        assert_eq!(signup_of("leave:x"), None);
    }
}
