CREATE TABLE admins (
    email TEXT PRIMARY KEY CHECK (email = lower(email)),
    created_at INTEGER NOT NULL
) WITHOUT ROWID;
