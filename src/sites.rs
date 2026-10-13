use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use sqlx::SqlitePool;
use time::OffsetDateTime;

const MISS_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Site {
    pub id: String,
    pub account_id: String,
    pub domain: String,
    pub timezone: String,
    pub attribution_model: String,
    pub window_days: i64,
    pub cookieless: bool,
}

#[derive(Default)]
pub struct SiteCache {
    entries: RwLock<HashMap<String, (Option<Arc<Site>>, Instant)>>,
}

impl SiteCache {
    pub async fn by_domain(&self, db: &SqlitePool, domain: &str) -> Option<Arc<Site>> {
        if let Some((site, at)) = self.entries.read().unwrap().get(domain) {
            if site.is_some() || at.elapsed() < MISS_TTL {
                return site.clone();
            }
        }
        let site = sqlx::query_as::<_, Site>(
            "SELECT id, account_id, domain, timezone, attribution_model, window_days, cookieless FROM sites WHERE domain = ?",
        )
        .bind(domain)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .map(Arc::new);
        let mut entries = self.entries.write().unwrap();
        if entries.len() > 50_000 {
            entries.retain(|_, (s, _)| s.is_some());
        }
        entries.insert(domain.to_string(), (site.clone(), Instant::now()));
        site
    }

    pub fn invalidate(&self, domain: &str) {
        self.entries.write().unwrap().remove(domain);
    }
}

pub async fn for_account(db: &SqlitePool, account_id: &str) -> anyhow::Result<Vec<Site>> {
    Ok(sqlx::query_as::<_, Site>(
        "SELECT id, account_id, domain, timezone, attribution_model, window_days, cookieless FROM sites WHERE account_id = ? ORDER BY created_at",
    )
    .bind(account_id)
    .fetch_all(db)
    .await?)
}

pub fn normalize_domain(input: &str) -> Option<String> {
    let s = input.trim().to_ascii_lowercase();
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(&s);
    let s = s.split(['/', '?', '#']).next()?;
    let s = s.strip_prefix("www.").unwrap_or(s);
    let valid = s.contains('.')
        && s.len() <= 253
        && s.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
    valid.then(|| s.to_string())
}

pub async fn find_or_create_account(db: &SqlitePool, email: &str) -> anyhow::Result<String> {
    let now = OffsetDateTime::now_utc().unix_timestamp().to_string();
    sqlx::query("INSERT INTO accounts (id, email, created_at) VALUES (?, ?, ?) ON CONFLICT(email) DO NOTHING")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(email)
        .bind(&now)
        .execute(db)
        .await?;
    let id: String = sqlx::query_scalar("SELECT id FROM accounts WHERE email = ?")
        .bind(email)
        .fetch_one(db)
        .await?;
    Ok(id)
}

pub async fn create_site(
    db: &SqlitePool,
    account_id: &str,
    domain: &str,
    timezone: &str,
) -> anyhow::Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO sites (id, account_id, domain, timezone, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(account_id)
    .bind(domain)
    .bind(timezone)
    .bind(OffsetDateTime::now_utc().unix_timestamp().to_string())
    .execute(db)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::normalize_domain;

    #[test]
    fn domains_are_normalized() {
        assert_eq!(
            normalize_domain("https://www.Example.com/pricing").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            normalize_domain("app.example.io").as_deref(),
            Some("app.example.io")
        );
        assert_eq!(normalize_domain("localhost"), None);
        assert_eq!(normalize_domain("bad domain.com"), None);
    }
}
