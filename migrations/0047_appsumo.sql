-- AppSumo lifetime licenses from its webhooks and OAuth redemption; the linked account holds plan 'lifetime'.
CREATE TABLE appsumo_licenses (
    license_key TEXT PRIMARY KEY,
    tier INTEGER,
    status TEXT NOT NULL,
    account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
    prev_license_key TEXT,
    event_at INTEGER NOT NULL DEFAULT 0,
    claim_hash TEXT UNIQUE,
    claim_expires_at INTEGER,
    redeemed_at INTEGER
);
CREATE INDEX appsumo_licenses_account ON appsumo_licenses (account_id);
