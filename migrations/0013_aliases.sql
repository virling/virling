-- Generated visitor names: each key gets the next free slot so no two people on a site share a name.
CREATE TABLE aliases (
    site_id TEXT NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    slot INTEGER NOT NULL,
    PRIMARY KEY (site_id, key),
    UNIQUE (site_id, slot)
);
