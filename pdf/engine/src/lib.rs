//! Document logic for Waraq (ورق), the PDF editor.
//!
//! The engine has no UI. It owns the editable document (`lopdf`), keeps a parsed
//! read-only view of the same bytes for rendering and text extraction (`hayro`),
//! and records every edit as a snapshot so undo and redo are exact.
//!
//! Coordinates handed to and from the UI are *view points*: PDF points with the
//! origin at the top-left of the page as displayed (rotation and crop box
//! applied, y pointing down). The engine converts to PDF user space internally.

mod annot;
mod document;
mod fonts;
mod forms;
mod geometry;
mod items;
mod redact;
mod render;
mod text;
mod textedit;

pub use annot::{AnnotKind, AnnotSummary, Color, NewAnnot};
pub use document::{Document, Error, Result};
pub use fonts::{FontFace, fallback_font_data};
pub use forms::{FieldInfo, FieldKind, NewField};
pub use geometry::{Point, Rect};
pub use items::{ItemEdit, ItemKind, NewItem, PageItem};
pub use redact::{Pattern, RedactHit, RedactReport};
pub use render::{RenderedPage, Renderer};
pub use text::{PageText, SearchHit, TextGlyph};
pub use textedit::TextLine;

#[cfg(test)]
mod tests;

#[doc(hidden)]
pub mod sample;
