//! Friendly generated names and animal faces, so lists never show a real name or email.

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::animals::{ADJECTIVES, ANIMALS};

/// Every adjective and animal pair, before a numbered second round starts.
const COMBOS: u32 = 3200;
/// Coprime with COMBOS, so neighbouring slots differ in both words.
const STRIDE: u32 = 1213;

const EXTRAS: [&str; 3] = [
    r##"<g stroke="#1F1A17" stroke-width="1.1" fill="none"><circle cx="16" cy="21" r="3"/><circle cx="24" cy="21" r="3"/><path d="M19 21h2"/></g>"##,
    r##"<path d="M9 33q11 5 22 0v4q-11 5-22 0z" fill="#E2483A"/>"##,
    r##"<path d="M15 12l5-10 5 10z" fill="#4C86C6"/><circle cx="20" cy="2.5" r="1.6" fill="#F4C84A"/>"##,
];

pub struct Alias {
    pub name: String,
    /// The animal face as inline SVG.
    pub avatar: String,
}

/// The name and face for a slot; no two slots share a name.
pub fn alias(slot: u32) -> Alias {
    let round = slot / COMBOS;
    let i = (slot % COMBOS * STRIDE + 517) % COMBOS;
    let (adjective, color) = ADJECTIVES[(i % 32) as usize];
    let (animal, face) = ANIMALS[(i / 32) as usize];
    let extra = match round {
        0 => "",
        r => EXTRAS[((r - 1) % 3) as usize],
    };
    Alias {
        name: match round {
            0 => format!("{adjective} {animal}"),
            r => format!("{adjective} {animal} {}", r + 1),
        },
        avatar: format!(
            r#"<svg viewBox="0 0 40 40" aria-hidden="true"><rect width="40" height="40" fill="{color}"/>{face}{extra}</svg>"#
        ),
    }
}

/// Slots for the people on one page, given out on first sight and kept for good.
pub struct Names(HashMap<String, u32>);

impl Names {
    pub async fn load(db: &SqlitePool, site_id: &str, keys: Vec<String>) -> sqlx::Result<Names> {
        let mut map = HashMap::new();
        let mut unique = keys;
        let mut seen = std::collections::HashSet::new();
        unique.retain(|k| seen.insert(k.clone()));
        for chunk in unique.chunks(500) {
            let sql = format!(
                "SELECT key, slot FROM aliases WHERE site_id = ? AND key IN ({})",
                vec!["?"; chunk.len()].join(", ")
            );
            let mut q = sqlx::query_as::<_, (String, u32)>(sqlx::AssertSqlSafe(sql)).bind(site_id);
            for key in chunk {
                q = q.bind(key);
            }
            map.extend(q.fetch_all(db).await?);
        }
        unique.retain(|k| !map.contains_key(k));
        if unique.is_empty() {
            return Ok(Names(map));
        }
        let mut tx = db.begin().await?;
        for key in unique {
            sqlx::query(
                "INSERT INTO aliases (site_id, key, slot)
                 SELECT ?1, ?2, COALESCE(MAX(slot) + 1, 0) FROM aliases WHERE site_id = ?1
                 ON CONFLICT (site_id, key) DO NOTHING",
            )
            .bind(site_id)
            .bind(&key)
            .execute(&mut *tx)
            .await?;
            let slot: u32 =
                sqlx::query_scalar("SELECT slot FROM aliases WHERE site_id = ? AND key = ?")
                    .bind(site_id)
                    .bind(&key)
                    .fetch_one(&mut *tx)
                    .await?;
            map.insert(key, slot);
        }
        tx.commit().await?;
        Ok(Names(map))
    }

    pub fn get(&self, key: &str) -> Alias {
        alias(self.0[key])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slot_in_a_round_is_unique() {
        let names: std::collections::HashSet<String> =
            (0..COMBOS * 2).map(|s| alias(s).name).collect();
        assert_eq!(names.len(), (COMBOS * 2) as usize);
        let (a, b) = (alias(0), alias(1));
        assert_ne!(a.name.split(' ').next(), b.name.split(' ').next());
        assert_ne!(a.name.split(' ').nth(1), b.name.split(' ').nth(1));
        assert!(a.avatar.starts_with("<svg") && a.avatar.ends_with("</svg>"));
        assert_ne!(alias(7).avatar, alias(COMBOS + 7).avatar);
    }
}
