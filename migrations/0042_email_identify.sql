-- Visitors who identified with an email, kept only as a salted SHA-256 hash, to credit payments that carry no visitor ID.
CREATE TABLE email_salts (
    site_id TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE,
    salt TEXT NOT NULL
);
CREATE TABLE visitor_emails (
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    email_hash TEXT NOT NULL,
    visitor_id TEXT NOT NULL,
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (site_id, email_hash)
);
-- Payments whose visitor came from an email match, to say so where they are listed.
CREATE TABLE email_matches (
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    provider_payment_id TEXT NOT NULL,
    PRIMARY KEY (site_id, provider, provider_payment_id)
);
