//! Telegram messages for new payments, traffic spikes and a daily digest, sent with the site owner's own bot.

use axum::Form;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use serde_json::json;
use time::{Duration, OffsetDateTime};

use crate::app::AppState;
use crate::auth::Account;
use crate::dashboard::{load, money, native};
use crate::sites::app_path;
use crate::stats::{PaymentFilter, Period};

const INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
/// Most payments one site is told about per round.
const BATCH: u32 = 20;
/// Local hour from which yesterday's digest goes out.
const DIGEST_HOUR: u8 = 9;
/// Visitors today that count as a spike, and how many times the daily average of the week before.
const SPIKE_MIN: u64 = 20;
const SPIKE_TIMES: f64 = 3.0;

async fn send(state: &AppState, token: &str, chat: &str, text: &str) -> anyhow::Result<()> {
    let res = state
        .http
        .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .json(&json!({ "chat_id": chat, "text": text }))
        .send()
        .await
        .map_err(|e| e.without_url())?;
    anyhow::ensure!(
        res.status().is_success(),
        "Telegram refused the message (HTTP {})",
        res.status().as_u16()
    );
    Ok(())
}

/// Messages about new payments, once a minute, forever.
pub async fn run(state: AppState) {
    loop {
        if let Err(err) = tick(&state).await {
            tracing::error!(%err, "telegram alert round failed");
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
    String,
    i64,
    i64,
    i64,
);

async fn tick(state: &AppState) -> anyhow::Result<()> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT t.site_id, s.domain, s.timezone, t.token_ciphertext, t.token_nonce, t.chat_id, t.sent_until,
                t.digest_day, t.spike_day
         FROM telegram_alerts t JOIN sites s ON s.id = t.site_id WHERE s.sample_data = 0",
    )
    .fetch_all(&state.db)
    .await?;
    for (site, domain, tz, ciphertext, nonce, chat, sent_until, digest_day, spike_day) in rows {
        let Ok(token) = state.vault.decrypt(&ciphertext, &nonce) else {
            continue;
        };
        let store = state.events.clone();
        let (id, zone) = (site.clone(), tz.clone());
        let page = tokio::task::spawn_blocking(move || {
            let today = OffsetDateTime::now_utc()
                .to_offset(store.utc_offset(&zone)?)
                .date();
            let period = Period {
                from: today - Duration::days(1),
                to: today,
                tz: zone,
            };
            store.payment_list(&id, &period, PaymentFilter::All, BATCH, 0)
        })
        .await??;
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
        for p in fresh {
            let source = p.touch.as_ref().map_or("an unknown source", |t| &t.source);
            let text = format!(
                "New payment on {domain}: {} from {source}",
                native(p.amount, &p.currency)
            );
            let result = send(state, &token, &chat, &text).await;
            let error = result.as_ref().err().map(|e| e.to_string());
            let sent = if result.is_ok() {
                p.ts.unix_timestamp()
            } else {
                sent_until
            };
            sqlx::query(
                "UPDATE telegram_alerts SET last_error = ?, sent_until = max(sent_until, ?) WHERE site_id = ?",
            )
            .bind(&error)
            .bind(sent)
            .bind(&site)
            .execute(&state.db)
            .await?;
            if error.is_some() {
                break;
            }
        }
        summaries(
            state, &site, &domain, &tz, &token, &chat, digest_day, spike_day,
        )
        .await?;
    }
    Ok(())
}

/// The daily digest after the local morning hour, and one message on a day with unusually many visitors.
#[allow(clippy::too_many_arguments)]
async fn summaries(
    state: &AppState,
    site: &str,
    domain: &str,
    tz: &str,
    token: &str,
    chat: &str,
    digest_day: i64,
    spike_day: i64,
) -> anyhow::Result<()> {
    let store = state.events.clone();
    let (id, zone) = (site.to_string(), tz.to_string());
    let (today, hour, yesterday, today_visitors, week_visitors) =
        tokio::task::spawn_blocking(move || {
            let now = OffsetDateTime::now_utc().to_offset(store.utc_offset(&zone)?);
            let day = |from: time::Date, to: time::Date| Period {
                from,
                to,
                tz: zone.clone(),
            };
            let today = now.date();
            let totals = |p: Period| store.overview(&id, &p).map(|o| o.current);
            anyhow::Ok((
                today,
                now.hour(),
                totals(day(today - Duration::days(1), today - Duration::days(1)))?,
                totals(day(today, today))?.visitors,
                totals(day(today - Duration::days(7), today - Duration::days(1)))?.visitors,
            ))
        })
        .await??;
    let day = i64::from(today.to_julian_day());
    let mut message = None;
    if hour >= DIGEST_HOUR && digest_day < day {
        message = Some((
            true,
            if yesterday.visitors > 0 || yesterday.payments > 0 {
                format!(
                    "Yesterday on {domain}: {} visitors, {} payments, {} revenue.",
                    yesterday.visitors,
                    yesterday.payments,
                    money(yesterday.revenue_usd)
                )
            } else {
                String::new()
            },
        ));
    } else if spike_day < day && today_visitors >= SPIKE_MIN {
        let usual = week_visitors as f64 / 7.0;
        if today_visitors as f64 >= SPIKE_TIMES * usual {
            message = Some((
                false,
                format!(
                    "Traffic is well above usual on {domain}: {today_visitors} visitors today, against about {usual:.0} a day this past week."
                ),
            ));
        }
    }
    let Some((digest, text)) = message else {
        return Ok(());
    };
    let error = if text.is_empty() {
        None
    } else {
        send(state, token, chat, &text)
            .await
            .err()
            .map(|e| e.to_string())
    };
    if error.is_none() {
        sqlx::query(if digest {
            "UPDATE telegram_alerts SET digest_day = ?, last_error = NULL WHERE site_id = ?"
        } else {
            "UPDATE telegram_alerts SET spike_day = ?, last_error = NULL WHERE site_id = ?"
        })
        .bind(day)
        .bind(site)
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query("UPDATE telegram_alerts SET last_error = ? WHERE site_id = ?")
            .bind(&error)
            .bind(site)
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct Input {
    token: String,
    chat_id: String,
}

/// Saves the bot token and chat after sending a test message with them.
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
    let (token, chat) = (input.token.trim(), input.chat_id.trim());
    let hello = format!(
        "Virling will message you here about new payments on {}.",
        ctx.site.domain
    );
    if token.is_empty() || chat.is_empty() || send(&state, token, chat, &hello).await.is_err() {
        return Redirect::to(&format!("{settings}?error=telegram#telegram")).into_response();
    }
    let Ok((ciphertext, nonce)) = state.vault.encrypt(token) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let saved = sqlx::query(
        "INSERT INTO telegram_alerts (site_id, token_ciphertext, token_nonce, chat_id, sent_until) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (site_id) DO UPDATE SET token_ciphertext = excluded.token_ciphertext,
           token_nonce = excluded.token_nonce, chat_id = excluded.chat_id, last_error = NULL",
    )
    .bind(&ctx.site.id)
    .bind(ciphertext)
    .bind(nonce)
    .bind(chat)
    .bind(OffsetDateTime::now_utc().unix_timestamp())
    .execute(&state.db)
    .await;
    if let Err(err) = saved {
        tracing::error!(%err, "saving Telegram alerts failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("{settings}?saved=telegram#telegram")).into_response()
}

/// Turns the site's Telegram alerts off.
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
    if let Err(err) = sqlx::query("DELETE FROM telegram_alerts WHERE site_id = ?")
        .bind(&ctx.site.id)
        .execute(&state.db)
        .await
    {
        tracing::error!(%err, "removing Telegram alerts failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!(
        "{}#telegram",
        app_path(&ctx.site.domain, "settings")
    ))
    .into_response()
}
