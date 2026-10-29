//! .xlsx files (io.js, xlsx-*.js): reading and writing SpreadsheetML packages.
//!
//! Reading streams each worksheet's cell data (no DOM for sheetData) and parses the small parts
//! (workbook, rels, styles, theme, comments) as trees. What the model has no field for is kept:
//! package parts by path in `Workbook.parts` (themes, drawings, charts, images, tables, pivot
//! parts, printer settings...), and worksheet elements as raw XML in `Sheet.extra`, with the
//! relationships they use, so writing puts them back in schema order. `Sheet.extra` holds
//! (element name, xml) pairs plus two bookkeeping kinds: ("rel", "id\ttype\ttarget\texternal")
//! for a carried relationship (the target as an absolute part name) and ("xmlns", attributes)
//! for the namespace declarations the raw XML needs. The file's [Content_Types].xml,
//! xl/workbook.xml, its rels and xl/styles.xml are kept in `parts` for reference only (content
//! types, pivot caches, external links, dxfs) and are regenerated on writing.
//!
//! Sizes: a column `width` attribute counts characters of the Calibri 11 digit (7 px) including
//! padding, so px = trunc((256·w + trunc(128/7)) / 256 · 7), i.e. trunc(7w + 0.49), and back
//! w = trunc(px / 7 · 256) / 256, which round-trips every whole pixel. The default width from
//! `baseColWidth` b is 7b + 5 px rounded up to a multiple of 8 (8 → 64 px). Row heights are
//! points: px = pt · 4/3.

mod pivot_parts;
pub(crate) mod native_objects;
pub(crate) mod chartex_keep;
pub(crate) mod vml_objects;
pub(crate) mod equation_parts;
pub(crate) mod model3d_parts;
pub(crate) mod richdata;
mod regionmap_parts;
mod smartart_parts;
#[cfg(test)]
pub(crate) mod pkgcheck;
#[cfg(test)]
pub(crate) mod schemacheck;
#[cfg(test)]
mod fullsample;
mod read;
mod rules;
pub(crate) mod slicer_parts;
pub(crate) mod timeline_parts;
mod styles;
mod write;
pub mod xml;

use crate::formula::lexer::{tokenize, Tok};

pub use read::read;
pub(crate) use pivot_parts::{load as load_pivots, stale as stale_pivots, store as store_pivots};
pub use write::write;
/// The relationships part name of a part.
pub fn rels_name_of(part: &str) -> String { read::rels_name(part) }
/// A relationship target resolved against its source part.
pub fn resolve_part(source: &str, target: &str) -> String { read::resolve(source, target) }

/// A column `width` (characters with padding) in pixels.
pub fn width_to_px(w: f64) -> f32 { (((256.0 * w + 18.0) / 256.0) * 7.0).trunc().max(0.0) as f32 }
/// Pixels as a column `width`.
pub fn px_to_width(px: f32) -> f64 { ((px.max(0.0).round() as f64) / 7.0 * 256.0).trunc() / 256.0 }
/// The default column width for `baseColWidth` characters.
pub fn base_width_px(base: f64) -> f32 { ((base * 7.0 + 5.0) / 8.0).ceil() as f32 * 8.0 }

/// Functions Excel 2007 doesn't know: files spell them _xlfn.NAME (FILTER and SORT also _xlws.).
const FUTURE: &[&str] = &[
    "XLOOKUP", "XMATCH", "FILTER", "SORT", "SORTBY", "UNIQUE", "SEQUENCE", "RANDARRAY", "LET", "LAMBDA", "MAP", "REDUCE", "SCAN", "BYROW", "BYCOL", "MAKEARRAY", "ISOMITTED",
    "TEXTJOIN", "CONCAT", "IFS", "SWITCH", "MAXIFS", "MINIFS", "XOR", "TEXTBEFORE", "TEXTAFTER", "TEXTSPLIT", "VSTACK", "HSTACK", "TAKE", "DROP", "CHOOSEROWS", "CHOOSECOLS",
    "TOCOL", "TOROW", "WRAPROWS", "WRAPCOLS", "EXPAND", "ARRAYTOTEXT", "VALUETOTEXT", "IFNA", "DAYS", "ISOWEEKNUM", "CEILING.MATH", "FLOOR.MATH", "CEILING.PRECISE", "FLOOR.PRECISE",
    "NUMBERVALUE", "UNICHAR", "UNICODE", "ENCODEURL", "FILTERXML", "WEBSERVICE", "FORMULATEXT", "ISFORMULA", "SHEET", "SHEETS", "AGGREGATE", "NETWORKDAYS.INTL", "WORKDAY.INTL",
    "STDEV.S", "STDEV.P", "VAR.S", "VAR.P", "MODE.SNGL", "MODE.MULT", "NORM.DIST", "NORM.INV", "NORM.S.DIST", "NORM.S.INV", "LOGNORM.DIST", "LOGNORM.INV", "T.DIST", "T.DIST.2T",
    "T.DIST.RT", "T.INV", "T.INV.2T", "T.TEST", "F.DIST", "F.DIST.RT", "F.INV", "F.INV.RT", "F.TEST", "CHISQ.DIST", "CHISQ.DIST.RT", "CHISQ.INV", "CHISQ.INV.RT", "CHISQ.TEST",
    "BETA.DIST", "BETA.INV", "BINOM.DIST", "BINOM.DIST.RANGE", "BINOM.INV", "EXPON.DIST", "GAMMA.DIST", "GAMMA.INV", "GAMMALN.PRECISE", "GAMMA", "GAUSS", "PHI", "POISSON.DIST",
    "WEIBULL.DIST", "HYPGEOM.DIST", "NEGBINOM.DIST", "Z.TEST", "CONFIDENCE.NORM", "CONFIDENCE.T", "COVARIANCE.P", "COVARIANCE.S", "PERCENTILE.INC", "PERCENTILE.EXC",
    "QUARTILE.INC", "QUARTILE.EXC", "PERCENTRANK.INC", "PERCENTRANK.EXC", "RANK.EQ", "RANK.AVG", "ERF.PRECISE", "ERFC.PRECISE", "FORECAST.LINEAR", "FORECAST.ETS",
    "FORECAST.ETS.CONFINT", "FORECAST.ETS.SEASONALITY", "FORECAST.ETS.STAT", "ACOT", "ACOTH", "COT", "COTH", "CSC", "CSCH", "SEC", "SECH", "ARABIC", "BASE", "DECIMAL",
    "COMBINA", "PERMUTATIONA", "BITAND", "BITOR", "BITXOR", "BITLSHIFT", "BITRSHIFT", "IMCOSH", "IMCOT", "IMCSC", "IMCSCH", "IMSEC", "IMSECH", "IMSINH", "IMTAN", "PDURATION",
    "RRI", "SKEW.P", "DBCS", "STOCKHISTORY", "IMAGE", "GROUPBY", "PIVOTBY", "PERCENTOF", "TRIMRANGE", "REGEXTEST", "REGEXEXTRACT", "REGEXREPLACE", "TEXTJOIN",
];

fn upper_ascii(s: &str) -> String { s.to_ascii_uppercase() }

/// A formula from a file as the formula bar shows it: _xlfn., _xlws. and _xlpm. prefixes removed.
pub fn from_file_formula(src: &str) -> String {
    if !src.contains("_xl") && !src.contains("_XL") { return src.to_string(); }
    let strip = |s: &str| -> String {
        let mut s = s;
        loop {
            let u = upper_ascii(&s[..s.len().min(6)]);
            if s.len() > 6 && (u == "_XLFN." || u == "_XLWS." || u == "_XLPM.") { s = &s[6..]; } else { return s.to_string(); }
        }
    };
    let Ok(tokens) = tokenize(src) else {
        return src.replace("_xlfn._xlws.", "").replace("_xlfn.", "").replace("_xlws.", "").replace("_xlpm.", "");
    };
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    for t in tokens {
        out.extend(&chars[at..t.start]);
        let orig: String = chars[t.start..t.end].iter().collect();
        if matches!(t.tok, Tok::Name { sheet: None, .. }) { out.push_str(&strip(&orig)); } else { out.push_str(&orig); }
        at = t.end;
    }
    out.extend(&chars[at.min(chars.len())..]);
    out
}

/// A formula as a file stores it: newer functions get _xlfn. (and _xlws.), LET/LAMBDA
/// parameters _xlpm., so Excel recognises them.
pub fn to_file_formula(src: &str) -> String {
    let Ok(tokens) = tokenize(src) else { return src.to_string() };
    let sig: Vec<usize> = (0..tokens.len()).filter(|&i| tokens[i].tok != Tok::Space).collect();
    let is_call = |k: usize| sig.get(k + 1).is_some_and(|&j| tokens[j].tok == Tok::LParen);
    // Parameter names: LAMBDA(p1, p2, body) and LET(n1, v1, n2, v2, body).
    let mut params: Vec<String> = vec![];
    for k in 0..sig.len() {
        let Tok::Name { value, sheet: None } = &tokens[sig[k]].tok else { continue };
        let (lambda, let_) = (value == "LAMBDA" || value == "_XLFN.LAMBDA", value == "LET" || value == "_XLFN.LET");
        if !(lambda || let_) || !is_call(k) { continue; }
        let mut args: Vec<Vec<&Tok>> = vec![vec![]];
        let mut depth = 0;
        for &j in &sig[k + 2..] {
            match &tokens[j].tok {
                Tok::LParen | Tok::LBrace => depth += 1,
                Tok::RParen | Tok::RBrace if depth == 0 => break,
                Tok::RParen | Tok::RBrace => depth -= 1,
                Tok::Sep(',') if depth == 0 => { args.push(vec![]); continue; }
                _ => {}
            }
            args.last_mut().unwrap().push(&tokens[j].tok);
        }
        let n = args.len();
        for (i, a) in args.iter().enumerate() {
            if i + 1 == n || (let_ && i % 2 == 1) { continue; }
            if let [Tok::Name { value, sheet: None }] = a.as_slice() { if !value.starts_with("_XLPM.") { params.push(value.clone()); } }
        }
    }
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len() + 16);
    let mut at = 0;
    for (k, &i) in sig.iter().enumerate() {
        let t = &tokens[i];
        let Tok::Name { value, sheet: None } = &t.tok else { continue };
        out.extend(&chars[at..t.start]);
        at = t.start;
        if params.contains(value) { out.push_str("_xlpm."); }
        else if is_call(k) && FUTURE.contains(&value.as_str()) { out.push_str(if value == "FILTER" || value == "SORT" { "_xlfn._xlws." } else { "_xlfn." }); }
    }
    out.extend(&chars[at..]);
    out
}


#[cfg(test)]
mod tests;
