CREATE TABLE integrations (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    key_ciphertext BLOB NOT NULL,
    key_nonce BLOB NOT NULL,
    sync_cursor TEXT,
    last_synced_at INTEGER,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (site_id, provider)
);
