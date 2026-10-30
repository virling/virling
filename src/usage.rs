//! The hosted plans' monthly allowance: counted hourly, warned at 80% and 100% by email and banner, never enforced.

use askama::Template;
use sqlx::SqlitePool;
use time::{Date, OffsetDateTime};

use crate::app::AppState;
use crate::billing::Email;
use crate::dashboard::group;

const INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);

/// The monthly allowance of a plan and what it counts.
pub fn limit(account_type: &str) -> (u64, &'static str) {
    match account_type {
        "agency" => (1_000_000, "pageviews"),
        _ => (100_000, "events"),
    }
}

fn month(d: Date) -> String {
    format!("{}-{:02}", d.year(), u8::from(d.month()))
}

/// 0, 80 or 100: the warning `used` of `cap` has reached.
pub fn level(used: u64, cap: u64) -> i64 {
    match used * 100 / cap.max(1) {
        100.. => 100,
        80.. => 80,
        _ => 0,
    }
}

/// Over the Personal allowance on average across this month and the two before, so Agency fits better.
async fn outgrown(db: &SqlitePool, account: &str, months: &[String]) -> sqlx::Result<bool> {
    let used: Vec<i64> = sqlx::query_scalar(
        "SELECT used FROM usage_months WHERE account_id = ? AND month IN (?, ?, ?)",
    )
    .bind(account)
    .bind(&months[0])
    .bind(&months[1])
    .bind(&months[2])
    .fetch_all(db)
    .await?;
    Ok(used.len() == 3 && used.iter().sum::<i64>() / 3 > limit("personal").0 as i64)
}

fn months(now: OffsetDateTime) -> Vec<String> {
    let first = now.date().replace_day(1).unwrap_or(now.date());
    let mut out = vec![month(first)];
    let mut d = first;
    for _ in 0..2 {
        d = d.previous_day().unwrap_or(d).replace_day(1).unwrap_or(d);
        out.push(month(d));
    }
    out
}

/// The plain line to show and send at a warning level.
fn line(level: i64, used: u64, cap: u64, unit: &str, upgrade: bool) -> String {
    let head = if level >= 100 {
        format!("You have used all {} {unit} of this month.", group(cap))
    } else {
        format!(
            "You have used {}% of this month's {} {unit}.",
            used * 100 / cap.max(1),
            group(cap)
        )
    };
    let tail = if upgrade {
        " Over the last three months you averaged more than Personal includes; Agency includes 1,000,000 pageviews."
    } else {
        ""
    };
    format!("{head} Nothing is dropped: Virling keeps counting every visit.{tail}")
}

/// The banner for the account this month, empty under 80%.
pub async fn banner(db: &SqlitePool, account: &str, account_type: &str) -> sqlx::Result<String> {
    let now = OffsetDateTime::now_utc();
    let m = months(now);
    let used: Option<i64> =
        sqlx::query_scalar("SELECT used FROM usage_months WHERE account_id = ? AND month = ?")
            .bind(account)
            .bind(&m[0])
            .fetch_optional(db)
            .await?;
    let (cap, unit) = limit(account_type);
    let used = used.unwrap_or(0) as u64;
    let lvl = level(used, cap);
    if lvl == 0 {
        return Ok(String::new());
    }
    let upgrade = account_type != "agency" && outgrown(db, account, &m).await?;
    Ok(line(lvl, used, cap, unit, upgrade))
}

/// Counts every account's month and emails the owner when it first reaches 80% and 100%.
pub async fn tick(state: &AppState, now: OffsetDateTime) -> anyhow::Result<()> {
    let m = months(now);
    let since = now.date().replace_day(1)?.midnight().assume_utc();
    let accounts: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, email, account_type FROM accounts WHERE suspended_at IS NULL")
            .fetch_all(&state.db)
            .await?;
    for (account, email, kind) in accounts {
        let sites: Vec<(String, String)> =
            sqlx::query_as("SELECT id, domain FROM sites WHERE account_id = ? ORDER BY created_at")
                .bind(&account)
                .fetch_all(&state.db)
                .await?;
        let Some(domain) = sites.first().map(|s| s.1.clone()) else {
            continue;
        };
        let store = state.events.clone();
        let ids: Vec<String> = sites.into_iter().map(|s| s.0).collect();
        let agency = kind == "agency";
        let used = tokio::task::spawn_blocking(move || {
            ids.iter().try_fold(0u64, |n, id| {
                let (all, views) = store.usage_since(id, since)?;
                anyhow::Ok(n + if agency { views } else { all })
            })
        })
        .await??;
        let warned: i64 = sqlx::query_scalar(
            "INSERT INTO usage_months (account_id, month, used) VALUES (?, ?, ?)
             ON CONFLICT (account_id, month) DO UPDATE SET used = excluded.used
             RETURNING warned",
        )
        .bind(&account)
        .bind(&m[0])
        .bind(used as i64)
        .fetch_one(&state.db)
        .await?;
        let (cap, unit) = limit(&kind);
        let lvl = level(used, cap);
        if lvl <= warned {
            continue;
        }
        let upgrade = !agency && outgrown(&state.db, &account, &m).await?;
        let lines = [line(lvl, used, cap, unit, upgrade)];
        let link = format!(
            "{}{}#plan",
            state.config.base_url,
            crate::sites::app_path(&domain, "account")
        );
        let html = Email {
            heading: if lvl >= 100 {
                "You reached this month's allowance"
            } else {
                "You are at 80% of this month's allowance"
            },
            lines: &lines,
            link: &link,
            button: "Open billing",
        }
        .render()?;
        let subject = format!("Virling usage: {lvl}% of this month's {unit}");
        state
            .mailer
            .send(&email, &subject, format!("{}\n\n{link}", lines[0]), html)
            .await?;
        sqlx::query("UPDATE usage_months SET warned = ? WHERE account_id = ? AND month = ?")
            .bind(lvl)
            .bind(&account)
            .bind(&m[0])
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

pub async fn run(state: AppState) {
    loop {
        if let Err(err) = tick(&state, OffsetDateTime::now_utc()).await {
            tracing::error!(%err, "counting plan usage failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::Mailer;

    #[test]
    fn warns_at_80_and_100_percent() {
        assert_eq!(level(79_999, 100_000), 0);
        assert_eq!(level(80_000, 100_000), 80);
        assert_eq!(level(250_000, 100_000), 100);
        let l = line(80, 85_000, 100_000, "events", false);
        assert!(
            l.starts_with("You have used 85% of this month's 100,000 events.")
                && l.contains("Nothing is dropped")
        );
        assert!(
            line(100, 1, 100_000, "events", true).contains("Agency includes 1,000,000 pageviews")
        );
        let now = time::macros::datetime!(2026-01-15 10:00 UTC);
        assert_eq!(months(now), ["2026-01", "2025-12", "2025-11"]);
    }

    #[tokio::test]
    async fn counts_the_month_and_suggests_agency_on_a_three_month_average() {
        let f = crate::api::tests::fixture().await;
        let db = &f.state.db;
        let now = OffsetDateTime::now_utc();
        tick(&f.state, now).await.unwrap();
        let used: i64 = sqlx::query_scalar("SELECT used FROM usage_months WHERE account_id = ?")
            .bind(&f.account)
            .fetch_one(db)
            .await
            .unwrap();
        assert!((0..80_000).contains(&used));
        match &*f.state.mailer {
            Mailer::Capture(sent) => assert!(sent.lock().unwrap().is_empty()),
            _ => unreachable!(),
        }
        assert_eq!(banner(db, &f.account, "personal").await.unwrap(), "");
        for (m, n) in months(now).iter().zip([120_000, 130_000, 110_000]) {
            sqlx::query("INSERT INTO usage_months (account_id, month, used) VALUES (?, ?, ?) ON CONFLICT (account_id, month) DO UPDATE SET used = excluded.used")
                .bind(&f.account)
                .bind(m)
                .bind(n)
                .execute(db)
                .await
                .unwrap();
        }
        let b = banner(db, &f.account, "personal").await.unwrap();
        assert!(
            b.starts_with("You have used all 100,000 events") && b.contains("Agency"),
            "{b}"
        );
        assert!(
            !banner(db, &f.account, "agency")
                .await
                .unwrap()
                .contains("Agency includes")
        );
    }
}
