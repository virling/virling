//! AI answers: asks the site's buyer questions to assistants with web search once a day, with the user's own keys.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{Value, json};
use sqlx::{Connection, SqlitePool};
use time::OffsetDateTime;
use url::Url;

use crate::app::AppState;
use crate::ingest::host_belongs;

const EVERY: Duration = Duration::from_secs(3600);
/// Runs a user may start by hand per site and UTC day.
pub const MANUAL_PER_DAY: i64 = 3;
pub const MIN_QUESTIONS: usize = 5;
pub const MAX_QUESTIONS: usize = 20;
/// Characters of each answer kept for the drill-down.
const KEEP_CHARS: usize = 4000;

/// Sites with a run in progress.
static RUNNING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assistant {
    ChatGpt,
    Claude,
    Perplexity,
    Gemini,
}

impl Assistant {
    pub const ALL: [Assistant; 4] = [
        Assistant::ChatGpt,
        Assistant::Claude,
        Assistant::Perplexity,
        Assistant::Gemini,
    ];

    /// Key of the provider the API key is for.
    pub fn as_str(self) -> &'static str {
        match self {
            Assistant::ChatGpt => "openai",
            Assistant::Claude => "anthropic",
            Assistant::Perplexity => "perplexity",
            Assistant::Gemini => "gemini",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.as_str() == s)
    }

    pub fn name(self) -> &'static str {
        match self {
            Assistant::ChatGpt => "ChatGPT",
            Assistant::Claude => "Claude",
            Assistant::Perplexity => "Perplexity",
            Assistant::Gemini => "Gemini",
        }
    }

    pub fn provider_name(self) -> &'static str {
        match self {
            Assistant::ChatGpt => "OpenAI",
            Assistant::Claude => "Anthropic",
            Assistant::Perplexity => "Perplexity",
            Assistant::Gemini => "Google Gemini",
        }
    }

    /// The company crawlers and referrers are grouped under on the AI card.
    pub fn company(self) -> &'static str {
        match self {
            Assistant::ChatGpt => "OpenAI",
            Assistant::Claude => "Anthropic",
            Assistant::Perplexity => "Perplexity",
            Assistant::Gemini => "Google",
        }
    }

    /// Name `icons::source` knows the logo by.
    pub fn logo(self) -> &'static str {
        match self {
            Assistant::ChatGpt => "chatgpt",
            Assistant::Claude => "claude",
            Assistant::Perplexity => "perplexity",
            Assistant::Gemini => "gemini",
        }
    }

    pub fn model(self) -> &'static str {
        match self {
            Assistant::ChatGpt => "gpt-6-luna",
            Assistant::Claude => "claude-haiku-5-5",
            Assistant::Perplexity => "fast preset",
            Assistant::Gemini => "gemini-2.5-flash-lite",
        }
    }

    /// Estimated USD per question with one web search, from the providers' list prices.
    pub fn cost(self) -> f64 {
        match self {
            Assistant::ChatGpt | Assistant::Claude => 0.011,
            Assistant::Perplexity => 0.002,
            Assistant::Gemini => 0.0002,
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Assistant::ChatGpt => "sk-proj-...",
            Assistant::Claude => "sk-ant-...",
            Assistant::Perplexity => "pplx-...",
            Assistant::Gemini => "AIza...",
        }
    }

    pub fn help_url(self) -> &'static str {
        match self {
            Assistant::ChatGpt => "https://platform.openai.com/api-keys",
            Assistant::Claude => "https://platform.claude.com/settings/keys",
            Assistant::Perplexity => "https://www.perplexity.ai/account/api/keys",
            Assistant::Gemini => "https://aistudio.google.com/apikey",
        }
    }

    pub fn base(self) -> &'static str {
        match self {
            Assistant::ChatGpt => "https://api.openai.com",
            Assistant::Claude => "https://api.anthropic.com",
            Assistant::Perplexity => "https://api.perplexity.ai",
            Assistant::Gemini => "https://generativelanguage.googleapis.com",
        }
    }

    /// Asks one question with web search on. Err carries the provider's message, never the key.
    pub async fn ask(
        self,
        http: &reqwest::Client,
        base: &str,
        key: &str,
        question: &str,
    ) -> anyhow::Result<Reply> {
        let req = match self {
            Assistant::ChatGpt => http
                .post(format!("{base}/v1/responses"))
                .bearer_auth(key)
                .json(&json!({
                    "model": self.model(),
                    "reasoning": { "effort": "none" },
                    "tools": [{ "type": "web_search", "search_context_size": "low" }],
                    "tool_choice": "required",
                    "max_output_tokens": 1200,
                    "input": question,
                })),
            Assistant::Claude => http
                .post(format!("{base}/v1/messages"))
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&json!({
                    "model": self.model(),
                    "max_tokens": 1200,
                    "thinking": { "type": "disabled" },
                    "messages": [{ "role": "user", "content": question }],
                    "tools": [{ "type": "web_search_20250305", "name": "web_search", "max_uses": 1 }],
                })),
            Assistant::Perplexity => http
                .post(format!("{base}/v1/agent"))
                .bearer_auth(key)
                .json(&json!({ "preset": "fast", "input": question })),
            Assistant::Gemini => http
                .post(format!(
                    "{base}/v1beta/models/{}:generateContent",
                    self.model()
                ))
                .header("x-goog-api-key", key)
                .json(&json!({
                    "contents": [{ "parts": [{ "text": question }] }],
                    "tools": [{ "google_search": {} }],
                })),
        };
        let res = req.timeout(Duration::from_secs(120)).send().await?;
        let status = res.status();
        let body: Value = res.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = body["error"]["message"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{} answered HTTP {status}", self.provider_name()));
            anyhow::bail!(msg);
        }
        Ok(self.reply(&body))
    }

    /// The answer text and every URL or site the provider says it used.
    pub fn reply(self, v: &Value) -> Reply {
        let mut r = Reply::default();
        let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
        match self {
            Assistant::ChatGpt | Assistant::Perplexity => {
                for item in list(&v["output"]) {
                    if item["type"] == "message" {
                        for c in list(&item["content"]) {
                            if c["type"] == "output_text" {
                                r.text.push_str(c["text"].as_str().unwrap_or_default());
                                for a in list(&c["annotations"]) {
                                    r.push(&a["url"]);
                                }
                            }
                        }
                    } else if item["type"] == "search_results" {
                        for s in list(&item["results"]) {
                            r.push(&s["url"]);
                        }
                    }
                }
            }
            Assistant::Claude => {
                for c in list(&v["content"]).into_iter().filter(|c| c["type"] == "text") {
                    r.text.push_str(c["text"].as_str().unwrap_or_default());
                    for cite in list(&c["citations"]) {
                        r.push(&cite["url"]);
                    }
                }
            }
            Assistant::Gemini => {
                let cand = &v["candidates"][0];
                for p in list(&cand["content"]["parts"]) {
                    r.text.push_str(p["text"].as_str().unwrap_or_default());
                }
                // Grounding URIs are redirects; the title is the cited site.
                for chunk in list(&cand["groundingMetadata"]["groundingChunks"]) {
                    r.push(&chunk["web"]["title"]);
                }
            }
        }
        r
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Reply {
    pub text: String,
    /// Cited URLs, or bare hosts when the provider gives no URL.
    pub urls: Vec<String>,
}

impl Reply {
    fn push(&mut self, v: &Value) {
        if let Some(u) = v.as_str().filter(|u| !u.is_empty())
            && !self.urls.iter().any(|x| x == u)
        {
            self.urls.push(u.to_string());
        }
    }
}

/// What one answer says about the site.
#[derive(Debug, PartialEq)]
pub struct Found {
    pub mentioned: bool,
    /// 1 when the site is the first product named.
    pub position: Option<i64>,
    /// Products and sites named, the site included.
    pub named: i64,
    /// The site's pages cited, as paths, or the bare domain when the provider names only the site.
    pub cited: Vec<String>,
}

/// The site's brand from its domain: "tallybird.app" is "Tallybird".
pub fn brand(domain: &str) -> String {
    let label = domain.trim_start_matches("www.").split('.').next().unwrap_or(domain);
    let mut c = label.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// Names that count as a mention: the saved ones, else the brand, plus the domain.
pub fn names(saved: &str, domain: &str) -> Vec<String> {
    let mut out: Vec<String> = saved
        .split([',', '\n'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if out.is_empty() {
        out.push(brand(domain));
    }
    if !out.iter().any(|n| n.eq_ignore_ascii_case(domain)) {
        out.push(domain.to_string());
    }
    out
}

/// Three starter questions from the homepage title, else from the domain.
pub fn default_questions(domain: &str, title: Option<&str>) -> Vec<String> {
    let brand = brand(domain);
    let topic = title.and_then(|t| {
        t.split([':', '|', '\u{2013}', '\u{2014}'])
            .flat_map(|p| p.split(" - "))
            .map(str::trim)
            .filter(|p| p.len() > 3 && !p.to_lowercase().contains(&brand.to_lowercase()))
            .max_by_key(|p| p.len())
            .map(|p| {
                let mut c = p.chars();
                let first = c.next().unwrap_or_default();
                let lower = c.clone().next().is_some_and(char::is_lowercase);
                let first = if lower {
                    first.to_lowercase().collect::<String>()
                } else {
                    first.to_string()
                };
                format!("{first}{}", c.as_str()).trim_end_matches('.').to_string()
            })
    });
    match topic {
        Some(t) => vec![
            format!("What is the best tool for {t}?"),
            format!("Which tools do you recommend for {t}, and why?"),
            format!("What are the cheapest good options for {t}?"),
        ],
        None => vec![
            format!("What are the best alternatives to {brand}?"),
            format!("Who are the main competitors of {domain}?"),
            format!("Is {brand} worth paying for?"),
        ],
    }
}

/// Up to 20 distinct questions, one per line.
pub fn clean_questions(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for q in input.lines().map(|l| l.trim().chars().take(300).collect::<String>()) {
        if !q.is_empty() && !out.contains(&q) && out.len() < MAX_QUESTIONS {
            out.push(q);
        }
    }
    out
}

/// USD a day for asking `questions` to each of `assistants` once.
pub fn daily_cost(questions: usize, assistants: &[Assistant]) -> f64 {
    questions as f64 * assistants.iter().map(|a| a.cost()).sum::<f64>()
}

/// Byte offset of `needle` in `hay` as a whole word, both lowercase.
fn find_word(hay: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let mut from = 0;
    while let Some(i) = hay[from..].find(needle) {
        let at = from + i;
        let end = at + needle.len();
        let before = hay[..at].chars().next_back();
        let after = hay[end..].chars().next();
        if !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric) {
            return Some(at);
        }
        from = at + hay[at..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// Lowercase letters and digits of a name's first label, to tell "Acme" and "acme.com" are one product.
fn product_key(name: &str) -> String {
    let n = name.to_lowercase();
    let n = n.trim_start_matches("www.");
    let n = if n.contains(' ') { n } else { n.split('.').next().unwrap_or(n) };
    n.chars().filter(|c| c.is_alphanumeric()).collect()
}

const NOT_PRODUCTS: &[&str] = &[
    "pros", "cons", "pricing", "price", "features", "key features", "summary", "conclusion",
    "note", "overview", "best for", "why", "bottom line", "verdict", "sources", "free", "paid",
    "tip", "tips", "option", "options", "recommendation", "recommendations", "alternatives",
    "example", "examples", "ease of use", "integrations", "support",
];

/// A list item, heading or bold phrase that reads like a product name.
fn product_name(s: &str) -> Option<String> {
    let s = s.trim().trim_start_matches("**").trim_start_matches('[');
    let end = ["**", ":", " - ", " \u{2013} ", " \u{2014} ", " (", ",", "|", "]", "\u{2014}"]
        .iter()
        .filter_map(|c| s.find(c))
        .min()
        .unwrap_or(s.len());
    let s = s[..end].trim().trim_end_matches(['.', '!', '?']);
    let words = s.split_whitespace().count();
    let first = s.chars().next()?;
    let lower = s.to_lowercase();
    let starts = ["best ", "top ", "how ", "what ", "why ", "if ", "for ", "when ", "the best"];
    ((1..=4).contains(&words)
        && (2..=40).contains(&s.len())
        && (first.is_uppercase() || first.is_ascii_digit())
        && !NOT_PRODUCTS.contains(&lower.as_str())
        && !starts.iter().any(|p| lower.starts_with(p)))
    .then(|| s.to_string())
}

/// Product names in an answer: list item and heading leads, bold phrases and domains.
fn products(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        let digits = l.chars().take_while(char::is_ascii_digit).count();
        let rest = if digits > 0 && l[digits..].starts_with(['.', ')']) {
            Some(&l[digits + 1..])
        } else if l.starts_with('#') {
            Some(l.trim_start_matches('#'))
        } else {
            ["- ", "* ", "\u{2022} "]
                .iter()
                .find_map(|m| l.strip_prefix(m))
        };
        if let Some(name) = rest.and_then(product_name) {
            out.push(name);
        }
    }
    for (i, part) in text.split("**").enumerate() {
        if i % 2 == 1
            && let Some(name) = product_name(part)
        {
            out.push(name);
        }
    }
    for token in text.split(|c: char| !(c.is_alphanumeric() || c == '.' || c == '-')) {
        let t = token.trim_matches(['.', '-']);
        let tld = t.rsplit('.').next().unwrap_or_default();
        if t.contains('.')
            && (2..=6).contains(&tld.len())
            && tld.chars().all(|c| c.is_ascii_lowercase())
            && t.split('.').all(|p| !p.is_empty())
        {
            out.push(t.trim_start_matches("www.").to_string());
        }
    }
    out
}

/// Whether the answer names the site, where among the products named, and which of its pages it cites.
pub fn analyze(reply: &Reply, names: &[String], domain: &str) -> Found {
    let lower = reply.text.to_lowercase();
    let keys: Vec<String> = names.iter().map(|n| product_key(n)).collect();
    let mention = names
        .iter()
        .filter_map(|n| find_word(&lower, &n.to_lowercase()))
        .min();
    let mut seen: Vec<(usize, String, bool)> = Vec::new();
    for name in products(&reply.text) {
        let key = product_key(&name);
        let n = name.to_lowercase();
        let ours = keys.contains(&key) || names.iter().any(|b| find_word(&n, &b.to_lowercase()).is_some());
        let Some(at) = find_word(&lower, &n) else {
            continue;
        };
        match seen.iter_mut().find(|s| s.1 == key || (ours && s.2)) {
            Some(s) => s.0 = s.0.min(at),
            None => seen.push((at, key, ours)),
        }
    }
    if let Some(at) = mention
        && !seen.iter().any(|s| s.2)
    {
        seen.push((at, String::new(), true));
    }
    seen.sort_by_key(|s| s.0);
    let position = seen.iter().position(|s| s.2).map(|i| i as i64 + 1);
    let mut cited: Vec<String> = Vec::new();
    let in_text = reply
        .text
        .split(|c: char| c.is_whitespace() || "()[]<>\"'".contains(c))
        .filter(|t| t.starts_with("http://") || t.starts_with("https://"))
        .map(|t| t.trim_end_matches(['.', ',', ';', ':']).to_string());
    for u in reply.urls.iter().cloned().chain(in_text) {
        let page = match Url::parse(&u) {
            Ok(url) => url
                .host_str()
                .filter(|h| host_belongs(h, domain))
                .map(|_| url.path().to_string()),
            Err(_) => {
                let host = u.trim().to_lowercase();
                host_belongs(&host, domain).then(|| host.trim_start_matches("www.").to_string())
            }
        };
        if let Some(p) = page
            && !cited.contains(&p)
            && cited.len() < 10
        {
            cited.push(p);
        }
    }
    Found {
        mentioned: mention.is_some(),
        position: mention.and(position),
        named: seen.len() as i64,
        cited,
    }
}

fn today() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp().div_euclid(86400)
}

/// Asks every due site's questions once a day, forever.
pub async fn run(state: AppState) {
    loop {
        if let Err(err) = run_due(&state).await {
            tracing::error!(%err, "AI answers round failed");
        }
        tokio::time::sleep(EVERY).await;
    }
}

async fn run_due(state: &AppState) -> anyhow::Result<()> {
    let due: Vec<String> = sqlx::query_scalar(
        "SELECT site_id FROM ai_settings s WHERE questions != '' AND run_day < ?
         AND EXISTS (SELECT 1 FROM ai_keys k WHERE k.site_id = s.site_id)",
    )
    .bind(today())
    .fetch_all(&state.db)
    .await?;
    for site in due {
        if claim(&state.db, &site, false).await? {
            run_site(state, &site, &|a| a.base().to_string()).await;
        }
    }
    Ok(())
}

/// Takes today's run for a site; a manual run fails past the daily limit.
pub async fn claim(db: &SqlitePool, site_id: &str, manual: bool) -> sqlx::Result<bool> {
    let manual = i64::from(manual);
    let done = sqlx::query(
        "UPDATE ai_settings SET manual_runs = CASE WHEN run_day = ?1 THEN manual_runs + ?2 ELSE ?2 END,
           run_day = ?1, last_run_at = ?3
         WHERE site_id = ?4 AND (run_day < ?1 OR (?2 = 1 AND manual_runs < ?5))",
    )
    .bind(today())
    .bind(manual)
    .bind(OffsetDateTime::now_utc().unix_timestamp())
    .bind(site_id)
    .bind(MANUAL_PER_DAY)
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Asks each question to each assistant with a key and stores today's answers; a failing key stops at its first error.
pub async fn run_site(state: &AppState, site_id: &str, base: &(dyn Fn(Assistant) -> String + Sync)) {
    if !RUNNING
        .lock()
        .unwrap()
        .get_or_insert_default()
        .insert(site_id.to_string())
    {
        return;
    }
    if let Err(err) = ask_all(state, site_id, base).await {
        tracing::error!(%err, site = %site_id, "AI answers run failed");
    }
    if let Some(set) = RUNNING.lock().unwrap().as_mut() {
        set.remove(site_id);
    }
}

type KeyRow = (String, Vec<u8>, Vec<u8>);

async fn ask_all(
    state: &AppState,
    site_id: &str,
    base: &(dyn Fn(Assistant) -> String + Sync),
) -> anyhow::Result<()> {
    let (questions, saved, domain): (String, String, String) = sqlx::query_as(
        "SELECT a.questions, a.names, s.domain FROM ai_settings a JOIN sites s ON s.id = a.site_id WHERE a.site_id = ?",
    )
    .bind(site_id)
    .fetch_one(&state.db)
    .await?;
    let keys: Vec<KeyRow> = sqlx::query_as(
        "SELECT provider, key_ciphertext, key_nonce FROM ai_keys WHERE site_id = ? ORDER BY provider",
    )
    .bind(site_id)
    .fetch_all(&state.db)
    .await?;
    let names = names(&saved, &domain);
    let day = today();
    for (provider, ciphertext, nonce) in keys {
        let Some(a) = Assistant::parse(&provider) else {
            continue;
        };
        let key = state.vault.decrypt(&ciphertext, &nonce)?;
        let mut error = None;
        for q in clean_questions(&questions) {
            match a.ask(&state.http, &base(a), &key, &q).await {
                Ok(reply) => {
                    let found = analyze(&reply, &names, &domain);
                    let mut conn = state.db.acquire().await?;
                    save(&mut conn, site_id, day, a, &q, &found, &reply.text).await?;
                }
                Err(err) => {
                    tracing::warn!(provider = %provider, site = %site_id, %err, "AI answer failed");
                    error = Some(err.to_string());
                    break;
                }
            }
        }
        sqlx::query("UPDATE ai_keys SET last_error = ? WHERE site_id = ? AND provider = ?")
            .bind(error)
            .bind(site_id)
            .bind(&provider)
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

/// Stores one answer, replacing an earlier one for the same day, assistant and question.
pub async fn save(
    db: &mut sqlx::SqliteConnection,
    site_id: &str,
    day: i64,
    a: Assistant,
    question: &str,
    found: &Found,
    text: &str,
) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT OR REPLACE INTO ai_answers (site_id, day, provider, question, ts, mentioned, position, named, cited, answer)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(site_id)
    .bind(day)
    .bind(a.as_str())
    .bind(question)
    .bind(OffsetDateTime::now_utc().unix_timestamp())
    .bind(found.mentioned)
    .bind(found.position)
    .bind(found.named)
    .bind(serde_json::to_string(&found.cited).unwrap_or_default())
    .bind(text.chars().take(KEEP_CHARS).collect::<String>())
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM ai_cited WHERE site_id = ? AND day = ? AND provider = ? AND question = ?")
        .bind(site_id)
        .bind(day)
        .bind(a.as_str())
        .bind(question)
        .execute(&mut *tx)
        .await?;
    for page in &found.cited {
        sqlx::query("INSERT INTO ai_cited (site_id, day, provider, question, page) VALUES (?, ?, ?, ?, ?)")
            .bind(site_id)
            .bind(day)
            .bind(a.as_str())
            .bind(question)
            .bind(page)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

/// Encrypts and stores an assistant key, replacing an older one and its error.
pub async fn save_key(state: &AppState, site_id: &str, a: Assistant, key: &str) -> anyhow::Result<()> {
    let (ciphertext, nonce) = state.vault.encrypt(key)?;
    sqlx::query(
        "INSERT INTO ai_keys (site_id, provider, key_ciphertext, key_nonce, created_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (site_id, provider) DO UPDATE SET key_ciphertext = excluded.key_ciphertext,
           key_nonce = excluded.key_nonce, last_error = NULL",
    )
    .bind(site_id)
    .bind(a.as_str())
    .bind(ciphertext)
    .bind(nonce)
    .bind(OffsetDateTime::now_utc().unix_timestamp())
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Saves the questions and names to look for.
pub async fn save_settings(db: &SqlitePool, site_id: &str, questions: &[String], names: &str) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ai_settings (site_id, questions, names) VALUES (?, ?, ?)
         ON CONFLICT (site_id) DO UPDATE SET questions = excluded.questions, names = excluded.names",
    )
    .bind(site_id)
    .bind(questions.join("\n"))
    .bind(names.trim())
    .execute(db)
    .await?;
    Ok(())
}

/// Answers and mentions per assistant between two UTC days, inclusive.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Tally {
    pub answers: i64,
    pub mentioned: i64,
}

pub async fn tallies(
    db: &SqlitePool,
    site_id: &str,
    from_day: i64,
    to_day: i64,
) -> sqlx::Result<Vec<(String, Tally)>> {
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT provider, COUNT(*), SUM(mentioned) FROM ai_answers
         WHERE site_id = ? AND day BETWEEN ? AND ? GROUP BY provider",
    )
    .bind(site_id)
    .bind(from_day)
    .bind(to_day)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(p, answers, mentioned)| (p, Tally { answers, mentioned }))
        .collect())
}

/// Each assistant's cited pages of the site with how many answers cited them, most first.
pub async fn cited(
    db: &SqlitePool,
    site_id: &str,
    from_day: i64,
    to_day: i64,
) -> sqlx::Result<Vec<(String, String, i64)>> {
    sqlx::query_as(
        "SELECT provider, page, COUNT(*) FROM ai_cited WHERE site_id = ? AND day BETWEEN ? AND ?
         GROUP BY provider, page ORDER BY 3 DESC, page",
    )
    .bind(site_id)
    .bind(from_day)
    .bind(to_day)
    .fetch_all(db)
    .await
}

/// Assistants with a saved key.
pub async fn keyed(db: &SqlitePool, site_id: &str) -> sqlx::Result<Vec<Assistant>> {
    let rows: Vec<String> = sqlx::query_scalar("SELECT provider FROM ai_keys WHERE site_id = ?")
        .bind(site_id)
        .fetch_all(db)
        .await?;
    Ok(Assistant::ALL
        .into_iter()
        .filter(|a| rows.iter().any(|r| r == a.as_str()))
        .collect())
}

/// One question's latest answer in a range, with how often it named the site.
#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Latest {
    pub question: String,
    /// Answers in the range and how many named the site.
    pub answers: i64,
    pub mentioned: i64,
    /// UTC day of the latest answer, days since 1970.
    #[serde(skip)]
    pub day: i64,
    pub latest_mentioned: bool,
    pub position: Option<i64>,
    pub named: i64,
    #[serde(skip)]
    pub cited: String,
    pub answer: String,
}

pub async fn latest(
    db: &SqlitePool,
    site_id: &str,
    a: Assistant,
    from_day: i64,
    to_day: i64,
) -> sqlx::Result<Vec<Latest>> {
    sqlx::query_as(
        "SELECT x.question, x.answers, x.mentioned, x.day, a.mentioned AS latest_mentioned, a.position, a.named, a.cited, a.answer
         FROM (SELECT question, COUNT(*) AS answers, SUM(mentioned) AS mentioned, MAX(day) AS day FROM ai_answers
               WHERE site_id = ?1 AND provider = ?2 AND day BETWEEN ?3 AND ?4 GROUP BY question) x
         JOIN ai_answers a ON a.site_id = ?1 AND a.day = x.day AND a.provider = ?2 AND a.question = x.question
         ORDER BY x.mentioned DESC, x.question",
    )
    .bind(site_id)
    .bind(a.as_str())
    .bind(from_day)
    .bind(to_day)
    .fetch_all(db)
    .await
}

impl Latest {
    pub fn cited_pages(&self) -> Vec<String> {
        serde_json::from_str(&self.cited).unwrap_or_default()
    }
}

/// `text` split into (piece, is a mention) for highlighting; markdown bold and heading marks dropped.
pub fn highlight(text: &str, names: &[String]) -> Vec<(String, bool)> {
    let plain: String = text
        .replace("**", "")
        .lines()
        .map(|l| l.trim_start_matches('#').trim_start())
        .collect::<Vec<_>>()
        .join("\n");
    let lower = plain.to_lowercase();
    let mut hits: Vec<(usize, usize)> = Vec::new();
    for n in names.iter().map(|n| n.to_lowercase()) {
        let mut from = 0;
        while let Some(at) = find_word(&lower[from..], &n).map(|i| from + i) {
            hits.push((at, at + n.len()));
            from = at + n.len();
        }
    }
    hits.sort_by_key(|h| (h.0, std::cmp::Reverse(h.1)));
    let mut out = Vec::new();
    let mut at = 0;
    for (s, e) in hits {
        // Lowercasing can change byte lengths; skip any match that no longer lines up.
        if s < at || !plain.is_char_boundary(s) || !plain.is_char_boundary(e) || e > plain.len() {
            continue;
        }
        if s > at {
            out.push((plain[at..s].to_string(), false));
        }
        out.push((plain[s..e].to_string(), true));
        at = e;
    }
    if at < plain.len() {
        out.push((plain[at..].to_string(), false));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOMAIN: &str = "tallybird.app";

    fn fixture(name: &str) -> Value {
        let path = format!("{}/src/answers/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn check(a: Assistant, file: &str) -> Found {
        let reply = a.reply(&fixture(file));
        assert!(reply.text.len() > 100, "{file} has text");
        analyze(&reply, &names("", DOMAIN), DOMAIN)
    }

    #[test]
    fn openai_answers_are_read() {
        let f = check(Assistant::ChatGpt, "openai");
        assert!(f.mentioned);
        assert_eq!(f.position, Some(3));
        assert_eq!(f.named, 5);
        assert_eq!(f.cited, ["/pricing"]);
    }

    #[test]
    fn anthropic_answers_are_read() {
        let f = check(Assistant::Claude, "anthropic");
        assert!(f.mentioned);
        assert_eq!(f.position, Some(1));
        assert_eq!(f.named, 4);
        assert_eq!(f.cited, ["/blog/net-30-explained", "/"]);
    }

    #[test]
    fn perplexity_answers_are_read() {
        let f = check(Assistant::Perplexity, "perplexity");
        assert!(!f.mentioned);
        assert_eq!(f.position, None);
        assert_eq!(f.named, 4);
        assert_eq!(f.cited, ["/compare"], "a source can be cited without a mention");
    }

    #[test]
    fn gemini_answers_are_read() {
        let f = check(Assistant::Gemini, "gemini");
        assert!(f.mentioned);
        assert_eq!(f.position, Some(2));
        assert_eq!(f.named, 4);
        assert_eq!(f.cited, ["tallybird.app"], "Gemini only names the site");
    }

    #[test]
    fn mentions_are_whole_words() {
        let r = |t: &str| Reply {
            text: t.into(),
            urls: Vec::new(),
        };
        let n = names("Tallybird, Tally Bird", DOMAIN);
        assert!(!analyze(&r("Try tallybirds or Tallybirdy."), &n, DOMAIN).mentioned);
        assert!(analyze(&r("Tally Bird is fine."), &n, DOMAIN).mentioned);
        assert!(analyze(&r("See tallybird.app/pricing."), &n, DOMAIN).mentioned);
        let pieces = highlight("**Tallybird** beats tallybird.app?", &n);
        assert_eq!(
            pieces,
            [
                ("Tallybird".to_string(), true),
                (" beats ".to_string(), false),
                ("tallybird.app".to_string(), true),
                ("?".to_string(), false)
            ]
        );
    }

    #[test]
    fn questions_and_costs() {
        assert_eq!(brand("www.tallybird.app"), "Tallybird");
        assert_eq!(
            default_questions(DOMAIN, Some("Tallybird | Invoicing for freelancers")),
            [
                "What is the best tool for invoicing for freelancers?",
                "Which tools do you recommend for invoicing for freelancers, and why?",
                "What are the cheapest good options for invoicing for freelancers?"
            ]
        );
        assert_eq!(
            default_questions(DOMAIN, None)[0],
            "What are the best alternatives to Tallybird?"
        );
        let q = clean_questions(" a \n\nb\na\n");
        assert_eq!(q, ["a", "b"]);
        let many: String = (0..30).map(|i| format!("q{i}\n")).collect();
        assert_eq!(clean_questions(&many).len(), MAX_QUESTIONS);
        let cost = daily_cost(20, &Assistant::ALL);
        assert!((cost - 0.484).abs() < 1e-9, "{cost}");
    }

    /// Serves the recorded replies; the Perplexity key is refused.
    async fn mock() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use axum::http::{HeaderMap, StatusCode};
        use axum::routing::post;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let reply = |file: &'static str, header: &'static str, want: &'static str| {
            let calls = calls.clone();
            post(move |h: HeaderMap| async move {
                calls.fetch_add(1, Ordering::Relaxed);
                if h.get(header).and_then(|v| v.to_str().ok()) == Some(want) {
                    (StatusCode::OK, axum::Json(fixture(file)))
                } else {
                    (
                        StatusCode::UNAUTHORIZED,
                        axum::Json(json!({ "error": { "message": "Invalid API key" } })),
                    )
                }
            })
        };
        let app = axum::Router::new()
            .route("/v1/responses", reply("openai", "authorization", "Bearer sk-test-openai"))
            .route("/v1/messages", reply("anthropic", "x-api-key", "sk-ant-test"))
            .route("/v1/agent", reply("perplexity", "authorization", "Bearer pplx-right"))
            .route("/v1beta/models/{model}", reply("gemini", "x-goog-api-key", "AIza-test"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), calls)
    }

    #[tokio::test]
    async fn the_daily_run_stores_answers_from_every_keyed_assistant() {
        let f = crate::api::tests::fixture().await;
        let (state, db) = (&f.state, &f.state.db);
        let site: String = sqlx::query_scalar("SELECT id FROM sites WHERE domain = 'example.com'")
            .fetch_one(db)
            .await
            .unwrap();
        let questions: Vec<String> = (1..=5).map(|i| format!("Question {i}?")).collect();
        save_settings(db, &site, &questions, "Tallybird").await.unwrap();
        for (a, key) in [
            (Assistant::ChatGpt, "sk-test-openai"),
            (Assistant::Claude, "sk-ant-test"),
            (Assistant::Perplexity, "pplx-wrong"),
            (Assistant::Gemini, "AIza-test"),
        ] {
            save_key(state, &site, a, key).await.unwrap();
        }
        let stored: Vec<u8> =
            sqlx::query_scalar("SELECT key_ciphertext FROM ai_keys WHERE provider = 'openai'")
                .fetch_one(db)
                .await
                .unwrap();
        assert!(!String::from_utf8_lossy(&stored).contains("sk-test"), "keys are encrypted");

        let (base, calls) = mock().await;
        assert!(claim(db, &site, false).await.unwrap());
        assert!(!claim(db, &site, false).await.unwrap(), "once a day");
        run_site(state, &site, &|_| base.clone()).await;
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::Relaxed),
            16,
            "a refused key stops at once"
        );

        let today = today();
        let t = tallies(db, &site, today, today).await.unwrap();
        let get = |p: &str| t.iter().find(|r| r.0 == p).map(|r| r.1.clone());
        let all = Some(Tally { answers: 5, mentioned: 5 });
        assert_eq!(get("openai"), all);
        assert_eq!(get("anthropic"), all);
        assert_eq!(get("gemini"), all);
        assert_eq!(get("perplexity"), None);
        let errors: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT provider, last_error FROM ai_keys ORDER BY provider")
                .fetch_all(db)
                .await
                .unwrap();
        assert_eq!(
            errors[3],
            ("perplexity".into(), Some("Invalid API key".into()))
        );
        assert!(errors.iter().filter(|e| e.0 != "perplexity").all(|e| e.1.is_none()));

        let rows = latest(db, &site, Assistant::ChatGpt, today - 29, today)
            .await
            .unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(
            (rows[0].answers, rows[0].mentioned, rows[0].position),
            (1, 1, Some(3))
        );
        assert!(rows[0].answer.starts_with("Here are strong invoicing tools"));

        run_site(state, &site, &|_| base.clone()).await;
        let t = tallies(db, &site, today, today).await.unwrap();
        assert_eq!(
            t.iter().map(|r| r.1.answers).sum::<i64>(),
            15,
            "a rerun replaces the day's answers"
        );
        for _ in 0..MANUAL_PER_DAY {
            assert!(claim(db, &site, true).await.unwrap());
        }
        assert!(!claim(db, &site, true).await.unwrap(), "manual runs are limited");
    }
}
