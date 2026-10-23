-- The hosted subscription of an account, from Dodo webhooks, and the webhooks already handled.
CREATE TABLE billing (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    subscription_id TEXT NOT NULL UNIQUE,
    customer_id TEXT NOT NULL,
    status TEXT NOT NULL,
    product TEXT NOT NULL,
    trial_ends_at INTEGER,
    renews_at INTEGER,
    cancelling INTEGER NOT NULL DEFAULT 0,
    extra_seats INTEGER NOT NULL DEFAULT 0,
    reminded INTEGER NOT NULL DEFAULT 0,
    event_at INTEGER NOT NULL
);
CREATE TABLE billing_webhooks (
    id TEXT PRIMARY KEY,
    received_at INTEGER NOT NULL
);
