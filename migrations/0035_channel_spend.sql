-- Monthly spend per channel, one "Channel amount" line each, entered in site settings.
CREATE TABLE channel_spend (
    site_id TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE,
    lines TEXT NOT NULL
);
