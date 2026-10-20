-- People who share an Agency account's sites; the account itself is the owner.
CREATE TABLE members (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    org_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL
);
CREATE INDEX members_org ON members (org_id);

CREATE TABLE invites (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    org_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX invites_org ON invites (org_id);

-- Users on the account, owner included, for billing extra seats.
ALTER TABLE accounts ADD COLUMN seats INTEGER NOT NULL DEFAULT 1;
