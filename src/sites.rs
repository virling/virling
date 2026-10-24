use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use sqlx::SqlitePool;
use time::OffsetDateTime;

const MISS_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Site {
    pub id: String,
    pub domain: String,
    pub timezone: String,
    pub cookieless: bool,
    /// Other domains of the same visit, space-separated.
    pub same_visit: String,
    /// Minutes away after which a return is a new visit.
    pub return_minutes: i64,
    /// Whether the site holds generated sample data.
    pub sample_data: bool,
}

type CacheEntry = (Option<Arc<Site>>, Instant);

#[derive(Default)]
pub struct SiteCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
}

impl SiteCache {
    pub async fn by_domain(&self, db: &SqlitePool, domain: &str) -> Option<Arc<Site>> {
        if let Some((site, at)) = self.entries.read().unwrap().get(domain)
            && (site.is_some() || at.elapsed() < MISS_TTL)
        {
            return site.clone();
        }
        let site = sqlx::query_as::<_, Site>(
            "SELECT id, domain, timezone, cookieless, same_visit, return_minutes, sample_data FROM sites WHERE domain = ?",
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

/// App URL of one of a site's pages; the dashboard is the site root.
pub fn app_path(domain: &str, page: &str) -> String {
    match page {
        "" | "dashboard" => format!("/app/{domain}"),
        _ => format!("/app/{domain}/{page}"),
    }
}

pub async fn for_account(db: &SqlitePool, account_id: &str) -> anyhow::Result<Vec<Site>> {
    Ok(sqlx::query_as::<_, Site>(
        "SELECT id, domain, timezone, cookieless, same_visit, return_minutes, sample_data FROM sites WHERE account_id = ? ORDER BY created_at",
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
    use super::{app_path, normalize_domain};

    #[test]
    fn app_paths_use_the_domain() {
        assert_eq!(app_path("virling.com", "dashboard"), "/app/virling.com");
        assert_eq!(app_path("virling.com", ""), "/app/virling.com");
        assert_eq!(
            app_path("virling.com", "payments"),
            "/app/virling.com/payments"
        );
    }

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
