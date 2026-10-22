use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use time::OffsetDateTime;

use crate::app::AppState;
use crate::events::Payment;
use crate::providers::{Fetched, Provider, lemonsqueezy};

const INTERVAL: Duration = Duration::from_secs(60);
const MAX_PAGES_PER_RUN: usize = 50;
/// Subscription status changes slowly, so it is read at most this often per integration.
const SUBSCRIPTIONS_EVERY: i64 = 3600;

/// When each integration's subscriptions were last read, by integration id.
static SUBSCRIPTIONS_READ: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

#[derive(sqlx::FromRow)]
struct Integration {
    id: String,
    site_id: String,
    provider: String,
    key_ciphertext: Vec<u8>,
    key_nonce: Vec<u8>,
    sync_cursor: Option<String>,
}

pub async fn add_integration(
    state: &AppState,
    site_id: &str,
    provider: Provider,
    key: &str,
) -> anyhow::Result<()> {
    provider.check_key(&state.http, key).await?;
    let (ciphertext, nonce) = state.vault.encrypt(key)?;
    // LemonSqueezy sends checkout custom data only in webhooks, which need a public https address.
    let hook =
        if provider == Provider::LemonSqueezy && state.config.base_url.starts_with("https://") {
            let secret = crate::auth::random_token();
            let url = format!(
                "{}/api/webhooks/lemonsqueezy/{site_id}",
                state.config.base_url
            );
            lemonsqueezy::register_webhook(&state.http, key, &url, &secret).await?;
            Some(state.vault.encrypt(&secret)?)
        } else {
            None
        };
    let (hook_ciphertext, hook_nonce) = hook.unzip();
    sqlx::query(
        "INSERT INTO integrations (id, site_id, provider, key_ciphertext, key_nonce, created_at, webhook_ciphertext, webhook_nonce) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (site_id, provider) DO UPDATE SET key_ciphertext = excluded.key_ciphertext,
           key_nonce = excluded.key_nonce, sync_cursor = NULL, last_error = NULL,
           webhook_ciphertext = excluded.webhook_ciphertext, webhook_nonce = excluded.webhook_nonce",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(site_id)
    .bind(provider.as_str())
    .bind(ciphertext)
    .bind(nonce)
    .bind(OffsetDateTime::now_utc().unix_timestamp())
    .bind(hook_ciphertext)
    .bind(hook_nonce)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Receives a LemonSqueezy order or subscription invoice with its visitor ID, signed with the site's webhook secret.
pub async fn lemonsqueezy_webhook(
    State(state): State<AppState>,
    Path(site_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let hook: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
        "SELECT webhook_ciphertext, webhook_nonce FROM integrations
         WHERE site_id = ? AND provider = 'lemonsqueezy' AND webhook_ciphertext IS NOT NULL",
    )
    .bind(&site_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let Some(secret) = hook.and_then(|(c, n)| state.vault.decrypt(&c, &n).ok()) else {
        return StatusCode::NOT_FOUND;
    };
    let sent = headers
        .get("x-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let want = lemonsqueezy::signature(&secret, &body);
    let same = sent.len() == want.len()
        && sent
            .bytes()
            .zip(want.bytes())
            .fold(0, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if !same {
        return StatusCode::UNAUTHORIZED;
    }
    let payment = match lemonsqueezy::webhook_payment(&body) {
        Ok(Some(p)) => p,
        Ok(None) => return StatusCode::NO_CONTENT,
        Err(err) => {
            tracing::warn!(%err, site = %site_id, "unreadable LemonSqueezy webhook");
            return StatusCode::BAD_REQUEST;
        }
    };
    if let Err(err) = store(&state, &site_id, Provider::LemonSqueezy, vec![payment]).await {
        tracing::error!(%err, site = %site_id, "storing a LemonSqueezy webhook payment failed");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    rebuild_attribution(&state, Some(&site_id)).await;
    StatusCode::NO_CONTENT
}

/// Converts fetched payments to USD and stores them for the site.
async fn store(
    state: &AppState,
    site_id: &str,
    provider: Provider,
    fetched: Vec<Fetched>,
) -> anyhow::Result<()> {
    let mut payments = Vec::with_capacity(fetched.len());
    for f in fetched {
        let amount_usd = state
            .rates
            .to_usd(f.amount, &f.currency)
            .await
            .unwrap_or(None);
        payments.push(Payment {
            site_id: site_id.to_string(),
            provider: provider.as_str().to_string(),
            provider_payment_id: f.provider_payment_id,
            ts: f.ts,
            amount: f.amount,
            currency: f.currency.to_ascii_lowercase(),
            amount_usd,
            refunded: f.refunded,
            status: f.status,
            visitor_id: f.visitor_id,
            customer_id: f.customer_id,
            is_renewal: f.is_renewal,
        });
    }
    if !payments.is_empty() {
        let store = state.events.clone();
        tokio::task::spawn_blocking(move || store.upsert_payments(&payments)).await??;
    }
    Ok(())
}

/// Polls every connected provider once a minute, forever.
pub async fn run(state: AppState) {
    rebuild_attribution(&state, None).await;
    loop {
        if let Err(err) = sync_all(&state).await {
            tracing::error!(%err, "payment sync round failed");
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

pub async fn sync_all(state: &AppState) -> anyhow::Result<()> {
    sync_integrations(state, None).await?;
    rebuild_attribution(state, None).await;
    Ok(())
}

/// Syncs one site's integrations right away, e.g. to start the backfill after connecting.
pub async fn sync_site(state: &AppState, site_id: &str) -> anyhow::Result<()> {
    sync_integrations(state, Some(site_id)).await?;
    rebuild_attribution(state, Some(site_id)).await;
    Ok(())
}

async fn sync_integrations(state: &AppState, site_id: Option<&str>) -> anyhow::Result<()> {
    let integrations: Vec<Integration> = sqlx::query_as(
        "SELECT id, site_id, provider, key_ciphertext, key_nonce, sync_cursor FROM integrations
         WHERE ?1 IS NULL OR site_id = ?1",
    )
    .bind(site_id)
    .fetch_all(&state.db)
    .await?;
    for integration in integrations {
        let mut result = sync_one(state, &integration).await;
        if result.is_ok() {
            result = sync_subscriptions(state, &integration).await;
        }
        let error = result.as_ref().err().map(|e| e.to_string());
        if let Some(err) = &error {
            tracing::warn!(provider = %integration.provider, site = %integration.site_id, %err, "sync failed");
        }
        sqlx::query("UPDATE integrations SET last_synced_at = ?, last_error = ? WHERE id = ?")
            .bind(OffsetDateTime::now_utc().unix_timestamp())
            .bind(error)
            .bind(&integration.id)
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

/// Recomputes attribution for one site, or every site, with its model and window from SQLite.
pub async fn rebuild_attribution(state: &AppState, site_id: Option<&str>) {
    let sites: Vec<(String, String, i64)> = match sqlx::query_as(
        "SELECT id, attribution_model, window_days FROM sites WHERE ?1 IS NULL OR id = ?1",
    )
    .bind(site_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(sites) => sites,
        Err(err) => {
            tracing::error!(%err, "loading sites for attribution failed");
            return;
        }
    };
    for (site_id, model, window_days) in sites {
        let store = state.events.clone();
        let id = site_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            store.rebuild_attribution(&id, &model, window_days)
        })
        .await;
        if !matches!(result, Ok(Ok(()))) {
            tracing::error!(site = %site_id, ?result, "attribution rebuild failed");
        }
    }
}

/// Stores the provider's subscription states once an hour; a key without access keeps the payment-recency rule.
async fn sync_subscriptions(state: &AppState, integration: &Integration) -> anyhow::Result<()> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let due = SUBSCRIPTIONS_READ
        .lock()
        .unwrap()
        .get_or_insert_default()
        .get(&integration.id)
        .is_none_or(|t| now - t >= SUBSCRIPTIONS_EVERY);
    if !due {
        return Ok(());
    }
    let provider = Provider::parse(&integration.provider)
        .ok_or_else(|| anyhow::anyhow!("unknown provider {}", integration.provider))?;
    let key = state
        .vault
        .decrypt(&integration.key_ciphertext, &integration.key_nonce)?;
    let subscriptions = provider.subscriptions(&state.http, &key).await?;
    SUBSCRIPTIONS_READ
        .lock()
        .unwrap()
        .get_or_insert_default()
        .insert(integration.id.clone(), now);
    let store = state.events.clone();
    let (site, name) = (integration.site_id.clone(), provider.as_str());
    let mut subscriptions = subscriptions.unwrap_or_default();
    for s in &mut subscriptions {
        if let (Some(monthly), Some(currency)) = (s.monthly(), s.currency.as_deref()) {
            s.mrr_usd = state
                .rates
                .to_usd(monthly.round() as i64, currency)
                .await
                .unwrap_or(None);
        }
    }
    tokio::task::spawn_blocking(move || store.replace_subscriptions(&site, name, &subscriptions))
        .await??;
    Ok(())
}

async fn sync_one(state: &AppState, integration: &Integration) -> anyhow::Result<()> {
    let provider = Provider::parse(&integration.provider)
        .ok_or_else(|| anyhow::anyhow!("unknown provider {}", integration.provider))?;
    let key = state
        .vault
        .decrypt(&integration.key_ciphertext, &integration.key_nonce)?;
    let mut cursor = integration.sync_cursor.clone();
    for _ in 0..MAX_PAGES_PER_RUN {
        let page = provider.fetch(&state.http, &key, cursor.as_deref()).await?;
        store(state, &integration.site_id, provider, page.payments).await?;
        if page.cursor.is_some() {
            cursor = page.cursor;
            sqlx::query("UPDATE integrations SET sync_cursor = ? WHERE id = ?")
                .bind(&cursor)
                .bind(&integration.id)
                .execute(&state.db)
                .await?;
        }
        if !page.more {
            break;
        }
    }
    Ok(())
}
