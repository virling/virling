//! Email to the site owner for each new sale and a daily digest at a chosen local hour.

use axum::Form;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::{Duration, OffsetDateTime};

use crate::app::AppState;
use crate::auth::Account;
use crate::chart::esc;
use crate::dashboard::{load, money, native};
use crate::sites::app_path;
use crate::stats::{Dimension, PaymentFilter, Period};

const INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
/// Most sales one site is told about per round.
const BATCH: u32 = 20;

/// Checks every site's alerts once a minute, forever.
pub async fn run(state: AppState) {
    loop {
        if let Err(err) = tick(&state, OffsetDateTime::now_utc()).await {
            tracing::error!(%err, "email alert round failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

type Row = (String, String, String, String, bool, bool, i64, i64, i64);

async fn email(state: &AppState, to: &str, subject: &str, lines: &[String], link: &str) {
    let text = format!("{}\n\n{link}\n", lines.join("\n"));
    let body: String = lines.iter().map(|l| format!("<p>{}</p>", esc(l))).collect();
    let html = format!(
        "{body}<p><a href=\"{}\">Open the dashboard</a></p>",
        esc(link)
    );
    if let Err(err) = state.mailer.send(to, subject, text, html).await {
        tracing::error!(%err, "sending an alert email failed");
    }
}

/// Sends the sale emails and the digest due at `now`; returns how many emails went out.
pub async fn tick(state: &AppState, now: OffsetDateTime) -> anyhow::Result<usize> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT e.site_id, s.domain, s.timezone, a.email, e.sales, e.digest, e.digest_hour, e.sent_until, e.digest_day
         FROM email_alerts e JOIN sites s ON s.id = e.site_id JOIN accounts a ON a.id = s.account_id
         WHERE s.sample_data = 0 AND (e.sales = 1 OR e.digest = 1)",
    )
    .fetch_all(&state.db)
    .await?;
    let mut sent = 0;
    for (site, domain, tz, to, sales, digest, hour, sent_until, digest_day) in rows {
        let link = format!("{}{}", state.config.base_url, app_path(&domain, ""));
        let store = state.events.clone();
        let (id, zone) = (site.clone(), tz.clone());
        let (today, local_hour, page, yesterday, channels) =
            tokio::task::spawn_blocking(move || {
                let local = now.to_offset(store.utc_offset(&zone)?);
                let today = local.date();
                let day = |from: time::Date, to: time::Date| Period {
                    from,
                    to,
                    tz: zone.clone(),
                };
                let before = day(today - Duration::days(1), today - Duration::days(1));
                anyhow::Ok((
                    today,
                    local.hour(),
                    if sales {
                        Some(store.payment_list(
                            &id,
                            &day(today - Duration::days(1), today),
                            PaymentFilter::All,
                            BATCH,
                            0,
                        )?)
                    } else {
                        None
                    },
                    store.overview(&id, &before)?.current,
                    store.breakdown(&id, &before, Dimension::Channel)?,
                ))
            })
            .await??;
        if let Some(page) = page {
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
                let amount = native(p.amount, &p.currency);
                let source = p.touch.as_ref().map_or("an unknown source", |t| &t.source);
                email(
                    state,
                    &to,
                    &format!("New sale on {domain}: {amount}"),
                    &[format!("{amount} from {source}.")],
                    &link,
                )
                .await;
                sqlx::query(
                    "UPDATE email_alerts SET sent_until = max(sent_until, ?) WHERE site_id = ?",
                )
                .bind(p.ts.unix_timestamp())
                .bind(&site)
                .execute(&state.db)
                .await?;
                sent += 1;
            }
        }
        let julian = i64::from(today.to_julian_day());
        if digest && i64::from(local_hour) >= hour && digest_day < julian {
            if yesterday.visitors > 0 || yesterday.payments > 0 {
                let mut lines = vec![format!(
                    "Yesterday on {domain}: {} visitors, {} sales, {} revenue.",
                    yesterday.visitors,
                    yesterday.payments,
                    money(yesterday.revenue_usd)
                )];
                if let Some(top) = channels.iter().find(|c| !c.unattributed && c.visitors > 0) {
                    lines.push(format!(
                        "Top channel: {}, {} visitors and {} revenue.",
                        top.label,
                        top.visitors,
                        money(top.revenue_usd)
                    ));
                }
                email(state, &to, &format!("Yesterday on {domain}"), &lines, &link).await;
                sent += 1;
            }
            sqlx::query("UPDATE email_alerts SET digest_day = ? WHERE site_id = ?")
                .bind(julian)
                .bind(&site)
                .execute(&state.db)
                .await?;
        }
    }
    Ok(sent)
}

/// What the Site settings form shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub sales: bool,
    pub digest: bool,
    pub hour: i64,
}

pub async fn load_settings(db: &sqlx::SqlitePool, site_id: &str) -> sqlx::Result<Settings> {
    let row: Option<(bool, bool, i64)> =
        sqlx::query_as("SELECT sales, digest, digest_hour FROM email_alerts WHERE site_id = ?")
            .bind(site_id)
            .fetch_optional(db)
            .await?;
    let (sales, digest, hour) = row.unwrap_or((false, false, 9));
    Ok(Settings {
        sales,
        digest,
        hour,
    })
}

#[derive(Deserialize)]
pub struct Input {
    sales: Option<String>,
    digest: Option<String>,
    hour: Option<i64>,
}

/// Saves which alert emails the owner gets; turning one on skips what already happened.
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
    let before = match load_settings(&state.db, &ctx.site.id).await {
        Ok(s) => s,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let (sales, digest) = (input.sales.is_some(), input.digest.is_some());
    let hour = input.hour.unwrap_or(9).clamp(0, 23);
    let now = OffsetDateTime::now_utc();
    let store = state.events.clone();
    let zone = ctx.site.timezone.clone();
    let Ok(Ok(today)) = tokio::task::spawn_blocking(move || {
        store.utc_offset(&zone).map(|o| now.to_offset(o).date())
    })
    .await
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let saved = sqlx::query(
        "INSERT INTO email_alerts (site_id, sales, digest, digest_hour, sent_until, digest_day) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (site_id) DO UPDATE SET sales = excluded.sales, digest = excluded.digest,
           digest_hour = excluded.digest_hour,
           sent_until = CASE WHEN ? THEN excluded.sent_until ELSE email_alerts.sent_until END,
           digest_day = CASE WHEN ? THEN excluded.digest_day ELSE email_alerts.digest_day END",
    )
    .bind(&ctx.site.id)
    .bind(sales)
    .bind(digest)
    .bind(hour)
    .bind(now.unix_timestamp())
    .bind(i64::from(today.to_julian_day()))
    .bind(sales && !before.sales)
    .bind(digest && !before.digest)
    .execute(&state.db)
    .await;
    if let Err(err) = saved {
        tracing::error!(%err, "saving email alerts failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!(
        "{}?saved=alerts#alerts",
        app_path(&ctx.site.domain, "settings")
    ))
    .into_response()
}
