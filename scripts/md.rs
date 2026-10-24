//! Writes a .md next to every .html under a directory: rustc -O scripts/md.rs -o md && ./md worker/public

#[path = "../src/html_md.rs"]
mod html_md;

use std::path::Path;

fn walk(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            // The demo is generated app pages, not site content.
            if path.file_name().is_some_and(|n| n != "demo") {
                walk(&path)?;
            }
        } else if path.extension().is_some_and(|e| e == "html") {
            let html = std::fs::read_to_string(&path)?;
            std::fs::write(path.with_extension("md"), html_md::convert(&html))?;
        }
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    walk(Path::new(&std::env::args().nth(1).expect("directory to convert")))
}
