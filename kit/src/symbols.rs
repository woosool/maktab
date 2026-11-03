//! Symbol subsets, special characters and equation structures (Insert ▸ Symbol, Equation),
//! shared by the Qalam apps.

/// Symbol subsets: (name, first, last code point).
pub const SUBSETS: &[(&str, u32, u32)] = &[
    ("Latin-1 Supplement", 0xA1, 0xFF), ("Latin Extended-A", 0x100, 0x17F), ("Greek and Coptic", 0x391, 0x3C9), ("Cyrillic", 0x410, 0x44F),
    ("General Punctuation", 0x2010, 0x2044), ("Superscripts and Subscripts", 0x2070, 0x209C), ("Currency Symbols", 0x20A0, 0x20C0), ("Letterlike Symbols", 0x2100, 0x214F),
    ("Number Forms", 0x2150, 0x218B), ("Arrows", 0x2190, 0x21FF), ("Mathematical Operators", 0x2200, 0x22FF), ("Miscellaneous Technical", 0x2300, 0x23FF),
    ("Enclosed Alphanumerics", 0x2460, 0x24FF), ("Box Drawing", 0x2500, 0x257F), ("Block Elements", 0x2580, 0x259F), ("Geometric Shapes", 0x25A0, 0x25FF),
    ("Miscellaneous Symbols", 0x2600, 0x26FF), ("Dingbats", 0x2700, 0x27BF), ("Supplemental Arrows", 0x27F0, 0x27FF), ("Arabic", 0x621, 0x64A), ("Hebrew", 0x5D0, 0x5EA),
    ("Emoji", 0x1F600, 0x1F64F),
];

/// Special characters: (char, name, shortcut hint).
pub const SPECIAL: &[(char, &str)] = &[
    ('—', "Em Dash"), ('–', "En Dash"), ('\u{2011}', "Nonbreaking Hyphen"), ('\u{AD}', "Optional Hyphen"), ('\u{2003}', "Em Space"), ('\u{2002}', "En Space"), ('\u{2005}', "1/4 Em Space"),
    ('\u{A0}', "Nonbreaking Space"), ('©', "Copyright"), ('®', "Registered"), ('™', "Trademark"), ('§', "Section"), ('¶', "Paragraph"), ('…', "Ellipsis"),
    ('‘', "Single Opening Quote"), ('’', "Single Closing Quote"), ('“', "Double Opening Quote"), ('”', "Double Closing Quote"), ('\u{200B}', "No-Width Optional Break"), ('\u{200C}', "No-Width Non Break"),
];

/// The quick Symbol gallery (Word's defaults).
pub const COMMON: &str = "€£¥©®™±≠≤≥÷×∞µαβπΩ∑°√∆←→↑↓♥☺✓★";

/// Structure templates for the equation editor: (group, [(label, linear source)]).
pub const STRUCTURES: &[(&str, &str, &[(&str, &str)])] = &[
    ("Fraction", "formula", &[("Stacked", "\\frac{a}{b}"), ("Skewed", "a/b"), ("dy/dx", "\\frac{dy}{dx}"), ("Δy/Δx", "\\frac{\\Delta y}{\\Delta x}"), ("π/2", "\\frac{\\pi}{2}")]),
    ("Script", "sup", &[("Superscript", "x^{2}"), ("Subscript", "x_{1}"), ("Sub-Superscript", "x_{1}^{2}"), ("e^{-iωt}", "e^{-i\\omega t}"), ("x_{ij}", "x_{ij}^{2}")]),
    ("Radical", "equation", &[("Square Root", "\\sqrt{x}"), ("Cube Root", "\\sqrt[3]{x}"), ("Radical with Degree", "\\sqrt[n]{x}"), ("√(a²+b²)", "\\sqrt{a^2+b^2}"), ("Quadratic", "\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}")]),
    ("Integral", "equation", &[("Integral", "\\int f(x)\\,dx"), ("Definite Integral", "\\int_{a}^{b} f(x)\\,dx"), ("Double Integral", "\\iint_{D} f\\,dA"), ("Triple Integral", "\\iiint_{V} f\\,dV"), ("Contour Integral", "\\oint_{C} F\\cdot dr")]),
    ("Large Operator", "equation", &[("Summation", "\\sum_{i=1}^{n} x_i"), ("Product", "\\prod_{k=1}^{n} a_k"), ("Union", "\\bigcup_{i=1}^{n} A_i"), ("Intersection", "\\bigcap_{i=1}^{n} A_i"), ("Binomial Sum", "\\sum_{k=0}^{n}\\binom{n}{k}")]),
    ("Bracket", "equation", &[("Parentheses", "(x)"), ("Brackets", "[x]"), ("Braces", "\\left\\{x\\right\\}"), ("Absolute Value", "|x|"), ("Angle Brackets", "\\left\\langle x\\right\\rangle"), ("Binomial", "\\binom{n}{k}"), ("Cases", "f(x)=\\begin{cases}x&x\\ge0\\\\-x&x<0\\end{cases}")]),
    ("Function", "equation", &[("Sine", "\\sin\\theta"), ("Cosine", "\\cos 2x"), ("Tangent", "\\tan\\theta=\\frac{\\sin\\theta}{\\cos\\theta}"), ("Log", "\\log_{b} x"), ("Natural Log", "\\ln x"), ("Exponential", "\\exp(x)")]),
    ("Accent", "equation", &[("Dot", "\\dot{a}"), ("Double Dot", "\\ddot{a}"), ("Hat", "\\hat{a}"), ("Bar", "\\bar{a}"), ("Vector", "\\vec{a}"), ("Tilde", "\\tilde{a}")]),
    ("Limit and Log", "equation", &[("Limit", "\\lim_{n\\to\\infty} a_n"), ("Maximum", "\\max_{0\\le x\\le1} f(x)"), ("Minimum", "\\min_{x} f(x)"), ("sin x / x", "\\lim_{x\\to0}\\frac{\\sin x}{x}=1"), ("Log base 10", "\\log_{10} x")]),
    ("Operator", "equation", &[("Plus–Minus", "a\\pm b"), ("Defined As", "a\\equiv b"), ("Approximately", "a\\approx b"), ("Implies", "p\\Rightarrow q"), ("Maps To", "x\\mapsto f(x)")]),
    ("Matrix", "table", &[("2×2 Parentheses", "\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}"), ("2×2 Brackets", "\\begin{bmatrix}a&b\\\\c&d\\end{bmatrix}"), ("Determinant", "\\begin{vmatrix}a&b\\\\c&d\\end{vmatrix}"), ("3×3 Identity", "\\begin{bmatrix}1&0&0\\\\0&1&0\\\\0&0&1\\end{bmatrix}"), ("Column Vector", "\\begin{pmatrix}x\\\\y\\\\z\\end{pmatrix}")]),
];

/// Math symbols for the equation editor.
pub const MATH_SYMBOLS: &str = "±∞=≠∼×÷!∝<≪>≫≤≥∓≅≈≡∀∁∂√∛∜∪∩∅%°℉℃∆∇∃∄∈∋←↑→↓↔∴+−¬αβγδεϵθϑμπρστφωΓΔΘΛΠΣΦΨΩ∗∙⋮⋯⋰⋱ℵℶ∎";

/// The face that draws `c` in the UI.
pub fn glyph_font(c: char, size: f32) -> egui::FontId {
    let base = word_engine::fonts::resolve("Noto Sans", false, false);
    let k = word_engine::fonts::fallback(base, c);
    egui::FontId::new(size, egui::FontFamily::Name(crate::fonts::face_name(k.face as usize).into()))
}

