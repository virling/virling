//! Email to the site owner for each new sale and a daily digest at a chosen local hour.

use askama::Template;
use axum::Form;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::{Duration, OffsetDateTime};

use crate::app::AppState;
use crate::auth::Account;
use crate::dashboard::{group, load, money, native};
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

/// One alert email: a headline, optional stat cards and rows, and a link to the dashboard.
#[derive(Template)]
#[template(path = "email/alert.html")]
struct Alert<'a> {
    subject: &'a str,
    base: &'a str,
    domain: &'a str,
    when: String,
    kicker: &'a str,
    headline: String,
    kpis: Vec<(&'static str, String)>,
    rows_title: &'static str,
    rows: Vec<(String, String, String)>,
    link: &'a str,
    settings: String,
}

async fn email(state: &AppState, to: &str, lines: &[String], alert: &Alert<'_>) {
    let text = format!("{}\n\n{}\n", lines.join("\n"), alert.link);
    let html = match alert.render() {
        Ok(html) => html,
        Err(err) => {
            tracing::error!(%err, "rendering an alert email failed");
            return;
        }
    };
    if let Err(err) = state.mailer.send(to, alert.subject, text, html).await {
        tracing::error!(%err, "sending an alert email failed");
    }
}

fn day_name(d: time::Date) -> String {
    format!("{}, {} {}", d.weekday(), d.day(), d.month())
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
        let base = state.config.base_url.as_str();
        let link = format!("{base}{}", app_path(&domain, ""));
        let settings = format!("{base}{}#alerts", app_path(&domain, "settings"));
        let store = state.events.clone();
        let (id, zone) = (site.clone(), tz.clone());
        let (today, local_offset, page, yesterday, channels) =
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
                    local.offset(),
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
                let subject = format!("New sale on {domain}: {amount}");
                let alert = Alert {
                    subject: &subject,
                    base,
                    domain: &domain,
                    when: day_name(p.ts.to_offset(local_offset).date()),
                    kicker: "New sale",
                    headline: format!("{amount} from {source}"),
                    kpis: vec![],
                    rows_title: "",
                    rows: vec![],
                    link: &link,
                    settings: settings.clone(),
                };
                email(state, &to, &[format!("{amount} from {source}.")], &alert).await;
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
        if digest && i64::from(now.to_offset(local_offset).hour()) >= hour && digest_day < julian {
            if yesterday.visitors > 0 || yesterday.payments > 0 {
                let mut lines = vec![format!(
                    "Yesterday on {domain}: {} visitors, {} sales, {} revenue.",
                    yesterday.visitors,
                    yesterday.payments,
                    money(yesterday.revenue_usd)
                )];
                let top: Vec<_> = channels
                    .iter()
                    .filter(|c| !c.unattributed && c.visitors > 0)
                    .take(3)
                    .collect();
                if let Some(c) = top.first() {
                    lines.push(format!(
                        "Top channel: {}, {} visitors and {} revenue.",
                        c.label,
                        c.visitors,
                        money(c.revenue_usd)
                    ));
                }
                let headline = if yesterday.payments > 0 {
                    format!(
                        "{} from {} {}",
                        money(yesterday.revenue_usd),
                        group(yesterday.payments),
                        if yesterday.payments == 1 {
                            "sale"
                        } else {
                            "sales"
                        }
                    )
                } else {
                    format!("{} visitors, no sales", group(yesterday.visitors))
                };
                let subject = format!("Yesterday on {domain}");
                let alert = Alert {
                    subject: &subject,
                    base,
                    domain: &domain,
                    when: day_name(today - Duration::days(1)),
                    kicker: "Daily digest",
                    headline,
                    kpis: vec![
                        ("Visitors", group(yesterday.visitors)),
                        ("Sales", group(yesterday.payments)),
                        ("Revenue", money(yesterday.revenue_usd)),
                    ],
                    rows_title: "Top channels",
                    rows: top
                        .iter()
                        .map(|c| {
                            (
                                c.label.clone(),
                                format!("{} visitors", group(c.visitors)),
                                money(c.revenue_usd),
                            )
                        })
                        .collect(),
                    link: &link,
                    settings: settings.clone(),
                };
                email(state, &to, &lines, &alert).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_shows_stats_and_channels() {
        let alert = Alert {
            subject: "Yesterday on example.com",
            base: "https://app.virling.com",
            domain: "example.com",
            when: day_name(time::macros::date!(2026 - 11 - 02)),
            kicker: "Daily digest",
            headline: "$120 from 3 sales".into(),
            kpis: vec![
                ("Visitors", "97".into()),
                ("Sales", "3".into()),
                ("Revenue", "$120".into()),
            ],
            rows_title: "Top channels",
            rows: vec![("Organic search".into(), "90 visitors".into(), "$80".into())],
            link: "https://app.virling.com/app/example.com",
            settings: "https://app.virling.com/app/example.com/settings#alerts".into(),
        };
        let html = alert.render().unwrap();
        assert!(html.contains("Monday, 2 November"));
        assert!(html.contains("$120 from 3 sales"));
        assert!(html.contains("Organic search"));
        assert!(html.contains("/settings#alerts"));
    }
}
