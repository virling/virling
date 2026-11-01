//! Hosted plans through Dodo Payments: the 14-day card-required trial, cancel, seats and the subscription webhooks.

use askama::Template;
use axum::Form;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Duration, OffsetDateTime};

use crate::app::AppState;
use crate::auth::Account;
use crate::sites::{self, app_path};

pub const TRIAL_DAYS: i64 = 14;
/// The first month a founding pre-order already paid for.
const FOUNDER_MONTH: Duration = Duration::days(30);
const REMIND_BEFORE: Duration = Duration::days(3);
const TOLERANCE_SECS: i64 = 5 * 60;
const INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);

/// Dodo API access and the hosted products, set with --dodo-api-key on the hosted service.
#[derive(Debug, Clone)]
pub struct Dodo {
    pub api_url: String,
    pub api_key: String,
    pub webhook_secret: String,
    pub personal: String,
    pub agency: String,
    /// The add-on for each Agency user past the included ones.
    pub seat: String,
    /// Products at the founding price, for the first 50 customers of each plan.
    pub personal_founding: String,
    pub agency_founding: String,
    /// The virling.com lookup of founding pre-orders by email, and its bearer token.
    pub founders_url: String,
    pub founders_token: String,
}

/// Monthly price in cents of a plan with `extra` paid seats.
pub fn price(product: &str, extra: i64, founding: bool) -> i64 {
    match (product, founding) {
        ("agency", false) => 7900 + 900 * extra,
        ("agency", true) => 2900 + 900 * extra,
        (_, false) => 1900,
        (_, true) => 1140,
    }
}

/// The accounts.plan value for a subscription status at `now`.
pub fn plan(status: &str, trial_ends_at: Option<i64>, now: i64) -> &'static str {
    match status {
        "active" if trial_ends_at.is_some_and(|t| now < t) => "trialing",
        "active" => "paid",
        "past_due" | "on_hold" | "paused" => "past_due",
        _ => "cancelled",
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Billing {
    pub subscription_id: String,
    pub customer_id: String,
    pub status: String,
    pub product: String,
    pub trial_ends_at: Option<i64>,
    pub renews_at: Option<i64>,
    pub cancelling: bool,
    pub extra_seats: i64,
    pub founding: bool,
}

pub async fn load(db: &sqlx::SqlitePool, account_id: &str) -> sqlx::Result<Option<Billing>> {
    sqlx::query_as(
        "SELECT subscription_id, customer_id, status, product, trial_ends_at, renews_at, cancelling, extra_seats, founding
         FROM billing WHERE account_id = ?",
    )
    .bind(account_id)
    .fetch_optional(db)
    .await
}

/// "November 6, 2026"
pub fn date(ts: i64) -> String {
    OffsetDateTime::from_unix_timestamp(ts)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
        .format(format_description!(
            "[month repr:long] [day padding:none], [year]"
        ))
        .unwrap_or_default()
}

/// "December 1"
pub fn day_of(d: time::Date) -> String {
    d.format(format_description!("[month repr:long] [day padding:none]"))
        .unwrap_or_default()
}

/// Whole days left until `ts`, counting a started day.
pub fn days_left(ts: i64, now: i64) -> i64 {
    ((ts - now).max(0) + 86399) / 86400
}

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// Standard Webhooks base64.
fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6 | u32::from(v)) & 0xffff;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Checks a Dodo webhook the Standard Webhooks way, as the virling.com worker does.
pub fn verify(
    secret: &str,
    id: &str,
    timestamp: &str,
    body: &[u8],
    signatures: &str,
    now: i64,
) -> bool {
    let (Ok(ts), Some(key)) = (
        timestamp.parse::<i64>(),
        base64(secret.strip_prefix("whsec_").unwrap_or(secret)),
    ) else {
        return false;
    };
    if (now - ts).abs() > TOLERANCE_SECS {
        return false;
    }
    let mut msg = format!("{id}.{timestamp}.").into_bytes();
    msg.extend_from_slice(body);
    let want = crate::crypto::hmac_sha256(&key, &msg);
    signatures
        .split_whitespace()
        .filter_map(|s| s.strip_prefix("v1,"))
        .filter_map(base64)
        .any(|sig| {
            sig.len() == 32 && sig.iter().zip(want).fold(0, |acc, (a, b)| acc | (a ^ b)) == 0
        })
}

#[derive(Deserialize)]
struct Hook {
    #[serde(rename = "type")]
    kind: String,
    timestamp: String,
    data: Value,
}

#[derive(Deserialize)]
struct Sub {
    subscription_id: String,
    status: String,
    product_id: String,
    customer: Customer,
    #[serde(default)]
    addons: Vec<Addon>,
    next_billing_date: Option<String>,
    #[serde(default)]
    trial_period_days: i64,
    #[serde(default)]
    cancel_at_next_billing_date: bool,
    #[serde(default)]
    metadata: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
struct Customer {
    customer_id: String,
}

#[derive(Deserialize)]
struct Addon {
    addon_id: String,
    quantity: i64,
}

pub async fn webhook(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let Some(dodo) = &state.config.billing else {
        return StatusCode::NOT_FOUND;
    };
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };
    let id = header("webhook-id");
    if id.is_empty()
        || !verify(
            &dodo.webhook_secret,
            id,
            header("webhook-timestamp"),
            &body,
            header("webhook-signature"),
            now(),
        )
    {
        return StatusCode::UNAUTHORIZED;
    }
    match apply(&state, id, &body).await {
        Ok(()) => StatusCode::OK,
        Err(err) => {
            tracing::error!(%err, "handling a Dodo billing webhook failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

/// Applies a verified webhook once, skipping events older than the last one applied.
pub async fn apply(state: &AppState, id: &str, body: &[u8]) -> anyhow::Result<()> {
    let Some(dodo) = &state.config.billing else {
        return Ok(());
    };
    let hook: Hook = serde_json::from_slice(body)?;
    let mut tx = state.db.begin().await?;
    let fresh = sqlx::query(
        "INSERT INTO billing_webhooks (id, received_at) VALUES (?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(now())
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if fresh && hook.kind.starts_with("subscription.") {
        let sub: Sub = serde_json::from_value(hook.data)?;
        let at = OffsetDateTime::parse(&hook.timestamp, &Rfc3339)?.unix_timestamp();
        subscription(&mut tx, dodo, &hook.kind, &sub, at).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn subscription(
    tx: &mut sqlx::SqliteConnection,
    dodo: &Dodo,
    kind: &str,
    sub: &Sub,
    at: i64,
) -> anyhow::Result<()> {
    let (product, founding) = match sub.product_id.as_str() {
        p if p == dodo.agency => ("agency", false),
        p if p == dodo.personal => ("personal", false),
        p if p == dodo.agency_founding => ("agency", true),
        p if p == dodo.personal_founding => ("personal", true),
        _ => return Ok(()),
    };
    let account = match sub.metadata.get("account_id").and_then(Value::as_str) {
        Some(a) => Some(a.to_string()),
        None => {
            sqlx::query_scalar("SELECT account_id FROM billing WHERE subscription_id = ?")
                .bind(&sub.subscription_id)
                .fetch_optional(&mut *tx)
                .await?
        }
    };
    let Some(account) = account else {
        return Ok(());
    };
    let old: Option<(String, Option<i64>, i64)> = sqlx::query_as(
        "SELECT subscription_id, trial_ends_at, event_at FROM billing WHERE account_id = ?",
    )
    .bind(&account)
    .fetch_optional(&mut *tx)
    .await?;
    if old.as_ref().is_some_and(|o| o.2 > at) || sub.status == "pending" {
        return Ok(());
    }
    let renews = sub
        .next_billing_date
        .as_deref()
        .and_then(|d| OffsetDateTime::parse(d, &Rfc3339).ok())
        .map(OffsetDateTime::unix_timestamp);
    let mut trial = old
        .filter(|o| o.0 == sub.subscription_id)
        .map_or_else(|| renews.filter(|_| sub.trial_period_days > 0), |o| o.1);
    if kind == "subscription.renewed" {
        trial = trial.map(|t| t.min(at));
    }
    let extra: i64 = sub
        .addons
        .iter()
        .filter(|a| a.addon_id == dodo.seat)
        .map(|a| a.quantity)
        .sum();
    sqlx::query(
        "INSERT INTO billing (account_id, subscription_id, customer_id, status, product, trial_ends_at, renews_at, cancelling, extra_seats, event_at, founding)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (account_id) DO UPDATE SET subscription_id = excluded.subscription_id,
           customer_id = excluded.customer_id, status = excluded.status, product = excluded.product,
           trial_ends_at = excluded.trial_ends_at, renews_at = excluded.renews_at,
           cancelling = excluded.cancelling, extra_seats = excluded.extra_seats, event_at = excluded.event_at,
           founding = excluded.founding",
    )
    .bind(&account)
    .bind(&sub.subscription_id)
    .bind(&sub.customer.customer_id)
    .bind(&sub.status)
    .bind(product)
    .bind(trial)
    .bind(renews)
    .bind(sub.cancel_at_next_billing_date)
    .bind(extra)
    .bind(at)
    .bind(founding)
    .execute(&mut *tx)
    .await?;
    let prepaid: Option<i64> =
        sqlx::query_scalar("SELECT founder_until FROM accounts WHERE id = ?")
            .bind(&account)
            .fetch_one(&mut *tx)
            .await?;
    let plan = match plan(&sub.status, trial, at) {
        "trialing" | "paid" if founding => "founding",
        "cancelled" if prepaid.is_some_and(|t| at < t) => "founding",
        p => p,
    };
    sqlx::query("UPDATE accounts SET plan = ?, account_type = ? WHERE id = ?")
        .bind(plan)
        .bind(product)
        .bind(&account)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

async fn call(state: &AppState, method: Method, path: &str, body: Value) -> anyhow::Result<Value> {
    let dodo = state
        .config
        .billing
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("billing is not set up"))?;
    let resp = state
        .http
        .request(method, format!("{}{path}", dodo.api_url))
        .bearer_auth(&dodo.api_key)
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    anyhow::ensure!(status.is_success(), "Dodo answered HTTP {status}: {text}");
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// The cart for a plan: Agency carries one seat add-on per user past the included ones.
fn cart(dodo: &Dodo, product: &str, extra: i64, founding: bool) -> Value {
    let id = match (product, founding) {
        ("agency", false) => &dodo.agency,
        ("agency", true) => &dodo.agency_founding,
        (_, false) => &dodo.personal,
        (_, true) => &dodo.personal_founding,
    };
    let mut item = json!({ "product_id": id, "quantity": 1 });
    if product == "agency" && extra > 0 {
        item["addons"] = json!([{ "addon_id": dodo.seat, "quantity": extra }]);
    }
    item
}

/// Users past the ones included with Agency.
async fn extra_seats(db: &sqlx::SqlitePool, account_id: &str) -> sqlx::Result<i64> {
    let seats: i64 = sqlx::query_scalar("SELECT seats FROM accounts WHERE id = ?")
        .bind(account_id)
        .fetch_one(db)
        .await?;
    Ok((seats - crate::team::INCLUDED).max(0))
}

/// The account page of an owner's site, or the status to answer with.
async fn owner_page(
    state: &AppState,
    account: &Account,
    domain: &str,
) -> Result<String, StatusCode> {
    if state.config.billing.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    let all = sites::for_account(&state.db, &account.id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !all.iter().any(|s| s.domain == domain) {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(app_path(domain, "account"))
}

/// A subscription that still runs: active, or waiting on a payment.
pub fn live(b: &Billing) -> bool {
    matches!(
        b.status.as_str(),
        "active" | "past_due" | "on_hold" | "paused"
    )
}

/// The Billing section of the Account page.
pub struct View {
    /// "none", "trial", "paid" or "past_due".
    pub state: &'static str,
    pub headline: String,
    pub detail: String,
    pub cancelling: bool,
    /// The label of the button that starts a plan, or of the one that cancels it.
    pub button: &'static str,
}

pub async fn view(db: &sqlx::SqlitePool, account_id: &str, now: i64) -> sqlx::Result<View> {
    let b = load(db, account_id).await?;
    let (founder, prepaid): (Option<String>, Option<i64>) =
        sqlx::query_as("SELECT founder, founder_until FROM accounts WHERE id = ?")
            .bind(account_id)
            .fetch_one(db)
            .await?;
    let Some(b) = b.clone().filter(live) else {
        let (headline, detail, button) = match (&founder, prepaid.filter(|t| now < *t), &b) {
            (Some(f), Some(until), _) => (
                format!("Founding {} spot", crate::dashboard::plan_label(f)),
                format!(
                    "Your first month is paid until {}. Add a card to keep {} a month for life after that.",
                    date(until),
                    crate::dashboard::money(price(f, 0, true) as f64 / 100.0)
                ),
                "Add card",
            ),
            (_, _, None) => (
                "Start your 14-day trial".to_string(),
                "$0 today. Cancel anytime before day 15.".to_string(),
                "Start 14-day trial",
            ),
            _ => (
                "Choose a plan".to_string(),
                "Your trial is used, so the first month is charged today.".to_string(),
                "Subscribe",
            ),
        };
        return Ok(View {
            state: "none",
            headline,
            detail,
            cancelling: false,
            button,
        });
    };
    let name = match b.founding {
        true => format!("Founding {}", crate::dashboard::plan_label(&b.product)),
        false => crate::dashboard::plan_label(&b.product).to_string(),
    };
    let amount =
        crate::dashboard::money(price(&b.product, b.extra_seats, b.founding) as f64 / 100.0);
    let (state, headline, detail, button) = match plan(&b.status, b.trial_ends_at, now) {
        "trialing" => {
            let ends = b.trial_ends_at.unwrap_or(now);
            let left = days_left(ends, now);
            (
                "trial",
                match b.founding {
                    true => format!("{name}, first month paid"),
                    false => format!(
                        "{name} trial, {left} day{} left",
                        if left == 1 { "" } else { "s" }
                    ),
                },
                if b.cancelling {
                    format!("Ends on {}. Nothing will be charged.", date(ends))
                } else {
                    format!(
                        "First charge {amount} on {}. Cancel anytime before then and nothing is charged.",
                        date(ends)
                    )
                },
                if b.founding {
                    "Cancel plan"
                } else {
                    "Cancel trial"
                },
            )
        }
        "paid" => {
            let on = b.renews_at.map(date).unwrap_or_default();
            (
                "paid",
                format!("{name}, {amount} a month"),
                if b.cancelling {
                    format!("Ends on {on}. Nothing more will be charged.")
                } else {
                    format!("Renews on {on}.")
                },
                "Cancel plan",
            )
        }
        _ => (
            "past_due",
            format!("{name}, payment failed"),
            "The last charge didn't go through. Update your card to keep your plan.".into(),
            "Cancel plan",
        ),
    };
    Ok(View {
        state,
        headline,
        detail,
        cancelling: b.cancelling,
        button,
    })
}

/// Links a founding pre-order bought on virling.com with the same email: plan 'founding' and a paid first month.
pub async fn link_founder(state: AppState, account_id: String) {
    let linked = async {
        let Some(dodo) = &state.config.billing else {
            return anyhow::Ok(());
        };
        let (email, founder): (String, Option<String>) =
            sqlx::query_as("SELECT email, founder FROM accounts WHERE id = ?")
                .bind(&account_id)
                .fetch_one(&state.db)
                .await?;
        if founder.is_some() || load(&state.db, &account_id).await?.is_some() {
            return Ok(());
        }
        let resp = state
            .http
            .get(&dodo.founders_url)
            .query(&[("email", &email)])
            .bearer_auth(&dodo.founders_token)
            .send()
            .await?
            .error_for_status()?;
        #[derive(Deserialize)]
        struct Found {
            founders: Vec<Spot>,
        }
        #[derive(Deserialize)]
        struct Spot {
            plan: String,
        }
        let found: Found = resp.json().await?;
        let Some(plan) = found
            .founders
            .iter()
            .map(|s| s.plan.as_str())
            .find(|p| *p == "agency")
            .or_else(|| found.founders.first().map(|s| s.plan.as_str()))
            .filter(|p| matches!(*p, "agency" | "personal"))
        else {
            return Ok(());
        };
        sqlx::query(
            "UPDATE accounts SET founder = ?, founder_until = ?, plan = 'founding', account_type = ?
             WHERE id = ? AND founder IS NULL",
        )
        .bind(plan)
        .bind(now() + FOUNDER_MONTH.whole_seconds())
        .bind(plan)
        .bind(&account_id)
        .execute(&state.db)
        .await?;
        Ok(())
    }
    .await;
    if let Err(err) = linked {
        tracing::error!(%err, account = %account_id, "looking up a founding spot failed");
    }
}

/// Ends the founding plan of founders whose paid month ran out without a card on file.
pub async fn expire_founders(db: &sqlx::SqlitePool, now: i64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE accounts SET plan = 'cancelled' WHERE plan = 'founding' AND founder_until <= ?
           AND id NOT IN (SELECT account_id FROM billing
                          WHERE status IN ('active', 'past_due', 'on_hold', 'paused'))",
    )
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct StartInput {
    plan: String,
}

/// Sends the owner to Dodo checkout: 14 days at $0 with a card, unless the account already had a trial.
pub async fn start(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<StartInput>,
) -> Response {
    let page = match owner_page(&state, &account, &domain).await {
        Ok(p) => p,
        Err(code) => return code.into_response(),
    };
    if !matches!(input.plan.as_str(), "personal" | "agency") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let session = async {
        let old = load(&state.db, &account.id).await?;
        if old.as_ref().is_some_and(live) {
            return Ok(None);
        }
        let (email, founder, prepaid): (String, Option<String>, Option<i64>) =
            sqlx::query_as("SELECT email, founder, founder_until FROM accounts WHERE id = ?")
                .bind(&account.id)
                .fetch_one(&state.db)
                .await?;
        let dodo = state
            .config
            .billing
            .as_ref()
            .expect("checked in owner_page");
        let extra = extra_seats(&state.db, &account.id).await?;
        let founding = founder.as_deref() == Some(input.plan.as_str());
        // A founder's prepaid month runs as the trial, so the first charge comes when it ends.
        let days = match (founding, old.is_some()) {
            (true, _) => days_left(prepaid.unwrap_or(0), now()),
            (false, true) => 0,
            (false, false) => TRIAL_DAYS,
        };
        let body = json!({
            "product_cart": [cart(dodo, &input.plan, extra, founding)],
            "customer": { "email": email },
            "subscription_data": { "trial_period_days": days },
            "return_url": format!("{}{page}?billing=started#plan", state.config.base_url),
            "metadata": { "account_id": account.id },
        });
        let session = call(&state, Method::POST, "/checkouts", body).await?;
        anyhow::Ok(session["checkout_url"].as_str().map(str::to_string))
    }
    .await;
    match session {
        Ok(Some(url)) => Redirect::to(&url).into_response(),
        Ok(None) => Redirect::to(&format!("{page}#plan")).into_response(),
        Err(err) => {
            tracing::error!(%err, "starting a Dodo checkout failed");
            Redirect::to(&format!("{page}?billing=error#plan")).into_response()
        }
    }
}

/// Turns renewal off (cancel) or back on (resume); the plan runs to the end of the trial or month.
async fn renewal(state: &AppState, account: &Account, domain: &str, cancel: bool) -> Response {
    let page = match owner_page(state, account, domain).await {
        Ok(p) => p,
        Err(code) => return code.into_response(),
    };
    let done = async {
        let Some(b) = load(&state.db, &account.id).await?.filter(live) else {
            return anyhow::Ok(());
        };
        call(
            state,
            Method::PATCH,
            &format!("/subscriptions/{}", b.subscription_id),
            json!({ "cancel_at_next_billing_date": cancel }),
        )
        .await?;
        sqlx::query("UPDATE billing SET cancelling = ? WHERE account_id = ?")
            .bind(cancel)
            .bind(&account.id)
            .execute(&state.db)
            .await?;
        Ok(())
    }
    .await;
    match done {
        Ok(()) => Redirect::to(&format!("{page}#plan")).into_response(),
        Err(err) => {
            tracing::error!(%err, "changing a Dodo renewal failed");
            Redirect::to(&format!("{page}?billing=error#plan")).into_response()
        }
    }
}

pub async fn cancel(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    renewal(&state, &account, &domain, true).await
}

pub async fn resume(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    renewal(&state, &account, &domain, false).await
}

/// Opens Dodo's customer portal to update the card or see invoices.
pub async fn portal(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let page = match owner_page(&state, &account, &domain).await {
        Ok(p) => p,
        Err(code) => return code.into_response(),
    };
    let link = async {
        let Some(b) = load(&state.db, &account.id).await? else {
            return anyhow::Ok(None);
        };
        let back: String = url::form_urlencoded::byte_serialize(
            format!("{}{page}", state.config.base_url).as_bytes(),
        )
        .collect();
        let session = call(
            &state,
            Method::POST,
            &format!(
                "/customers/{}/customer-portal/session?return_url={back}",
                b.customer_id
            ),
            json!({}),
        )
        .await?;
        Ok(session["link"].as_str().map(str::to_string))
    }
    .await;
    match link {
        Ok(Some(url)) => Redirect::to(&url).into_response(),
        Ok(None) => Redirect::to(&format!("{page}#plan")).into_response(),
        Err(err) => {
            tracing::error!(%err, "opening the Dodo portal failed");
            Redirect::to(&format!("{page}?billing=error#plan")).into_response()
        }
    }
}

/// Moves a running subscription to `product`, with the seats the account has now.
pub async fn change_plan(state: &AppState, account_id: &str, product: &str) -> anyhow::Result<()> {
    let (Some(dodo), Some(b)) = (
        state.config.billing.as_ref(),
        load(&state.db, account_id).await?.filter(live),
    ) else {
        return Ok(());
    };
    let extra = if product == "agency" {
        extra_seats(&state.db, account_id).await?
    } else {
        0
    };
    let item = cart(dodo, product, extra, b.founding && b.product == product);
    call(
        state,
        Method::POST,
        &format!("/subscriptions/{}/change-plan", b.subscription_id),
        json!({
            "product_id": item["product_id"],
            "quantity": 1,
            "proration_billing_mode": "prorated_immediately",
            "addons": item.get("addons").cloned().unwrap_or(json!([])),
        }),
    )
    .await?;
    Ok(())
}

/// Bills Agency seats past the included ones after the team changes.
pub async fn sync_seats(state: AppState, account_id: String) {
    let synced = async {
        let Some(b) = load(&state.db, &account_id).await? else {
            return anyhow::Ok(());
        };
        if b.status != "active"
            || b.product != "agency"
            || b.extra_seats == extra_seats(&state.db, &account_id).await?
        {
            return Ok(());
        }
        change_plan(&state, &account_id, "agency").await
    }
    .await;
    if let Err(err) = synced {
        tracing::error!(%err, account = %account_id, "syncing Agency seats to Dodo failed");
    }
}

#[derive(Template)]
#[template(path = "email/billing.html")]
pub struct Email<'a> {
    pub heading: &'a str,
    pub lines: &'a [String],
    pub link: &'a str,
    pub button: &'a str,
}

/// Account, email, product, extra seats, first charge, founding and the first site's domain.
type DueRow = (String, String, String, i64, i64, bool, Option<String>);

/// Emails each trial 3 days before its first charge, with the date and amount.
pub async fn remind(state: &AppState, now: i64) -> anyhow::Result<()> {
    let due: Vec<DueRow> = sqlx::query_as(
        "SELECT b.account_id, a.email, b.product, b.extra_seats, b.trial_ends_at, b.founding,
                (SELECT domain FROM sites WHERE account_id = a.id ORDER BY created_at LIMIT 1)
         FROM billing b JOIN accounts a ON a.id = b.account_id
         WHERE b.status = 'active' AND NOT b.cancelling AND NOT b.reminded
           AND b.trial_ends_at > ? AND b.trial_ends_at <= ?",
    )
    .bind(now)
    .bind(now + REMIND_BEFORE.whole_seconds())
    .fetch_all(&state.db)
    .await?;
    for (account, email, product, extra, ends, founding, domain) in due {
        let (day, amount) = (
            date(ends),
            crate::dashboard::money(price(&product, extra, founding) as f64 / 100.0),
        );
        let name = crate::dashboard::plan_label(&product);
        let link = match domain {
            Some(d) => format!("{}{}#plan", state.config.base_url, app_path(&d, "account")),
            None => format!("{}/app", state.config.base_url),
        };
        let lines = if founding {
            [
                format!(
                    "Your paid founding month of Virling {name} ends on {day}. That day your card is charged {amount}, your founding price, then {amount} each month."
                ),
                "To stop it, open Account settings and click Cancel plan before then. Nothing is charged if you cancel.".to_string(),
            ]
        } else {
            [
                format!(
                    "Your 14-day trial of Virling {name} ends on {day}. That day your card is charged {amount} for the first month, then {amount} each month."
                ),
                "To stop it, open Account settings and click Cancel trial before then. Nothing is charged if you cancel.".to_string(),
            ]
        };
        let html = Email {
            heading: if founding {
                "Your first charge is in 3 days"
            } else {
                "Your trial ends in 3 days"
            },
            lines: &lines,
            link: &link,
            button: "Open billing",
        }
        .render()?;
        let text = format!("{}\n\n{}\n\n{link}", lines[0], lines[1]);
        state
            .mailer
            .send(
                &email,
                &if founding {
                    format!("Your first Virling charge is on {day}")
                } else {
                    format!("Your Virling trial ends on {day}")
                },
                text,
                html,
            )
            .await?;
        sqlx::query("UPDATE billing SET reminded = 1 WHERE account_id = ?")
            .bind(&account)
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

pub async fn run(state: AppState) {
    loop {
        if let Err(err) = remind(&state, now()).await {
            tracing::error!(%err, "sending trial reminders failed");
        }
        if let Err(err) = expire_founders(&state.db, now()).await {
            tracing::error!(%err, "ending founding months failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::extract::connect_info::MockConnectInfo;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::app::Config;
    use crate::mail::Mailer;

    const SECRET: &str = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
    const NOV_1: i64 = 1793527200;
    const DAY: i64 = 86400;

    type Calls = Arc<Mutex<Vec<(String, String, Value)>>>;

    /// A local stand-in for the Dodo API that records every request.
    async fn mock() -> (String, Calls) {
        let calls: Calls = Arc::default();
        let seen = calls.clone();
        let app = axum::Router::new().fallback(move |req: Request<Body>| {
            let seen = seen.clone();
            async move {
                let method = req.method().to_string();
                let uri = req.uri().to_string();
                let bearer = req.headers()["authorization"].to_str().unwrap().to_string();
                let bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                seen.lock().unwrap().push((method, uri.clone(), body));
                let reply = if uri.starts_with("/api/founders") {
                    assert_eq!(bearer, "Bearer founders_test");
                    match uri.as_str() {
                        "/api/founders?email=owner%40agency.com" => {
                            json!({ "founders": [{ "plan": "agency", "number": 7 }] })
                        }
                        _ => json!({ "founders": [] }),
                    }
                } else if uri == "/checkouts" {
                    assert_eq!(bearer, "Bearer test_key");
                    json!({ "session_id": "cks_1", "checkout_url": "https://test.checkout.dodopayments.com/session/cks_1" })
                } else if uri.contains("customer-portal") {
                    json!({ "link": "https://customer.dodopayments.com/portal/1" })
                } else {
                    json!({})
                };
                axum::Json(reply)
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, calls)
    }

    pub(crate) struct T {
        _dir: tempfile::TempDir,
        pub(crate) state: AppState,
        pub(crate) account: String,
        cookie: String,
        calls: Calls,
    }

    pub(crate) async fn setup() -> T {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::open_sqlite(dir.path()).await.unwrap();
        let account = sites::find_or_create_account(&db, "owner@agency.com")
            .await
            .unwrap();
        sites::create_site(&db, &account, "agency.com", "UTC")
            .await
            .unwrap();
        let token = crate::auth::random_token();
        sqlx::query("INSERT INTO sessions (token_hash, account_id, created_at, expires_at) VALUES (?, ?, 0, ?)")
            .bind(crate::auth::hash_token(&token))
            .bind(&account)
            .bind(now() + 3600)
            .execute(&db)
            .await
            .unwrap();
        let (url, calls) = mock().await;
        let founders_url = format!("{url}/api/founders");
        let config = Config {
            marketing: false,
            app: true,
            trust_proxy: false,
            events_per_second: 200,
            base_url: "https://app.virling.com".into(),
            waitlist_from: String::new(),
            hosted: true,
            ai_keys: Default::default(),
            ai_budget_usd: None,
            billing: Some(Dodo {
                api_url: url,
                api_key: "test_key".into(),
                webhook_secret: SECRET.into(),
                personal: "pdt_personal".into(),
                agency: "pdt_agency".into(),
                seat: "adn_seat".into(),
                personal_founding: "pdt_personal_founding".into(),
                agency_founding: "pdt_agency_founding".into(),
                founders_url,
                founders_token: "founders_test".into(),
            }),
        };
        let store =
            Arc::new(crate::events::DuckStore::open(&dir.path().join("events.duckdb")).unwrap());
        let state = AppState::new(
            config,
            db,
            store,
            Mailer::Capture(Default::default()),
            crate::crypto::Vault::load(dir.path(), None).unwrap(),
        );
        T {
            _dir: dir,
            state,
            account,
            cookie: format!("virling_session={token}"),
            calls,
        }
    }

    /// The documented subscription payload as `kind` at `at`, edited by `edit`.
    fn hook(t: &T, kind: &str, at: i64, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut v: Value = serde_json::from_str(include_str!(
            "providers/fixtures/dodo_billing_subscription.json"
        ))
        .unwrap();
        v["type"] = json!(kind);
        v["timestamp"] = json!(rfc(at));
        v["data"]["metadata"]["account_id"] = json!(t.account);
        edit(&mut v["data"]);
        serde_json::to_vec(&v).unwrap()
    }

    fn rfc(ts: i64) -> String {
        OffsetDateTime::from_unix_timestamp(ts)
            .unwrap()
            .format(&Rfc3339)
            .unwrap()
    }

    async fn send(t: &T, id: &str, body: Vec<u8>) {
        apply(&t.state, id, &body).await.unwrap();
    }

    async fn plan_of(t: &T) -> (String, String) {
        sqlx::query_as("SELECT plan, account_type FROM accounts WHERE id = ?")
            .bind(&t.account)
            .fetch_one(&t.state.db)
            .await
            .unwrap()
    }

    async fn post(t: &T, path: &str, form: &str) -> axum::response::Response {
        crate::app::router(t.state.clone())
            .layer(MockConnectInfo(SocketAddr::from(([203, 0, 113, 7], 5555))))
            .oneshot(
                Request::post(path)
                    .header("cookie", &t.cookie)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(form.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    pub(crate) async fn page(t: &T, path: &str) -> String {
        let res = crate::app::router(t.state.clone())
            .layer(MockConnectInfo(SocketAddr::from(([203, 0, 113, 7], 5555))))
            .oneshot(
                Request::get(path)
                    .header("cookie", &t.cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    pub(crate) fn mails(t: &T) -> Vec<(String, String, String)> {
        match &*t.state.mailer {
            Mailer::Capture(sent) => sent.lock().unwrap().clone(),
            _ => unreachable!(),
        }
    }

    fn encode(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in bytes.chunks(3) {
            let n = c
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..4 {
                out.push(if i <= c.len() {
                    A[(n >> (18 - 6 * i) & 63) as usize] as char
                } else {
                    '='
                });
            }
        }
        out
    }

    #[test]
    fn verifies_standard_webhooks_signatures() {
        let body = br#"{"test": 2432232314}"#;
        let (id, ts) = ("msg_p5jXN8AQM9LWM0D4loKWxJek", "1614265330");
        let sig = "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=";
        assert!(verify(SECRET, id, ts, body, sig, 1614265330));
        assert!(verify(
            SECRET,
            id,
            ts,
            body,
            &format!("v1,AAAA {sig}"),
            1614265400
        ));
        assert!(!verify(SECRET, id, ts, br#"{"test": 1}"#, sig, 1614265330));
        assert!(!verify(SECRET, "msg_other", ts, body, sig, 1614265330));
        assert!(
            !verify(SECRET, id, ts, body, sig, 1614265330 + 301),
            "stale"
        );
        assert!(!verify(SECRET, id, "nope", body, sig, 1614265330));
        assert!(!verify(SECRET, id, ts, body, "", 1614265330));
        assert_eq!(
            encode(&base64("g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=").unwrap()),
            "g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE="
        );
    }

    #[test]
    fn plans_follow_status_and_trial() {
        let ends = Some(NOV_1 + TRIAL_DAYS * DAY);
        assert_eq!(plan("active", ends, NOV_1), "trialing");
        assert_eq!(plan("active", ends, NOV_1 + TRIAL_DAYS * DAY), "paid");
        assert_eq!(plan("active", None, NOV_1), "paid");
        for s in ["on_hold", "past_due", "paused"] {
            assert_eq!(plan(s, ends, NOV_1), "past_due");
        }
        for s in ["cancelled", "expired", "failed"] {
            assert_eq!(plan(s, ends, NOV_1), "cancelled");
        }
        assert_eq!(days_left(NOV_1 + 9 * DAY - 60, NOV_1), 9);
        assert_eq!(days_left(NOV_1 - 1, NOV_1), 0);
        assert_eq!(price("agency", 2, false), 9700);
        assert_eq!(price("personal", 0, false), 1900);
        assert_eq!(price("agency", 1, true), 3800);
        assert_eq!(price("personal", 0, true), 1140);
    }

    #[tokio::test]
    async fn webhook_endpoint_checks_the_signature() {
        let t = setup().await;
        let body = hook(&t, "subscription.active", NOV_1, |_| {});
        let ts = now().to_string();
        let mut msg = format!("msg_1.{ts}.").into_bytes();
        msg.extend_from_slice(&body);
        let key = base64(&SECRET[6..]).unwrap();
        let good = format!("v1,{}", encode(&crate::crypto::hmac_sha256(&key, &msg)));
        for (sig, want) in [
            ("v1,AAAA", StatusCode::UNAUTHORIZED),
            (good.as_str(), StatusCode::OK),
        ] {
            let res = crate::app::router(t.state.clone())
                .oneshot(
                    Request::post("/api/webhooks/dodo")
                        .header("webhook-id", "msg_1")
                        .header("webhook-timestamp", &ts)
                        .header("webhook-signature", sig)
                        .body(Body::from(body.clone()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), want);
        }
        assert_eq!(plan_of(&t).await.0, "trialing");
    }

    #[tokio::test]
    async fn trial_starts_with_a_card_and_converts_on_day_15() {
        let t = setup().await;
        let res = post(&t, "/app/agency.com/account/billing", "plan=agency").await;
        assert_eq!(
            res.headers()["location"],
            "https://test.checkout.dodopayments.com/session/cks_1"
        );
        let (method, uri, body) = t.calls.lock().unwrap()[0].clone();
        assert_eq!((method.as_str(), uri.as_str()), ("POST", "/checkouts"));
        assert_eq!(body["product_cart"][0]["product_id"], "pdt_agency");
        assert!(body["product_cart"][0].get("addons").is_none());
        assert_eq!(body["subscription_data"]["trial_period_days"], 14);
        assert_eq!(body["customer"]["email"], "owner@agency.com");
        assert_eq!(body["metadata"]["account_id"], json!(t.account));
        assert_eq!(
            body["return_url"],
            "https://app.virling.com/app/agency.com/account?billing=started#plan"
        );

        send(&t, "msg_1", hook(&t, "subscription.active", NOV_1, |_| {})).await;
        assert_eq!(plan_of(&t).await, ("trialing".into(), "agency".into()));
        let b = load(&t.state.db, &t.account).await.unwrap().unwrap();
        assert_eq!(b.trial_ends_at, Some(NOV_1 + 14 * DAY));
        let v = view(&t.state.db, &t.account, NOV_1 + 5 * DAY)
            .await
            .unwrap();
        assert_eq!(v.state, "trial");
        assert_eq!(v.headline, "Agency trial, 9 days left");
        assert_eq!(
            v.detail,
            "First charge $79 on November 15, 2026. Cancel anytime before then and nothing is charged."
        );

        let renewed = hook(&t, "subscription.renewed", NOV_1 + 14 * DAY, |d| {
            d["next_billing_date"] = json!("2026-12-15T10:00:00Z");
            d["previous_billing_date"] = json!("2026-11-15T10:00:00Z");
        });
        send(&t, "msg_2", renewed).await;
        assert_eq!(plan_of(&t).await.0, "paid");
        let v = view(&t.state.db, &t.account, NOV_1 + 15 * DAY)
            .await
            .unwrap();
        assert_eq!(v.headline, "Agency, $79 a month");
        assert_eq!(v.detail, "Renews on December 15, 2026.");

        let again = post(&t, "/app/agency.com/account/billing", "plan=agency").await;
        assert_eq!(again.headers()["location"], "/app/agency.com/account#plan");
        assert_eq!(
            t.calls.lock().unwrap().len(),
            1,
            "no second checkout while subscribed"
        );
    }

    #[tokio::test]
    async fn cancel_before_day_15_charges_nothing() {
        let t = setup().await;
        send(&t, "msg_1", hook(&t, "subscription.active", NOV_1, |_| {})).await;
        let res = post(&t, "/app/agency.com/account/billing/cancel", "").await;
        assert_eq!(res.headers()["location"], "/app/agency.com/account#plan");
        let (method, uri, body) = t.calls.lock().unwrap()[0].clone();
        assert_eq!(
            (method.as_str(), uri.as_str()),
            ("PATCH", "/subscriptions/sub_fixture")
        );
        assert_eq!(body, json!({ "cancel_at_next_billing_date": true }));
        let v = view(&t.state.db, &t.account, NOV_1 + DAY).await.unwrap();
        assert!(v.cancelling);
        assert_eq!(
            v.detail,
            "Ends on November 15, 2026. Nothing will be charged."
        );
        remind(&t.state, NOV_1 + 12 * DAY).await.unwrap();
        assert!(mails(&t).is_empty(), "no reminder after cancelling");

        let updated = hook(&t, "subscription.updated", NOV_1 + DAY, |d| {
            d["cancel_at_next_billing_date"] = json!(true);
        });
        send(&t, "msg_2", updated).await;
        assert_eq!(plan_of(&t).await.0, "trialing", "the trial runs to its end");
        let ended = hook(&t, "subscription.cancelled", NOV_1 + 14 * DAY, |d| {
            d["status"] = json!("cancelled");
            d["cancel_at_next_billing_date"] = json!(true);
        });
        send(&t, "msg_3", ended).await;
        assert_eq!(plan_of(&t).await.0, "cancelled");

        let v = view(&t.state.db, &t.account, NOV_1 + 15 * DAY)
            .await
            .unwrap();
        assert_eq!((v.state, v.button), ("none", "Subscribe"));
        post(&t, "/app/agency.com/account/billing", "plan=personal").await;
        let (_, uri, body) = t.calls.lock().unwrap()[1].clone();
        assert_eq!(uri, "/checkouts");
        assert_eq!(
            body["subscription_data"]["trial_period_days"], 0,
            "one trial per account"
        );
        assert_eq!(body["product_cart"][0]["product_id"], "pdt_personal");
    }

    #[tokio::test]
    async fn reminds_three_days_before_the_first_charge() {
        let t = setup().await;
        send(&t, "msg_1", hook(&t, "subscription.active", NOV_1, |_| {})).await;
        remind(&t.state, NOV_1 + 10 * DAY).await.unwrap();
        assert!(mails(&t).is_empty());
        remind(&t.state, NOV_1 + 11 * DAY).await.unwrap();
        remind(&t.state, NOV_1 + 12 * DAY).await.unwrap();
        let sent = mails(&t);
        assert_eq!(sent.len(), 1);
        let (to, subject, text) = &sent[0];
        assert_eq!(to, "owner@agency.com");
        assert_eq!(subject, "Your Virling trial ends on November 15, 2026");
        assert!(text.contains("charged $79 for the first month"), "{text}");
        assert!(text.contains("https://app.virling.com/app/agency.com/account#plan"));
    }

    #[tokio::test]
    async fn failed_payment_is_past_due_until_paid() {
        let t = setup().await;
        send(&t, "msg_1", hook(&t, "subscription.active", NOV_1, |_| {})).await;
        let held = hook(&t, "subscription.on_hold", NOV_1 + 14 * DAY, |d| {
            d["status"] = json!("on_hold");
        });
        send(&t, "msg_2", held.clone()).await;
        assert_eq!(plan_of(&t).await.0, "past_due");
        let v = view(&t.state.db, &t.account, NOV_1 + 14 * DAY)
            .await
            .unwrap();
        assert_eq!(v.headline, "Agency, payment failed");
        assert!(
            page(&t, "/app/agency.com/account")
                .await
                .contains("Update card")
        );

        let stale = hook(&t, "subscription.active", NOV_1 + DAY, |_| {});
        send(&t, "msg_3", stale).await;
        assert_eq!(plan_of(&t).await.0, "past_due", "older events are skipped");
        let paid = hook(&t, "subscription.active", NOV_1 + 15 * DAY, |d| {
            d["next_billing_date"] = json!("2026-12-16T10:00:00Z");
        });
        send(&t, "msg_2", paid.clone()).await;
        assert_eq!(
            plan_of(&t).await.0,
            "past_due",
            "a repeated webhook id is applied once"
        );
        send(&t, "msg_4", paid).await;
        assert_eq!(plan_of(&t).await.0, "paid");
    }

    #[tokio::test]
    async fn seats_and_plan_changes_reach_dodo() {
        let t = setup().await;
        send(&t, "msg_1", hook(&t, "subscription.active", NOV_1, |_| {})).await;
        sqlx::query("UPDATE accounts SET seats = 5 WHERE id = ?")
            .bind(&t.account)
            .execute(&t.state.db)
            .await
            .unwrap();
        sync_seats(t.state.clone(), t.account.clone()).await;
        let (method, uri, body) = t.calls.lock().unwrap()[0].clone();
        assert_eq!(
            (method.as_str(), uri.as_str()),
            ("POST", "/subscriptions/sub_fixture/change-plan")
        );
        assert_eq!(body["product_id"], "pdt_agency");
        assert_eq!(
            body["addons"],
            json!([{ "addon_id": "adn_seat", "quantity": 2 }])
        );
        assert_eq!(body["proration_billing_mode"], "prorated_immediately");

        let changed = hook(&t, "subscription.plan_changed", NOV_1 + DAY, |d| {
            d["addons"] = json!([{ "addon_id": "adn_seat", "quantity": 2 }]);
        });
        send(&t, "msg_2", changed).await;
        let b = load(&t.state.db, &t.account).await.unwrap().unwrap();
        assert_eq!(b.extra_seats, 2);
        sync_seats(t.state.clone(), t.account.clone()).await;
        assert_eq!(t.calls.lock().unwrap().len(), 1, "seats already billed");

        sqlx::query("UPDATE accounts SET seats = 1 WHERE id = ?")
            .bind(&t.account)
            .execute(&t.state.db)
            .await
            .unwrap();
        post(&t, "/app/agency.com/account/plan", "account_type=personal").await;
        let (_, uri, body) = t.calls.lock().unwrap()[1].clone();
        assert_eq!(uri, "/subscriptions/sub_fixture/change-plan");
        assert_eq!(body["product_id"], "pdt_personal");
        assert_eq!(body["addons"], json!([]));
        let moved = hook(&t, "subscription.plan_changed", NOV_1 + 2 * DAY, |d| {
            d["product_id"] = json!("pdt_personal");
        });
        send(&t, "msg_3", moved).await;
        assert_eq!(plan_of(&t).await, ("trialing".into(), "personal".into()));
    }

    #[tokio::test]
    async fn trial_banner_and_account_section() {
        let t = setup().await;
        let fresh = page(&t, "/app/agency.com/account").await;
        assert!(fresh.contains("Start 14-day trial"));
        assert!(fresh.contains("$0 today. Cancel anytime before day 15."));
        sqlx::query(
            "INSERT INTO billing (account_id, subscription_id, customer_id, status, product, trial_ends_at, event_at)
             VALUES (?, 'sub_1', 'cus_1', 'active', 'personal', ?, 0)",
        )
        .bind(&t.account)
        .bind(now() + 9 * DAY - 60)
        .execute(&t.state.db)
        .await
        .unwrap();
        let dash = page(&t, "/app/agency.com/settings").await;
        assert!(
            dash.contains("Trial: 9 days left. Connect payments to see which visits pay."),
            "{dash}"
        );
        let account = page(&t, "/app/agency.com/account").await;
        assert!(account.contains("Personal trial, 9 days left"));
        assert!(account.contains("Cancel trial"));
        post(&t, "/app/agency.com/account/billing/portal", "").await;
        let (_, uri, _) = t.calls.lock().unwrap()[0].clone();
        assert!(uri.starts_with(
            "/customers/cus_1/customer-portal/session?return_url=https%3A%2F%2Fapp.virling.com"
        ));
    }

    async fn founder_until(t: &T) -> Option<i64> {
        sqlx::query_scalar("SELECT founder_until FROM accounts WHERE id = ?")
            .bind(&t.account)
            .fetch_one(&t.state.db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn founders_link_by_email_and_keep_their_price() {
        let t = setup().await;
        link_founder(t.state.clone(), t.account.clone()).await;
        assert_eq!(plan_of(&t).await, ("founding".into(), "agency".into()));
        let until = founder_until(&t).await.unwrap();
        assert!(
            (until - now() - 30 * DAY).abs() < 60,
            "the prepaid month starts at sign-up"
        );
        link_founder(t.state.clone(), t.account.clone()).await;
        assert_eq!(t.calls.lock().unwrap().len(), 1, "looked up once");

        let v = view(&t.state.db, &t.account, now()).await.unwrap();
        assert_eq!((v.state, v.button), ("none", "Add card"));
        assert_eq!(v.headline, "Founding Agency spot");
        assert!(
            v.detail.contains("Add a card to keep $29 a month for life"),
            "{}",
            v.detail
        );

        post(&t, "/app/agency.com/account/billing", "plan=agency").await;
        let (_, uri, body) = t.calls.lock().unwrap()[1].clone();
        assert_eq!(uri, "/checkouts");
        assert_eq!(body["product_cart"][0]["product_id"], "pdt_agency_founding");
        assert_eq!(
            body["subscription_data"]["trial_period_days"], 30,
            "no charge for the paid month"
        );

        let start = until - 30 * DAY;
        let active = hook(&t, "subscription.active", start, |d| {
            d["product_id"] = json!("pdt_agency_founding");
            d["trial_period_days"] = json!(30);
            d["next_billing_date"] = json!(rfc(until));
        });
        send(&t, "msg_1", active).await;
        assert_eq!(plan_of(&t).await.0, "founding");
        let v = view(&t.state.db, &t.account, start + DAY).await.unwrap();
        assert_eq!(v.headline, "Founding Agency, first month paid");
        assert!(v.detail.starts_with("First charge $29 on"), "{}", v.detail);

        remind(&t.state, until - 3 * DAY).await.unwrap();
        let (_, subject, text) = mails(&t)[0].clone();
        assert!(
            subject.starts_with("Your first Virling charge is on"),
            "{subject}"
        );
        assert!(text.contains("charged $29, your founding price"), "{text}");

        let renewed = hook(&t, "subscription.renewed", until, |d| {
            d["product_id"] = json!("pdt_agency_founding");
            d["next_billing_date"] = json!(rfc(until + 30 * DAY));
        });
        send(&t, "msg_2", renewed).await;
        assert_eq!(plan_of(&t).await.0, "founding");
        assert!(
            crate::answers::paid(&t.state, &site(&t).await)
                .await
                .unwrap()
        );
    }

    async fn site(t: &T) -> String {
        sqlx::query_scalar("SELECT id FROM sites WHERE account_id = ?")
            .bind(&t.account)
            .fetch_one(&t.state.db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn other_emails_are_not_founders() {
        let t = setup().await;
        let other = sites::find_or_create_account(&t.state.db, "someone@else.com")
            .await
            .unwrap();
        link_founder(t.state.clone(), other.clone()).await;
        let plan: String = sqlx::query_scalar("SELECT plan FROM accounts WHERE id = ?")
            .bind(&other)
            .fetch_one(&t.state.db)
            .await
            .unwrap();
        assert_eq!(plan, "self_hosted");
        send(&t, "msg_1", hook(&t, "subscription.active", NOV_1, |_| {})).await;
        link_founder(t.state.clone(), t.account.clone()).await;
        assert_eq!(
            plan_of(&t).await.0,
            "trialing",
            "a subscribed account is not looked up"
        );
        assert_eq!(t.calls.lock().unwrap().len(), 1);
        assert!(
            !crate::answers::paid(&t.state, &site(&t).await)
                .await
                .unwrap(),
            "trials skip AI answers"
        );
    }

    #[tokio::test]
    async fn founding_month_ends_without_a_card() {
        let t = setup().await;
        link_founder(t.state.clone(), t.account.clone()).await;
        let until = founder_until(&t).await.unwrap();
        expire_founders(&t.state.db, until - 1).await.unwrap();
        assert_eq!(plan_of(&t).await.0, "founding");
        let cancelled = hook(&t, "subscription.cancelled", until - 5 * DAY, |d| {
            d["product_id"] = json!("pdt_agency_founding");
            d["status"] = json!("cancelled");
        });
        send(&t, "msg_1", cancelled).await;
        assert_eq!(plan_of(&t).await.0, "founding", "the paid month still runs");
        expire_founders(&t.state.db, until).await.unwrap();
        assert_eq!(plan_of(&t).await.0, "cancelled");
    }
}
