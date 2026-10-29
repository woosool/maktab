//! Test helpers: unzip a package and check its consistency (the checks of pivot_parts' tests):
//! every relationship target exists, every part has a content type, overrides name parts, and
//! the workbook's r:ids resolve. `check_package` also parses every XML part.

use super::xml::Element;
use std::collections::{BTreeMap, HashSet};
use std::io::Read;

/// Every part of a package; binary parts as lossy text.
pub fn unzip(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..z.len()).map(|i| { let mut f = z.by_index(i).unwrap(); let mut v = vec![]; f.read_to_end(&mut v).unwrap(); (f.name().to_string(), super::xml::part_text(&v)) }).collect()
}

/// The raw bytes of one part.
pub fn part(bytes: &[u8], name: &str) -> Vec<u8> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut f = z.by_name(name).unwrap();
    let mut v = vec![]; f.read_to_end(&mut v).unwrap(); v
}

pub fn check_package(files: &BTreeMap<String, String>) {
    let ct = Element::parse(&files["[Content_Types].xml"]).unwrap();
    let defaults: HashSet<String> = ct.all("Default").filter_map(|e| e.attr("Extension").map(|x| x.to_ascii_lowercase())).collect();
    let overrides: HashSet<String> = ct.all("Override").filter_map(|e| e.attr("PartName").map(|p| p.trim_start_matches('/').to_string())).collect();
    for o in &overrides { assert!(files.contains_key(o), "override for missing part {o}"); }
    for (k, v) in files {
        if k == "[Content_Types].xml" { continue; }
        let ext = k.rsplit('.').next().unwrap().to_ascii_lowercase();
        assert!(overrides.contains(k) || (defaults.contains(&ext) && ext != "xml") || k.ends_with(".rels"), "no content type for {k}");
        if matches!(ext.as_str(), "xml" | "rels" | "vml") { assert!(Element::parse(v).is_ok(), "{k} is not well-formed XML"); }
        if let Some(i) = k.rfind("_rels/") {
            let owner = format!("{}{}", &k[..i], k[i + 6..].trim_end_matches(".rels"));
            let rels = Element::parse(v).unwrap();
            for r in rels.all("Relationship") {
                if r.attr("TargetMode") == Some("External") { continue; }
                let t = super::read::resolve(&owner, r.attr("Target").unwrap());
                assert!(files.contains_key(&t), "{k}: missing target {t}");
            }
            // Every r:id-like attribute of the owner part names one of its relationships.
            if let Some(src) = files.get(&owner).filter(|_| owner.ends_with(".xml") || owner.ends_with(".vml")) {
                let ids: HashSet<&str> = rels.all("Relationship").filter_map(|r| r.attr("Id")).collect();
                for pre in ["r:id=\"", "r:embed=\"", "r:link=\"", "r:dm=\"", "r:lo=\"", "r:qs=\"", "r:cs=\"", "o:relid=\"", "r:pict=\""] {
                    for piece in src.split(pre).skip(1) { let id = &piece[..piece.find('"').unwrap()]; assert!(ids.contains(id), "{owner}: {pre}{id} has no relationship"); }
                }
            }
        }
    }
    let rels = Element::parse(&files["xl/_rels/workbook.xml.rels"]).unwrap();
    let ids: HashSet<&str> = rels.all("Relationship").filter_map(|r| r.attr("Id")).collect();
    let w = &files["xl/workbook.xml"];
    for part in w.split("r:id=\"").skip(1) { let id = &part[..part.find('"').unwrap()]; assert!(ids.contains(id), "workbook r:id {id} missing"); }
}

/// Every package a test writes passes through here: dumped when JADWAL_DUMP_ALL names a folder.
pub fn on_write(bytes: &[u8]) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    if let Ok(dir) = std::env::var("JADWAL_DUMP_ALL") {
        let t = std::thread::current();
        let name = t.name().unwrap_or("t").replace("::", ".");
        let _ = std::fs::write(format!("{dir}/{name}-{}.xlsx", N.fetch_add(1, Ordering::Relaxed)), bytes);
    }
    // Every written package passes the structural schema check.
    let errs = super::schemacheck::check_files(&unzip(bytes));
    assert!(errs.is_empty(), "schema check failed:\n{}", errs.join("\n"));
}
