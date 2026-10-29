//! Jadwal's engine: a workbook model, an Excel-compatible formula engine, number formats and
//! spreadsheet files (.xlsx, .csv), ported from the JS spreadsheet's core.

pub mod addr;
pub mod audit;
pub mod book;
pub mod chart;
pub mod condformat;
pub mod csv;
pub mod date;
pub mod drawing;
pub mod embed;
pub mod model3d;
pub mod shape_geom;
pub mod fill;
pub mod follow;
pub mod formula;
pub mod numfmt;
pub mod outline;
pub mod whatif;
pub mod solver;
pub mod consolidate;
pub mod analysis;
pub mod import;
pub mod connections;
pub mod powerquery;
pub mod forecast;
pub mod ops;
pub mod pivot;
pub mod print;
pub mod hfpic;
pub mod sparkline;
pub mod cellimage;
pub mod cfb;
pub mod style;
pub mod tables;
pub mod theme;
pub mod sheet_background;
pub mod group;
pub mod timeline;
pub mod geo;
pub mod pivot_chart;
pub mod scripts;
pub mod validation;
pub mod value;
pub mod xlsx;
pub mod review;
pub mod ods;
pub mod templates;

pub use addr::{Pos, Range};
pub use book::{Cell, Input, Sheet, Workbook};
pub use value::{ErrKind, Value};

/// Opens a spreadsheet file by its extension.
pub fn open(path: &std::path::Path) -> Result<Workbook, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    match ext.as_str() {
        "xlsx" | "xlsm" | "xltx" | "xltm" => xlsx::read(&bytes),
        "ods" => ods::read(&bytes),
        "csv" | "tsv" | "txt" | "tab" => {
            let text = String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|b| *b as char).collect());
            Ok(csv::read(&text, &path.file_name().and_then(|n| n.to_str()).unwrap_or("Sheet1.csv").to_string()))
        }
        _ => Err(format!("Files of type .{ext} can't be opened.")),
    }
}

/// Saves a workbook by the path's extension (CSV saves the active sheet).
pub fn save(wb: &Workbook, path: &std::path::Path) -> Result<(), String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let bytes = match ext.as_str() {
        "csv" => csv::write(wb, wb.active, ',').into_bytes(),
        "tsv" | "txt" | "tab" => csv::write(wb, wb.active, '\t').into_bytes(),
        "ods" => ods::write(wb)?,
        _ => xlsx::write(wb)?,
    };
    let tmp = path.with_extension(format!("{ext}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}
