-- Email alerts: new sales and a daily digest at a chosen local hour, with the newest sale already sent and the last digest day as a Julian day.
CREATE TABLE email_alerts (
    site_id TEXT PRIMARY KEY,
    sales INTEGER NOT NULL DEFAULT 0,
    digest INTEGER NOT NULL DEFAULT 0,
    digest_hour INTEGER NOT NULL DEFAULT 9,
    sent_until INTEGER NOT NULL DEFAULT 0,
    digest_day INTEGER NOT NULL DEFAULT 0
);
