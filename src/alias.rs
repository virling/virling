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
    pub initials: String,
    pub color: &'static str,
}

/// FNV-1a, stable across builds and platforms.
fn hash(id: &str) -> u64 {
    id.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The same id always gets the same name and avatar color.
pub fn alias(id: &str) -> Alias {
    let h = hash(id);
    let adjective = ADJECTIVES[(h % 32) as usize];
    let animal = ANIMALS[((h >> 8) % 32) as usize];
    Alias {
        name: format!("{adjective} {animal}"),
        initials: format!("{}{}", &adjective[..1], &animal[..1]),
        color: COLORS[((h >> 16) % 8) as usize],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_unrelated_to_the_email() {
        let id = "stripe:maya.chen@tallybird.app";
        let (a, b) = (alias(id), alias(id));
        assert_eq!((a.name.as_str(), a.color), (b.name.as_str(), b.color));
        assert_eq!(a.initials.len(), 2);
        let name = a.name.to_lowercase();
        for part in ["maya", "chen", "tallybird", "stripe"] {
            assert!(!name.contains(part), "{name} contains {part}");
        }
        assert_ne!(alias("cus_1").name, alias("cus_2").name);
    }
}
