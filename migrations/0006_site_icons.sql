CREATE TABLE site_icons (
    site_id TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE,
    content_type TEXT,
    body BLOB,
    fetched_at INTEGER NOT NULL
);
