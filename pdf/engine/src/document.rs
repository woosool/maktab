use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;

use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::Affine;
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::Color;
use crate::annot::{self, AnnotSummary, NewAnnot};
use crate::fonts::FontFace;
use crate::geometry::Point;
use crate::items::{self, ItemEdit, NewItem, PageItem};
use crate::text::{self, PageText, SearchHit};
use crate::textedit::{self, TextLine};

/// Everything that can go wrong while reading or editing a document.
#[derive(Debug)]
pub enum Error {
    /// The bytes are not a PDF the engine can read.
    Parse(String),
    /// Password-protected files are not supported yet.
    Encrypted,
    /// The edit makes no sense for this document (e.g. deleting every page).
    Invalid(&'static str),
    /// The edit was refused, with an explanation for the user.
    Refused(String),
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse(m) => write!(f, "This file could not be read as a PDF ({m})."),
            Error::Encrypted => write!(f, "Password-protected PDFs are not supported yet."),
            Error::Invalid(m) => write!(f, "{m}"),
            Error::Refused(m) => write!(f, "{m}"),
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<lopdf::Error> for Error {
    fn from(e: lopdf::Error) -> Self {
        Error::Parse(e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Most snapshots kept for undo; older ones are dropped.
const HISTORY_LIMIT: usize = 50;

/// An editable PDF with undo history.
///
/// Every edit is applied to the `lopdf` object graph, serialized, and re-parsed
/// by `hayro` so rendering, text extraction and coordinate mapping always see
/// exactly what will be saved. Undo/redo restores whole snapshots, which keeps
/// every operation trivially reversible.
pub struct Document {
    doc: lopdf::Document,
    bytes: Arc<Vec<u8>>,
    view: Pdf,
    undo: Vec<Arc<Vec<u8>>>,
    redo: Vec<Arc<Vec<u8>>>,
    /// Bumped on every change of `bytes`, so callers can invalidate caches.
    revision: u64,
    saved_revision: u64,
    texts: RefCell<Vec<Option<Arc<PageText>>>>,
}

impl Document {
    /// Opens a PDF from its bytes.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let mut doc = load(&data)?;
        normalize_page_tree(&mut doc)?;
        crate::forms::normalize(&mut doc);
        let bytes = Arc::new(serialize(&mut doc)?);
        let view = parse_view(&bytes)?;
        let pages = view.pages().len();
        if pages == 0 {
            return Err(Error::Parse("no pages".into()));
        }
        Ok(Self {
            doc,
            bytes,
            view,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            saved_revision: 0,
            texts: RefCell::new(vec![None; pages]),
        })
    }

    /// Opens a PDF file from disk.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        Self::from_bytes(std::fs::read(path)?)
    }

    /// A new document with one blank Letter page.
    pub fn blank() -> Self {
        let mut doc = lopdf::Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let content = doc.add_object(Stream::new(Dictionary::new(), Vec::new()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => Dictionary::new(),
            "Contents" => content,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page.into()],
                "Count" => 1,
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let bytes = serialize(&mut doc).expect("blank document serializes");
        Self::from_bytes(bytes).expect("blank document parses")
    }

    /// The current document bytes (what Save writes).
    pub fn bytes(&self) -> Arc<Vec<u8>> {
        self.bytes.clone()
    }

    /// Changes whenever the document content changes (edits, undo, redo).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Records that the current bytes were written to disk.
    pub fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }

    /// Writes the document to `path` and marks it saved.
    pub fn save(&mut self, path: &std::path::Path) -> Result<()> {
        // Write next to the target first so a failed write never truncates the original.
        let tmp = path.with_extension("pdf.saving");
        std::fs::write(&tmp, self.bytes.as_slice())?;
        std::fs::rename(&tmp, path)?;
        self.mark_saved();
        Ok(())
    }

    pub fn page_count(&self) -> usize {
        self.view.pages().len()
    }

    /// Displayed page size in points (rotation applied).
    pub fn page_size(&self, index: usize) -> (f32, f32) {
        self.view.pages()[index].render_dimensions()
    }

    /// Rotation of the page in degrees (0, 90, 180 or 270).
    pub fn page_rotation(&self, index: usize) -> i64 {
        let Ok(kids) = kids(&self.doc) else { return 0 };
        kids.get(index)
            .and_then(|id| self.doc.get_dictionary(*id).ok())
            .and_then(|d| d.get(b"Rotate").ok())
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0)
            .rem_euclid(360)
    }

    /// Maps PDF user space of page `index` to view points.
    pub(crate) fn user_to_view(&self, index: usize) -> Affine {
        self.view.pages()[index].initial_transform(true).to_kurbo()
    }

    pub(crate) fn view_pdf(&self) -> &Pdf {
        &self.view
    }

    // ----- history -------------------------------------------------------

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> Result<()> {
        let Some(prev) = self.undo.pop() else {
            return Ok(());
        };
        let current = self.bytes.clone();
        self.restore(prev)?;
        self.redo.push(current);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<()> {
        let Some(next) = self.redo.pop() else {
            return Ok(());
        };
        let current = self.bytes.clone();
        self.restore(next)?;
        self.undo.push(current);
        Ok(())
    }

    fn restore(&mut self, bytes: Arc<Vec<u8>>) -> Result<()> {
        self.doc = load(&bytes)?;
        self.view = parse_view(&bytes)?;
        self.bytes = bytes;
        self.after_change();
        Ok(())
    }

    fn after_change(&mut self) {
        self.revision += 1;
        // A restored snapshot can coincide with the saved one; still count it as a
        // change unless it is byte-identical, which `restore` cannot cheaply tell.
        *self.texts.borrow_mut() = vec![None; self.view.pages().len()];
    }

    /// Runs one edit as a transaction: on error the document is left untouched.
    fn edit(&mut self, f: impl FnOnce(&mut lopdf::Document) -> Result<()>) -> Result<()> {
        if let Err(e) = f(&mut self.doc) {
            self.doc = load(&self.bytes)?;
            return Err(e);
        }
        let bytes = Arc::new(serialize(&mut self.doc)?);
        let view = match parse_view(&bytes) {
            Ok(v) if !v.pages().is_empty() => v,
            Ok(_) => {
                self.doc = load(&self.bytes)?;
                return Err(Error::Invalid(
                    "The edit would leave the document without pages.",
                ));
            }
            Err(e) => {
                self.doc = load(&self.bytes)?;
                return Err(e);
            }
        };
        let old = std::mem::replace(&mut self.bytes, bytes);
        self.view = view;
        self.undo.push(old);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.after_change();
        Ok(())
    }

    // ----- page operations ---------------------------------------------

    /// Turns the pages by `degrees` (a multiple of 90, clockwise when positive).
    pub fn rotate_pages(&mut self, pages: &[usize], degrees: i64) -> Result<()> {
        self.edit(|doc| {
            let ids = kids(doc)?;
            for &i in pages {
                let id = *ids.get(i).ok_or(Error::Invalid("No such page."))?;
                let page = doc.get_dictionary_mut(id)?;
                let current = page.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
                page.set("Rotate", (current + degrees).rem_euclid(360));
            }
            Ok(())
        })
    }

    pub fn delete_pages(&mut self, pages: &[usize]) -> Result<()> {
        self.edit(|doc| {
            let ids = kids(doc)?;
            let keep: Vec<ObjectId> = ids
                .iter()
                .enumerate()
                .filter(|(i, _)| !pages.contains(i))
                .map(|(_, id)| *id)
                .collect();
            if keep.is_empty() {
                return Err(Error::Invalid("A PDF needs at least one page."));
            }
            set_kids(doc, &keep)?;
            doc.prune_objects();
            Ok(())
        })
    }

    /// Inserts a copy of each page right after it.
    pub fn duplicate_pages(&mut self, pages: &[usize]) -> Result<()> {
        self.edit(|doc| {
            let mut ids = kids(doc)?;
            let mut sorted = pages.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            for &i in sorted.iter().rev() {
                let src = *ids.get(i).ok_or(Error::Invalid("No such page."))?;
                let copy = copy_page(doc, src)?;
                ids.insert(i + 1, copy);
            }
            set_kids(doc, &ids)
        })
    }

    /// Inserts an empty page at `at`, sized like its neighbour.
    pub fn insert_blank_page(&mut self, at: usize) -> Result<()> {
        self.edit(|doc| {
            let mut ids = kids(doc)?;
            let at = at.min(ids.len());
            let neighbour = ids.get(at.saturating_sub(1)).or(ids.first()).copied();
            let media = neighbour
                .and_then(|id| doc.get_dictionary(id).ok())
                .and_then(|d| d.get(b"MediaBox").ok().cloned())
                .unwrap_or_else(|| vec![0.into(), 0.into(), 612.into(), 792.into()].into());
            let root = pages_root(doc)?;
            let content = doc.add_object(Stream::new(Dictionary::new(), Vec::new()));
            let page = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => root,
                "MediaBox" => media,
                "Resources" => Dictionary::new(),
                "Contents" => content,
            });
            ids.insert(at, page);
            set_kids(doc, &ids)
        })
    }

    /// Moves the given pages, keeping their relative order, so the first of them
    /// lands at `to` (an index into the page list *before* the move, 0..=len).
    pub fn move_pages(&mut self, pages: &[usize], to: usize) -> Result<()> {
        self.edit(|doc| {
            let ids = kids(doc)?;
            let order = reorder(ids.len(), pages, to);
            let new: Vec<ObjectId> = order.iter().map(|&i| ids[i]).collect();
            set_kids(doc, &new)
        })
    }

    /// A new PDF holding copies of the given pages, in the given order.
    pub fn extract_pages(&self, pages: &[usize]) -> Result<Vec<u8>> {
        if pages.is_empty() {
            return Err(Error::Invalid("Choose at least one page to extract."));
        }
        let mut doc = load(&self.bytes)?;
        let ids = kids(&doc)?;
        let keep: Vec<ObjectId> = pages
            .iter()
            .map(|&i| ids.get(i).copied().ok_or(Error::Invalid("No such page.")))
            .collect::<Result<_>>()?;
        set_kids(&mut doc, &keep)?;
        // Interactive form fields may point at pages that are gone.
        if let Ok(cat) = doc.catalog_mut() {
            cat.remove(b"AcroForm");
            cat.remove(b"Outlines");
        }
        doc.prune_objects();
        serialize(&mut doc)
    }

    /// Inserts every page of another PDF at `at`.
    pub fn insert_pdf(&mut self, data: &[u8], at: usize) -> Result<()> {
        let mut other = load(data)?;
        normalize_page_tree(&mut other)?;
        self.edit(|doc| {
            other.renumber_objects_with(doc.max_id + 1);
            let other_kids = kids(&other)?;
            let other_root = pages_root(&other)?;
            let other_catalog = other
                .trailer
                .get(b"Root")
                .and_then(Object::as_reference)
                .map_err(Error::from)?;
            let root = pages_root(doc)?;
            for (id, obj) in std::mem::take(&mut other.objects) {
                if id == other_root || id == other_catalog {
                    continue;
                }
                doc.objects.insert(id, obj);
            }
            doc.max_id = doc.max_id.max(other.max_id);
            for id in &other_kids {
                doc.get_dictionary_mut(*id)?.set("Parent", root);
            }
            let mut ids = kids(doc)?;
            let at = at.min(ids.len());
            ids.splice(at..at, other_kids);
            set_kids(doc, &ids)?;
            doc.prune_objects();
            Ok(())
        })
    }

    // ----- annotations ---------------------------------------------------

    /// Adds an annotation to page `index`; coordinates are view points.
    pub fn add_annotation(&mut self, index: usize, annot: &NewAnnot) -> Result<()> {
        let to_user = self.user_to_view(index).inverse();
        self.edit(|doc| {
            let page = *kids(doc)?
                .get(index)
                .ok_or(Error::Invalid("No such page."))?;
            annot::add(doc, page, to_user, annot)
        })
    }

    /// Annotations shown on page `index`, with rectangles in view points.
    pub fn annotations(&self, index: usize) -> Vec<AnnotSummary> {
        let Ok(ids) = kids(&self.doc) else {
            return Vec::new();
        };
        let Some(page) = ids.get(index) else {
            return Vec::new();
        };
        annot::list(&self.doc, *page, self.user_to_view(index))
    }

    /// Removes the annotation at position `slot` of the page's /Annots array.
    pub fn delete_annotation(&mut self, index: usize, slot: usize) -> Result<()> {
        self.edit(|doc| {
            let page = *kids(doc)?
                .get(index)
                .ok_or(Error::Invalid("No such page."))?;
            annot::remove(doc, page, slot)
        })
    }

    /// Reads a comment back as an editable shape, if the editor can rewrite it.
    pub fn annotation_shape(&self, index: usize, slot: usize) -> Option<NewAnnot> {
        let page = *kids(&self.doc).ok()?.get(index)?;
        annot::read(&self.doc, page, slot, self.user_to_view(index))
    }

    /// Rewrites a comment (moved, resized, recoloured or re-worded).
    pub fn replace_annotation(
        &mut self,
        index: usize,
        slot: usize,
        annot: &NewAnnot,
    ) -> Result<()> {
        let to_user = self.user_to_view(index).inverse();
        self.edit(|doc| {
            let page = *kids(doc)?
                .get(index)
                .ok_or(Error::Invalid("No such page."))?;
            annot::replace(doc, page, slot, to_user, annot)
        })
    }

    // ----- page objects ----------------------------------------------------

    /// Adds text, a picture or strokes to the page content; returns the item id.
    pub fn add_item(&mut self, index: usize, item: &NewItem) -> Result<u32> {
        let to_user = self.user_to_view(index).inverse();
        let mut id = 0;
        self.edit(|doc| {
            let page = *kids(doc)?
                .get(index)
                .ok_or(Error::Invalid("No such page."))?;
            id = items::add(doc, page, to_user, item)?;
            Ok(())
        })?;
        Ok(id)
    }

    /// Items the editor added to page `index`.
    pub fn items(&self, index: usize) -> Vec<PageItem> {
        let Some(page) = kids(&self.doc).ok().and_then(|k| k.get(index).copied()) else {
            return Vec::new();
        };
        items::list(&self.doc, page)
    }

    pub fn update_item(&mut self, index: usize, id: u32, edit: &ItemEdit) -> Result<()> {
        let to_user = self.user_to_view(index).inverse();
        self.edit(|doc| {
            let page = *kids(doc)?
                .get(index)
                .ok_or(Error::Invalid("No such page."))?;
            items::update(doc, page, to_user, id, edit)
        })
    }

    pub fn delete_item(&mut self, index: usize, id: u32) -> Result<()> {
        self.edit(|doc| {
            let page = *kids(doc)?
                .get(index)
                .ok_or(Error::Invalid("No such page."))?;
            items::delete(doc, page, id)
        })
    }

    /// The existing text line under `at`, or why it can't be edited.
    pub fn text_line_at(&self, index: usize, at: Point) -> std::result::Result<TextLine, String> {
        let page = *kids(&self.doc)
            .map_err(|e| e.to_string())?
            .get(index)
            .ok_or("No such page.")?;
        let text = self.page_text(index);
        textedit::line_at(&self.doc, page, self.user_to_view(index), &text, at)
    }

    /// Replaces the text line under `at` with `new_text` (removing the original
    /// glyphs from the content stream). `style` overrides font, size and colour.
    pub fn replace_text_line(
        &mut self,
        index: usize,
        at: Point,
        new_text: &str,
        style: Option<(FontFace, f32, Color)>,
    ) -> Result<TextLine> {
        let to_view = self.user_to_view(index);
        let text = self.page_text(index);
        let mut out = None;
        self.edit(|doc| {
            let page = *kids(doc)?
                .get(index)
                .ok_or(Error::Invalid("No such page."))?;
            out = Some(
                textedit::replace_line(doc, page, to_view, &text, at, new_text, style)
                    .map_err(Error::Refused)?,
            );
            Ok(())
        })?;
        Ok(out.expect("set on success"))
    }

    // ----- forms ------------------------------------------------------------

    /// Every form field, with its widgets in view points.
    pub fn fields(&self) -> Vec<crate::FieldInfo> {
        let Ok(ids) = kids(&self.doc) else {
            return Vec::new();
        };
        let views: Vec<hayro::kurbo::Affine> =
            (0..ids.len()).map(|i| self.user_to_view(i)).collect();
        crate::forms::list(&self.doc, &ids, &|i| views[i])
    }

    /// Fills a field: text, the chosen option, or a check box/radio export value ("Off" clears).
    pub fn set_field_value(&mut self, name: &str, value: &str) -> Result<()> {
        self.edit(|doc| crate::forms::set_value(doc, name, value))
    }

    /// Adds a field (Prepare form); returns its name.
    pub fn add_field(&mut self, index: usize, field: &crate::NewField) -> Result<String> {
        let to_user = self.user_to_view(index).inverse();
        let mut name = String::new();
        self.edit(|doc| {
            let page = *kids(doc)?
                .get(index)
                .ok_or(Error::Invalid("No such page."))?;
            name = crate::forms::add(doc, page, to_user, field)?;
            Ok(())
        })?;
        Ok(name)
    }

    /// Renames a field and sets its default value and choices.
    pub fn set_field_properties(
        &mut self,
        name: &str,
        new_name: &str,
        default: &str,
        options: &[String],
    ) -> Result<()> {
        self.edit(|doc| crate::forms::set_properties(doc, name, new_name, default, options))
    }

    /// Moves or resizes the widget in `slot` of page `index`.
    pub fn set_widget_rect(&mut self, index: usize, slot: usize, rect: crate::Rect) -> Result<()> {
        let to_user = self.user_to_view(index).inverse();
        self.edit(|doc| {
            let page = *kids(doc)?
                .get(index)
                .ok_or(Error::Invalid("No such page."))?;
            crate::forms::set_widget_rect(doc, page, slot, to_user, rect)
        })
    }

    /// Deletes the field whose widget is in `slot` of page `index`.
    pub fn delete_field(&mut self, index: usize, slot: usize) -> Result<()> {
        self.edit(|doc| {
            let ids = kids(doc)?;
            let page = *ids.get(index).ok_or(Error::Invalid("No such page."))?;
            crate::forms::delete_field(doc, &ids, page, slot)?;
            doc.prune_objects();
            Ok(())
        })
    }

    // ----- redaction ------------------------------------------------------

    /// Words and patterns to redact, with their places (view points).
    pub fn find_redactions(
        &self,
        words: &[String],
        patterns: &[crate::Pattern],
    ) -> Vec<crate::RedactHit> {
        (0..self.page_count())
            .flat_map(|i| crate::redact::find(&self.page_text(i), i, words, patterns))
            .collect()
    }

    /// Removes everything under the marks (page, rect in view points), covers them with black
    /// boxes and optionally removes hidden information. One undo step.
    pub fn redact(
        &mut self,
        marks: &[(usize, crate::Rect)],
        remove_hidden: bool,
    ) -> Result<crate::RedactReport> {
        if marks.is_empty() && !remove_hidden {
            return Err(Error::Invalid("Mark something to redact first."));
        }
        let mut by_page: std::collections::BTreeMap<usize, Vec<crate::Rect>> = Default::default();
        for (p, r) in marks {
            if *p < self.page_count() {
                by_page.entry(*p).or_default().push(r.normalized());
            }
        }
        let views: Vec<(usize, hayro::kurbo::Affine)> =
            by_page.keys().map(|&p| (p, self.user_to_view(p))).collect();
        let mut report = crate::RedactReport::default();
        self.edit(|doc| {
            let ids = kids(doc)?;
            for (p, to_view) in &views {
                crate::redact::redact_page(doc, ids[*p], *to_view, &by_page[p], *p, &mut report)?;
            }
            if remove_hidden {
                crate::redact::remove_hidden(doc, &ids, &mut report);
            }
            doc.prune_objects();
            Ok(())
        })?;
        let mut seen = std::collections::HashSet::new();
        report.unhandled.retain(|u| seen.insert(u.clone()));
        // Check: no text may remain under a mark.
        for (p, rects) in &by_page {
            let left: String = self
                .page_text(*p)
                .glyphs
                .iter()
                .filter(|g| {
                    !g.synthetic
                        && rects.iter().any(|m| {
                            m.contains(crate::Point::new(
                                (g.rect.x0 + g.rect.x1) / 2.0,
                                (g.rect.y0 + g.rect.y1) / 2.0,
                            ))
                        })
                })
                .map(|g| g.ch)
                .collect();
            if !left.trim().is_empty() {
                report.unhandled.push(format!(
                    "Page {}: some text under a mark is inside an embedded object and could not be removed (“{}”). It is covered, but still in the file.",
                    p + 1,
                    left.trim()
                ));
            }
        }
        Ok(report)
    }

    // ----- text ------------------------------------------------------------

    /// Positioned characters of page `index` (cached until the next change).
    pub fn page_text(&self, index: usize) -> Arc<PageText> {
        if let Some(Some(t)) = self.texts.borrow().get(index) {
            return t.clone();
        }
        let t = Arc::new(text::extract(&self.view.pages()[index]));
        if let Some(slot) = self.texts.borrow_mut().get_mut(index) {
            *slot = Some(t.clone());
        }
        t
    }

    /// Case-insensitive search over all pages.
    pub fn search(&self, query: &str) -> Vec<SearchHit> {
        (0..self.page_count())
            .flat_map(|i| text::search(&self.page_text(i), i, query))
            .collect()
    }
}

/// The new order of `len` items after moving `moved` so they start at `to`.
pub(crate) fn reorder(len: usize, moved: &[usize], to: usize) -> Vec<usize> {
    let mut moved: Vec<usize> = moved.iter().copied().filter(|&i| i < len).collect();
    moved.sort_unstable();
    moved.dedup();
    let rest: Vec<usize> = (0..len).filter(|i| !moved.contains(i)).collect();
    let before = moved.iter().filter(|&&i| i < to).count();
    let at = to.min(len).saturating_sub(before).min(rest.len());
    let mut out = rest[..at].to_vec();
    out.extend(&moved);
    out.extend(&rest[at..]);
    out
}

fn load(data: &[u8]) -> Result<lopdf::Document> {
    let doc = lopdf::Document::load_mem(data)?;
    if doc.trailer.has(b"Encrypt") || doc.is_encrypted() {
        return Err(Error::Encrypted);
    }
    Ok(doc)
}

fn serialize(doc: &mut lopdf::Document) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}

fn parse_view(bytes: &Arc<Vec<u8>>) -> Result<Pdf> {
    Pdf::new(bytes.clone()).map_err(|e| match e {
        hayro::hayro_syntax::LoadPdfError::Decryption(_) => Error::Encrypted,
        hayro::hayro_syntax::LoadPdfError::Invalid => Error::Parse("invalid structure".into()),
    })
}

pub(crate) fn pages_root(doc: &lopdf::Document) -> Result<ObjectId> {
    Ok(doc.catalog()?.get(b"Pages")?.as_reference()?)
}

/// Page ids in display order (the tree is flat after normalization).
pub(crate) fn kids(doc: &lopdf::Document) -> Result<Vec<ObjectId>> {
    let root = pages_root(doc)?;
    let kids = doc.get_dictionary(root)?.get(b"Kids")?.as_array()?;
    Ok(kids.iter().filter_map(|k| k.as_reference().ok()).collect())
}

fn set_kids(doc: &mut lopdf::Document, ids: &[ObjectId]) -> Result<()> {
    let root = pages_root(doc)?;
    let node = doc.get_dictionary_mut(root)?;
    node.set(
        "Kids",
        ids.iter()
            .map(|&id| Object::Reference(id))
            .collect::<Vec<_>>(),
    );
    node.set("Count", ids.len() as i64);
    Ok(())
}

/// Attributes a page may inherit from its ancestors in the page tree.
const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Flattens the page tree to a single root node whose kids are the pages, copying
/// inherited attributes onto each page. Every later page operation is then a
/// simple edit of one /Kids array.
fn normalize_page_tree(doc: &mut lopdf::Document) -> Result<()> {
    let pages: Vec<ObjectId> = doc.page_iter().collect();
    let root = pages_root(doc)?;
    for &id in &pages {
        let mut found: Vec<(&[u8], Object)> = Vec::new();
        let mut parent = doc
            .get_dictionary(id)?
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok();
        let mut guard = 0;
        while let Some(pid) = parent {
            guard += 1;
            if guard > 64 {
                break;
            }
            let Ok(node) = doc.get_dictionary(pid) else {
                break;
            };
            for key in INHERITED {
                if !found.iter().any(|(k, _)| *k == key)
                    && let Ok(v) = node.get(key)
                {
                    found.push((key, v.clone()));
                }
            }
            parent = node.get(b"Parent").and_then(Object::as_reference).ok();
        }
        let page = doc.get_dictionary_mut(id)?;
        for (key, value) in found {
            if !page.has(key) {
                page.set(key, value);
            }
        }
        if !page.has(b"MediaBox") {
            page.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
        }
        if !page.has(b"Resources") {
            page.set("Resources", Dictionary::new());
        }
        page.set("Parent", root);
    }
    let node = doc.get_dictionary_mut(root)?;
    for key in INHERITED {
        node.remove(key);
    }
    node.remove(b"Parent");
    set_kids(doc, &pages)?;
    doc.prune_objects();
    Ok(())
}

/// Copies a page object (sharing its content and resources) and its annotations.
fn copy_page(doc: &mut lopdf::Document, src: ObjectId) -> Result<ObjectId> {
    let mut page = doc.get_dictionary(src)?.clone();
    let annots: Vec<Object> = page
        .get(b"Annots")
        .and_then(|a| match a {
            Object::Reference(id) => doc.get_object(*id).and_then(Object::as_array).cloned(),
            other => other.as_array().cloned(),
        })
        .unwrap_or_default();
    page.remove(b"Annots");
    let new_id = doc.add_object(page);
    let mut copies = Vec::new();
    for a in annots {
        let Ok(id) = a.as_reference() else { continue };
        let Ok(dict) = doc.get_dictionary(id) else {
            continue;
        };
        if dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Popup") {
            continue;
        }
        // Form widgets belong to fields; copying them would create orphan widgets.
        if dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Widget") {
            continue;
        }
        let mut dict = dict.clone();
        dict.remove(b"Popup");
        dict.set("P", new_id);
        copies.push(Object::Reference(doc.add_object(dict)));
    }
    if !copies.is_empty() {
        doc.get_dictionary_mut(new_id)?.set("Annots", copies);
    }
    Ok(new_id)
}
