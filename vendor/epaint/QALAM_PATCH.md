# epaint 0.37.0 with right-to-left text

This is epaint 0.37.0 from crates.io with bidi support added. Every app in this repository uses it
through `[patch.crates-io]` in its Cargo.toml (qalam, sard, kit, jadwal, barid, pdf, saboura).

## What changed

- `src/text/text_layout/bidi.rs` (new): the Unicode bidi algorithm (UAX #9) on top of egui's layout.
- `src/text/text_layout.rs`:
  - shaping runs are split where the bidi level changes, and each is shaped in its own direction
    (`shape_text(.., rtl)`);
  - a right-to-left run's glyphs are put back in reading order, one glyph per char; extra glyphs
    of a char (the dots of an Arabic letter) go in `Paragraph::attached` / `Row::attached`;
  - after line breaking, each row is moved into visual order (`bidi::reorder_row`);
  - `halign_and_justify_row` uses the min/max x of a row, not its first and last glyph;
  - `tessellate_glyphs` also draws attached glyphs.
- `src/text/text_layout_types.rs`: `Row::attached`; `Row::char_at` and `Row::x_offset` put
  carets and clicks on the right side of right-to-left letters.
- `src/shapes/text_shape.rs`: the `Row` destructure names `attached`.
- `Cargo.toml`: depends on `unicode-bidi`.

## Upgrading egui

1. Copy the new epaint version over this folder, keeping this file.
2. Apply the changes above again (`git log -p -- vendor/epaint` shows them).
3. Run `cargo test` in `kit` (the `arabic_words_run_right_to_left` test checks glyph positions)
   and in `qalam` (`arabic_comment_reads_right_to_left`).
4. If egui gains bidi support of its own, drop this folder and the `[patch.crates-io]` entries.
