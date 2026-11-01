//! Signed webhooks for attributed payments, goal completions and traffic spikes; a Slack URL gets plain messages instead.

use axum::Form;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::app::AppState;
use crate::auth::{Account, random_token};
use crate::crypto::hmac_sha256;
use crate::dashboard::schedule::valid_slack;
use crate::dashboard::{load, native};
use crate::sites::app_path;
use crate::stats::{PaymentFilter, PaymentRow, Period};
use crate::telegram::{SPIKE_MIN, SPIKE_TIMES};

const INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
/// Most payments and goals one site sends per round.
const PAYMENTS: u32 = 20;
const GOALS: u32 = 100;
/// How long a payment without a credited visit waits for attribution before it is skipped.
const SETTLE_SECS: i64 = 600;

/// The `Virling-Signature` header: the time and an HMAC-SHA256 of "time.body".
pub fn signature(secret: &str, ts: i64, body: &str) -> String {
    let mac = hmac_sha256(secret.as_bytes(), format!("{ts}.{body}").as_bytes());
    let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
    format!("t={ts},v1={hex}")
}

/// Sends one event: signed JSON, or `text` when the URL is a Slack incoming webhook.
async fn deliver(
    state: &AppState,
    url: &str,
    secret: &str,
    domain: &str,
    kind: &str,
    data: Value,
    text: &str,
) -> anyhow::Result<()> {
    let now = OffsetDateTime::now_utc();
    let req = if valid_slack(url) {
        state.favicons.post(url).json(&json!({ "text": text }))
    } else {
        let body = json!({
            "type": kind,
            "site": domain,
            "created": now.format(&Rfc3339)?,
            "data": data,
        })
        .to_string();
        state
            .favicons
            .post(url)
            .header("content-type", "application/json")
            .header("virling-event", kind)
            .header(
                "virling-signature",
                signature(secret, now.unix_timestamp(), &body),
            )
            .body(body)
    };
    let res = req.send().await.map_err(|e| e.without_url())?;
    anyhow::ensure!(
        res.status().is_success(),
        "The webhook answered HTTP {}",
        res.status().as_u16()
    );
    Ok(())
}

fn payment_data(p: &PaymentRow) -> Value {
    let t = p.touch.as_ref();
    json!({
        "provider": p.provider,
        "payment_id": p.provider_payment_id,
        "paid_at": p.ts.format(&Rfc3339).ok(),
        "amount": p.amount,
        "currency": p.currency,
        "amount_usd": p.amount_usd,
        "renewal": p.is_renewal,
        "visitor_id": p.visitor,
        "customer_id": p.customer_id,
        "source": t.map(|t| &t.source),
        "channel": t.map(|t| &t.channel),
        "referrer": t.and_then(|t| t.referrer_host.as_ref()),
        "campaign": t.and_then(|t| t.campaign.as_ref()),
        "landing_page": t.and_then(|t| t.landing_page.as_ref()),
    })
}

/// Sends new events every minute, forever.
pub async fn run(state: AppState) {
    loop {
        if let Err(err) = tick(&state).await {
            tracing::error!(%err, "webhook round failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

type Row = (
    String,
    String,
    String,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    i64,
    i64,
    i64,
);

/// Records how far the site got and the error that stopped it, if any.
async fn mark(
    state: &AppState,
    site: &str,
    column: &'static str,
    until: i64,
    error: Option<String>,
) -> sqlx::Result<()> {
    sqlx::query(match column {
        "sent_until" => {
            "UPDATE webhooks SET last_error = ?, sent_until = max(sent_until, ?) WHERE site_id = ?"
        }
        "goals_until" => {
            "UPDATE webhooks SET last_error = ?, goals_until = max(goals_until, ?) WHERE site_id = ?"
        }
        _ => "UPDATE webhooks SET last_error = ?, spike_day = max(spike_day, ?) WHERE site_id = ?",
    })
    .bind(error)
    .bind(until)
    .bind(site)
    .execute(&state.db)
    .await
    .map(|_| ())
}

pub async fn tick(state: &AppState) -> anyhow::Result<()> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT w.site_id, s.domain, s.timezone, w.url_ciphertext, w.url_nonce, w.secret_ciphertext,
                w.secret_nonce, w.sent_until, w.goals_until, w.spike_day
         FROM webhooks w JOIN sites s ON s.id = w.site_id WHERE s.sample_data = 0",
    )
    .fetch_all(&state.db)
    .await?;
    for (site, domain, tz, uc, un, sc, sn, sent_until, goals_until, spike_day) in rows {
        let (Ok(url), Ok(secret)) = (state.vault.decrypt(&uc, &un), state.vault.decrypt(&sc, &sn))
        else {
            continue;
        };
        let store = state.events.clone();
        let (id, zone) = (site.clone(), tz.clone());
        let (page, goals, today, visitors, week) = tokio::task::spawn_blocking(move || {
            let today = OffsetDateTime::now_utc()
                .to_offset(store.utc_offset(&zone)?)
                .date();
            let day = |from, to| Period {
                from,
                to,
                tz: zone.clone(),
            };
            let visitors = |p: Period| store.overview(&id, &p).map(|o| o.current.visitors);
            anyhow::Ok((
                store.payment_list(
                    &id,
                    &day(today - Duration::days(1), today),
                    PaymentFilter::All,
                    PAYMENTS,
                    0,
                )?,
                store.goals_since(&id, goals_until, GOALS)?,
                today,
                visitors(day(today, today))?,
                visitors(day(today - Duration::days(7), today - Duration::days(1)))?,
            ))
        })
        .await??;

        let now = OffsetDateTime::now_utc().unix_timestamp();
        let mut fresh: Vec<_> = page
            .rows
            .into_iter()
            .filter(|p| {
                p.ts.unix_timestamp() > sent_until
                    && p.status == "paid"
                    && p.provider != crate::events::SAMPLE
            })
            .collect();
        fresh.reverse();
        let mut failed = false;
        for p in fresh {
            let ts = p.ts.unix_timestamp();
            let Some(t) = &p.touch else {
                if now - ts < SETTLE_SECS {
                    break;
                }
                mark(state, &site, "sent_until", ts, None).await?;
                continue;
            };
            let text = format!(
                "New payment on {domain}: {} from {}",
                native(p.amount, &p.currency),
                t.source
            );
            let sent = deliver(
                state,
                &url,
                &secret,
                &domain,
                "payment.attributed",
                payment_data(&p),
                &text,
            )
            .await;
            let error = sent.err().map(|e| e.to_string());
            failed = error.is_some();
            mark(
                state,
                &site,
                "sent_until",
                if failed { 0 } else { ts },
                error,
            )
            .await?;
            if failed {
                break;
            }
        }
        if failed {
            continue;
        }

        for (us, goal, path, visitor) in goals {
            let at = OffsetDateTime::from_unix_timestamp_nanos(i128::from(us) * 1000)?;
            let data = json!({
                "goal": goal,
                "path": path,
                "visitor_id": visitor,
                "completed_at": at.format(&Rfc3339)?,
            });
            let text = format!("Goal {goal} completed on {domain}{path}");
            let sent = deliver(state, &url, &secret, &domain, "goal.completed", data, &text).await;
            let error = sent.err().map(|e| e.to_string());
            failed = error.is_some();
            mark(
                state,
                &site,
                "goals_until",
                if failed { 0 } else { us },
                error,
            )
            .await?;
            if failed {
                break;
            }
        }
        if failed {
            continue;
        }

        let day = i64::from(today.to_julian_day());
        let usual = week as f64 / 7.0;
        if spike_day < day && visitors >= SPIKE_MIN && visitors as f64 >= SPIKE_TIMES * usual {
            let text = format!(
                "Traffic is well above usual on {domain}: {visitors} visitors today, against about {usual:.0} a day this past week."
            );
            let data = json!({ "visitors_today": visitors, "usual_per_day": (usual * 10.0).round() / 10.0 });
            let sent = deliver(
                state,
                &url,
                &secret,
                &domain,
                "alert.traffic_spike",
                data,
                &text,
            )
            .await;
            let error = sent.err().map(|e| e.to_string());
            let until = if error.is_some() { 0 } else { day };
            mark(state, &site, "spike_day", until, error).await?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct Input {
    url: String,
}

/// Saves the URL with a new signing secret after a test event goes through.
pub async fn save(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<Input>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if !account.owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    let settings = app_path(&ctx.site.domain, "settings");
    let url = input.url.trim();
    let host = url::Url::parse(url)
        .ok()
        .filter(|u| u.scheme() == "https" && url.len() <= 500)
        .and_then(|u| u.host_str().map(String::from));
    let Some(host) = host else {
        return Redirect::to(&format!("{settings}?error=webhook#webhooks")).into_response();
    };
    let secret = format!("whsec_{}", &random_token()[..40]);
    let hello = format!(
        "Virling will post new payments, goals and traffic spikes on {} here.",
        ctx.site.domain
    );
    let test = deliver(
        &state,
        url,
        &secret,
        &ctx.site.domain,
        "ping",
        json!({}),
        &hello,
    )
    .await;
    if test.is_err() {
        return Redirect::to(&format!("{settings}?error=webhook#webhooks")).into_response();
    }
    let (Ok((uc, un)), Ok((sc, sn))) = (state.vault.encrypt(url), state.vault.encrypt(&secret))
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let now = OffsetDateTime::now_utc();
    let saved = sqlx::query(
        "INSERT INTO webhooks (site_id, url_ciphertext, url_nonce, secret_ciphertext, secret_nonce, host, sent_until, goals_until)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (site_id) DO UPDATE SET url_ciphertext = excluded.url_ciphertext, url_nonce = excluded.url_nonce,
           secret_ciphertext = excluded.secret_ciphertext, secret_nonce = excluded.secret_nonce,
           host = excluded.host, last_error = NULL",
    )
    .bind(&ctx.site.id)
    .bind(uc)
    .bind(un)
    .bind(sc)
    .bind(sn)
    .bind(host)
    .bind(now.unix_timestamp())
    .bind((now.unix_timestamp_nanos() / 1000) as i64)
    .execute(&state.db)
    .await;
    if let Err(err) = saved {
        tracing::error!(%err, "saving the webhook failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("{settings}?saved=webhook#webhooks")).into_response()
}

/// Turns the site's webhook off.
pub async fn remove(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
) -> Response {
    let ctx = match load(&state, &account, &domain, "settings").await {
        Ok(ctx) => ctx,
        Err(code) => return code.into_response(),
    };
    if !account.owner {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(err) = sqlx::query("DELETE FROM webhooks WHERE site_id = ?")
        .bind(&ctx.site.id)
        .execute(&state.db)
        .await
    {
        tracing::error!(%err, "removing the webhook failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!(
        "{}#webhooks",
        app_path(&ctx.site.domain, "settings")
    ))
    .into_response()
}

/// The webhook's host, signing secret, whether it posts to Slack, and its last error.
pub async fn view(state: &AppState, site_id: &str) -> Option<(String, String, bool, String)> {
    type Saved = (String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Option<String>);
    let row: Option<Saved> =
        sqlx::query_as(
            "SELECT host, url_ciphertext, url_nonce, secret_ciphertext, secret_nonce, last_error FROM webhooks WHERE site_id = ?",
        )
        .bind(site_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let (host, uc, un, sc, sn, error) = row?;
    let url = state.vault.decrypt(&uc, &un).ok()?;
    let secret = state.vault.decrypt(&sc, &sn).ok()?;
    Some((host, secret, valid_slack(&url), error.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_time_and_body() {
        assert_eq!(
            signature("whsec_test", 1_760_000_000, r#"{"type":"ping"}"#),
            "t=1760000000,v1=5aa19891e0607e571335e9a7a00b5dfa2d6fb7d302b46f94ce8ea85f8d26d7b3"
        );
    }
}
