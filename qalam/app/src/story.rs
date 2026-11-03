//! Editing headers and footers in place, as Word does: the story becomes the edited document,
//! laid out where it sits on the page, while the body shows dimmed behind it.

use crate::app::WordApp;
use word_engine::layout::{DrawItem, Layout, PageBox};
use word_engine::model::{Document, Paragraph, Story};

pub struct StoryEdit {
    pub header: bool,
    /// The document body, set aside while the story is edited.
    pub host: Document,
    pub host_layout: Layout,
    /// The page whose header or footer was opened (the caret's page).
    pub page: usize,
    /// The story before editing, to tell whether it changed.
    pub original: Option<Story>,
}

fn shift_item(it: &mut DrawItem, dy: f32) {
    match it {
        DrawItem::Glyphs { baseline, .. } => *baseline += dy,
        DrawItem::Rect { y, .. } | DrawItem::Image { y, .. } | DrawItem::Frame { y, .. } => *y += dy,
        DrawItem::Line { y1, y2, .. } => { *y1 += dy; *y2 += dy; }
        DrawItem::Path { points, .. } => for p in points { p.1 += dy; },
    }
}

impl WordApp {
    /// Edit the header (or footer) on `page` (default: the caret's page).
    pub fn edit_story(&mut self, header: bool, page: Option<usize>) {
        if self.extras.story.is_some() { self.end_story(); }
        let page = page.unwrap_or_else(|| self.layout.page_of(self.doc.range().0));
        let mut host = std::mem::replace(&mut self.doc, Document::new(vec![Paragraph::new("")]));
        let original = host.story(header).cloned();
        let paras = original.as_ref().map(|s| s.paragraphs.clone()).unwrap_or_else(|| vec![Paragraph::new("")]);
        // The body's layout without the story being edited (it's drawn live from the story).
        if header { host.header = None; } else { host.footer = None; }
        let host_layout = word_engine::layout_document(&host);
        let mut story = Document::new(paras);
        story.styles = host.styles.clone();
        story.numbering = host.numbering.clone();
        story.assets = host.assets.clone();
        story.properties = host.properties.clone();
        story.file_name = host.file_name.clone();
        story.clock = host.clock;
        story.local_clock = host.local_clock;
        story.revision_author = host.revision_author.clone();
        story.page = host.page.clone();
        story.page.color = None;
        story.page.borders = None;
        story.page.title_page = false;
        // Headers start at the header distance; footers are moved to sit above the footer distance.
        story.page.top = if header { host.page.header } else { 0 };
        story.page.bottom = 0;
        let end = story.len();
        story.select(end, end);
        story.dirty = false;
        self.doc = story;
        self.extras.story = Some(StoryEdit { header, host, host_layout, page, original });
        self.layout = Layout::default();
        self.tab = crate::ribbon::Tab::HeaderFooter;
        self.canvas_focus = true;
        self.relayout();
        self.scroll_to_caret = true;
    }

    /// Close the header or footer and put its content back into the document.
    pub fn end_story(&mut self) {
        let Some(st) = self.extras.story.take() else { return };
        let story = std::mem::replace(&mut self.doc, st.host);
        if st.header { self.doc.header = st.original.clone(); } else { self.doc.footer = st.original.clone(); }
        for (k, a) in story.assets { self.doc.assets.entry(k).or_insert(a); }
        let paras = story.paragraphs;
        let empty = paras.iter().all(|p| p.is_empty());
        let new = if empty { None } else { Some(Story { paragraphs: paras.clone() }) };
        if new != st.original { self.doc.set_story(st.header, (!empty).then_some(paras)); }
        self.layout = Layout::default();
        if self.tab == crate::ribbon::Tab::HeaderFooter { self.tab = crate::ribbon::Tab::Insert; }
        self.relayout();
    }

    /// Switch between the header and footer while editing.
    pub fn go_to_story(&mut self, header: bool) {
        let page = self.extras.story.as_ref().map(|s| s.page);
        if self.extras.story.as_ref().is_some_and(|s| s.header == header) { return; }
        self.end_story();
        self.edit_story(header, page);
    }

    /// The story's layout placed on its page: footers moved up to the footer distance, and the
    /// page list matching the body so page positions on screen agree.
    pub fn place_story_layout(&mut self) {
        let Some(st) = &self.extras.story else { return };
        let g = st.host.page.geometry();
        let mut l = std::mem::take(&mut self.layout);
        let page = st.page.min(st.host_layout.pages.len().saturating_sub(1));
        let Some(mut first) = (!l.pages.is_empty()).then(|| l.pages.remove(0)) else { self.layout = l; return };
        let bottom = l.lines.iter().map(|x| x.y + x.height).fold(0.0f32, f32::max);
        let dy = if st.header { 0.0 } else { (g.height - g.footer - bottom).max(g.height * 0.5) };
        if dy != 0.0 {
            for it in first.items.iter_mut().chain(first.under.iter_mut()).chain(first.over.iter_mut()) { shift_item(it, dy); }
            for link in &mut first.links { link.1 += dy; }
            for line in &mut l.lines { line.y += dy; line.baseline += dy; }
            for o in l.float_origins.values_mut() { o.2 += dy; }
        }
        let mut pages: Vec<PageBox> = st.host_layout.pages.iter().map(|p| PageBox { width: p.width, height: p.height, items: vec![], under: vec![], links: vec![], over: vec![] }).collect();
        if pages.is_empty() { pages.push(PageBox { width: first.width, height: first.height, items: vec![], under: vec![], links: vec![], over: vec![] }); }
        pages[page] = first;
        for line in &mut l.lines { line.page = page; }
        for o in l.float_origins.values_mut() { o.0 = page; }
        l.lines.retain(|x| x.page == page);
        l.pages = pages;
        self.layout = l;
    }

    /// The editable band of the page: (top, bottom) in page px.
    pub fn story_band(&self) -> Option<(f32, f32)> {
        let st = self.extras.story.as_ref()?;
        let g = st.host.page.geometry();
        let lines = self.layout.lines.iter();
        if st.header {
            let bottom = lines.map(|l| l.y + l.height).fold(g.top, f32::max);
            Some((0.0, bottom + 4.0))
        } else {
            let top = lines.map(|l| l.y).fold(g.height - g.bottom, f32::min);
            Some((top - 4.0, g.height))
        }
    }
}
