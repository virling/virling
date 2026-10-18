//! The weekly report email: when it is due, and sending it once per week.

use askama::Template;
use sqlx::SqlitePool;
use time::{Date, Duration, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

use super::blocking;
use super::report::{self, ReportView, week_start};
use crate::app::AppState;
use crate::auth::random_token;
use crate::sites::Site;
use crate::waitlist::valid_email;

pub const DAYS: [(&str, &str); 7] = [
    ("Mon", "Monday"),
    ("Tue", "Tuesday"),
    ("Wed", "Wednesday"),
    ("Thu", "Thursday"),
    ("Fri", "Friday"),
    ("Sat", "Saturday"),
    ("Sun", "Sunday"),
];
const INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
const MAX_RECIPIENTS: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub on: bool,
    /// 0 is Monday.
    pub weekday: u8,
    /// Minutes after local midnight.
    pub minute: u16,
    pub recipients: Vec<String>,
    /// When the schedule was saved; send times before it are skipped.
    pub from: i64,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            on: false,
            weekday: 0,
            minute: 9 * 60,
            recipients: Vec::new(),
            from: 0,
        }
    }
}

/// "9:00"
pub fn clock(minute: u16) -> String {
    format!("{}:{:02}", minute / 60, minute % 60)
}

/// Addresses split on commas, semicolons and whitespace, lowercased and deduped; None when one is invalid or there are too many.
pub fn recipients(input: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for a in input.split([',', ';', ' ', '\n', '\r', '\t']) {
        let a = a.trim().to_ascii_lowercase();
        if a.is_empty() || out.contains(&a) {
            continue;
        }
        if !valid_email(&a) {
            return None;
        }
        out.push(a);
    }
    (out.len() <= MAX_RECIPIENTS).then_some(out)
}

/// Whether the site's account is an agency, and the account's email.
pub async fn owner(db: &SqlitePool, site_id: &str) -> sqlx::Result<(bool, String)> {
    sqlx::query_as(
        "SELECT a.account_type = 'agency', a.email FROM accounts a JOIN sites s ON s.account_id = a.id WHERE s.id = ?",
    )
    .bind(site_id)
    .fetch_one(db)
    .await
}

/// The site's schedule and agency note.
pub async fn load(db: &SqlitePool, site_id: &str) -> sqlx::Result<(Schedule, String)> {
    let row: Option<(bool, i64, i64, String, i64, String)> = sqlx::query_as(
        "SELECT send_on, send_weekday, send_minute, recipients, send_from, note FROM report_brands WHERE site_id = ?",
    )
    .bind(site_id)
    .fetch_optional(db)
    .await?;
    Ok(match row {
        Some((on, weekday, minute, recipients, from, note)) => (
            Schedule {
                on,
                weekday: weekday.clamp(0, 6) as u8,
                minute: minute.clamp(0, 24 * 60 - 1) as u16,
                recipients: recipients
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(String::from)
                    .collect(),
                from,
            },
            note,
        ),
        None => (Schedule::default(), String::new()),
    })
}

pub async fn save(db: &SqlitePool, site_id: &str, s: &Schedule) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO report_brands (site_id, send_on, send_weekday, send_minute, recipients, send_from) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(site_id) DO UPDATE SET send_on = excluded.send_on, send_weekday = excluded.send_weekday,
           send_minute = excluded.send_minute, recipients = excluded.recipients, send_from = excluded.send_from",
    )
    .bind(site_id)
    .bind(s.on)
    .bind(i64::from(s.weekday))
    .bind(i64::from(s.minute))
    .bind(s.recipients.join("\n"))
    .bind(s.from)
    .execute(db)
    .await?;
    Ok(())
}

/// The send time in the week starting `monday`, in a zone `offset` ahead of UTC.
fn moment(monday: Date, s: &Schedule, offset: UtcOffset) -> OffsetDateTime {
    let day = monday + Duration::days(s.weekday.into());
    let time =
        Time::from_hms((s.minute / 60) as u8, (s.minute % 60) as u8, 0).unwrap_or(Time::MIDNIGHT);
    PrimitiveDateTime::new(day, time)
        .assume_offset(offset)
        .to_offset(UtcOffset::UTC)
}

/// Monday of the week to send at `now`: last week, once this week's send time has passed since the schedule was saved.
pub fn due(now: OffsetDateTime, offset: UtcOffset, s: &Schedule) -> Option<Date> {
    if !s.on || s.recipients.is_empty() {
        return None;
    }
    let monday = week_start(now.to_offset(offset).date());
    let at = moment(monday, s, offset);
    (now >= at && at.unix_timestamp() >= s.from).then(|| monday - Duration::days(7))
}

/// Local date of the next scheduled send after `now`.
pub fn next(now: OffsetDateTime, offset: UtcOffset, s: &Schedule) -> Date {
    let at = moment(week_start(now.to_offset(offset).date()), s, offset);
    let at = if now < at { at } else { at + Duration::days(7) };
    at.to_offset(offset).date()
}

/// Takes the send of `week` for this run; false when it was already taken.
async fn claim(db: &SqlitePool, site_id: &str, week: Date) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "INSERT INTO report_sends (site_id, week) VALUES (?, ?) ON CONFLICT(site_id, week) DO NOTHING",
    )
    .bind(site_id)
    .bind(week.to_string())
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Marks `week` as sent to `n` recipients, so the schedule skips it.
pub async fn record(db: &SqlitePool, site_id: &str, week: Date, n: usize) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO report_sends (site_id, week, sent_at, recipients) VALUES (?, ?, ?, ?)
         ON CONFLICT(site_id, week) DO UPDATE SET sent_at = excluded.sent_at, recipients = excluded.recipients",
    )
    .bind(site_id)
    .bind(week.to_string())
    .bind(OffsetDateTime::now_utc().unix_timestamp())
    .bind(n as i64)
    .execute(db)
    .await?;
    Ok(())
}

/// Checks every schedule once a minute, forever.
pub async fn run(state: AppState) {
    loop {
        if let Err(err) = tick(&state, OffsetDateTime::now_utc()).await {
            tracing::error!(%err, "report schedule round failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

type Row = (
    String,
    String,
    String,
    bool,
    i64,
    i64,
    String,
    i64,
    bool,
    String,
);

/// Sends every report due at `now` that has not gone out for its week; returns how many went out.
pub async fn tick(state: &AppState, now: OffsetDateTime) -> anyhow::Result<usize> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.id, s.domain, s.timezone, s.cookieless, b.send_weekday, b.send_minute, b.recipients, b.send_from,
            a.account_type = 'agency', a.email
         FROM report_brands b JOIN sites s ON s.id = b.site_id JOIN accounts a ON a.id = s.account_id
         WHERE b.send_on = 1 AND (b.recipients <> '' OR a.account_type <> 'agency')",
    )
    .fetch_all(&state.db)
    .await?;
    let mut sent = 0;
    for (id, domain, timezone, cookieless, weekday, minute, list, from, agency, email) in rows {
        // A personal account's weekly email only ever goes to the account itself.
        let list = if agency { list } else { email };
        let site = Site {
            id,
            domain,
            timezone,
            cookieless,
            same_visit: String::new(),
            return_minutes: 5,
        };
        let s = Schedule {
            on: true,
            weekday: weekday.clamp(0, 6) as u8,
            minute: minute.clamp(0, 24 * 60 - 1) as u16,
            recipients: list
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
            from,
        };
        let store = state.events.clone();
        let tz = site.timezone.clone();
        let Ok(offset) = blocking(move || store.utc_offset(&tz)).await else {
            continue;
        };
        let Some(week) = due(now, offset, &s) else {
            continue;
        };
        if !claim(&state.db, &site.id, week).await? {
            continue;
        }
        match send(state, &site, week, &s.recipients).await {
            Ok(n) => {
                record(&state.db, &site.id, week, n).await?;
                sent += 1;
            }
            Err(err) => {
                tracing::error!(%err, site = %site.domain, "weekly report email failed");
                sqlx::query(
                    "DELETE FROM report_sends WHERE site_id = ? AND week = ? AND sent_at IS NULL",
                )
                .bind(&site.id)
                .bind(week.to_string())
                .execute(&state.db)
                .await?;
            }
        }
    }
    Ok(sent)
}

/// The site's read-only link token, made now when there is none.
async fn share_token(db: &SqlitePool, site_id: &str) -> sqlx::Result<String> {
    sqlx::query(
        "INSERT INTO report_brands (site_id, share_token) VALUES (?, ?)
         ON CONFLICT(site_id) DO UPDATE SET share_token = coalesce(report_brands.share_token, excluded.share_token)",
    )
    .bind(site_id)
    .bind(random_token())
    .execute(db)
    .await?;
    sqlx::query_scalar("SELECT share_token FROM report_brands WHERE site_id = ?")
        .bind(site_id)
        .fetch_one(db)
        .await
}

#[derive(Template)]
#[template(path = "email/report.html")]
struct EmailHtml<'a> {
    r: &'a ReportView,
    base: &'a str,
    link: &'a str,
}

#[derive(Template)]
#[template(path = "email/report.txt")]
struct EmailText<'a> {
    r: &'a ReportView,
    link: &'a str,
}

/// Subject, text and HTML of the report email; `base` prefixes image paths, `link` opens the full report.
pub fn email(r: &ReportView, base: &str, link: &str) -> askama::Result<(String, String, String)> {
    Ok((
        format!("Weekly revenue report for {}, {}", r.site, r.week_short),
        EmailText { r, link }.render()?,
        EmailHtml { r, base, link }.render()?,
    ))
}

/// Emails the report of the week starting `monday` to each recipient; returns how many it reached.
pub async fn send(
    state: &AppState,
    site: &Site,
    monday: Date,
    recipients: &[String],
) -> anyhow::Result<usize> {
    let r = report::view(state, site, monday, false)
        .await
        .map_err(|code| anyhow::anyhow!("building the report failed: {code}"))?;
    let base = &state.config.base_url;
    let link = format!(
        "{base}/r/{}?week={monday}",
        share_token(&state.db, &site.id).await?
    );
    let reply: String = sqlx::query_scalar(
        "SELECT a.email FROM accounts a JOIN sites s ON s.account_id = a.id WHERE s.id = ?",
    )
    .bind(&site.id)
    .fetch_one(&state.db)
    .await?;
    let (subject, text, html) = email(&r, base, &link)?;
    let mut n = 0;
    for to in recipients {
        match state
            .mailer
            .send_named(&r.agency, &reply, to, &subject, text.clone(), html.clone())
            .await
        {
            Ok(()) => n += 1,
            Err(err) => tracing::warn!(%err, %to, "report email failed"),
        }
    }
    anyhow::ensure!(n > 0, "no report email went out");
    Ok(n)
}

#[cfg(test)]
mod tests {
    use time::macros::{date, datetime, offset};

    use super::*;

    fn monday_nine() -> Schedule {
        Schedule {
            on: true,
            recipients: vec!["maya@tallybird.app".into()],
            ..Schedule::default()
        }
    }

    #[test]
    fn due_once_the_local_send_time_passes() {
        let s = monday_nine();
        let utc = offset!(UTC);
        assert_eq!(due(datetime!(2026-10-19 08:59 UTC), utc, &s), None);
        assert_eq!(
            due(datetime!(2026-10-19 09:00 UTC), utc, &s),
            Some(date!(2026 - 10 - 12))
        );
        assert_eq!(
            due(datetime!(2026-10-25 23:00 UTC), utc, &s),
            Some(date!(2026 - 10 - 12)),
            "still the same week, so the claim stops a second send"
        );
        let berlin = offset!(+2);
        assert_eq!(due(datetime!(2026-10-19 06:59 UTC), berlin, &s), None);
        assert_eq!(
            due(datetime!(2026-10-19 07:00 UTC), berlin, &s),
            Some(date!(2026 - 10 - 12))
        );
        let kolkata = offset!(+5:30);
        assert_eq!(
            due(datetime!(2026-10-19 03:30 UTC), kolkata, &s),
            Some(date!(2026 - 10 - 12))
        );
        let la = offset!(-7);
        assert_eq!(
            due(datetime!(2026-10-19 15:00 UTC), la, &s),
            None,
            "Monday 8:00 in LA"
        );
        assert_eq!(
            due(datetime!(2026-10-19 03:00 UTC), la, &s),
            Some(date!(2026 - 10 - 05)),
            "still Sunday in LA, so last week's send"
        );
        let off = Schedule {
            on: false,
            ..s.clone()
        };
        assert_eq!(due(datetime!(2026-10-19 10:00 UTC), utc, &off), None);
        let nobody = Schedule {
            recipients: Vec::new(),
            ..s.clone()
        };
        assert_eq!(due(datetime!(2026-10-19 10:00 UTC), utc, &nobody), None);
    }

    #[test]
    fn saving_after_the_send_time_waits_a_week() {
        let s = Schedule {
            from: datetime!(2026-10-21 12:00 UTC).unix_timestamp(),
            ..monday_nine()
        };
        let utc = offset!(UTC);
        assert_eq!(due(datetime!(2026-10-21 12:01 UTC), utc, &s), None);
        assert_eq!(
            next(datetime!(2026-10-21 12:01 UTC), utc, &s),
            date!(2026 - 10 - 26)
        );
        assert_eq!(
            due(datetime!(2026-10-26 09:00 UTC), utc, &s),
            Some(date!(2026 - 10 - 19))
        );
    }

    #[test]
    fn next_send_is_in_site_time() {
        let fri = Schedule {
            weekday: 4,
            minute: 17 * 60 + 30,
            ..monday_nine()
        };
        let utc = offset!(UTC);
        assert_eq!(
            next(datetime!(2026-10-16 10:00 UTC), utc, &fri),
            date!(2026 - 10 - 16)
        );
        assert_eq!(
            next(datetime!(2026-10-16 17:30 UTC), utc, &fri),
            date!(2026 - 10 - 23)
        );
        let tokyo = offset!(+9);
        assert_eq!(
            next(datetime!(2026-10-16 09:00 UTC), tokyo, &fri),
            date!(2026 - 10 - 23),
            "already 18:00 Friday in Tokyo"
        );
        assert_eq!(
            next(datetime!(2026-10-16 10:00 UTC), utc, &monday_nine()),
            date!(2026 - 10 - 19)
        );
        assert_eq!(clock(9 * 60), "9:00");
        assert_eq!(clock(17 * 60 + 30), "17:30");
    }

    #[test]
    fn recipients_are_checked() {
        assert_eq!(
            recipients("Maya@Tallybird.app, ops@tallybird.app\nmaya@tallybird.app"),
            Some(vec![
                "maya@tallybird.app".to_string(),
                "ops@tallybird.app".to_string()
            ])
        );
        assert_eq!(recipients(""), Some(Vec::new()));
        assert_eq!(recipients("maya@tallybird.app nope"), None);
        let many: Vec<String> = (0..11).map(|i| format!("a{i}@x.com")).collect();
        assert_eq!(recipients(&many.join(",")), None);
    }
}
