-- Tracked short links that redirect to the site with UTM tags so their visits and revenue are counted.
CREATE TABLE short_links (
    id TEXT PRIMARY KEY,
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    destination TEXT NOT NULL,
    source TEXT NOT NULL,
    channel TEXT NOT NULL,
    clicks INTEGER NOT NULL DEFAULT 0,
    last_click_at INTEGER,
    created_at INTEGER NOT NULL
);
CREATE INDEX short_links_site ON short_links (site_id, created_at);
