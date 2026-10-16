//! Friendly generated names for payers, so lists never show a real name or email.

const ADJECTIVES: [&str; 32] = [
    "Amber", "Quiet", "Bright", "Calm", "Clever", "Gentle", "Golden", "Happy", "Lucky", "Merry",
    "Misty", "Noble", "Polar", "Rapid", "Silver", "Sunny", "Swift", "Tidy", "Brave", "Cosy",
    "Crisp", "Dapper", "Eager", "Fuzzy", "Jolly", "Keen", "Lively", "Mellow", "Nimble", "Plucky",
    "Rosy", "Witty",
];
const ANIMALS: [&str; 32] = [
    "Otter", "Heron", "Badger", "Falcon", "Fox", "Hare", "Koala", "Lynx", "Marten", "Owl", "Panda",
    "Puffin", "Quail", "Raven", "Robin", "Seal", "Sparrow", "Stoat", "Swan", "Tapir", "Wren",
    "Yak", "Beaver", "Bison", "Crane", "Dingo", "Finch", "Gecko", "Ibex", "Lemur", "Moose", "Newt",
];
const COLORS: [&str; 8] = [
    "#3D5A80", "#5B6C5D", "#7A5C61", "#4F6D7A", "#6D597A", "#8C6A43", "#3E6B5C", "#5C5F8C",
];

pub struct Alias {
    pub name: String,
    /// A small illustrated face as inline SVG, drawn from the same id.
    pub avatar: String,
}

const BACKGROUNDS: [&str; 8] = [
    "#DCE7F5", "#FDE5DA", "#E3EFE6", "#F3E8F6", "#FFF1CC", "#E6E9EE", "#DDF1F4", "#F6E3E3",
];
const SKIN: [&str; 6] = [
    "#F6D7C3", "#EDC2A1", "#D9A47C", "#B97E55", "#8D5A3B", "#5E3B26",
];
const HAIR: [&str; 6] = [
    "#2B2522", "#5A3825", "#A0663A", "#D6A75B", "#8C8F96", "#B5452C",
];

/// Hair drawn over a 40x40 head centred at (20, 22) with radius 11.
fn hair(style: u64, color: &str) -> String {
    let path = match style {
        0 => "M9 21c0-8 5-13 11-13s11 5 11 13c-3-4-7-6-11-6s-8 2-11 6z",
        1 => "M8 23c-1-10 5-15 12-15s13 5 12 15l-3-5c-2 2-6 3-9 3s-7-1-9-3z",
        2 => "M10 18c1-6 5-9 10-9s9 3 10 9c-4-1-6-3-7-5-2 3-7 5-13 5z",
        3 => "M8 22c0-9 5-14 12-14s12 5 12 14v9h-3v-12c-3-2-6-3-9-3s-6 1-9 3v12H8z",
        4 => "M11 13c2-3 5-5 9-5s7 2 9 5c-3 0-6-1-9-1s-6 1-9 1z",
        _ => "M9 20c0-7 5-12 11-12 4 0 7 1 9 4 2 0 3 2 3 4-4-2-8-2-12-2-4 0-8 2-11 6z",
    };
    format!(r##"<path d="{path}" fill="{color}"/>"##)
}

fn eyes(style: u64) -> &'static str {
    match style {
        0 => {
            r##"<circle cx="16" cy="22" r="1.4" fill="#1F1A17"/><circle cx="24" cy="22" r="1.4" fill="#1F1A17"/>"##
        }
        1 => {
            r##"<path d="M14.5 22.5q1.5-1.6 3 0M22.5 22.5q1.5-1.6 3 0" stroke="#1F1A17" stroke-width="1.3" fill="none" stroke-linecap="round"/>"##
        }
        _ => {
            r##"<ellipse cx="16" cy="22" rx="1.2" ry="1.7" fill="#1F1A17"/><ellipse cx="24" cy="22" rx="1.2" ry="1.7" fill="#1F1A17"/>"##
        }
    }
}

fn mouth(style: u64) -> &'static str {
    match style {
        0 => {
            r##"<path d="M16.5 27q3.5 3 7 0" stroke="#1F1A17" stroke-width="1.3" fill="none" stroke-linecap="round"/>"##
        }
        1 => {
            r##"<path d="M17 27.5h6" stroke="#1F1A17" stroke-width="1.3" stroke-linecap="round"/>"##
        }
        2 => r##"<path d="M16.5 26.5q3.5 4 7 0z" fill="#1F1A17"/>"##,
        _ => r##"<circle cx="20" cy="27.6" r="1.5" fill="#1F1A17"/>"##,
    }
}

const GLASSES: &str = r##"<g stroke="#1F1A17" stroke-width="1.1" fill="none"><circle cx="16" cy="22" r="3"/><circle cx="24" cy="22" r="3"/><path d="M19 22h2"/></g>"##;

/// Deterministic illustrated face: background, skin, hair, eyes, mouth and sometimes glasses.
fn face(h: u64) -> String {
    let pick = |shift: u32, n: u64| (h >> shift) % n;
    let skin = SKIN[pick(24, 6) as usize];
    format!(
        concat!(
            r#"<svg viewBox="0 0 40 40" aria-hidden="true">"#,
            r#"<rect width="40" height="40" fill="{bg}"/>"#,
            r#"<path d="M7 40c1-7 6-10 13-10s12 3 13 10z" fill="{shirt}"/>"#,
            r#"<circle cx="20" cy="22" r="11" fill="{skin}"/>"#,
            "{hair}{eyes}{mouth}{glasses}</svg>"
        ),
        bg = BACKGROUNDS[pick(20, 8) as usize],
        shirt = COLORS[pick(16, 8) as usize],
        skin = skin,
        hair = hair(pick(28, 6), HAIR[pick(32, 6) as usize]),
        eyes = eyes(pick(36, 3)),
        mouth = mouth(pick(40, 4)),
        glasses = if pick(44, 5) == 0 { GLASSES } else { "" },
    )
}

/// FNV-1a, stable across builds and platforms.
fn hash(id: &str) -> u64 {
    id.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The same id always gets the same name and face.
pub fn alias(id: &str) -> Alias {
    let h = hash(id);
    let adjective = ADJECTIVES[(h % 32) as usize];
    let animal = ANIMALS[((h >> 8) % 32) as usize];
    Alias {
        name: format!("{adjective} {animal}"),
        avatar: face(h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_unrelated_to_the_email() {
        let id = "stripe:maya.chen@tallybird.app";
        let (a, b) = (alias(id), alias(id));
        assert_eq!(a.name, b.name);
        let name = a.name.to_lowercase();
        for part in ["maya", "chen", "tallybird", "stripe"] {
            assert!(!name.contains(part), "{name} contains {part}");
        }
        assert_ne!(alias("cus_1").name, alias("cus_2").name);
        assert_eq!(a.avatar, b.avatar);
        assert!(a.avatar.starts_with("<svg") && a.avatar.ends_with("</svg>"));
        let faces: std::collections::HashSet<String> =
            (0..50).map(|i| alias(&format!("v{i}")).avatar).collect();
        assert!(faces.len() > 40, "avatars vary: {}", faces.len());
    }
}
