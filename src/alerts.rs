//! Email to the site owner for each new sale, a daily digest and unusual changes, at a chosen local hour.

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
use crate::stats::{BreakdownRow, Dimension, PaymentFilter, Period, Totals};
use crate::telegram::{SPIKE_MIN, SPIKE_TIMES};

const INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
/// Most sales one site is told about per round.
const BATCH: u32 = 20;
/// Revenue this many times above or below the usual day is a change.
const REVENUE_TIMES: f64 = 2.0;
/// Payments in the week before that make the usual day of revenue worth comparing.
const REVENUE_MIN_WEEK: u64 = 7;
/// Usual visitors a day that make a day with none look like broken tracking.
const TRACKING_MIN: f64 = 5.0;
/// Most changes listed in one email.
const MOST_CHANGES: usize = 5;

/// Checks every site's alerts once a minute, forever.
pub async fn run(state: AppState) {
    loop {
        if let Err(err) = tick(&state, OffsetDateTime::now_utc()).await {
            tracing::error!(%err, "email alert round failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

type Row = (
    String,
    String,
    String,
    String,
    bool,
    bool,
    bool,
    i64,
    i64,
    i64,
    i64,
);

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

/// One thing that moved away from usual: a headline and a row for the email.
#[derive(Debug, PartialEq)]
struct Change {
    headline: String,
    row: (String, String, String),
}

impl Change {
    fn new(headline: String, name: &str, was: String, now: String) -> Self {
        Change {
            headline,
            row: (name.to_string(), was, now),
        }
    }

    fn sentence(&self) -> String {
        format!("{}: {}, {}.", self.row.0, self.row.1, self.row.2)
    }
}

/// Compares yesterday with the week before it, and its referrers with the 30 days before.
fn detect(
    yesterday: &Totals,
    week: &Totals,
    channels: &[BreakdownRow],
    week_channels: &[BreakdownRow],
    referrers: &[BreakdownRow],
    past_referrers: &[BreakdownRow],
) -> Vec<Change> {
    let usual = week.visitors as f64 / 7.0;
    if yesterday.visitors == 0 && usual >= TRACKING_MIN {
        return vec![Change::new(
            "Tracking may have stopped".into(),
            "Visitors",
            format!("about {usual:.0} a day"),
            "none yesterday".into(),
        )];
    }
    let mut found = vec![];
    let revenue = week.revenue_usd / 7.0;
    if week.payments >= REVENUE_MIN_WEEK && revenue > 0.0 {
        let now = yesterday.revenue_usd;
        let headline = if now >= revenue * REVENUE_TIMES {
            Some("Revenue is well above usual")
        } else if now <= revenue / REVENUE_TIMES {
            Some("Revenue is well below usual")
        } else {
            None
        };
        if let Some(headline) = headline {
            found.push(Change::new(
                headline.into(),
                "Revenue",
                format!("about {} a day", money(revenue)),
                money(now),
            ));
        }
    }
    let usual_of = |rows: &[BreakdownRow], label: &str| {
        rows.iter()
            .find(|c| c.label == label)
            .map_or(0.0, |c| c.visitors as f64 / 7.0)
    };
    let row = |label: &str, up: bool, was: f64, now: u64| {
        let word = if up { "up" } else { "down" };
        Change::new(
            format!("{label} traffic is {word} sharply"),
            label,
            format!("about {was:.0} visitors a day"),
            format!("{} visitors", group(now)),
        )
    };
    for c in channels.iter().filter(|c| !c.unattributed) {
        let was = usual_of(week_channels, &c.label);
        if c.visitors >= SPIKE_MIN && c.visitors as f64 >= SPIKE_TIMES * was {
            found.push(row(&c.label, true, was, c.visitors));
        }
    }
    for c in week_channels.iter().filter(|c| !c.unattributed) {
        let was = c.visitors as f64 / 7.0;
        let now = channels
            .iter()
            .find(|y| y.label == c.label)
            .map_or(0, |y| y.visitors);
        if was >= SPIKE_MIN as f64 && now as f64 <= was / SPIKE_TIMES {
            found.push(row(&c.label, false, was, now));
        }
    }
    let paid_before = |key: &str| {
        past_referrers
            .iter()
            .any(|r| r.key.as_deref() == Some(key) && r.payments > 0)
    };
    for r in referrers
        .iter()
        .filter(|r| r.payments > 0 && !r.unattributed)
    {
        if r.key.as_deref().is_some_and(|key| !paid_before(key)) {
            found.push(Change::new(
                format!("{} sent its first buyer", r.label),
                &r.label,
                "new source of buyers".into(),
                money(r.revenue_usd),
            ));
        }
    }
    found.truncate(MOST_CHANGES);
    found
}

/// Sends the sale emails and the digest due at `now`; returns how many emails went out.
pub async fn tick(state: &AppState, now: OffsetDateTime) -> anyhow::Result<usize> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT e.site_id, s.domain, s.timezone, a.email, e.sales, e.digest, e.changes, e.digest_hour, e.sent_until, e.digest_day, e.changes_day
         FROM email_alerts e JOIN sites s ON s.id = e.site_id JOIN accounts a ON a.id = s.account_id
         WHERE s.sample_data = 0 AND (e.sales = 1 OR e.digest = 1 OR e.changes = 1)",
    )
    .fetch_all(&state.db)
    .await?;
    let mut sent = 0;
    for (site, domain, tz, to, sales, digest, changes, hour, sent_until, digest_day, changes_day) in
        rows
    {
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
        let due = i64::from(now.to_offset(local_offset).hour()) >= hour;
        if digest && due && digest_day < julian {
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
        if changes && due && changes_day < julian {
            let store = state.events.clone();
            let (id, zone) = (site.clone(), tz.clone());
            let (week, week_channels, referrers, past_referrers) =
                tokio::task::spawn_blocking(move || {
                    let day = |from: time::Date, to: time::Date| Period {
                        from,
                        to,
                        tz: zone.clone(),
                    };
                    let days = |n: i64| today - Duration::days(n);
                    let week = day(days(8), days(2));
                    anyhow::Ok((
                        store.overview(&id, &week)?.current,
                        store.breakdown(&id, &week, Dimension::Channel)?,
                        store.breakdown(&id, &day(days(1), days(1)), Dimension::Referrer)?,
                        store.breakdown(&id, &day(days(31), days(2)), Dimension::Referrer)?,
                    ))
                })
                .await??;
            let found = detect(
                &yesterday,
                &week,
                &channels,
                &week_channels,
                &referrers,
                &past_referrers,
            );
            if let Some(first) = found.first() {
                let subject = format!("{domain}: {}", first.headline);
                let lines: Vec<String> = found.iter().map(|c| c.sentence()).collect();
                let alert = Alert {
                    subject: &subject,
                    base,
                    domain: &domain,
                    when: day_name(today - Duration::days(1)),
                    kicker: "Unusual change",
                    headline: first.headline.clone(),
                    kpis: vec![],
                    rows_title: "Against the week before",
                    rows: found.into_iter().map(|c| c.row).collect(),
                    link: &link,
                    settings: settings.clone(),
                };
                email(state, &to, &lines, &alert).await;
                sent += 1;
            }
            sqlx::query("UPDATE email_alerts SET changes_day = ? WHERE site_id = ?")
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
    pub changes: bool,
    pub hour: i64,
}

pub async fn load_settings(db: &sqlx::SqlitePool, site_id: &str) -> sqlx::Result<Settings> {
    let row: Option<(bool, bool, bool, i64)> = sqlx::query_as(
        "SELECT sales, digest, changes, digest_hour FROM email_alerts WHERE site_id = ?",
    )
    .bind(site_id)
    .fetch_optional(db)
    .await?;
    let (sales, digest, changes, hour) = row.unwrap_or((false, false, false, 9));
    Ok(Settings {
        sales,
        digest,
        changes,
        hour,
    })
}

#[derive(Deserialize)]
pub struct Input {
    sales: Option<String>,
    digest: Option<String>,
    changes: Option<String>,
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
    let (sales, digest, changes) = (
        input.sales.is_some(),
        input.digest.is_some(),
        input.changes.is_some(),
    );
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
        "INSERT INTO email_alerts (site_id, sales, digest, changes, digest_hour, sent_until, digest_day, changes_day) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (site_id) DO UPDATE SET sales = excluded.sales, digest = excluded.digest,
           changes = excluded.changes, digest_hour = excluded.digest_hour,
           sent_until = CASE WHEN ? THEN excluded.sent_until ELSE email_alerts.sent_until END,
           digest_day = CASE WHEN ? THEN excluded.digest_day ELSE email_alerts.digest_day END,
           changes_day = CASE WHEN ? THEN excluded.changes_day ELSE email_alerts.changes_day END",
    )
    .bind(&ctx.site.id)
    .bind(sales)
    .bind(digest)
    .bind(changes)
    .bind(hour)
    .bind(now.unix_timestamp())
    .bind(i64::from(today.to_julian_day()))
    .bind(i64::from(today.to_julian_day()))
    .bind(sales && !before.sales)
    .bind(digest && !before.digest)
    .bind(changes && !before.changes)
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

    fn row(label: &str, visitors: u64, payments: u64, revenue_usd: f64) -> BreakdownRow {
        BreakdownRow {
            key: Some(label.to_string()),
            label: label.to_string(),
            unattributed: false,
            visitors,
            payments,
            customers: payments,
            refunded: 0,
            revenue_usd,
            revenue_per_visitor: None,
            avg_visit_secs: None,
        }
    }

    fn totals(visitors: u64, payments: u64, revenue_usd: f64) -> Totals {
        Totals {
            visitors,
            payments,
            revenue_usd,
            ..Default::default()
        }
    }

    fn headlines(found: Vec<Change>) -> Vec<String> {
        found.into_iter().map(|c| c.headline).collect()
    }

    #[test]
    fn a_normal_day_sends_nothing() {
        let week = totals(700, 14, 1400.0);
        let found = detect(
            &totals(100, 2, 200.0),
            &week,
            &[row("Search", 100, 2, 200.0)],
            &[row("Search", 700, 14, 1400.0)],
            &[row("google.com", 100, 2, 200.0)],
            &[row("google.com", 3000, 60, 6000.0)],
        );
        assert!(found.is_empty());
    }

    #[test]
    fn no_visitors_after_a_busy_week_is_only_a_tracking_alert() {
        let found = detect(
            &totals(0, 0, 0.0),
            &totals(700, 14, 1400.0),
            &[],
            &[row("Search", 700, 14, 1400.0)],
            &[],
            &[],
        );
        assert_eq!(headlines(found), ["Tracking may have stopped"]);
    }

    #[test]
    fn a_quiet_site_is_not_called_broken() {
        assert!(detect(&totals(0, 0, 0.0), &totals(14, 0, 0.0), &[], &[], &[], &[]).is_empty());
    }

    #[test]
    fn revenue_moves_only_count_after_enough_payments() {
        let up = detect(
            &totals(100, 5, 500.0),
            &totals(700, 14, 1400.0),
            &[],
            &[],
            &[],
            &[],
        );
        assert_eq!(headlines(up), ["Revenue is well above usual"]);
        let down = detect(
            &totals(100, 1, 50.0),
            &totals(700, 14, 1400.0),
            &[],
            &[],
            &[],
            &[],
        );
        assert_eq!(headlines(down), ["Revenue is well below usual"]);
        let thin = detect(
            &totals(100, 5, 500.0),
            &totals(700, 3, 300.0),
            &[],
            &[],
            &[],
            &[],
        );
        assert!(thin.is_empty());
    }

    #[test]
    fn channels_spike_and_fall_against_their_usual_day() {
        let week = totals(1400, 0, 0.0);
        let spike = detect(
            &totals(300, 0, 0.0),
            &week,
            &[row("Reddit", 90, 0, 0.0)],
            &[row("Reddit", 140, 0, 0.0)],
            &[],
            &[],
        );
        assert_eq!(headlines(spike), ["Reddit traffic is up sharply"]);
        let fall = detect(
            &totals(100, 0, 0.0),
            &week,
            &[row("Search", 10, 0, 0.0)],
            &[row("Search", 700, 0, 0.0)],
            &[],
            &[],
        );
        assert_eq!(headlines(fall), ["Search traffic is down sharply"]);
    }

    #[test]
    fn a_referrer_is_new_only_with_no_buyers_before() {
        let today = [row("news.site", 12, 1, 49.0)];
        let week = totals(700, 0, 0.0);
        let first = detect(&totals(100, 1, 49.0), &week, &[], &[], &today, &[]);
        assert_eq!(headlines(first), ["news.site sent its first buyer"]);
        let past = [row("news.site", 40, 2, 98.0)];
        assert!(detect(&totals(100, 1, 49.0), &week, &[], &[], &today, &past).is_empty());
    }
}
