//! The `.sab` file: a board with its pictures inside, as a gzip-compressed tar archive.
//!
//! ```text
//! mimetype          application/vnd.woosool.saboura   (first, uncompressed text)
//! meta.json         { "format": 1, "app": "Saboura", "version": "…" }
//! board.json        the board (the JS whiteboard's format); pictures point into assets/
//! assets/<sha256>.<png|jpeg|webp>   each picture once, however often the board uses it
//! thumbnail.png     optional preview of the board
//! ```
//!
//! An image object's `src` is `sab:assets/<name>` in the archive, and a `data:` URL again
//! once read, so the rest of the app (and the JS format) never sees the difference.

use crate::model::Board;
use base64::Engine;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};

pub const MIMETYPE: &str = "application/vnd.woosool.saboura";
pub const EXTENSION: &str = "sab";
const FORMAT: u64 = 1;
const PREFIX: &str = "sab:";
/// Limits for reading: the archive's unpacked size and file count.
const MAX_UNPACKED: u64 = 1 << 30;
const MAX_FILES: usize = 20_000;

/// Whether bytes look like a `.sab` archive (gzip's magic number).
pub fn is_sab(bytes: &[u8]) -> bool { bytes.starts_with(&[0x1f, 0x8b]) }

/// The board as a `.sab` archive, with an optional PNG preview.
pub fn write(board: &Board, thumbnail: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut board = board.clone();
    let mut assets: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for i in board.items.iter_mut() {
        let Some(src) = i.str("src") else { continue };
        let Some((mime, data)) = src.strip_prefix("data:").and_then(|s| s.split_once(";base64,")) else { continue };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else { continue };
        let ext = match mime { "image/jpeg" => "jpeg", "image/webp" => "webp", "image/png" => "png", _ => continue };
        use sha2::Digest;
        let hash: String = sha2::Sha256::digest(&bytes).iter().take(16).map(|b| format!("{b:02x}")).collect();
        let name = format!("assets/{hash}.{ext}");
        i.set("src", format!("{PREFIX}{name}"));
        assets.entry(name).or_insert(bytes);
    }
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(gz);
    let mut add = |tar: &mut tar::Builder<_>, path: &str, data: &[u8]| -> Result<(), String> {
        let mut h = tar::Header::new_ustar();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_mtime((crate::model::now_ms() / 1000.0) as u64);
        h.set_cksum();
        tar.append_data(&mut h, path, data).map_err(|e| e.to_string())
    };
    add(&mut tar, "mimetype", MIMETYPE.as_bytes())?;
    let meta = serde_json::json!({ "format": FORMAT, "app": "Saboura", "version": env!("CARGO_PKG_VERSION"), "assets": assets.len() });
    add(&mut tar, "meta.json", meta.to_string().as_bytes())?;
    add(&mut tar, "board.json", board.to_json().as_bytes())?;
    for (name, bytes) in &assets { add(&mut tar, name, bytes)?; }
    if let Some(t) = thumbnail { add(&mut tar, "thumbnail.png", t)?; }
    let gz = tar.into_inner().map_err(|e| e.to_string())?;
    gz.finish().map_err(|e| e.to_string())
}

/// The files of a `.sab` archive by path, checked against the limits.
fn files(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let bad = || "This is not a Saboura file.".to_string();
    let gz = flate2::read::GzDecoder::new(bytes).take(MAX_UNPACKED);
    let mut tar = tar::Archive::new(gz);
    let mut out = BTreeMap::new();
    for entry in tar.entries().map_err(|_| bad())? {
        let mut e = entry.map_err(|_| bad())?;
        if !e.header().entry_type().is_file() { continue; }
        let path = e.path().map_err(|_| bad())?.to_string_lossy().replace('\\', "/");
        // Only plain names inside the archive.
        if path.starts_with('/') || path.split('/').any(|p| p == ".." || p.is_empty()) { return Err(bad()); }
        let mut data = Vec::new();
        e.read_to_end(&mut data).map_err(|_| bad())?;
        out.insert(path, data);
        if out.len() > MAX_FILES { return Err("This file holds too many parts.".into()); }
    }
    Ok(out)
}

/// Reads a `.sab` archive: the board, with its pictures back as `data:` URLs.
pub fn read(bytes: &[u8]) -> Result<Board, String> {
    let files = files(bytes)?;
    if files.get("mimetype").map(|m| m.as_slice()) != Some(MIMETYPE.as_bytes()) { return Err("This is not a Saboura file.".into()); }
    if let Some(meta) = files.get("meta.json").and_then(|m| serde_json::from_slice::<Value>(m).ok())
        && meta["format"].as_u64().is_some_and(|f| f > FORMAT) {
        return Err("This file was made by a newer version of Saboura.".into());
    }
    let json = files.get("board.json").ok_or("This Saboura file has no board.")?;
    let mut board: Board = serde_json::from_slice(json).map_err(|_| "This is not a supported Whiteboard file.".to_string())?;
    for i in board.items.iter_mut() {
        let Some(name) = i.str("src").and_then(|s| s.strip_prefix(PREFIX)).map(str::to_string) else { continue };
        let Some(data) = files.get(&name) else { i.extra.remove("src"); continue };
        let mime = match name.rsplit('.').next() { Some("jpeg" | "jpg") => "image/jpeg", Some("webp") => "image/webp", _ => "image/png" };
        i.set("src", format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(data)));
    }
    board.validate()?;
    Ok(board)
}

/// The preview picture of a `.sab` file, if it has one.
pub fn thumbnail(bytes: &[u8]) -> Option<Vec<u8>> { files(bytes).ok()?.remove("thumbnail.png") }

/// A board from either kind of file: `.sab` or the JS app's JSON.
pub fn open(bytes: &[u8]) -> Result<Board, String> {
    if is_sab(bytes) { read(bytes) } else { Board::from_json(std::str::from_utf8(bytes).map_err(|_| "This is not a supported Whiteboard file.".to_string())?) }
}
