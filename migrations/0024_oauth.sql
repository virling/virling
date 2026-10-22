-- OAuth sign-in for MCP connectors: registered apps, one-time codes and the access each owner allowed.
CREATE TABLE oauth_clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    redirect_uris TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE oauth_codes (
    code_hash TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth_clients(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    redirect_uri TEXT NOT NULL,
    challenge TEXT NOT NULL,
    resource TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE oauth_grants (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    client_id TEXT NOT NULL REFERENCES oauth_clients(id) ON DELETE CASCADE,
    resource TEXT NOT NULL,
    access_hash TEXT NOT NULL UNIQUE,
    access_expires_at INTEGER NOT NULL,
    refresh_hash TEXT NOT NULL UNIQUE,
    refresh_expires_at INTEGER NOT NULL,
    old_refresh_hash TEXT,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    revoked_at INTEGER
);
CREATE INDEX oauth_grants_account ON oauth_grants (account_id, created_at);
CREATE INDEX oauth_grants_old_refresh ON oauth_grants (old_refresh_hash);
