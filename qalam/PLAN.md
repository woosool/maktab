# Feature port plan

Order: output first (users need to share documents), then the review and
reference features most Word users touch, then layout depth, then objects.
Each step ships with engine tests, DOCX round-trip coverage and, where it has
UI, a headless GUI test. Status: ✅ done · 🚧 in progress · ⬜ planned.

| # | Step | JS sources | Status |
|---|---|---|---|
| 1 | PDF export with searchable text (embedded fonts, pictures, links) | `views/backstage.js` savePdf, `lib/document-export.js` | ✅ |
| 2 | Incremental layout (per-paragraph measure cache) so typing stays fast in long documents | `engine/paragraph-measure-cache.js` | ✅ |
| 2a | Ribbon UI with the JS command icons (resvg, DPI-sharp), title bar, status bar | `ui/ribbon.js`, `ui/command-icons.js` | ✅ |
| 2b | Text: drag-and-drop move/copy, word/paragraph/line/sentence selection, Move Paragraph Up/Down, Format Painter | `views/editor.js`, `engine/canvas-editor.js` | ✅ |
| 2c | Tables: cell blocks, merge/split cells, split table, row height, distribute, AutoFit, column drag, 9-way alignment, shading, borders, quick styles, header rows, sort, text↔table, per-side cell borders with pen, cell margins, insert/delete cells, formulas, Draw Table pen/eraser | `engine/table-editing.js`, `table-formula.js`, `cell-borders.js`, `cell-margins.js`, `views/table-tools.js` | ✅ |
| 3 | Comments: anchors, margin thread column, reply/resolve/delete, DOCX `comments.xml` | `engine/comments.js`, `docx-comments.js`, `views/review.js` | ✅ |
| 4 | Track changes: record insertions/deletions/format changes, show markup, accept/reject | `engine/revisions.js`, `views/review.js` | ✅ (insertions/deletions/paragraph marks; formatting changes ⬜) |
| 5 | Footnotes and endnotes: insert, layout at page foot, round-trip | `engine/footnotes.js`, `footnote-layout.js`, `docx-notes.js` | ✅ (note text edited in a pane) |
| 6 | Fields: TOC, PAGEREF/REF, SEQ captions, DATE/AUTHOR; Update Fields (F9) | `engine/body-fields.js`, `field-update.js`, `toc-*` | ⬜ |
| 7 | Bookmarks and cross-references UI | `engine/bookmarks.js` | ⬜ |
| 8 | Header/footer editing: create, different first/odd-even, page-number insertion | `engine/page-stories.js`, `docx-page-story-create.js` | ⬜ |
| 9 | Multiple sections, section breaks, columns, line numbers, page borders/backgrounds | `engine/section-*`, `column-layout.js`, `page-borders.js` | ⬜ |
| 10 | Style editing: modify/create paragraph and character styles | `engine/styles.js` editNamedStyle | ⬜ |
| 11 | Floating pictures and text wrapping (square/tight/top-bottom, behind/in front) | `engine/floating-images.js`, `image-wrap.js` | ✅ |
| 12 | Shapes, text boxes, preset geometries | `engine/canvas-shapes.js`, `preset-geometries*.js` | ⬜ |
| 13 | Charts (bar/line/pie/doughnut/xy) | `engine/canvas-*charts.js`, `docx-charts.js` | ⬜ |
| 14 | Equations (OMML linear editing and display) | `engine/office-math.js`, `math-linear.js` | ⬜ |
| 15 | Spell checking (Hunspell-format dictionaries) | `nspell`, `views/proofing` | ⬜ |
| 16 | Content controls, mail merge | `engine/content-controls.js`, `merge-fields.js` | ⬜ |
| 17 | Bidi / RTL text, Arabic/Hebrew shaping, Unicode graphemes, script fallback, glyph-id rendering | `engine/bidi-*.js` | ✅ (kerning on request ⬜) |
| 18 | Importers: RTF, HTML, HWP | `lib/hwp`, rtf.js | ⬜ |
| 19 | Printing via the OS print dialog | backstage print | ⬜ |
