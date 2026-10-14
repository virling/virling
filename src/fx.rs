use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_secs(12 * 3600);
const RATES_URL: &str = "https://api.frankfurter.dev/v1/latest?base=USD";

/// ISO 4217 currencies with no minor unit; everything else is treated as 2 decimals.
const ZERO_DECIMAL: &[&str] = &[
    "BIF", "CLP", "DJF", "GNF", "JPY", "KMF", "KRW", "MGA", "PYG", "RWF", "UGX", "VND", "VUV",
    "XAF", "XOF", "XPF",
];

pub fn minor_to_major(amount_minor: i64, currency: &str) -> f64 {
    if ZERO_DECIMAL.contains(&currency.to_ascii_uppercase().as_str()) {
        amount_minor as f64
    } else {
        amount_minor as f64 / 100.0
    }
}

/// USD exchange rates from the ECB via frankfurter.dev, refreshed twice a day.
pub struct Rates {
    client: reqwest::Client,
    cache: Mutex<Option<(Instant, HashMap<String, f64>)>>,
}

#[derive(serde::Deserialize)]
struct Latest {
    rates: HashMap<String, f64>,
}

impl Rates {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            cache: Mutex::new(None),
        }
    }

    /// Converts minor units in `currency` to USD. None when the currency is unknown.
    pub async fn to_usd(&self, amount_minor: i64, currency: &str) -> anyhow::Result<Option<f64>> {
        let currency = currency.to_ascii_uppercase();
        let major = minor_to_major(amount_minor, &currency);
        if currency == "USD" {
            return Ok(Some(major));
        }
        let fresh = matches!(&*self.cache.lock().unwrap(), Some((at, _)) if at.elapsed() < REFRESH);
        if !fresh {
            let latest: Latest = self
                .client
                .get(RATES_URL)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            *self.cache.lock().unwrap() = Some((Instant::now(), latest.rates));
        }
        let cache = self.cache.lock().unwrap();
        Ok(cache
            .as_ref()
            .and_then(|(_, r)| r.get(&currency))
            .map(|rate| major / rate))
    }
}

#[cfg(test)]
mod tests {
    use super::minor_to_major;

    #[test]
    fn minor_units() {
        assert_eq!(minor_to_major(2900, "usd"), 29.0);
        assert_eq!(minor_to_major(2900, "JPY"), 2900.0);
    }
}
