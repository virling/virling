//! Team members on an Agency account: invites, seats and removal.

use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::app::AppState;
use crate::auth::{self, Account, LoginPage, hash_token, random_token};
use crate::chart::esc;
use crate::sites::{self, app_path};
use crate::waitlist::valid_email;

/// Users an Agency account gets without extra seats, the owner included.
pub const INCLUDED: i64 = 3;
pub const SEAT_PRICE: &str = "$9/month";
const INVITE_TTL_SECS: i64 = 7 * 24 * 3600;

/// What inviting one more person means with `used` users on the account: "upgrade", "free", "paid" or "capped".
pub fn next_seat(agency: bool, hosted: bool, used: i64) -> &'static str {
    match (agency, hosted) {
        (false, _) => "upgrade",
        _ if used < INCLUDED => "free",
        (true, true) => "paid",
        (true, false) => "capped",
    }
}

pub struct Person {
    pub id: String,
    pub email: String,
    pub role: String,
    /// "remove" for members, "revoke" for invites, empty for the owner.
    pub action: &'static str,
}

pub struct Team {
    pub people: Vec<Person>,
    pub seats: String,
    pub next: &'static str,
}

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// Users on the account: the owner, members and open invites.
async fn used(db: &SqlitePool, org: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT 1 + (SELECT count(*) FROM members WHERE org_id = ?1)
           + (SELECT count(*) FROM invites WHERE org_id = ?1 AND expires_at > ?2)",
    )
    .bind(org)
    .bind(now())
    .fetch_one(db)
    .await
}

/// Stores the user count and bills the extra seats.
async fn record_seats(state: &AppState, org: &str) -> sqlx::Result<()> {
    let n = used(&state.db, org).await?;
    sqlx::query("UPDATE accounts SET seats = ? WHERE id = ?")
        .bind(n)
        .bind(org)
        .execute(&state.db)
        .await?;
    if state.config.billing.is_some() {
        tokio::spawn(crate::billing::sync_seats(state.clone(), org.to_string()));
    }
    Ok(())
}

pub async fn load(db: &SqlitePool, org: &str, hosted: bool) -> sqlx::Result<Team> {
    let (owner, kind): (String, String) =
        sqlx::query_as("SELECT email, account_type FROM accounts WHERE id = ?")
            .bind(org)
            .fetch_one(db)
            .await?;
    let members: Vec<(String, String)> = sqlx::query_as(
        "SELECT m.account_id, a.email FROM members m JOIN accounts a ON a.id = m.account_id
         WHERE m.org_id = ? ORDER BY m.created_at",
    )
    .bind(org)
    .fetch_all(db)
    .await?;
    let invites: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT id, email, expires_at FROM invites WHERE org_id = ? AND expires_at > ? ORDER BY created_at",
    )
    .bind(org)
    .bind(now())
    .fetch_all(db)
    .await?;
    let n = 1 + (members.len() + invites.len()) as i64;
    let mut people = vec![Person {
        id: org.to_string(),
        email: owner,
        role: "Owner".into(),
        action: "",
    }];
    people.extend(members.into_iter().map(|(id, email)| Person {
        id,
        email,
        role: "Member".into(),
        action: "remove",
    }));
    people.extend(invites.into_iter().map(|(id, email, expires)| {
        let days = ((expires - now()) as f64 / 86400.0).ceil() as i64;
        Person {
            id,
            email,
            role: format!(
                "Invited, link expires in {days} day{}",
                if days == 1 { "" } else { "s" }
            ),
            action: "revoke",
        }
    }));
    let seats = if n <= INCLUDED {
        format!("{n} of {INCLUDED} included")
    } else {
        format!(
            "{n} users: {INCLUDED} included, {} extra at {SEAT_PRICE} each",
            n - INCLUDED
        )
    };
    Ok(Team {
        people,
        seats,
        next: next_seat(kind == "agency", hosted, n),
    })
}

/// The owner's account page, or 403 for members and 404 for a site the account doesn't have.
async fn owner_page(
    state: &AppState,
    account: &Account,
    domain: &str,
) -> Result<String, StatusCode> {
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

#[derive(Deserialize)]
pub struct InviteInput {
    email: String,
    confirm: Option<String>,
}

pub async fn invite(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<InviteInput>,
) -> Response {
    let page = match owner_page(&state, &account, &domain).await {
        Ok(p) => p,
        Err(code) => return code.into_response(),
    };
    let email = input.email.trim().to_ascii_lowercase();
    let back = |q: &str| Redirect::to(&format!("{page}?{q}#team")).into_response();
    if !valid_email(&email) {
        return back("team=email");
    }
    let taken: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM accounts a LEFT JOIN members m ON m.account_id = a.id
                 WHERE a.email = ?1 AND (a.id = ?2 OR m.org_id = ?2))
              + (SELECT count(*) FROM invites WHERE org_id = ?2 AND email = ?1 AND expires_at > ?3)",
    )
    .bind(&email)
    .bind(&account.id)
    .bind(now())
    .fetch_one(&state.db)
    .await
    .unwrap_or(1);
    if taken > 0 {
        return back("team=taken");
    }
    let Ok(team) = load(&state.db, &account.id, state.config.hosted).await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match team.next {
        "free" => {}
        "paid" if input.confirm.is_some() => {}
        "paid" => return back(&format!("add={}", urlencode(&email))),
        _ => return back("team=limit"),
    }
    let token = random_token();
    let sent = async {
        sqlx::query(
            "INSERT INTO invites (id, token_hash, org_id, email, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(hash_token(&token))
        .bind(&account.id)
        .bind(&email)
        .bind(now())
        .bind(now() + INVITE_TTL_SECS)
        .execute(&state.db)
        .await?;
        record_seats(&state, &account.id).await?;
        let owner = &team.people[0].email;
        let link = format!("{}/invite?token={token}", state.config.base_url);
        let text = format!(
            "{owner} invited you to their sites on Virling.\n\nJoin the team:\n{link}\n\nThe link expires in 7 days."
        );
        let html = format!(
            r#"<p>{} invited you to their sites on Virling.</p><p><a href="{}">Join the team</a></p><p>The link expires in 7 days.</p>"#,
            esc(owner),
            esc(&link)
        );
        state
            .mailer
            .send(&email, "You're invited to a Virling team", text, html)
            .await
    }
    .await;
    if let Err(err) = sent {
        tracing::error!(%err, "sending an invite failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    back("saved=invite")
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[derive(Deserialize)]
pub struct IdInput {
    id: String,
}

pub async fn revoke(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<IdInput>,
) -> Response {
    let page = match owner_page(&state, &account, &domain).await {
        Ok(p) => p,
        Err(code) => return code.into_response(),
    };
    let done = async {
        sqlx::query("DELETE FROM invites WHERE id = ? AND org_id = ?")
            .bind(&input.id)
            .bind(&account.id)
            .execute(&state.db)
            .await?;
        record_seats(&state, &account.id).await
    }
    .await;
    if let Err(err) = done {
        tracing::error!(%err, "revoking an invite failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("{page}#team")).into_response()
}

/// Takes a member off the team and signs them out everywhere.
pub async fn remove(
    State(state): State<AppState>,
    account: Account,
    Path(domain): Path<String>,
    Form(input): Form<IdInput>,
) -> Response {
    let page = match owner_page(&state, &account, &domain).await {
        Ok(p) => p,
        Err(code) => return code.into_response(),
    };
    let done = async {
        let gone = sqlx::query("DELETE FROM members WHERE account_id = ? AND org_id = ?")
            .bind(&input.id)
            .bind(&account.id)
            .execute(&state.db)
            .await?;
        if gone.rows_affected() > 0 {
            sqlx::query("DELETE FROM sessions WHERE account_id = ?")
                .bind(&input.id)
                .execute(&state.db)
                .await?;
        }
        record_seats(&state, &account.id).await
    }
    .await;
    if let Err(err) = done {
        tracing::error!(%err, "removing a member failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Redirect::to(&format!("{page}#team")).into_response()
}

#[derive(Deserialize)]
pub struct TokenInput {
    token: String,
}

const EXPIRED: &str = "This invite has expired or was revoked. Ask for a new one.";

async fn pending(db: &SqlitePool, token: &str) -> Option<(String, String, String)> {
    sqlx::query_as("SELECT id, org_id, email FROM invites WHERE token_hash = ? AND expires_at > ?")
        .bind(hash_token(token))
        .bind(now())
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// Shows a join button, so email scanners that open links can't use the invite.
pub async fn confirm(State(state): State<AppState>, Query(input): Query<TokenInput>) -> Response {
    match pending(&state.db, &input.token).await {
        Some((_, _, email)) => auth::page(LoginPage {
            view: "invite",
            turnstile: None,
            email,
            token: input.token,
            error: None,
        }),
        None => auth::page(LoginPage {
            view: "enter",
            turnstile: None,
            email: String::new(),
            token: String::new(),
            error: Some(EXPIRED),
        }),
    }
}

/// Joins the team and signs in.
pub async fn accept(State(state): State<AppState>, Form(input): Form<TokenInput>) -> Response {
    let fail = |error: &'static str| {
        auth::page(LoginPage {
            view: "enter",
            turnstile: None,
            email: String::new(),
            token: String::new(),
            error: Some(error),
        })
    };
    let Some((id, org, email)) = pending(&state.db, &input.token).await else {
        return fail(EXPIRED);
    };
    let joined = async {
        let user = sites::find_or_create_account(&state.db, &email).await?;
        let (sites, member): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM sites WHERE account_id = ?1),
                    (SELECT count(*) FROM members WHERE account_id = ?1)",
        )
        .bind(&user)
        .fetch_one(&state.db)
        .await?;
        if user == org || sites > 0 || member > 0 {
            return Ok(None);
        }
        sqlx::query("INSERT INTO members (account_id, org_id, created_at) VALUES (?, ?, ?)")
            .bind(&user)
            .bind(&org)
            .bind(now())
            .execute(&state.db)
            .await?;
        sqlx::query("DELETE FROM invites WHERE id = ?")
            .bind(&id)
            .execute(&state.db)
            .await?;
        record_seats(&state, &org).await?;
        auth::start_session(&state, &user).await.map(Some)
    }
    .await;
    match joined {
        Ok(Some(res)) => res,
        Ok(None) => fail(
            "This email already has its own sites or team in Virling, so it can't join another team.",
        ),
        Err(err) => {
            tracing::error!(%err, "accepting an invite failed");
            fail("Something went wrong. Try again.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seats_follow_the_plan() {
        assert_eq!(next_seat(false, true, 1), "upgrade");
        assert_eq!(next_seat(false, false, 1), "upgrade");
        for hosted in [true, false] {
            assert_eq!(next_seat(true, hosted, 1), "free");
            assert_eq!(next_seat(true, hosted, 2), "free");
        }
        assert_eq!(next_seat(true, true, 3), "paid");
        assert_eq!(next_seat(true, true, 7), "paid");
        assert_eq!(next_seat(true, false, 3), "capped");
    }
}
