CREATE TABLE source_icons (
    host TEXT PRIMARY KEY,
    content_type TEXT,
    body BLOB,
    fetched_at INTEGER NOT NULL
);
