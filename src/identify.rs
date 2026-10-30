//! Visitors who identified with an email, kept only as a per-site salted hash, so payments without a visitor ID still find their visit.

use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use crate::events::Payment;

/// The email trimmed and lowercased, or None when it is not an address.
pub fn normalize(email: &str) -> Option<String> {
    let e = email.trim().to_lowercase();
    let (user, host) = e.split_once('@')?;
    (!user.is_empty() && host.contains('.') && e.len() <= 254 && !e.contains(char::is_whitespace))
        .then_some(e)
}

fn hash(salt: &str, email: &str) -> String {
    Sha256::digest(format!("{salt}:{email}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

async fn salt(db: &SqlitePool, site_id: &str) -> sqlx::Result<String> {
    sqlx::query("INSERT OR IGNORE INTO email_salts (site_id, salt) VALUES (?, ?)")
        .bind(site_id)
        .bind(crate::auth::random_token())
        .execute(db)
        .await?;
    sqlx::query_scalar("SELECT salt FROM email_salts WHERE site_id = ?")
        .bind(site_id)
        .fetch_one(db)
        .await
}

/// Remembers that `visitor_id` identified with `email` on the site; the latest visitor wins.
pub async fn save(
    db: &SqlitePool,
    site_id: &str,
    visitor_id: &str,
    email: &str,
) -> sqlx::Result<bool> {
    let Some(email) = normalize(email) else {
        return Ok(false);
    };
    let h = hash(&salt(db, site_id).await?, &email);
    sqlx::query(
        "INSERT INTO visitor_emails (site_id, email_hash, visitor_id, seen_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (site_id, email_hash) DO UPDATE SET visitor_id = excluded.visitor_id, seen_at = excluded.seen_at",
    )
    .bind(site_id)
    .bind(h)
    .bind(visitor_id)
    .bind(OffsetDateTime::now_utc().unix_timestamp())
    .execute(db)
    .await?;
    Ok(true)
}

/// Gives each payment without a visitor the visitor who identified with its payer's email on this site.
pub async fn fill(
    db: &SqlitePool,
    site_id: &str,
    payments: &mut [Payment],
    emails: &[Option<String>],
) -> sqlx::Result<()> {
    if !payments
        .iter()
        .zip(emails)
        .any(|(p, e)| p.visitor_id.is_none() && e.is_some())
    {
        return Ok(());
    }
    let salt = salt(db, site_id).await?;
    let known: HashMap<String, String> =
        sqlx::query_as("SELECT email_hash, visitor_id FROM visitor_emails WHERE site_id = ?")
            .bind(site_id)
            .fetch_all(db)
            .await?
            .into_iter()
            .collect();
    for (p, email) in payments.iter_mut().zip(emails) {
        if p.visitor_id.is_some() {
            continue;
        }
        let Some(email) = email.as_deref().and_then(normalize) else {
            continue;
        };
        if let Some(v) = known.get(&hash(&salt, &email)) {
            p.visitor_id = Some(v.clone());
            sqlx::query("INSERT OR IGNORE INTO email_matches (site_id, provider, provider_payment_id) VALUES (?, ?, ?)")
                .bind(site_id)
                .bind(&p.provider)
                .bind(&p.provider_payment_id)
                .execute(db)
                .await?;
        }
    }
    Ok(())
}

/// The site's payments credited through an email match, as (provider, payment ID).
pub async fn matched(db: &SqlitePool, site_id: &str) -> sqlx::Result<HashSet<(String, String)>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT provider, provider_payment_id FROM email_matches WHERE site_id = ?")
            .bind(site_id)
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sites;

    fn pay(id: &str, site: &str) -> Payment {
        Payment {
            site_id: site.into(),
            provider: "stripe".into(),
            provider_payment_id: id.into(),
            ts: OffsetDateTime::now_utc(),
            amount: 900,
            currency: "usd".into(),
            amount_usd: Some(9.0),
            refunded: 0,
            status: "paid".into(),
            visitor_id: None,
            customer_id: None,
            is_renewal: false,
        }
    }

    #[test]
    fn normalizes_addresses() {
        assert_eq!(
            normalize("  Ann@Example.COM ").as_deref(),
            Some("ann@example.com")
        );
        assert_eq!(normalize("ann"), None);
        assert_eq!(normalize("a b@x.com"), None);
    }

    #[tokio::test]
    async fn matches_by_email_on_the_same_site_only() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::open_sqlite(dir.path()).await.unwrap();
        let account = sites::find_or_create_account(&db, "me@example.com")
            .await
            .unwrap();
        let a = sites::create_site(&db, &account, "a.com", "UTC")
            .await
            .unwrap();
        let b = sites::create_site(&db, &account, "b.com", "UTC")
            .await
            .unwrap();
        assert!(save(&db, &a, "v1", "Ann@Example.com").await.unwrap());
        assert!(!save(&db, &a, "v2", "not an email").await.unwrap());
        let stored: Vec<String> = sqlx::query_scalar("SELECT email_hash FROM visitor_emails")
            .fetch_all(&db)
            .await
            .unwrap();
        assert!(
            stored.len() == 1 && !stored[0].contains("ann"),
            "only a hash is kept"
        );

        let mut on_a = vec![pay("pi_1", &a), pay("pi_2", &a)];
        let emails = [
            Some(" ann@example.com".to_string()),
            Some("bob@example.com".into()),
        ];
        fill(&db, &a, &mut on_a, &emails).await.unwrap();
        assert_eq!(on_a[0].visitor_id.as_deref(), Some("v1"));
        assert_eq!(on_a[1].visitor_id, None);
        let mut on_b = vec![pay("pi_3", &b)];
        fill(&db, &b, &mut on_b, &emails[..1]).await.unwrap();
        assert_eq!(on_b[0].visitor_id, None, "never across sites");
        let m = matched(&db, &a).await.unwrap();
        assert!(m.contains(&("stripe".into(), "pi_1".into())) && m.len() == 1);
        assert!(matched(&db, &b).await.unwrap().is_empty());
    }
}
