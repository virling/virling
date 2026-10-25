-- Launches a site logs by hand (a post, a thread, a Product Hunt day) to see what followed.
CREATE TABLE launches (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    url TEXT NOT NULL DEFAULT '',
    channel TEXT NOT NULL DEFAULT '',
    day TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX launches_site ON launches (site_id, day);
