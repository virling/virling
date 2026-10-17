//! Shared with the site worker, which renders some of the same templates.

/// Changes whenever app.css or app.js change, so browsers fetch the new files after an update.
pub fn asset_v() -> &'static str {
    static V: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let h = include_str!("../assets/app.css")
            .bytes()
            .chain(include_str!("../assets/app.js").bytes())
            .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
            });
        format!("{:08x}", h as u32)
    });
    &V
}
