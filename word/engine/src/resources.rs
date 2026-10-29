//! Bundled resources (fonts, the spelling dictionary) live in files beside the program rather
//! than inside it, which keeps the executable small. They're looked up in, in order:
//! `WORD_RESOURCES`, `resources/` next to the executable, the macOS bundle's `Resources/`,
//! `../share/word/` (Linux installs), and the repository's `assets/` (development and tests).
//! With the `embed-assets` feature they're compiled in instead.

use std::path::PathBuf;

fn candidates() -> Vec<PathBuf> {
    let mut out = vec![];
    if let Some(d) = std::env::var_os("WORD_RESOURCES") { out.push(PathBuf::from(d)); }
    if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        out.push(exe.join("resources"));
        out.push(exe.join("../Resources"));
        out.push(exe.join("../share/qalam"));
        out.push(exe.join("../share/word"));
    }
    out.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets")));
    out
}

/// The folder resources are read from, if one exists.
pub fn dir() -> Option<PathBuf> { candidates().into_iter().find(|d| d.join("fonts").is_dir()) }

/// A resource's bytes, loaded once and kept for the life of the program.
pub fn load(path: &str) -> Option<&'static [u8]> {
    let bytes = candidates().into_iter().find_map(|d| std::fs::read(d.join(path)).ok())?;
    Some(Box::leak(bytes.into_boxed_slice()))
}

/// A resource's bytes; a missing resource is reported clearly instead of failing later.
pub fn require(path: &str) -> &'static [u8] {
    load(path).unwrap_or_else(|| panic!("Missing resource {path}: put the app's resources folder next to the executable or set WORD_RESOURCES."))
}
