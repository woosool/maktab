//! Page Layout ▸ Background: a picture tiled behind a sheet's cells, saved as the worksheet's
//! `<picture r:id>` with an image relationship. It lives in the sheet's carried elements
//! (`Sheet.extra`: the element, its relationship, the media part in `Workbook.parts`), so files
//! read with one keep it and writing needs nothing special.

use crate::book::Workbook;
use crate::xlsx::xml::Element;

const RT_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

fn rel_of(wb: &Workbook, si: usize) -> Option<(String, String)> {
    let s = wb.sheets.get(si)?;
    let raw = s.extra.iter().find(|(k, _)| k == "picture")?.1.clone();
    let id = Element::parse(&raw).ok()?.attrs.iter().find(|(k, _)| k.ends_with(":id") || k == "id").map(|(_, v)| v.clone())?;
    let rel = s.extra.iter().find(|(k, v)| k == "rel" && v.split('\t').next() == Some(id.as_str()))?.1.clone();
    let target = rel.split('\t').nth(2)?.to_string();
    Some((id, target))
}

/// The sheet's background picture (its bytes), if it has one.
pub fn get(wb: &Workbook, si: usize) -> Option<&[u8]> {
    let (_, target) = rel_of(wb, si)?;
    wb.parts.get(&target).map(|b| b.as_slice()).filter(|b| !b.is_empty())
}

/// Delete Background.
pub fn remove(wb: &mut Workbook, si: usize) {
    let found = rel_of(wb, si);
    let Some(s) = wb.sheets.get_mut(si) else { return };
    s.extra.retain(|(k, _)| k != "picture");
    if let Some((id, target)) = found {
        s.extra.retain(|(k, v)| !(k == "rel" && v.split('\t').next() == Some(id.as_str())));
        // The media part stays in `parts` (undo brings the reference back); unreferenced, it
        // isn't read back as anything.
        let _ = target;
    }
}

/// Sets the background picture (`ext` such as "png" or "jpeg").
pub fn set(wb: &mut Workbook, si: usize, bytes: Vec<u8>, ext: &str) {
    remove(wb, si);
    let ext = if ext == "jpg" { "jpeg" } else { ext };
    let path = (1..).map(|i| format!("xl/media/background{i}.{ext}")).find(|p| !wb.parts.contains_key(p)).unwrap();
    wb.parts.insert(path.clone(), bytes);
    let s = &mut wb.sheets[si];
    let used: Vec<String> = s.extra.iter().filter(|(k, _)| k == "rel").filter_map(|(_, v)| v.split('\t').next().map(String::from)).collect();
    let id = (1..).map(|i| format!("rIdBg{i}")).find(|i| !used.contains(i)).unwrap();
    s.extra.push(("rel".into(), format!("{id}\t{RT_IMAGE}\t{path}\t0")));
    s.extra.push(("picture".into(), format!("<picture r:id=\"{id}\"/>")));
}

#[cfg(test)]
mod tests {
    use super::*;
    const PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0, 0x05, 0x00, 0x01, 0xFF, 0x2D, 0x7A, 0x9C, 0x0F, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82];

    #[test]
    fn background_round_trip() {
        let mut wb = Workbook::new();
        wb.set_input(0, crate::Pos::new(0, 0), "1");
        set(&mut wb, 0, PNG.to_vec(), "png");
        assert_eq!(get(&wb, 0), Some(PNG));
        let bytes = crate::xlsx::write(&wb).unwrap();
        let back = crate::xlsx::read(&bytes).unwrap();
        assert_eq!(get(&back, 0), Some(PNG));
        let mut b = back.clone();
        remove(&mut b, 0);
        assert!(get(&b, 0).is_none());
        let again = crate::xlsx::read(&crate::xlsx::write(&b).unwrap()).unwrap();
        assert!(get(&again, 0).is_none());
    }
}
