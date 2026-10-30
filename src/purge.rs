use duckdb::{Connection, params};

const TABLES: [&str; 11] = [
    "events",
    "bot_events",
    "reads",
    "payments",
    "trials",
    "attribution_dirty",
    "attribution",
    "subscriptions",
    "touches",
    "first_touches",
    "imported",
];

/// Removes everything stored for a site from the event database.
pub fn site(conn: &Connection, site_id: &str) -> anyhow::Result<()> {
    for table in TABLES {
        conn.execute(
            &format!("DELETE FROM {table} WHERE site_id = ?"),
            params![site_id],
        )?;
    }
    Ok(())
}
