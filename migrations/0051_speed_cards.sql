-- The last 30 days of page speed per site as JSON, refreshed in the background so the dashboard only reads it.
CREATE TABLE speed_cards (
    site_id TEXT PRIMARY KEY REFERENCES sites (id) ON DELETE CASCADE,
    data TEXT NOT NULL
);
