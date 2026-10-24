-- Telegram alerts: a bot token (encrypted) and chat per site, and the newest payment already sent.
CREATE TABLE telegram_alerts (
    site_id TEXT PRIMARY KEY,
    token_ciphertext BLOB NOT NULL,
    token_nonce BLOB NOT NULL,
    chat_id TEXT NOT NULL,
    last_error TEXT,
    sent_until INTEGER NOT NULL
);
