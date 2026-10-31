-- Named dashboard filters, one "key=value" per line, and the one the client report shows.
CREATE TABLE segments (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    filters TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX segments_site ON segments (site_id, created_at);
ALTER TABLE report_brands ADD COLUMN segment_id TEXT;
