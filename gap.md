# Qalam issue triage — gaps

Issues from the external tracker were checked against Qalam in 54 tasks of 40 issues (`.claude/wordcraft-chunks/taskNN.txt`, results in `progress-taskNN.md`).

## Tasks

| Task | Checked | Fixed | Not affected | Not applicable | Gap |
|---|---|---|---|---|---|
| 00 | 40/40 | 1 | 24 | 6 | 9 |
| 01 | 40/40 | 4 | 19 | 4 | 13 |
| 02 | 40/40 | 4 | 20 | 12 | 4 |
| 03 | 40/40 | 4 | 16 | 16 | 4 |
| 04 | 40/40 | 5 | 20 | 2 | 13 |
| 05 | 40/40 | 6 | 21 | 4 | 9 |
| 06 | 40/40 | 2 | 9 | 25 | 4 |
| 07 | 40/40 | 4 | 14 | 13 | 9 |
| 08 | 40/40 | 3 | 15 | 19 | 3 |
| 09 | 40/40 | 3 | 12 | 15 | 10 |
| 10 | 13/40 | 7 | 6 | 0 | 0 |
| 11 | 40/40 | 1 | 10 | 25 | 4 |
| 12 | 40/40 | 2 | 8 | 20 | 10 |
| 13 | 40/40 | 2 | 7 | 28 | 3 |
| 14 | 40/40 | 2 | 9 | 27 | 2 |
| 15 | 40/40 | 9 | 9 | 19 | 3 |
| 16 | 40/40 | 4 | 10 | 22 | 4 |
| 17 | 40/40 | 8 | 16 | 13 | 3 |
| 18 | 40/40 | 3 | 16 | 19 | 2 |
| 19 | 40/40 | 9 | 11 | 18 | 2 |
| 20 | 40/40 | 3 | 6 | 31 | 0 |
| 21 | 40/40 | 2 | 11 | 20 | 7 |
| 22 | 40/40 | 3 | 7 | 27 | 3 |
| 23 | 40/40 | 4 | 18 | 8 | 10 |
| 24 | 40/40 | 9 | 18 | 7 | 6 |
| 25 | 40/40 | 1 | 10 | 28 | 1 |
| 26 | not started | | | | |
| 27 | not started | | | | |
| 28 | not started | | | | |
| 29 | not started | | | | |
| 30 | not started | | | | |
| 31 | not started | | | | |
| 32 | not started | | | | |
| 33 | not started | | | | |
| 34 | not started | | | | |
| 35 | not started | | | | |
| 36 | not started | | | | |
| 37 | not started | | | | |
| 38 | not started | | | | |
| 39 | not started | | | | |
| 40 | not started | | | | |
| 41 | not started | | | | |
| 42 | not started | | | | |
| 43 | not started | | | | |
| 44 | not started | | | | |
| 45 | not started | | | | |
| 46 | not started | | | | |
| 47 | not started | | | | |
| 48 | not started | | | | |
| 49 | not started | | | | |
| 50 | not started | | | | |
| 51 | not started | | | | |
| 52 | not started | | | | |
| 53 | not started | | | | |

**Totals:** 1013 checked — not-applicable: 428, not-affected: 342, affected-not-fixed: 138, fixed: 105

## Affected, not fixed

- task 00: #66 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no (non-visual) | umbrella RTL request; core RTL/shaping/Arabic UI (kit/src/i18n.rs Lang::Ar) present, but RTL tables/sections (bidiVisual, sectPr bidi) and kashida missing (see #362, #1891)
- task 00: #362 | affected-not-fixed | screenshots: 0 viewed | qalam.png: yes | w:bidiVisual only in TBLPR_ORDER (docx.rs:1636), never read; tables/sections always laid out LTR; not a focused fix
- task 00: #529 | affected-not-fixed | screenshots: 2 viewed | qalam.png: yes | screenshots: Arabic table column order reversed vs Word; same root as #362 (no bidiVisual table layout); qalam.png confirms Arabic table columns run left to right
- task 00: #615 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no (non-visual) | duplicate of #362: bidiVisual ignored
- task 00: #1383 | affected-not-fixed | screenshots: 1 viewed | qalam.png: no (non-visual) | engine/src/pdf.rs emits glyphs in visual order with ToUnicode only, no ActualText/ReversedChars; RTL extraction order likely visual (unverified, needs PDF text extractor in tests)
- task 00: #1891 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no (non-visual) | format.rs:162 maps lowKashida/mediumKashida/highKashida to plain Justify; no kashida elongation in layout (feature, not focused)
- task 00: #2883 | affected-not-fixed | screenshots: 2 viewed | qalam.png: yes | screenshots: Recent filenames as ??? (not affected, see #2067) and Persian table columns LTR, which Qalam shares (no bidiVisual, #362)
- task 00: #2930 | affected-not-fixed | screenshots: 1 viewed | qalam.png: no (non-visual) | screenshot: RTL section should place pages right to left in multi-page view; Qalam has no section direction (sections.rs, no sectPr bidi)
- task 00: #3436 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no (non-visual) | engine/src/proofing.rs:39 treats any non-ASCII word as correct; no Hebrew dictionary (needs dictionary data)
- task 01: #4174 | affected-not-fixed | screenshots: 1 viewed | Find/Navigation query is plain egui TextEdit (app/src/ui.rs:113, nav.rs:46); egui has no bidi/shaping, Hebrew/Arabic shows in logical order. Needs bidi-aware text input, not focused | qalam.png: yes (nav search box shows "بالعالم مرحبا" reversed -> defect confirmed in Qalam)
- task 01: #4200 | affected-not-fixed | screenshots: 0 viewed | fonts.rs maps Arial->Liberation Sans and system fonts per OS; Arabic falls back to bundled Noto, so page count differs by platform; needs metric-compatible Arabic face / substitution report | qalam.png: no (non-visual)
- task 01: #4350 | affected-not-fixed | screenshots: 0 viewed | proofing.rs uses only bundled en_US Hunspell; no custom/Arabic dictionaries (feature) | qalam.png: no (non-visual)
- task 01: #4553 | affected-not-fixed | screenshots: 0 viewed | no direction shortcut in app/src/ui.rs shortcuts(); egui can't tell left vs right Ctrl+Shift, needs a different binding | qalam.png: no (non-visual)
- task 01: #4800 | affected-not-fixed | screenshots: 0 viewed | find_all(query, match_case, whole_words) has no diacritic-insensitive option (feature) | qalam.png: no (non-visual)
- task 01: #4976 | affected-not-fixed | screenshots: 0 viewed | no PDF import/OCR in Qalam open_path (feature) | qalam.png: no (non-visual)
- task 01: #5297 | affected-not-fixed | screenshots: 0 viewed | kit i18n Lang is En/Fr/Ar only; no Hebrew UI or spell-check (feature) | qalam.png: no (non-visual)
- task 01: #7 | affected-not-fixed | screenshots: 0 viewed | open_path handles docx/md/txt only, no .tex (feature) | qalam.png: no (non-visual)
- task 01: #8 | affected-not-fixed | screenshots: 0 viewed | Settings::load defaults lang to "en" (app/src/options.rs), no system-locale detection | qalam.png: no (non-visual)
- task 01: #25 | affected-not-fixed | screenshots: 0 viewed | spelling English only (proofing.rs) (feature) | qalam.png: no (non-visual)
- task 01: #28 | affected-not-fixed | screenshots: 0 viewed | no .doc reader; open_path sends .doc to docx::read which errors (shows "Couldn't open", not an empty doc) | qalam.png: no (non-visual)
- task 01: #40 | affected-not-fixed | screenshots: 0 viewed | only en_US dictionary (proofing.rs) (feature) | qalam.png: no (non-visual)
- task 01: #41 | affected-not-fixed | screenshots: 0 viewed | formatting changes not tracked; existing w:rPrChange/w:pPrChange dropped on import (docx.rs:150,212); needs revision model work | qalam.png: no (non-visual)
- task 02: #55 | affected-not-fixed | screenshots: 0 viewed | no ECMA-376 agile-encryption open/save in engine/src/docx.rs (feature, not focused) | qalam.png: no (non-visual)
- task 02: #94 | affected-not-fixed | screenshots: 0 viewed | app.rs uses synchronous rfd::FileDialog on the UI thread (rfd 0.17 xdg-portal on Linux); needs async dialog refactor | qalam.png: no (non-visual)
- task 02: #100 | affected-not-fixed | screenshots: 0 viewed | proofing.rs bundles en_US Hunspell only, ignores w:lang; multilingual dictionaries is a feature | qalam.png: no (non-visual)
- task 02: #115 | affected-not-fixed | screenshots: 0 viewed | options.rs Settings.dark is a bool; no follow-system theme option (feature) | qalam.png: no (non-visual)
- task 03: #122 | affected-not-fixed | screenshots: 0 viewed | canvas.rs uses default egui ScrollArea: no kinetic/momentum touchpad scrolling; needs scroll-physics work, not a focused fix | qalam.png: no (non-visual)
- task 03: #146 | affected-not-fixed | screenshots: 0 viewed | no New/Modify Table Style UI in Qalam; feature too large for focused fix | qalam.png: no (non-visual)
- task 03: #219 | affected-not-fixed | screenshots: 1 viewed | fonts.rs register_font_file keeps only regular/bold per typographic family; Light/Medium/SemiBold faces collapse, "Jost Light" not addressable; needs weight model | qalam.png: yes (only Regular/Bold faces available)
- task 03: #226 | affected-not-fixed | screenshots: 1 viewed | Qalam has no cell Text Direction (vertical tbRl/btLr) support; tcPr textDirection only round-tripped in cell.extra; vertical layout too large | qalam.png: yes (Arabic cell horizontal only, no vertical direction)
- task 04: #265 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | Group button always disabled (app/src/ribbon.rs:1212); grouping is a feature, not focused
- task 04: #266 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no Manage Styles dialog in app/src
- task 04: #269 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no Asian Typography paragraph options
- task 04: #275 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | shadow/glow only as text effects (ribbon.rs:241); no shape effects
- task 04: #288 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no ruby/Phonetic Guide (no w:ruby support)
- task 04: #297 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no Enclose Characters
- task 04: #320 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no Tabs dialog nor paragraph Borders and Shading dialog (only Page Borders, design.rs:128)
- task 04: #326 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no display-equation line breaking (no m:brk) in engine/src/equation.rs
- task 04: #328 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | multilevel list exists; no Track Changes Options dialog
- task 04: #339 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no Toggle Field Codes / Unlink / Lock / F9/F11 field commands
- task 04: #343 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | Move Paragraph Up/Down exists (canvas.rs:999); Go Back (Shift+F5) and Alt+X missing
- task 04: #352 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | footnotes are not split across pages in layout.rs (no continuation)
- task 04: #368 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no Customize Keyboard dialog
- task 05: #369 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | feature gap: objects.rs shapes have no text direction/anchor/linked text boxes; large parity feature
- task 05: #377 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no Templates and Add-ins (attachedTemplate) UI; feature gap
- task 05: #379 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | Customize Ribbon exists (options.rs) but no Quick Access Toolbar customization; feature gap
- task 05: #381 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | docx.rs reads w:sdt by flattening sdtContent; content controls are lost on save; large feature
- task 05: #385 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no Footnote and Endnote dialog / per-section footnotePr options; feature gap
- task 05: #391 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | w:docGrid only kept in SECTPR_ORDER; no grid layout/East Asian line-break options; feature gap
- task 05: #395 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | Draw tab lacks lasso/ink-to-shape/replay; feature gap
- task 05: #400 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | docx.rs reads w:moveFrom/moveTo as Del/Ins (no move kind); feature gap
- task 05: #416 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | WordArt present (ribbon.rs) but no shape connectors (only straightConnector1 as Arrow); feature gap
- task 06: #464 | affected-not-fixed | screenshots: 0 viewed | docx.rs BodyReader::block ignores body-level w:bookmarkStart/End, so a bookmark around a final table is dropped entirely
- task 06: #475 | affected-not-fixed | screenshots: 0 viewed | feature request: no interface scale setting in options.rs
- task 06: #485 | affected-not-fixed | screenshots: 0 viewed | feature request: Options not split into Word-style panes
- task 06: #488 | affected-not-fixed | screenshots: 0 viewed | feature request: canvas exposes no AccessKit text nodes
- task 07: #491 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no vertical section text (w:textDirection only in docx.rs element order, not modelled/laid out); large layout feature
- task 07: #498 | affected-not-fixed | screenshots: 1 viewed | qalam.png: no | proofing.rs bundles only en_US Hunspell; adding Indonesian needs a dictionary + language selection (feature)
- task 07: #534 | affected-not-fixed | screenshots: 0 viewed (uploads failed; only docx/doc attachments) | qalam.png: no | docx.rs never reads w:rFonts/@w:eastAsia (writes Latin font into it), so a document's Chinese font is lost; needs model field for East Asian font. Tofu part fixed under #505
- task 07: #543 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | Find (app.rs FindState: match_case, whole_words only) has no ^p/^t codes or wildcards; feature
- task 07: #544 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no TWo INitial CApitals / cAPS LOCK AutoCorrect in Qalam; feature
- task 07: #546 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no +---+ table / > arrow-bullet AutoFormat As You Type; feature
- task 07: #548 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no Flat OPC / Word 2003 XML open or save (app.rs open filter: docx/dotx/docm/dotm/md/txt); feature
- task 07: #575 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | no LanguageTool integration (proofing.rs local Hunspell + few grammar checks); feature
- task 07: #589 | affected-not-fixed | screenshots: 0 viewed | qalam.png: no | pdf.rs export draws no comment markup (no "comment" in pdf.rs/print.rs); print-with-markup feature
- task 08: #634 | affected-not-fixed | screenshots: 1 viewed | phone-width window (360px): Qalam ribbon is also cut off (qalam.png); mobile layout is a large feature; qalam.png: yes
- task 08: #662 | affected-not-fixed | screenshots: 0 viewed | at 1000px Qalam Home tab cuts off groups after Styles (img/662/qalam.png); adaptive group collapsing is a large ribbon change; qalam.png: yes
- task 08: #687 | affected-not-fixed | screenshots: 0 viewed | external rich text pastes as plain (app.rs paste_text / paste_merged use text only; no HTML clipboard import); Paste Special/Merge Formatting work; needs HTML importer; qalam.png: no
- task 09: #700 | affected-not-fixed | screenshots: 0 viewed | ruler.rs shows/drag table column markers and cell-relative indents, but the vertical ruler has no row markers (feature work)
- task 09: #711 | affected-not-fixed | screenshots: 0 viewed | column border drag (canvas.rs Drag::Column) has no Ctrl proportional / Alt measure / left-edge / selected-cells variants (feature work)
- task 09: #714 | affected-not-fixed | screenshots: 0 viewed | Qalam has no custom table style editor (feature request)
- task 09: #743 | affected-not-fixed | screenshots: 1 viewed | qalam.png: yes, Business Plan cover in Qalam shows title text only, no coloured cover panel (pctPosVOffset in mc:AlternateContent, unclosed custGeom fill, theme gradient) - larger drawing work
- task 09: #766 | affected-not-fixed | screenshots: 0 viewed | WordApp::new(cc, None) opens a blank document, no Start screen (feature request)
- task 09: #775 | affected-not-fixed | screenshots: 0 viewed | no STYLEREF field in engine (Field enum); needs new field evaluation in headers
- task 09: #777 | affected-not-fixed | screenshots: 2 viewed | qalam.png: yes, spacing_test.docx 6 lines like Word, Wingdings empty lines normal height (qalam-wingdings.png); default page is Letter (engine/src/page.rs:77), A4 default is a locale preference not done
- task 09: #778 | affected-not-fixed | screenshots: 1 viewed | xlsx supported, rows can be excluded, but CSV decoded with from_utf8_lossy (app/src/mailings.rs:39) so GBK CSV headers garble; needs a legacy-encoding decoder
- task 09: #780 | affected-not-fixed | screenshots: 1 viewed | qalam.png: no (no file attached); drawing-canvas shapes missing in Wordcraft; cannot reproduce without the document
- task 09: #788 | affected-not-fixed | screenshots: 0 viewed | Qalam selection is one anchor/focus range; Ctrl+click multi-selection not supported (feature)
- task 11: #897 | affected-not-fixed | screenshots: 0 viewed | rendered Qalam French UI with caret in table: fits at 1280px, but at 820px the tab strip runs under the quick-access/search area and the Table tab can't be reached (ribbon.rs tab strip has no overflow handling); test tests.rs wordcraft_task11_shots; qalam.png: yes
- task 11: #966 | affected-not-fixed | screenshots: 0 viewed | not verified: layout.rs keep_next/widow interplay needs a pagination test, too big for this pass
- task 11: #980 | affected-not-fixed | screenshots: 0 viewed | Qalam has no OS full-screen toggle (no Fullscreen viewport command in app/src); feature, not a focused fix
- task 11: #988 | affected-not-fixed | screenshots: 1 viewed | compared the WordCraft ribbon with a Qalam render: Qalam's Font/Paragraph icons are also ~16px; a design preference, not a defect; qalam.png: yes
- task 12: #1012 | affected-not-fixed | KeyTips/ribbon screen-reader exposure is a broad accessibility feature, not a focused fix
- task 12: #1014 | affected-not-fixed | canvas.rs has no Ctrl+drag duplicate of objects; feature work
- task 12: #1015 | affected-not-fixed | Save Changes dialog accessibility announcements: broad a11y feature
- task 12: #1031 | affected-not-fixed | Selection model is single anchor/focus range; multi-range Ctrl selection is a large feature
- task 12: #1047 | affected-not-fixed | no user font import UI (only system fonts via fonts.rs); feature request
- task 12: #1055 | affected-not-fixed | comment visibility option is a new feature request
- task 12: #1065 | affected-not-fixed | no middle-click autoscroll (no PointerButton::Middle handling); feature request
- task 12: #1067 | affected-not-fixed | no =rand()/=lorem() expansion in editing; feature request
- task 12: #1077 | affected-not-fixed | forms/read-only regions are a large feature
- task 12: #1091 | affected-not-fixed | large-window performance is profiling work, not a focused fix
- task 13: #1171 | affected-not-fixed | Qalam ignores fonts embedded in .docx (no odttf deobfuscation/loading); needs font-system integration, not a focused fix
- task 13: #1174 | affected-not-fixed | gridBefore is laid out and saved (layout.rs grid_before, docx.rs gridBefore); FORMCHECKBOX is drawn as a box glyph but ffData is not written back on save, so form field types are lost: needs a form-field model
- task 13: #1181 | affected-not-fixed | Qalam has no mirror-margins model (w:mirrorMargins only kept in settings order); even pages use the same left/right margins; needs per-page geometry in layout
- task 14: #1243 | affected-not-fixed | copy puts only plain text on the system clipboard (app.rs copy -> ctx.copy_text); HTML clipboard export is a larger feature
- task 14: #1290 | affected-not-fixed | docx reader has no VML v:textpath watermark import; needs new header-shape import
- task 15: #1317 | affected-not-fixed | review.rs compare diffs only body paragraphs (cell paragraphs excluded), so table changes are untracked; table-level revision model absent
- task 15: #1320 | affected-not-fixed | no page vertical alignment (sectPr w:vAlign) in model/layout; feature request, not a focused fix
- task 15: #1321 | affected-not-fixed | engine/src/picture.rs uses image::load_from_memory with only the crate's default limits; no explicit raster budget
- task 16: #1365 | affected-not-fixed | word-engine depends on rustybuzz 0.20 / ttf-parser 0.25; migrating the shaper is not a focused fix
- task 16: #1396 | affected-not-fixed | Qalam has no native NSMenu bar (egui ribbon only); large feature
- task 16: #1422 | affected-not-fixed | proofing ships only dict/en_US; adding en_GB needs a new dictionary asset and language setting
- task 16: #1434 | affected-not-fixed | interface languages are En/Fr/Ar only; Vietnamese translation is a feature
- task 17: #1454 | affected-not-fixed | proofing is English-only; adding Spanish (es-419) dictionaries is a feature, not a focused fix
- task 17: #1500 | affected-not-fixed | Shape.fill is Option<Color>; gradients/alpha flattened and saved as solidFill - needs gradient fill model + renderer
- task 17: #1523 | affected-not-fixed | copy puts only plain text on the system clipboard; no HTML serializer exists to offer text/html
- task 18: #1548 | affected-not-fixed | Thai lays out with breaks mid-word ("…จั|ดการ"), "[A]" alone on line 1; needs a dictionary line segmenter (none in deps)
- task 18: #1549 | affected-not-fixed | fonts::fallback picks a face per character (Thai -> system Thai font, Latin/digits stay in the run's face); run-level substitution is a font-engine redesign
- task 19: #1687 | affected-not-fixed | DrawingML gradient fills with alpha on shapes aren't read (no gradFill support for shapes); needs a gradient-fill model and renderer
- task 19: #1744 | affected-not-fixed | Document::fragment drops cell membership, so copying a table pastes paragraphs; needs table-carrying clipboard fragments
- task 21: #1890 | affected-not-fixed | objects.rs ChartKind has no Radar (Column/Bar/Line/Pie/Area only); a new chart type is not a focused fix
- task 21: #1893 | affected-not-fixed | feature request (type into shape on double-click, shape gallery as pictures); Shape has text but no double-click-to-type path checked; out of scope
- task 21: #1894 | affected-not-fixed | feature request: picture recolor gallery thumbnails; not a defect
- task 21: #1910 | affected-not-fixed | SVG maps to image/svg+xml in docx.rs but asvg:svgBlip read/sharp rendering is a feature, not focused
- task 21: #1938 | affected-not-fixed | app.rs paste_text pastes clipboard as plain text only; reading HTML table clipboard flavor is a larger feature
- task 21: #1944 | affected-not-fixed | legacy OLE Equation 3.0 objects are not rendered (equation.rs handles OMML only); large feature
- task 21: #1984 | affected-not-fixed | page borders (page.rs) have no art borders; feature request
- task 22: #2068 | affected-not-fixed | font picker has no Recently Used section; feature, not a focused fix
- task 22: #2080 | affected-not-fixed | Ctrl +/- work (ui.rs:59); no configurable default zoom setting (feature)
- task 22: #2108 | affected-not-fixed | Print dialog has no paper size option (print.rs Settings); feature
- task 23: #2129 | affected-not-fixed | no reproduction details; Malayalam justify depends on shaping/justification of complex scripts, not a focused fix
- task 23: #2155 | affected-not-fixed | touch drag selects text (egui reports touch as pointer); needs touch-vs-mouse input model, not focused
- task 23: #2191 | affected-not-fixed | broad docx drawing fidelity (lines/arcs, gradients, SmartArt, wrapNone, font fallback): multiple large features
- task 23: #2197 | affected-not-fixed | Footer Three Columns uses tab stops with [Type here] placeholders, not a table; design change not focused
- task 23: #2221 | affected-not-fixed | egui full redraw on software rasterizer is architectural
- task 23: #2246 | affected-not-fixed | no sample file; header/footer edit via story swap works in tests; cannot reproduce
- task 23: #2253 | affected-not-fixed | macOS press-and-hold accent menu relies on winit/NSTextInputClient support; not testable headless
- task 23: #2277 | affected-not-fixed | rotate drag shows only a dashed rotated frame + angle, image not rotated live; needs rotated texture preview
- task 23: #2278 | affected-not-fixed | GPU/driver black window, no repro
- task 23: #2289 | affected-not-fixed | comments added while editing a header live in the swapped story doc and aren't listed/navigable; needs comments outside body
- task 24: #2361 | affected-not-fixed | fixture render: header logo (wps anchor inside framed header paragraph) not drawn and header wraps; needs frame + header float support, not a focused fix
- task 24: #2363 | affected-not-fixed | band shape and text box both get z=0 (relativeHeight 251658241/242 collapse under the /1024 scaling), so the band covers the white link text; band fill schemeClr tx2 also unresolved. Needs a z-order representation change
- task 24: #2399 | affected-not-fixed | glyph outlines come from ttf-parser without a TrueType hinting interpreter, which tricky fonts like DFKai-SB need; font not available to test
- task 24: #2406 | affected-not-fixed | document places most text with w:framePr frames on a zero-margin page; Qalam has no text-frame positioning (only drop caps), so text stacks at top-left
- task 24: #2417 | affected-not-fixed | forms protection undoes every edit (enforce_protection), and form fields/content controls are flattened on read, so fields can't be filled; needs form-field model
- task 24: #2427 | affected-not-fixed | all block w:sdt wrappers are flattened on read (docx.rs block "w:sdt") so the TOC content control is not written back; needs a block content-control model
- task 25: #2534 | affected-not-fixed | w:outline/w:shadow drawn; w:emboss/w:imprint are not read or drawn (needs new rendering effect, not a focused fix)

## Known Arabic / RTL gaps

- Right-to-left tables (w:bidiVisual): columns are not mirrored.
- Kashida justification for Arabic is not done.
- PDF text copied out of Arabic may come out in the wrong order.
- Multi-page view lays pages left to right even for RTL documents.
- egui's bidi comes from the vendored patch in `vendor/epaint`; it must be carried forward when egui is upgraded.
