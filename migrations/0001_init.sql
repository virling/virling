CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    plan TEXT NOT NULL DEFAULT 'self_hosted',
    created_at TEXT NOT NULL
);

CREATE TABLE sites (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    domain TEXT NOT NULL UNIQUE,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    attribution_model TEXT NOT NULL DEFAULT 'last_non_direct',
    window_days INTEGER NOT NULL DEFAULT 90,
    cookieless INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE waitlist (
    email TEXT NOT NULL,
    wedge TEXT NOT NULL,
    clients TEXT,
    referrer TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (email, wedge)
);
