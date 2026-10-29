//! State for the newer ribbon features (Tell Me, collapsed ribbon, header/footer editing, ink,
//! symbols and equations, Design tab), kept apart from the long `WordApp` constructor.

#[derive(Default)]
pub struct Extras {
    /// The ribbon shows only its tab row.
    pub ribbon_collapsed: bool,
    pub refs: crate::refs::RefsState,
    pub review: crate::review::ReviewState,
    pub view: crate::view::ViewState,
    pub mailings: crate::mailings::MailingsState,
    pub print: Option<crate::print::PrintState>,
    pub macros: crate::macros::MacrosState,
    pub nav: crate::nav::NavState,
    pub options: crate::options::OptionsState,
    /// The last print settings, offered again next time.
    pub print_settings: Option<crate::print::Settings>,
    pub styles_pane: bool,
    pub formatting_pane: bool,
    /// "Tell me what you want to do" query.
    pub tell_me: String,
    pub story: Option<crate::story::StoryEdit>,
    pub draw: crate::draw::DrawState,
    pub symbol: Option<crate::symbols::SymbolDialog>,
    pub recent_symbols: Vec<char>,
    pub equation: Option<crate::symbols::EquationDialog>,
    pub drop_cap: Option<crate::text_tools::DropCapDraft>,
    pub date_time: Option<crate::text_tools::DateTimeDraft>,
    pub signature: Option<crate::text_tools::SignatureDraft>,
    pub field: Option<crate::text_tools::FieldDraft>,
    pub quick_parts: Vec<(String, String)>,
    pub icons: Option<crate::text_tools::IconsDialog>,
    pub watermark: Option<crate::design::WatermarkDraft>,
    pub page_borders: Option<crate::design::PageBorderDraft>,
}
