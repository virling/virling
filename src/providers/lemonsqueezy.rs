use super::Page;

pub async fn check_key(_client: &reqwest::Client, _key: &str) -> anyhow::Result<()> {
    anyhow::bail!("LemonSqueezy sync is not available yet")
}

pub async fn fetch(
    _client: &reqwest::Client,
    _key: &str,
    _cursor: Option<&str>,
) -> anyhow::Result<Page> {
    anyhow::bail!("LemonSqueezy sync is not available yet")
}
