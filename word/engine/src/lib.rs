//! Word engine: document model, editing transactions, DOCX package I/O,
//! font metrics, line building, pagination and hit testing.
//!
//! Ported from the JS canvas engine (`documents/src/engine`). The same layer
//! separation is kept: mutations (`model`), serialization (`docx`), layout
//! (`layout`) and painting (in the GUI crate, from `layout::DrawItem`s).
//!
//! Units: layout uses CSS pixels at 96 dpi (twips / 15), as the JS engine does.
//! Positions are character offsets into paragraphs joined by '\n'.

pub mod format;
pub mod page;
pub mod styles;
pub mod numbering;
pub mod model;
pub mod fonts;
pub mod layout;
pub mod docx;
pub mod import;
pub mod xml;
pub use ttf_parser;
pub mod svg;
pub mod pdf;
pub mod tables;
pub mod formula;
pub mod editing;
pub mod objects;
pub mod picture;
pub mod equation;
pub mod fields;
pub mod design;
pub mod sections;
pub mod arrange;
pub mod references;
pub mod proofing;
pub mod mailmerge;
pub mod review;
pub mod macros;
pub mod resources;

pub use format::*;
pub use model::{Document, Paragraph, Field, Run, Table, Row, Cell, Asset, Comment, Marker, Revision, RevKind, Markup, Note, NoteKind, NoteRef};
pub use page::PageSettings;
pub use layout::{Layout, layout_document, layout_with_cache, MeasureCache};

/// Special characters stored in run text.
pub mod chars {
    pub const TAB: char = '\t';
    /// Soft line break (`w:br`).
    pub const LINE_BREAK: char = '\u{000B}';
    /// Page break (`w:br w:type="page"`).
    pub const PAGE_BREAK: char = '\u{000C}';
    /// Column break, laid out as a page break (single-column subset).
    pub const COLUMN_BREAK: char = '\u{000E}';
    /// Inline object placeholder (pictures).
    pub const OBJECT: char = '\u{FFFC}';
    /// Footnote/endnote reference mark (its number is computed at layout).
    pub const NOTE: char = '\u{E000}';
}
