-- AI answers: the user's own assistant API keys, buyer questions, and each day's answer per question and assistant.
CREATE TABLE ai_keys (
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    key_ciphertext BLOB NOT NULL,
    key_nonce BLOB NOT NULL,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (site_id, provider)
);
CREATE TABLE ai_settings (
    site_id TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE,
    questions TEXT NOT NULL DEFAULT '',
    names TEXT NOT NULL DEFAULT '',
    last_run_at INTEGER,
    run_day INTEGER NOT NULL DEFAULT 0,
    manual_runs INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE ai_answers (
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    day INTEGER NOT NULL,
    provider TEXT NOT NULL,
    question TEXT NOT NULL,
    ts INTEGER NOT NULL,
    mentioned INTEGER NOT NULL,
    position INTEGER,
    named INTEGER NOT NULL,
    cited TEXT NOT NULL DEFAULT '[]',
    answer TEXT NOT NULL,
    PRIMARY KEY (site_id, day, provider, question)
);
CREATE INDEX ai_answers_mentions ON ai_answers (site_id, provider, day, question, mentioned);
CREATE TABLE ai_cited (
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    day INTEGER NOT NULL,
    provider TEXT NOT NULL,
    question TEXT NOT NULL,
    page TEXT NOT NULL,
    PRIMARY KEY (site_id, day, provider, question, page)
) WITHOUT ROWID;
ALTER TABLE site_icons ADD COLUMN title TEXT;
