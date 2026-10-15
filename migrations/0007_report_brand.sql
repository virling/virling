CREATE TABLE report_brands (
    site_id TEXT PRIMARY KEY REFERENCES sites(id) ON DELETE CASCADE,
    agency_name TEXT NOT NULL DEFAULT '',
    logo_url TEXT NOT NULL DEFAULT '',
    accent TEXT NOT NULL DEFAULT '',
    footer TEXT NOT NULL DEFAULT ''
);
