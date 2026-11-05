-- Which one-time setup reminders a site has been sent.
CREATE TABLE setup_emails (
    site_id TEXT PRIMARY KEY REFERENCES sites (id) ON DELETE CASCADE,
    script_sent INTEGER NOT NULL DEFAULT 0,
    payments_sent INTEGER NOT NULL DEFAULT 0
);
