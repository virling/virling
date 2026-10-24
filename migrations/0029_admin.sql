ALTER TABLE accounts ADD COLUMN suspended_at INTEGER;
-- Set on a support session: the admin who signed in as this account.
ALTER TABLE sessions ADD COLUMN impersonator TEXT;
CREATE INDEX sites_account ON sites (account_id);
CREATE INDEX accounts_plan ON accounts (plan);

CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    admin TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT NOT NULL,
    detail TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);

CREATE TABLE beta_invites (
    email TEXT PRIMARY KEY,
    invited_by TEXT NOT NULL,
    invited_at INTEGER NOT NULL
);
