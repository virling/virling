use super::Page;

pub async fn check_key(_client: &reqwest::Client, _key: &str) -> anyhow::Result<()> {
    anyhow::bail!("Dodo sync is not available yet")
}

pub async fn fetch(
    _client: &reqwest::Client,
    _key: &str,
    _cursor: Option<&str>,
) -> anyhow::Result<Page> {
    anyhow::bail!("Dodo sync is not available yet")
}
