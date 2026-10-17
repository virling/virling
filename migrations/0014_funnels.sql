-- Named funnels: the steps between a visit and paying, one "page:/path" or "goal:name" per line.
CREATE TABLE funnels (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    steps TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX funnels_site ON funnels (site_id, created_at);
