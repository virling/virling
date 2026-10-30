-- Events (Personal) or pageviews (Agency) an account stored each UTC month, and the highest warning sent: 0, 80 or 100.
CREATE TABLE usage_months (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    month TEXT NOT NULL,
    used INTEGER NOT NULL,
    warned INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, month)
);
