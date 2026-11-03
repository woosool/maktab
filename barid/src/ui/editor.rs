mod ribbon;
use super::*;
use crate::rich_text::{
    Alignment, FontFamily, InlineImage, Mark, MessageMode, RichBody, Span, safe_link,
};
use std::collections::HashMap;

#[derive(Clone)]
pub(super) enum EditorCommand {
    Align(Alignment),
    Style(Vec<Mark>),
    Mark(Mark),
    Clear,
    Bullet,
    Numbered,
    Indent,
    Outdent,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    PastePlain,
    SelectAll,
    Image,
}
#[derive(Default)]
pub(super) struct EditorState {
    pub reader_fragment: Option<Result<Option<crate::html_paste::Fragment>, String>>,
    pub inline: bool,
    pub reader_plain: bool,
    pub reader_loading: bool,
    pub reader_image_error: String,
    pub widget_id: Option<egui::Id>,
    pub inputs: [String; 3],
    pub recipient_errors: [String; 3],
    pub selection: std::ops::Range<usize>,
    pub typing: Vec<Mark>,
    pub link: String,
    pub preview: bool,
    pub error: String,
    pub pending: Option<EditorCommand>,
    pub history: Vec<EditorSnapshot>,
    pub future: Vec<EditorSnapshot>,
    pub textures: HashMap<String, egui::TextureHandle>,
    pub focus: bool,
    pub ribbon_tab: usize,
    pub ribbon_initialized: bool,
    pub ribbon_collapsed: bool,
    pub zoom: u16,
    pub find: String,
    pub replace: String,
    pub paste_fallback: Option<String>,
    pub assets: HashMap<String, InlineImage>,
}
pub(super) struct EditorSnapshot {
    body: String,
    spans: Vec<Span>,
    alignment: Alignment,
    images: Vec<InlineImage>,
    tables: Vec<crate::rich_text::RichTable>,
    rules: Vec<std::ops::Range<usize>>,
    paragraphs: Vec<crate::rich_text::RichParagraph>,
}
impl EditorState {
    fn snapshot(&mut self, draft: &Draft) -> EditorSnapshot {
        let images = draft
            .rich
            .images
            .iter()
            .map(|image| {
                self.assets
                    .entry(image.cid.clone())
                    .or_insert_with(|| image.clone());
                InlineImage {
                    cid: image.cid.clone(),
                    name: image.name.clone(),
                    mime: image.mime.clone(),
                    data: Vec::new(),
                    position: image.position,
                    width: image.width,
                    alt: image.alt.clone(),
                }
            })
            .collect();
        let mut table_rich = RichBody {
            tables: draft.rich.tables.clone(),
            ..Default::default()
        };
        table_rich.visit_images_mut(&mut |image| {
            self.assets
                .entry(image.cid.clone())
                .or_insert_with(|| image.clone());
            image.data.clear();
        });
        EditorSnapshot {
            paragraphs: draft.rich.paragraphs.clone(),
            rules: draft.rich.rules.clone(),
            tables: table_rich.tables,
            body: draft.body.clone(),
            spans: draft.rich.spans.clone(),
            alignment: draft.rich.alignment.clone(),
            images,
        }
    }
    fn restore(&mut self, draft: &mut Draft, snapshot: EditorSnapshot) {
        draft.body = snapshot.body;
        draft.rich.paragraphs = snapshot.paragraphs;
        draft.rich.rules = snapshot.rules;
        draft.rich.tables = snapshot.tables;
        draft.rich.visit_images_mut(&mut |image| {
            if let Some(asset) = self.assets.get(&image.cid) {
                image.data = asset.data.clone();
            }
        });
        draft.rich.spans = snapshot.spans;
        draft.rich.alignment = snapshot.alignment;
        draft.rich.images = snapshot
            .images
            .into_iter()
            .map(|mut image| {
                if let Some(asset) = self.assets.get(&image.cid) {
                    image.data = asset.data.clone();
                }
                image
            })
            .collect();
        state_selection_clamp(self, draft);
        self.focus = true;
    }
    fn checkpoint(&mut self, draft: &Draft) {
        let snapshot = self.snapshot(draft);
        self.history.push(snapshot);
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        self.future.clear();
    }
}
fn state_selection_clamp(state: &mut EditorState, draft: &Draft) {
    let end = state.selection.end.min(draft.body.chars().count());
    state.selection = end..end;
}
fn char_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}
fn replace_selection(draft: &mut Draft, state: &mut EditorState, text: &str) {
    let old = draft.body.clone();
    let len = old.chars().count();
    let start = state.selection.start.min(len);
    let end = state.selection.end.min(len);
    draft
        .body
        .replace_range(char_byte(&old, start)..char_byte(&old, end), text);
    draft.rich.rebase(&old, &draft.body);
    let new_end = start + text.chars().count();
    for mark in &state.typing {
        draft.rich.apply(start, new_end, mark.clone());
    }
    state.selection = new_end..new_end;
    state.focus = true;
}
fn commit_recipient(value: &mut String, input: &mut String) -> Result<(), String> {
    let text = input.trim().trim_end_matches([',', ';']).trim();
    if text.is_empty() {
        return Ok(());
    }
    let mut addresses = network::parse_addresses(value)?;
    for mailbox in network::parse_addresses(text)? {
        if !addresses.iter().any(|m| {
            m.email
                .to_string()
                .eq_ignore_ascii_case(mailbox.email.as_ref())
        }) {
            addresses.push(mailbox);
        }
    }
    *value = addresses
        .into_iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    input.clear();
    Ok(())
}
fn recipient_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    input: &mut String,
) -> Option<String> {
    let mut error = None;
    let parsed = network::parse_addresses(value);
    let mut remove = None;
    let mut edit = None;
    ui.horizontal_wrapped(|ui| {
        ui.add_sized(
            [super::ENVELOPE_LABEL_WIDTH, ui.spacing().interact_size.y],
            egui::Label::new(RichText::new(label).size(12.0).color(MUTED)),
        );
        if let Ok(addresses) = &parsed {
            for (i, address) in addresses.iter().enumerate() {
                let text = address.name.as_deref().unwrap_or(address.email.as_ref());
                let palette = super::theme::palette(ui.visuals().dark_mode);
                let response = egui::Frame::new()
                    .fill(palette.accent_soft)
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin {
                        left: 10,
                        right: 4,
                        top: 3,
                        bottom: 3,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if ui
                                .add(
                                    egui::Label::new(crate::i18n::widget(
                                        RichText::new(text).size(12.5).color(palette.accent_text),
                                        ui.style(),
                                    ))
                                    .sense(egui::Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::Text)
                                .clicked()
                            {
                                edit = Some(i);
                            }
                            if ui
                                .add(
                                    action_button(
                                        RichText::new(crate::i18n::display("×")).size(12.0),
                                    )
                                    .frame(false)
                                    .min_size(Vec2::splat(14.0)),
                                )
                                .on_hover_text(crate::i18n::display("Remove recipient"))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    })
                    .response;
                response
                    .on_hover_text(address.to_string())
                    .context_menu(|ui| {
                        if ui.add(action_button("Edit address")).clicked() {
                            edit = Some(i);
                            ui.close();
                        }
                        if ui.add(action_button("Copy email address")).clicked() {
                            ui.ctx().copy_text(address.email.to_string());
                            ui.close();
                        }
                        if ui.add(action_button("Remove recipient")).clicked() {
                            remove = Some(i);
                            ui.close();
                        }
                    });
            }
        } else {
            ui.add(egui::TextEdit::singleline(value).desired_width(200.0));
        }
        if let (Some(i), Ok(addresses)) = (edit, &parsed) {
            // Move the chip back into the input; it is re-committed on Enter.
            *input = addresses[i].to_string();
            remove = Some(i);
        }
        let response = ui.add(
            egui::TextEdit::singleline(input)
                .id_salt(label)
                .frame(egui::Frame::NONE)
                .min_size(Vec2::new(0.0, ui.spacing().interact_size.y))
                .align(egui::Align2::LEFT_CENTER)
                .desired_width(155.0)
                .hint_text(if value.is_empty() {
                    "Add email…"
                } else {
                    "Add another…"
                }),
        );
        if edit.is_some() {
            response.request_focus();
        }
        let confirm = response.has_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Tab));
        let delimiter = input.ends_with(',') || input.ends_with(';');
        if (confirm || delimiter || response.lost_focus())
            && edit.is_none()
            && !input.trim().is_empty()
            && let Err(e) = commit_recipient(value, input)
        {
            error = Some(e);
        }
    });
    if let Some(index) = remove
        && let Ok(mut addresses) = network::parse_addresses(value)
    {
        addresses.remove(index);
        *value = addresses
            .into_iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join(", ");
    }
    ui.separator();
    error
}
impl Barid {
    pub(super) fn recipient_chips(&mut self, ui: &mut egui::Ui, draft: &mut Draft, kind: usize) {
        let state = self.editor_states.entry(draft.id.clone()).or_default();
        let (label, value) = match kind {
            0 => ("To", &mut draft.to),
            1 => ("Cc", &mut draft.cc),
            _ => ("Bcc", &mut draft.bcc),
        };
        if let Some(error) = recipient_field(ui, label, value, &mut state.inputs[kind]) {
            state.recipient_errors[kind] = error;
        } else if state.inputs[kind].is_empty() {
            state.recipient_errors[kind].clear();
        }
        if !state.recipient_errors[kind].is_empty() {
            ui.label(crate::i18n::widget(
                RichText::new(&state.recipient_errors[kind])
                    .size(11.0)
                    .color(Color32::from_rgb(190, 65, 55)),
                ui.style(),
            ));
        }
    }
    pub(super) fn flush_recipients(&mut self, draft: &mut Draft) -> Result<(), String> {
        if let Some(state) = self.editor_states.get_mut(&draft.id) {
            for (kind, value) in [&mut draft.to, &mut draft.cc, &mut draft.bcc]
                .into_iter()
                .enumerate()
            {
                commit_recipient(value, &mut state.inputs[kind])
                    .map_err(|e| format!("{}: {e}", ["To", "Cc", "Bcc"][kind]))?;
            }
        }
        Ok(())
    }
    pub(super) fn rich_editor(&mut self, ui: &mut egui::Ui, draft: &mut Draft, height: f32) {
        let mut state = self.editor_states.remove(&draft.id).unwrap_or_default();
        let mut command = state.pending.take();
        if self.active_editor == draft.id && self.pending_editor.is_some() {
            command = self.pending_editor.take();
        }
        if !state.ribbon_initialized {
            state.ribbon_tab = 2;
            state.zoom = 100;
            state.ribbon_initialized = true;
        }
        // An inline reply can move to a compose window later in this same frame.
        // Draft state stays keyed by draft ID; egui widgets must stay on their own layer.
        let editor_id = egui::Id::unique(("rich_body", &draft.id, ui.layer_id()));
        if state
            .widget_id
            .is_some_and(|previous| previous != editor_id)
        {
            state.focus = true;
        }
        state.widget_id = Some(editor_id);
        if ui.ctx().memory(|m| m.has_focus(editor_id)) {
            let mut paste = None;
            let mut copy = None;
            ui.ctx().input_mut(|i| {
                i.events.retain(|event| match event {
                    egui::Event::Paste(text) => {
                        paste = Some((text.clone(), i.modifiers.shift));
                        false
                    }
                    egui::Event::Copy => {
                        copy = Some(EditorCommand::Copy);
                        false
                    }
                    egui::Event::Cut => {
                        copy = Some(EditorCommand::Cut);
                        false
                    }
                    _ => true,
                });
            });
            if let Some((text, plain)) = paste {
                state.paste_fallback = Some(text);
                command = Some(if plain {
                    EditorCommand::PastePlain
                } else {
                    EditorCommand::Paste
                });
            } else if let Some(copy) = copy {
                command = Some(copy);
            }
            if ui.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers {
                        command: true,
                        shift: true,
                        ..Default::default()
                    },
                    egui::Key::Z,
                ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
            }) {
                command = Some(EditorCommand::Redo);
            } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                command = Some(EditorCommand::Undo);
            }
        }
        let toolbar_top = ui.cursor().top();
        if let Some(ribbon_command) = ribbon::show(ui, draft, &mut state) {
            command = Some(ribbon_command);
        }
        if let Some(command) = command {
            self.apply_editor_command(ui.ctx(), draft, &mut state, command);
        }

        let height = (height - (ui.cursor().top() - toolbar_top)).max(65.0);
        let body_visuals = ui.visuals().clone();
        if state.preview {
            egui::ScrollArea::vertical()
                .id_salt(("rich_preview", &draft.id))
                .max_height(height)
                .show(ui, |ui| {
                    self.rich_preview(ui, draft, &mut state);
                });
        } else {
            let before = draft.body.clone();
            let before_snapshot = state.snapshot(draft);
            let rich = RichBody {
                spans: if draft.mode == MessageMode::Html {
                    draft.rich.spans.clone()
                } else {
                    Vec::new()
                },
                alignment: if draft.mode == MessageMode::Html {
                    draft.rich.alignment.clone()
                } else {
                    Alignment::Left
                },
                ..Default::default()
            };
            let color = ui.visuals().text_color();
            let zoom = f32::from(state.zoom.max(75)) / 100.0;
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, width: f32| {
                let mut job = rich_job(text.as_str(), &rich, color, 0);
                job.wrap.max_width = width;
                for section in &mut job.sections {
                    section.format.font_id.size *= zoom;
                }
                ui.fonts_mut(|fonts| fonts.layout_job(job))
            };
            let output = egui::ScrollArea::vertical()
                .id_salt(("rich_edit_scroll", &draft.id))
                .max_height(height)
                .show(ui, |ui| {
                    egui::TextEdit::multiline(&mut draft.body)
                        .id(editor_id)
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY)
                        .desired_rows((height / 20.0) as usize)
                        .min_size(egui::vec2(ui.available_width(), height))
                        .layouter(&mut layouter)
                        .hint_text(crate::i18n::display("Write your message…"))
                        .show(ui)
                })
                .inner;
            if output.response.changed() {
                state.history.push(before_snapshot);
                state.future.clear();
                if state.history.len() > 100 {
                    state.history.remove(0);
                }
                let inserted = draft.rich.rebase(&before, &draft.body);
                for mark in &state.typing {
                    draft.rich.apply(inserted.start, inserted.end, mark.clone());
                }
            }
            if let Some(range) = output.cursor_range
                && (output.response.has_focus() || output.response.changed())
            {
                let range = range.as_sorted_char_range();
                state.selection = range.start.0..range.end.0;
                self.active_editor = draft.id.clone();
            }
            if state.focus {
                let mut edit = output.state;
                edit.cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(state.selection.start),
                        egui::text::CCursor::new(state.selection.end),
                    )));
                edit.store(ui.ctx(), editor_id);
                output.response.request_focus();
                state.focus = false;
            }
            output.response.context_menu(|ui| {
                for (label, cmd) in [
                    ("Undo typing", EditorCommand::Undo),
                    ("Redo typing", EditorCommand::Redo),
                    ("Cut", EditorCommand::Cut),
                    ("Copy", EditorCommand::Copy),
                    ("Paste with formatting", EditorCommand::Paste),
                    ("Paste as plain text", EditorCommand::PastePlain),
                    ("Select all", EditorCommand::SelectAll),
                    ("Bold", EditorCommand::Mark(Mark::Bold)),
                    ("Italic", EditorCommand::Mark(Mark::Italic)),
                    ("Insert image…", EditorCommand::Image),
                ] {
                    if ui.add(action_button(label)).clicked() {
                        state.pending = Some(cmd);
                        ui.close();
                    }
                }
            });
        }
        *ui.visuals_mut() = body_visuals;
        if draft.mode == MessageMode::Html && !draft.rich.images.is_empty() {
            self.image_options(ui, draft, &mut state);
        }
        if !state.error.is_empty() {
            ui.label(crate::i18n::widget(
                RichText::new(&state.error)
                    .size(11.0)
                    .color(Color32::from_rgb(190, 65, 55)),
                ui.style(),
            ));
        }
        self.editor_states.insert(draft.id.clone(), state);
    }
    pub(super) fn rich_message(&mut self, ui: &mut egui::Ui, mail: &Mail) {
        let id = format!("read:{}", mail.id);
        let mut state = self.editor_states.remove(&id).unwrap_or_default();
        let rendered = state
            .reader_fragment
            .get_or_insert_with(|| {
                if mail.raw.is_empty() {
                    Ok(None)
                } else {
                    network::message_html::from_mime(&mail.raw)
                }
            })
            .clone();
        let mut draft = Draft {
            body: mail.body.clone(),
            rich: mail.rich.clone(),
            mode: mail.mode.clone(),
            ..Default::default()
        };
        if let Ok(Some(fragment)) = &rendered {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::widget(
                    crate::i18n::display("HTML message"),
                    ui.style(),
                ));
                ui.checkbox(
                    &mut state.reader_plain,
                    crate::i18n::widget(crate::i18n::display("Plain text"), ui.style()),
                );
            });
            if !state.reader_plain {
                draft.body = fragment.text.clone();
                draft.rich = fragment.rich.clone();
                draft.mode = MessageMode::Html;
                draft.rich.visit_images_mut(&mut |image| {
                    if image.data.is_empty()
                        && let Some(asset) = state.assets.get(&image.cid)
                    {
                        image.data = asset.data.clone();
                        image.mime = asset.mime.clone();
                    }
                });
                let urls: std::collections::BTreeSet<_> = draft
                    .rich
                    .all_images()
                    .into_iter()
                    .filter(|i| {
                        i.data.is_empty()
                            && (i.cid.starts_with("https://") || i.cid.starts_with("http://"))
                    })
                    .map(|i| i.cid.clone())
                    .collect();
                if !urls.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(crate::i18n::widget(
                            crate::i18n::display("Remote images are hidden"),
                            ui.style(),
                        ));
                        if ui
                            .add_enabled(!state.reader_loading, action_button("Load remote images"))
                            .clicked()
                        {
                            state.reader_loading = true;
                            state.reader_image_error.clear();
                            let tx = self.tx.clone();
                            let key = id.clone();
                            let repaint = ui.ctx().clone();
                            std::thread::spawn(move || {
                                let images =
                                    network::message_html::load_remote(urls.into_iter().collect());
                                let _ = tx.send(Event::ReaderImages(key, images));
                                repaint.request_repaint();
                            });
                        }
                        if state.reader_loading {
                            ui.spinner();
                        }
                    });
                }
                if fragment.skipped_images > 0 {
                    ui.label(crate::i18n::display(
                        "Some images could not be resolved from this message.",
                    ));
                }
            } else {
                draft.mode = MessageMode::Plain;
            }
        } else if let Err(error) = &rendered {
            ui.label(crate::i18n::widget(
                format!("HTML could not be displayed: {error}"),
                ui.style(),
            ));
            draft.mode = MessageMode::Plain;
        }
        if !state.reader_image_error.is_empty() {
            ui.label(crate::i18n::widget(&state.reader_image_error, ui.style()));
        }
        self.rich_preview(ui, &draft, &mut state);
        self.editor_states.insert(id, state);
    }
    fn apply_editor_command(
        &mut self,
        ctx: &egui::Context,
        draft: &mut Draft,
        state: &mut EditorState,
        command: EditorCommand,
    ) {
        match command {
            EditorCommand::Style(marks) => {
                state.checkpoint(draft);
                if state.selection.is_empty() {
                    state.typing = marks;
                } else {
                    draft.rich.clear(state.selection.start, state.selection.end);
                    for mark in marks {
                        draft
                            .rich
                            .apply(state.selection.start, state.selection.end, mark);
                    }
                }
                state.focus = true;
            }
            EditorCommand::Align(align) => {
                state.checkpoint(draft);
                draft.rich.alignment = align;
            }
            EditorCommand::Mark(mark) => {
                state.checkpoint(draft);
                if state.selection.is_empty() {
                    if let Some(i) = state.typing.iter().position(|m| m.same_kind(&mark)) {
                        if state.typing[i] == mark
                            && matches!(
                                mark,
                                Mark::Bold
                                    | Mark::Italic
                                    | Mark::Underline
                                    | Mark::Strike
                                    | Mark::Superscript
                                    | Mark::Subscript
                            )
                        {
                            state.typing.remove(i);
                        } else {
                            state.typing[i] = mark;
                        }
                    } else {
                        state.typing.push(mark);
                    }
                } else {
                    draft
                        .rich
                        .apply(state.selection.start, state.selection.end, mark);
                }
                state.focus = true;
            }
            EditorCommand::Clear => {
                state.checkpoint(draft);
                draft.rich.clear(state.selection.start, state.selection.end);
                state.typing.clear();
                state.focus = true;
            }
            EditorCommand::SelectAll => {
                state.selection = 0..draft.body.chars().count();
                state.focus = true;
            }
            EditorCommand::Copy | EditorCommand::Cut => {
                let len = draft.body.chars().count();
                let start = state.selection.start.min(len);
                let end = state.selection.end.min(len);
                let text = &draft.body[char_byte(&draft.body, start)..char_byte(&draft.body, end)];
                let mut rich = RichBody {
                    alignment: draft.rich.alignment.clone(),
                    ..Default::default()
                };
                for span in draft
                    .rich
                    .spans
                    .iter()
                    .filter(|s| s.start < end && s.end > start)
                {
                    rich.spans.push(Span {
                        start: span.start.max(start) - start,
                        end: span.end.min(end) - start,
                        mark: span.mark.clone(),
                    });
                }
                rich.rules = draft
                    .rich
                    .rules
                    .iter()
                    .filter(|r| r.start >= start && r.end <= end)
                    .map(|r| r.start - start..r.end - start)
                    .collect();
                rich.paragraphs = draft
                    .rich
                    .paragraphs
                    .iter()
                    .filter(|p| p.start >= start && p.end <= end)
                    .cloned()
                    .map(|mut p| {
                        p.start -= start;
                        p.end -= start;
                        p
                    })
                    .collect();
                rich.tables = draft
                    .rich
                    .tables
                    .iter()
                    .filter(|t| t.start >= start && t.end <= end)
                    .cloned()
                    .map(|mut t| {
                        t.start -= start;
                        t.end -= start;
                        t
                    })
                    .collect();
                if draft.mode == MessageMode::Plain
                    || arboard::Clipboard::new()
                        .and_then(|mut clipboard| {
                            clipboard.set_html(rich.html(text), Some(text.to_string()))
                        })
                        .is_err()
                {
                    ctx.copy_text(text.into());
                }
                if matches!(command, EditorCommand::Cut) {
                    state.checkpoint(draft);
                    replace_selection(draft, state, "");
                }
            }
            EditorCommand::Paste | EditorCommand::PastePlain => {
                match clipboard_fragment(
                    matches!(command, EditorCommand::PastePlain)
                        || draft.mode == MessageMode::Plain,
                    state.paste_fallback.take(),
                ) {
                    Ok(fragment) => insert_fragment(draft, state, fragment),
                    Err(e) => state.error = e,
                }
            }
            EditorCommand::Undo => {
                if let Some(snapshot) = state.history.pop() {
                    let current = state.snapshot(draft);
                    state.future.push(current);
                    state.restore(draft, snapshot);
                }
            }
            EditorCommand::Redo => {
                if let Some(snapshot) = state.future.pop() {
                    let current = state.snapshot(draft);
                    state.history.push(current);
                    state.restore(draft, snapshot);
                }
            }
            EditorCommand::Bullet
            | EditorCommand::Numbered
            | EditorCommand::Indent
            | EditorCommand::Outdent => {
                state.checkpoint(draft);
                let start = char_byte(&draft.body, state.selection.start);
                let end = char_byte(&draft.body, state.selection.end);
                let line_start = draft.body[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
                let line_end = draft.body[end..]
                    .find('\n')
                    .map(|p| p + end)
                    .unwrap_or(draft.body.len());
                let text = draft.body[line_start..line_end]
                    .lines()
                    .enumerate()
                    .map(|(i, line)| match command {
                        EditorCommand::Bullet => format!("• {line}"),
                        EditorCommand::Numbered => format!("{}. {line}", i + 1),
                        EditorCommand::Indent => format!("    {line}"),
                        _ => line.strip_prefix("    ").unwrap_or(line).to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                state.selection = draft.body[..line_start].chars().count()
                    ..draft.body[..line_end].chars().count();
                replace_selection(draft, state, &text);
            }
            EditorCommand::Image => {
                state.checkpoint(draft);
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
                    .pick_files()
                {
                    for path in paths {
                        match read_inline_image(&path, state.selection.end) {
                            Ok(image) => draft.rich.images.push(image),
                            Err(e) => state.error = e,
                        }
                    }
                }
            }
        }
    }
    fn image_options(&self, ui: &mut egui::Ui, draft: &mut Draft, state: &mut EditorState) {
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt(("image_options", &draft.id))
            .max_height(110.0)
            .show(ui, |ui| {
                for (i, image) in draft.rich.images.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        show_image(ui, image, &mut state.textures, 64.0);
                        ui.vertical(|ui| {
                            ui.label(crate::i18n::widget(
                                RichText::new(&image.name).size(11.0),
                                ui.style(),
                            ));
                            ui.horizontal(|ui| {
                                ui.label(crate::i18n::widget(
                                    crate::i18n::display("Width"),
                                    ui.style(),
                                ));
                                ui.add(
                                    egui::DragValue::new(&mut image.width)
                                        .range(40..=1200)
                                        .suffix(" px"),
                                );
                            });
                            ui.add(
                                egui::TextEdit::singleline(&mut image.alt)
                                    .hint_text(crate::i18n::display("Image description"))
                                    .desired_width(180.0),
                            );
                        });
                        if ui.add(action_button("Remove").small()).clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
        if let Some(i) = remove {
            let image = draft.rich.images.remove(i);
            state.textures.remove(&image.cid);
        }
    }
    fn rich_preview(&self, ui: &mut egui::Ui, draft: &Draft, state: &mut EditorState) {
        // A message written right to left (its first letter is Arabic or Hebrew) reads from the
        // right edge unless it sets an alignment of its own.
        let rtl = draft.body.chars().find(|c| c.is_alphabetic()).is_some_and(|c| ('\u{0590}'..='\u{08FF}').contains(&c));
        let side = if rtl { egui::Align::Max } else { egui::Align::Min };
        if draft.mode == MessageMode::Plain {
            ui.with_layout(egui::Layout::top_down(side), |ui| {
                let label = egui::Label::new(crate::i18n::widget(&draft.body, ui.style()));
                ui.add(if rtl { label.halign(egui::Align::RIGHT) } else { label });
            });
            return;
        }
        let mut rich = draft.rich.clone();
        if rtl && rich.alignment == crate::rich_text::Alignment::Left { rich.alignment = crate::rich_text::Alignment::Right; }
        ui.with_layout(egui::Layout::top_down(side), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            self.render_body(ui, &draft.body, &rich, state, 0);
        });
    }
    fn render_body(
        &self,
        ui: &mut egui::Ui,
        text: &str,
        rich: &RichBody,
        state: &mut EditorState,
        depth: usize,
    ) {
        if depth > 8 {
            return;
        }
        enum Block<'a> {
            Image(&'a InlineImage),
            Table(&'a crate::rich_text::RichTable),
            Rule,
            Paragraph(&'a crate::rich_text::RichParagraph),
        }
        let mut blocks: Vec<(usize, usize, Block<'_>)> = rich
            .images
            .iter()
            .filter(|i| {
                !rich
                    .paragraphs
                    .iter()
                    .any(|p| p.start <= i.position && i.position < p.end)
            })
            .map(|i| (i.position, i.position, Block::Image(i)))
            .chain(
                rich.tables
                    .iter()
                    .map(|t| (t.start, t.end, Block::Table(t))),
            )
            .chain(
                rich.paragraphs
                    .iter()
                    .map(|p| (p.start, p.end, Block::Paragraph(p))),
            )
            .chain(rich.rules.iter().map(|r| (r.start, r.end, Block::Rule)))
            .collect();
        blocks.sort_by_key(|b| b.0);
        let len = text.chars().count();
        let mut start = 0;
        for (pos, end, block) in blocks {
            let pos = pos.min(len);
            if pos < start {
                continue;
            }
            if pos > start {
                ui.add(
                    egui::Label::new(crate::i18n::widget(
                        rich_job(
                            &text[char_byte(text, start)..char_byte(text, pos)],
                            rich,
                            ui.visuals().text_color(),
                            start,
                        ),
                        ui.style(),
                    ))
                    .halign(match rich.alignment {
                        Alignment::Left => egui::Align::Min,
                        Alignment::Center => egui::Align::Center,
                        Alignment::Right => egui::Align::Max,
                    })
                    .wrap()
                    .selectable(true),
                );
            }
            let paragraph_block = matches!(block, Block::Paragraph(_));
            match block {
                Block::Paragraph(p) => {
                    let mut child = rich.clone();
                    child.paragraphs.clear();
                    child.tables.clear();
                    child.rules.clear();
                    child.alignment = p.alignment.clone();
                    child.spans = rich
                        .spans
                        .iter()
                        .filter_map(|s| {
                            let a = s.start.max(pos);
                            let b = s.end.min(end);
                            (a < b).then(|| Span {
                                start: a - pos,
                                end: b - pos,
                                mark: s.mark.clone(),
                            })
                        })
                        .collect();
                    child.images = rich
                        .images
                        .iter()
                        .filter(|i| pos <= i.position && i.position < end)
                        .cloned()
                        .map(|mut i| {
                            i.position -= pos;
                            i
                        })
                        .collect();
                    ui.add_space(p.before);
                    egui::Frame::new()
                        .inner_margin(egui::Margin {
                            left: p.indent.min(ui.available_width() * 0.6).min(127.0) as i8,
                            ..Default::default()
                        })
                        .show(ui, |ui| {
                            self.render_body(
                                ui,
                                &text[char_byte(text, pos)..char_byte(text, end.min(len))],
                                &child,
                                state,
                                depth + 1,
                            );
                        });
                    ui.add_space(p.after);
                }
                Block::Rule => {
                    ui.separator();
                }
                Block::Image(image) => {
                    if image.data.is_empty()
                        && let Some(asset) = state.assets.get(&image.cid)
                    {
                        let mut loaded = image.clone();
                        loaded.data = asset.data.clone();
                        show_image(ui, &loaded, &mut state.textures, image.width as f32);
                    } else {
                        show_image(ui, image, &mut state.textures, image.width as f32);
                    }
                }
                Block::Table(table) => self.render_table(ui, table, state, depth + 1),
            }
            start = end.min(len).max(pos);
            if paragraph_block && text.chars().nth(start) == Some('\n') {
                start += 1;
            }
        }
        if start < len {
            ui.add(
                egui::Label::new(crate::i18n::widget(
                    rich_job(
                        &text[char_byte(text, start)..],
                        rich,
                        ui.visuals().text_color(),
                        start,
                    ),
                    ui.style(),
                ))
                .halign(match rich.alignment {
                    Alignment::Left => egui::Align::Min,
                    Alignment::Center => egui::Align::Center,
                    Alignment::Right => egui::Align::Max,
                })
                .wrap()
                .selectable(true),
            );
        }
    }
    fn render_table(
        &self,
        ui: &mut egui::Ui,
        table: &crate::rich_text::RichTable,
        state: &mut EditorState,
        depth: usize,
    ) {
        let (columns, cells) = table_cells(table);
        if columns == 0 {
            return;
        }
        let desired = table
            .width
            .or_else(|| {
                if table.columns.is_empty() {
                    None
                } else {
                    Some(table.columns.iter().sum())
                }
            })
            .map(|w| w as f32)
            .unwrap_or(ui.available_width())
            .min(ui.available_width())
            .max(columns as f32 * 45.0);
        egui::ScrollArea::horizontal()
            .id_salt(("html_table", table.start, table.end))
            .auto_shrink([true, true])
            .show(ui, |ui| {
                let mut widths: Vec<_> = (0..columns)
                    .map(|i| table.columns.get(i).map(|w| *w as f32))
                    .collect();
                for placed in &cells {
                    if placed.colspan == 1
                        && let Some(w) = placed.cell.width
                    {
                        widths[placed.column] = Some((w as f32).min(desired));
                    }
                }
                let fixed: f32 = widths.iter().flatten().sum();
                let free = widths.iter().filter(|w| w.is_none()).count();
                let mut widths: Vec<f32> = widths
                    .into_iter()
                    .map(|w| w.unwrap_or(((desired - fixed) / free.max(1) as f32).max(45.0)))
                    .collect();
                let total: f32 = widths.iter().sum();
                if total > desired {
                    for w in &mut widths {
                        *w = (*w * desired / total).max(35.0);
                    }
                }
                let origin = ui.cursor().min;
                let mut row_heights = vec![0.0f32; table.rows.len() + 128];
                let mut tops = vec![origin.y];
                let mut backgrounds = Vec::new();
                let row_count = cells
                    .iter()
                    .map(|c| c.row + c.rowspan)
                    .max()
                    .unwrap_or(table.rows.len());
                for row in 0..row_count {
                    let y = tops[row];
                    for (index, placed) in cells.iter().enumerate().filter(|(_, c)| c.row == row) {
                        let x = origin.x + widths[..placed.column].iter().sum::<f32>();
                        let width = widths[placed.column..placed.column + placed.colspan]
                            .iter()
                            .sum::<f32>();
                        let background = ui.painter().add(egui::Shape::Noop);
                        let response = ui.scope_builder(
                            egui::UiBuilder::new()
                                .id_salt(("html_cell", index))
                                .max_rect(egui::Rect::from_min_size(
                                    egui::pos2(x, y),
                                    egui::vec2(width, 10000.0),
                                ))
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                            |ui| {
                                egui::Frame::new()
                                    .inner_margin(egui::Margin::symmetric(
                                        placed.cell.padding[0] as i8,
                                        placed.cell.padding[1] as i8,
                                    ))
                                    .show(ui, |ui| {
                                        ui.set_width(
                                            (width - 2.0 * placed.cell.padding[0] as f32).max(10.0),
                                        );
                                        self.render_body(
                                            ui,
                                            &placed.cell.text,
                                            &placed.cell.rich,
                                            state,
                                            depth,
                                        );
                                    });
                            },
                        );
                        let height =
                            response.response.rect.height().max(28.0) / placed.rowspan as f32;
                        for min in &mut row_heights[row..row + placed.rowspan] {
                            *min = (*min).max(height);
                        }
                        backgrounds.push((
                            background,
                            x,
                            width,
                            row,
                            placed.rowspan,
                            placed.cell.background.or(table.background),
                            table.border || placed.cell.border,
                        ));
                    }
                    tops.push(y + row_heights[row].max(28.0));
                }
                for (index, x, width, row, span, color, border) in backgrounds {
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(x, tops[row]),
                        egui::pos2(x + width, tops[row + span]),
                    );
                    if let Some(c) = color {
                        ui.painter().set(
                            index,
                            egui::Shape::rect_filled(
                                rect,
                                0.0,
                                Color32::from_rgb(c[0], c[1], c[2]),
                            ),
                        );
                    }
                    if border {
                        ui.painter().rect_stroke(
                            rect,
                            0.0,
                            Stroke::new(1.0, Color32::from_gray(190)),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                ui.advance_cursor_after_rect(egui::Rect::from_min_max(
                    origin,
                    egui::pos2(
                        origin.x + widths.iter().sum::<f32>(),
                        *tops.last().unwrap_or(&origin.y),
                    ),
                ));
            });
    }
}
struct PlacedCell<'a> {
    row: usize,
    column: usize,
    colspan: usize,
    rowspan: usize,
    cell: &'a crate::rich_text::RichCell,
}
fn table_cells(table: &crate::rich_text::RichTable) -> (usize, Vec<PlacedCell<'_>>) {
    let mut occupied = vec![[false; 32]; table.rows.len() + 128];
    let mut cells = Vec::new();
    let mut columns = 0;
    for (row, entries) in table.rows.iter().enumerate() {
        let mut column = 0;
        for cell in entries {
            let span = cell.colspan.clamp(1, 32);
            while column < 32
                && occupied[row][column..(column + span).min(32)]
                    .iter()
                    .any(|v| *v)
            {
                column += 1;
            }
            if column >= 32 {
                break;
            }
            let colspan = span.min(32 - column);
            let rowspan = cell.rowspan.clamp(1, 128);
            for occupied in &mut occupied[row..row + rowspan] {
                occupied[column..column + colspan].fill(true);
            }
            cells.push(PlacedCell {
                row,
                column,
                colspan,
                rowspan,
                cell,
            });
            column += colspan;
            columns = columns.max(column);
        }
    }
    (columns, cells)
}
fn rich_job(text: &str, rich: &RichBody, color: Color32, offset: usize) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob {
        halign: match rich.alignment {
            Alignment::Left => egui::Align::LEFT,
            Alignment::Center => egui::Align::Center,
            Alignment::Right => egui::Align::RIGHT,
        },
        ..Default::default()
    };
    for (i, c) in text.chars().enumerate() {
        let mut format = egui::TextFormat {
            font_id: egui::FontId::proportional(14.0),
            color,
            ..Default::default()
        };
        let mut bold = false;
        let mut size = 14.0;
        for span in rich
            .spans
            .iter()
            .filter(|s| s.start <= i + offset && i + offset < s.end)
        {
            match &span.mark {
                Mark::Family(family) => {
                    format.font_id = egui::FontId::new(
                        size,
                        match family {
                            FontFamily::Sans => egui::FontFamily::Proportional,
                            FontFamily::Serif => egui::FontFamily::Name("barid-serif".into()),
                            FontFamily::Mono => egui::FontFamily::Monospace,
                        },
                    )
                }
                Mark::Highlight(c) => format.background = Color32::from_rgb(c[0], c[1], c[2]),
                Mark::Superscript => {
                    format.valign = egui::Align::TOP;
                    size = 11.0;
                }
                Mark::Subscript => {
                    format.valign = egui::Align::BOTTOM;
                    size = 11.0;
                }
                Mark::Bold => bold = true,
                Mark::Italic => format.italics = true,
                Mark::Underline => format.underline = Stroke::new(1.0, color),
                Mark::Strike => format.strikethrough = Stroke::new(1.0, color),
                Mark::Color(c) => format.color = Color32::from_rgb(c[0], c[1], c[2]),
                Mark::Size(s) => size = (*s).clamp(10, 48) as f32,
                Mark::Link(_) => {
                    format.color = BLUE;
                    format.underline = Stroke::new(1.0, BLUE);
                }
            }
        }
        let family = format.font_id.family.clone();
        format.font_id = if bold {
            match &family {
                egui::FontFamily::Proportional => bold_font(size),
                egui::FontFamily::Monospace => {
                    egui::FontId::new(size, egui::FontFamily::Name("barid-mono-bold".into()))
                }
                egui::FontFamily::Name(name) if name.as_ref() == "barid-serif" => {
                    egui::FontId::new(size, egui::FontFamily::Name("barid-serif-bold".into()))
                }
                _ => egui::FontId::new(size, family),
            }
        } else {
            egui::FontId::new(size, family)
        };
        format.line_height = Some(size * 1.35);
        job.append(&c.to_string(), 0.0, format);
    }
    job
}
fn read_inline_image(path: &std::path::Path, position: usize) -> Result<InlineImage, String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 15_000_000 {
        return Err("Choose an image smaller than 15 MB.".into());
    }
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let format = image::guess_format(&data).map_err(|e| e.to_string())?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Gif => "image/gif",
        image::ImageFormat::WebP => "image/webp",
        _ => return Err("Choose a PNG, JPEG, GIF, or WebP image.".into()),
    };
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(&data), format)
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    if u64::from(width) * u64::from(height) > 32_000_000 {
        return Err("Choose an image with at most 32 megapixels.".into());
    }
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    Ok(InlineImage {
        cid: format!("{}@barid", uuid::Uuid::new_v4()),
        alt: name.clone(),
        name,
        mime: mime.into(),
        data,
        position,
        width: width.min(480),
    })
}
fn show_image(
    ui: &mut egui::Ui,
    image: &InlineImage,
    textures: &mut HashMap<String, egui::TextureHandle>,
    max_width: f32,
) {
    if image.data.is_empty() {
        ui.label(crate::i18n::widget(format!("[{}]", image.alt), ui.style()));
        return;
    }
    if !textures.contains_key(&image.cid)
        && let Ok(decoded) = image::load_from_memory(&image.data)
    {
        let rgba = decoded.thumbnail(2048, 2048).to_rgba8();
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        );
        textures.insert(
            image.cid.clone(),
            ui.ctx()
                .load_texture(&image.cid, color, egui::TextureOptions::LINEAR),
        );
    }
    if let Some(texture) = textures.get(&image.cid) {
        let size = texture.size_vec2();
        let width = max_width.min(ui.available_width()).min(size.x);
        ui.add(
            egui::Image::new(texture).fit_to_exact_size(Vec2::new(width, width * size.y / size.x)),
        );
    }
}

fn insert_fragment(
    draft: &mut Draft,
    state: &mut EditorState,
    fragment: crate::html_paste::Fragment,
) {
    state.checkpoint(draft);
    let start = state.selection.start.min(draft.body.chars().count());
    replace_selection(draft, state, &fragment.text);
    for mut span in fragment.rich.spans {
        span.start += start;
        span.end += start;
        draft.rich.spans.push(span);
    }
    for mut p in fragment.rich.paragraphs {
        p.start += start;
        p.end += start;
        draft.rich.paragraphs.push(p);
    }
    for mut rule in fragment.rich.rules {
        rule.start += start;
        rule.end += start;
        draft.rich.rules.push(rule);
    }
    for mut table in fragment.rich.tables {
        table.start += start;
        table.end += start;
        draft.rich.tables.push(table);
    }
    for mut image in fragment.rich.images {
        image.position += start;
        draft.rich.images.push(image);
    }
    if fragment.rich.alignment != Alignment::Left {
        draft.rich.alignment = fragment.rich.alignment;
    }
    state.error = if fragment.skipped_images > 0 {
        format!(
            "{} external image(s) were not imported. Insert local images to include them.",
            fragment.skipped_images
        )
    } else {
        String::new()
    };
    state.focus = true;
}
fn clipboard_fragment(
    plain: bool,
    fallback: Option<String>,
) -> Result<crate::html_paste::Fragment, String> {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(e) => {
            return fallback
                .map(|text| crate::html_paste::Fragment {
                    text,
                    ..Default::default()
                })
                .ok_or(e.to_string());
        }
    };
    if !plain
        && let Ok(html) = clipboard.get().html()
        && !html.trim().is_empty()
    {
        let fragment = crate::html_paste::import_html(&html)?;
        if !fragment.text.is_empty() || !fragment.rich.images.is_empty() {
            return Ok(fragment);
        }
    }
    let text = fallback.or_else(|| clipboard.get_text().ok());
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        return Ok(crate::html_paste::Fragment {
            text,
            ..Default::default()
        });
    }
    if !plain && let Ok(image) = clipboard.get_image() {
        if image.width.saturating_mul(image.height) > 32_000_000 {
            return Err("Choose an image with at most 32 megapixels.".into());
        }
        let pixels = image::RgbaImage::from_raw(
            image.width as u32,
            image.height as u32,
            image.bytes.into_owned(),
        )
        .ok_or("Invalid clipboard image")?;
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        return Ok(crate::html_paste::Fragment {
            rich: RichBody {
                tables: Vec::new(),
                rules: Vec::new(),
                paragraphs: Vec::new(),
                images: vec![InlineImage {
                    cid: format!("{}@barid", uuid::Uuid::new_v4()),
                    name: "Clipboard image.png".into(),
                    mime: "image/png".into(),
                    data: bytes.into_inner(),
                    position: 0,
                    width: (image.width as u32).min(480),
                    alt: "Clipboard image".into(),
                }],
                ..Default::default()
            },
            ..Default::default()
        });
    }
    Err("The clipboard has no supported text, HTML, or image.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_tables_keep_columns_spans_colors_and_survive_paste_undo_and_mime() {
        let source = include_str!("../../tests/fixtures/table-email.html");
        let fragment = crate::html_paste::import_html(source).unwrap();
        let table = &fragment.rich.tables[0];
        assert_eq!(table.width, Some(625));
        assert_eq!(table.rows[0].len(), 4);
        assert_eq!(table.rows[0][0].background, Some([31, 78, 120]));
        assert!(
            table.rows[0][0]
                .rich
                .spans
                .iter()
                .any(|s| s.mark == Mark::Color([255, 255, 255]))
        );
        assert_eq!(table.rows[2][0].background, Some([242, 245, 248]));
        let (columns, cells) = table_cells(table);
        assert_eq!(columns, 4);
        assert!(cells.iter().any(|c| c.colspan == 2));
        assert!(cells.iter().any(|c| c.rowspan == 2));
        assert_eq!(
            cells
                .iter()
                .find(|c| c.row == 4 && c.cell.text == "Row two")
                .unwrap()
                .column,
            3
        );
        let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        let ctx = egui::Context::default();
        let mut draft = Draft {
            to: "you@example.com".into(),
            ..Default::default()
        };
        let mut state = EditorState::default();
        state.checkpoint(&draft);
        insert_fragment(&mut draft, &mut state, fragment);
        let raw = network::build_message(
            &Account {
                email: "me@example.com".into(),
                ..Default::default()
            },
            &draft,
        )
        .unwrap()
        .formatted();
        let received = network::message_html::from_mime(&raw).unwrap().unwrap();
        assert_eq!(received.rich.tables[0].rows[0].len(), 4);
        assert_eq!(received.rich.tables[0].width, Some(625));
        assert_eq!(received.text.matches("Technical Handover").count(), 1);
        app.apply_editor_command(&ctx, &mut draft, &mut state, EditorCommand::Undo);
        assert!(draft.rich.tables.is_empty());
        app.apply_editor_command(&ctx, &mut draft, &mut state, EditorCommand::Redo);
        assert_eq!(draft.rich.tables.len(), 1);
        assert_eq!(draft.rich.rules.len(), 1);
    }
    #[test]
    fn paragraph_indentation_applies_to_the_entire_wrapped_block() {
        let app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        let f = crate::html_paste::import_html(r#"<p style="margin:0">Baseline</p><p style="margin-left:36pt;padding-left:24pt">Indented paragraph with enough words to wrap onto several lines within this narrow reading pane.</p>"#).unwrap();
        let draft = Draft {
            body: f.text,
            rich: f.rich,
            ..Default::default()
        };
        let mut state = EditorState::default();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(380.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| app.rich_preview(ui, &draft, &mut state),
        );
        let mut base = None;
        let mut indented = None;
        output.textures_delta.clear();
        for shape in output.shapes {
            if let egui::Shape::Text(t) = shape.shape {
                if t.galley.text() == "Baseline" {
                    base = Some(t.pos.x);
                }
                if t.galley.text().starts_with("Indented paragraph") {
                    assert!(t.galley.rows.len() > 1);
                    indented = Some(t.pos.x);
                }
            }
        }
        assert!((indented.unwrap() - base.unwrap() - 80.0).abs() < 1.0);
    }
    #[test]
    fn table_cells_render_as_columns_on_a_white_canvas_in_dark_mode() {
        let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        app.dark = true;
        app.prefs.email_body_dark = false;
        let raw = format!(
            "Content-Type: text/html; charset=utf-8\r\n\r\n{}",
            include_str!("../../tests/fixtures/table-email.html")
        );
        app.mails = vec![Mail {
            id: "table".into(),
            raw: raw.into_bytes(),
            ..Default::default()
        }];
        app.selected = Some("table".into());
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        ctx.set_visuals(egui::Visuals::dark());
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1440.0, 1600.0),
                )),
                ..Default::default()
            },
            |ui| {
                app.reader(ui);
                assert!(ui.visuals().dark_mode);
            },
        );
        fn visit(shape: &egui::Shape, white: &mut bool, headers: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        visit(shape, white, headers)
                    }
                }
                egui::Shape::Rect(rect)
                    if rect.fill == Color32::WHITE
                        && rect.rect.width() > 500.0
                        && rect.rect.height() >= 160.0 =>
                {
                    *white = true
                }
                egui::Shape::Text(text)
                    if [
                        "Layer",
                        "Component",
                        "Version / Specification",
                        "Architectural Purpose",
                    ]
                    .contains(&text.galley.text()) =>
                {
                    headers.push(text.pos)
                }
                _ => {}
            }
        }
        let mut white = false;
        let mut headers = Vec::new();
        for shape in &output.shapes {
            visit(&shape.shape, &mut white, &mut headers);
        }
        assert!(white);
        assert_eq!(headers.len(), 4);
        assert!(
            headers
                .windows(2)
                .all(|p| p[0].x < p[1].x && (p[0].y - p[1].y).abs() < 1.0)
        );
        output.textures_delta.clear();
    }
    #[test]
    fn cached_mime_html_is_rendered_with_its_signature_image_without_resyncing() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(32, 16, image::Rgba([24, 118, 224, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let mut draft = Draft {
            to: "you@example.com".into(),
            body: "Signature".into(),
            ..Default::default()
        };
        draft.rich.apply(0, 9, Mark::Bold);
        draft.rich.images.push(InlineImage {
            cid: "logo@example.com".into(),
            name: "signature.png".into(),
            mime: "image/png".into(),
            data: bytes.into_inner(),
            position: 9,
            width: 32,
            alt: "Signature logo".into(),
        });
        let raw = network::build_message(
            &Account {
                email: "me@example.com".into(),
                ..Default::default()
            },
            &draft,
        )
        .unwrap()
        .formatted();
        let mail = Mail {
            id: "cached-html".into(),
            body: "Plain fallback for quoting".into(),
            raw,
            ..Default::default()
        };
        let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.rich_message(ui, &mail));
        output.textures_delta.clear();
        let state = &app.editor_states["read:cached-html"];
        assert!(
            matches!(&state.reader_fragment,Some(Ok(Some(fragment))) if fragment.text.contains("Signature")&&fragment.rich.images.len()==1)
        );
        assert!(state.textures.contains_key("logo@example.com"));
        assert!(!state.reader_loading);
        assert_eq!(mail.body, "Plain fallback for quoting");
    }
    #[test]
    fn html_paste_inserts_at_selection_and_undo_redo_restores_styles_and_images() {
        let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        let ctx = egui::Context::default();
        let mut draft = Draft {
            body: "Start end".into(),
            ..Default::default()
        };
        let mut state = EditorState {
            selection: 6..6,
            ..Default::default()
        };
        let mut fragment = crate::html_paste::import_html("<b>café</b>").unwrap();
        fragment.rich.images.push(InlineImage {
            cid: "test@barid".into(),
            name: "test.png".into(),
            mime: "image/png".into(),
            data: vec![1, 2, 3],
            position: 4,
            width: 180,
            alt: "Test".into(),
        });
        insert_fragment(&mut draft, &mut state, fragment);
        assert_eq!(draft.body, "Start caféend");
        assert!(
            draft
                .rich
                .spans
                .iter()
                .any(|s| s.start == 6 && s.end == 10 && s.mark == Mark::Bold)
        );
        assert_eq!(draft.rich.images[0].position, 10);
        app.apply_editor_command(&ctx, &mut draft, &mut state, EditorCommand::Undo);
        assert_eq!(draft.body, "Start end");
        assert!(draft.rich.spans.is_empty());
        assert!(draft.rich.images.is_empty());
        app.apply_editor_command(&ctx, &mut draft, &mut state, EditorCommand::Redo);
        assert_eq!(draft.body, "Start caféend");
        assert_eq!(draft.rich.images[0].data, [1, 2, 3]);
    }
    #[test]
    fn recipient_chips_commit_quoted_names_and_deduplicate_case_insensitively() {
        let mut value = String::new();
        let mut input = "\"Chen, Amelia\" <amelia@example.com>, bob@example.com,".to_string();
        commit_recipient(&mut value, &mut input).unwrap();
        assert!(input.is_empty());
        assert_eq!(network::parse_addresses(&value).unwrap().len(), 2);
        let mut duplicate = "AMELIA@example.com".to_string();
        commit_recipient(&mut value, &mut duplicate).unwrap();
        assert_eq!(network::parse_addresses(&value).unwrap().len(), 2);
        let old = value.clone();
        let mut invalid = "not an email".to_string();
        assert!(commit_recipient(&mut value, &mut invalid).is_err());
        assert_eq!(value, old);
        assert_eq!(invalid, "not an email");
    }
    #[test]
    fn image_insertion_validates_content_and_uses_a_unique_cid() {
        let path = std::env::temp_dir().join(format!("barid-image-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::new(2, 3).save(&path).unwrap();
        let image = read_inline_image(&path, 4).unwrap();
        assert_eq!(image.mime, "image/png");
        assert_eq!(image.position, 4);
        assert_eq!(image.width, 2);
        assert!(image.cid.ends_with("@barid"));
        assert_ne!(read_inline_image(&path, 4).unwrap().cid, image.cid);
        std::fs::write(&path, b"not an image").unwrap();
        assert!(read_inline_image(&path, 0).is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn three_compose_windows_keep_independent_drafts_and_editor_state() {
        let mut app = Barid::with_store(Store::open(std::path::Path::new(":memory:")).unwrap());
        let ctx = egui::Context::default();
        configure_fonts(&ctx);
        let mut ids = Vec::new();
        for i in 0..3 {
            app.compose("new");
            let draft = app.composer.as_mut().unwrap();
            draft.subject = format!("Message {i}");
            draft.body = format!("Body {i}");
            draft.rich.apply(0, 4, Mark::Bold);
            ids.push(draft.id.clone());
        }
        assert_eq!(app.compose_sessions.len(), 2);
        app.save_all_composers();
        assert_eq!(app.store.mails().unwrap().len(), 3);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1920.0, 1080.0),
                )),
                ..Default::default()
            },
            |_| app.compose_windows(&ctx),
        );
        output.textures_delta.clear();
        let drafts: Vec<_> = app
            .compose_sessions
            .iter()
            .map(|s| &s.draft)
            .chain(app.composer.iter())
            .collect();
        assert_eq!(drafts.len(), 3);
        for (i, id) in ids.iter().enumerate() {
            let draft = drafts.iter().find(|d| d.id == *id).unwrap();
            assert_eq!(draft.body, format!("Body {i}"));
            assert_eq!(draft.rich.spans.len(), 1);
        }
    }
    #[test]
    fn styled_draft_roundtrip_preserves_formatting_and_image_data() {
        let draft = Draft {
            id: "rich-draft".into(),
            body: "Hello café".into(),
            rich: RichBody {
                tables: Vec::new(),
                rules: Vec::new(),
                paragraphs: Vec::new(),
                spans: vec![Span {
                    start: 0,
                    end: 5,
                    mark: Mark::Bold,
                }],
                images: vec![InlineImage {
                    cid: "image@barid".into(),
                    name: "photo.png".into(),
                    mime: "image/png".into(),
                    data: vec![0, 1, 2],
                    position: 5,
                    width: 320,
                    alt: "A picture".into(),
                }],
                alignment: Alignment::Center,
            },
            ..Default::default()
        };
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        store.save("mail", &draft.id, &draft.as_mail()).unwrap();
        let mail = store.mails().unwrap().remove(0);
        assert_eq!(mail.rich.spans[0].mark, Mark::Bold);
        assert_eq!(mail.rich.images[0].data, [0, 1, 2]);
        assert_eq!(mail.rich.alignment, Alignment::Center);
    }
}
