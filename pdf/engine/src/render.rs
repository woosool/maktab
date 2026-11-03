use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

/// A rasterized page: premultiplied RGBA8 (opaque, on white).
#[derive(Clone)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Largest bitmap edge we produce; bigger zooms are clamped.
const MAX_EDGE: f32 = 8000.0;

/// Renders page `index` of `pdf` at `scale` pixels per point, annotations included.
pub fn render_page(pdf: &Pdf, index: usize, scale: f32) -> Option<RenderedPage> {
    let page = pdf.pages().get(index)?;
    let (w, h) = page.render_dimensions();
    let scale = scale.min(MAX_EDGE / w.max(h).max(1.0)).max(0.01);
    let cache = RenderCache::new();
    let pixmap = hayro::render(
        page,
        &cache,
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    Some(RenderedPage {
        width: pixmap.width() as u32,
        height: pixmap.height() as u32,
        rgba: pixmap.data_as_u8_slice().to_vec(),
    })
}

/// A self-contained renderer that can live on a worker thread: it parses the
/// bytes once and renders pages on request.
pub struct Renderer {
    pdf: Pdf,
}

impl Renderer {
    pub fn new(bytes: std::sync::Arc<Vec<u8>>) -> Option<Self> {
        Pdf::new(bytes).ok().map(|pdf| Self { pdf })
    }
    pub fn render(&self, index: usize, scale: f32) -> Option<RenderedPage> {
        render_page(&self.pdf, index, scale)
    }
}

impl crate::Document {
    /// Renders page `index` at `scale` pixels per point.
    pub fn render(&self, index: usize, scale: f32) -> Option<RenderedPage> {
        render_page(self.view_pdf(), index, scale)
    }
}
