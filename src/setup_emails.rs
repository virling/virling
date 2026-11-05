//! One reminder each for a new hosted site with no script or no payments connected.

use time::{Duration, OffsetDateTime};

use crate::alerts::{Alert, day_name, email};
use crate::app::AppState;
use crate::sites::app_path;

const INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);
/// A site gets a day to finish setup, and no reminder once it is two weeks old.
const WAIT: Duration = Duration::days(1);
const LATEST: Duration = Duration::days(14);

/// Checks new sites once an hour, forever.
pub async fn run(state: AppState) {
    loop {
        if let Err(err) = tick(&state, OffsetDateTime::now_utc()).await {
            tracing::error!(%err, "setup email round failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

type Row = (String, String, String, bool, bool, bool);

/// Sends the reminders due at `now`; returns how many emails went out.
pub async fn tick(state: &AppState, now: OffsetDateTime) -> anyhow::Result<usize> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.id, s.domain, a.email, coalesce(e.script_sent, 0), coalesce(e.payments_sent, 0),
                EXISTS (SELECT 1 FROM integrations i WHERE i.site_id = s.id)
         FROM sites s JOIN accounts a ON a.id = s.account_id
         LEFT JOIN setup_emails e ON e.site_id = s.id
         WHERE s.sample_data = 0 AND CAST(s.created_at AS INTEGER) BETWEEN ? AND ?",
    )
    .bind((now - LATEST).unix_timestamp())
    .bind((now - WAIT).unix_timestamp())
    .fetch_all(&state.db)
    .await?;
    let base = state.config.base_url.as_str();
    let mut sent = 0;
    for (site, domain, to, script_sent, payments_sent, connected) in rows {
        let store = state.events.clone();
        let id = site.clone();
        let visited = tokio::task::spawn_blocking(move || store.first_event(&id))
            .await??
            .is_some();
        let (script, page, headline, note) = if !visited && !script_sent {
            (
                true,
                "setup/script",
                "Add the script to see your first visit",
                format!(
                    "Virling has not received a visit from {domain} yet. Paste one line before </head> on your pages, and visits show up within seconds."
                ),
            )
        } else if visited && !connected && !payments_sent {
            (
                false,
                "setup/payments",
                "Connect payments to see which channels pay",
                format!(
                    "Visits are coming in from {domain}. Connect your payment provider with a read-only key, and Virling links each payment to the visit before it."
                ),
            )
        } else {
            continue;
        };
        let link = format!("{base}{}", app_path(&domain, page));
        let lines = [note.clone()];
        let alert = Alert {
            subject: headline,
            base,
            domain: &domain,
            when: day_name(now.date()),
            kicker: "Finish setup",
            headline: headline.to_string(),
            note,
            kpis: vec![],
            rows_title: "",
            rows: vec![],
            link: &link,
            action: "Finish setup",
            footer: "This is a one-time reminder. Need a hand? Write to".into(),
            footer_link: "help@virling.com",
            settings: "mailto:help@virling.com".into(),
        };
        email(state, &to, &lines, &alert).await;
        sent += 1;
        sqlx::query(
            "INSERT INTO setup_emails (site_id, script_sent, payments_sent) VALUES (?, ?, ?)
             ON CONFLICT (site_id) DO UPDATE SET
               script_sent = max(script_sent, excluded.script_sent),
               payments_sent = max(payments_sent, excluded.payments_sent)",
        )
        .bind(&site)
        .bind(script)
        .bind(!script)
        .execute(&state.db)
        .await?;
    }
    Ok(sent)
}
