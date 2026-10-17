-- AI crawler fetches, kept apart from visits: hits per site, UTC day and bot, and each page a bot read.
CREATE TABLE crawler_days (
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    day INTEGER NOT NULL,
    bot TEXT NOT NULL,
    hits INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    PRIMARY KEY (site_id, day, bot)
) WITHOUT ROWID;
CREATE TABLE crawler_pages (
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    bot TEXT NOT NULL,
    path TEXT NOT NULL,
    hits INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    PRIMARY KEY (site_id, bot, path)
) WITHOUT ROWID;
CREATE INDEX crawler_pages_seen ON crawler_pages (site_id, last_seen, bot, hits, path);
