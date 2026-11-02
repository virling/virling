-- One webhook per site: its URL and signing secret (both encrypted), and what was already sent.
CREATE TABLE webhooks (
    site_id TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE,
    url_ciphertext BLOB NOT NULL,
    url_nonce BLOB NOT NULL,
    secret_ciphertext BLOB NOT NULL,
    secret_nonce BLOB NOT NULL,
    host TEXT NOT NULL,
    last_error TEXT,
    sent_until INTEGER NOT NULL,
    goals_until INTEGER NOT NULL,
    spike_day INTEGER NOT NULL DEFAULT 0
);
