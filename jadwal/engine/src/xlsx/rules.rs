//! Conditional formats and data validation in a package (xlsx-condformat.js, xlsx-databar.js,
//! xlsx-validation.js): <conditionalFormatting> and <dataValidations> with their x14 variants in
//! the worksheet's extLst read into the sheet's rules, and written back with their differential
//! formats appended to styles.xml's <dxfs> (the file's own dxfs keep their indexes, as tables
//! refer to them too).

use super::styles::{self, Palette};
use super::xml::{attr, text, Element, Node};
use super::{from_file_formula, to_file_formula};
use crate::addr::{parse_range, Pos, Range};
use crate::book::Sheet;
use crate::condformat::{icon_count, CfKind, Cfvo, CondFormat, DatePeriod, Dxf, Op, TextOp, VoKind};
use crate::style::{Border, LineStyle, Underline};
use crate::validation::{AlertStyle, DvKind, Validation};

const X14_CF: &str = "{78C0D931-6437-407d-A8EE-F0AAD7539E65}";
const X14_DV: &str = "{CCE6A557-97BC-4b89-ADB6-D9C93CAAB3DF}";
const X14_ID: &str = "{B025F937-C7B1-47D3-B67F-A62EFF666E3E}";
const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const NS_XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";
/// Icon sets only the 2010 extension knows.
const X14_SETS: &[&str] = &["3Stars", "3Triangles", "5Boxes"];

// ---- differential formats ------------------------------------------------------------------------

pub fn read_dxf(e: &Element, pal: &Palette) -> Dxf {
    let mut d = Dxf::default();
    if let Some(f) = e.child("font") {
        for x in f.elements() {
            match x.local() {
                "b" => d.bold = Some(x.flag("val", true)), "i" => d.italic = Some(x.flag("val", true)), "strike" => d.strike = Some(x.flag("val", true)),
                "u" => d.underline = Some(match x.attr("val").unwrap_or("single") { "double" => Underline::Double, "none" => Underline::None, _ => Underline::Single }),
                "color" => d.color = pal.color(x),
                _ => {}
            }
        }
    }
    if let Some(p) = e.path("fill/patternFill") {
        if p.attr("patternType") != Some("none") { d.fill = p.child("bgColor").and_then(|c| pal.color(c)).or_else(|| p.child("fgColor").and_then(|c| pal.color(c))); }
    }
    if let Some(n) = e.child("numFmt") { d.num_fmt = n.attr("formatCode").map(String::from).or_else(|| n.attr_u32("numFmtId").and_then(crate::numfmt::builtin).map(String::from)); }
    if let Some(b) = e.child("border") {
        d.border = ["left", "right", "top", "bottom"].iter().filter_map(|s| b.child(s)).map(|s| Border { style: LineStyle::parse(s.attr("style").unwrap_or("none")), color: s.child("color").and_then(|c| pal.color(c)) }).find(|x| x.style != LineStyle::None);
    }
    d
}

fn dxf_xml(d: &Dxf) -> String {
    let mut o = String::from("<dxf>");
    let mut font = String::new();
    if let Some(b) = d.bold { font.push_str(if b { "<b/>" } else { "<b val=\"0\"/>" }); }
    if let Some(i) = d.italic { font.push_str(if i { "<i/>" } else { "<i val=\"0\"/>" }); }
    if let Some(s) = d.strike { font.push_str(if s { "<strike/>" } else { "<strike val=\"0\"/>" }); }
    if let Some(u) = d.underline { font.push_str(match u { Underline::None => "<u val=\"none\"/>", Underline::Double | Underline::DoubleAccounting => "<u val=\"double\"/>", _ => "<u/>" }); }
    if let Some(c) = d.color { font.push_str(&format!("<color rgb=\"{}\"/>", styles::rgb_hex(c))); }
    if !font.is_empty() { o.push_str(&format!("<font>{font}</font>")); }
    if let Some(n) = &d.num_fmt {
        let id = (0..50).find(|&i| crate::numfmt::builtin(i) == Some(n.as_str())).unwrap_or(176);
        o.push_str(&format!("<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>", attr(n)));
    }
    if let Some(f) = d.fill { o.push_str(&format!("<fill><patternFill><bgColor rgb=\"{}\"/></patternFill></fill>", styles::rgb_hex(f))); }
    if let Some(b) = d.border {
        let side = |n: &str| format!("<{n} style=\"{}\">{}</{n}>", b.style.name(), b.color.map(|c| format!("<color rgb=\"{}\"/>", styles::rgb_hex(c))).unwrap_or_else(|| "<color auto=\"1\"/>".into()));
        o.push_str(&format!("<border>{}{}{}{}</border>", side("left"), side("right"), side("top"), side("bottom")));
    }
    o.push_str("</dxf>");
    o
}

fn normalize(xml: &str) -> String { Element::parse(xml).map(|e| styles::strip_prefix(&e).to_xml()).unwrap_or_else(|_| xml.to_string()) }

/// The file's differential formats, as written back (their indexes stay).
pub fn dxf_table(original: Option<&Element>) -> Vec<String> {
    original.and_then(|s| s.child("dxfs")).map(|d| d.all("dxf").map(|e| styles::strip_prefix(e).to_xml()).collect()).unwrap_or_default()
}

/// The index of a format in the table, appending it when new.
fn dxf_id(table: &mut Vec<String>, d: &Dxf) -> usize {
    let x = normalize(&dxf_xml(d));
    if let Some(i) = table.iter().position(|t| *t == x) { return i; }
    table.push(x);
    table.len() - 1
}

pub fn dxfs_xml(table: &[String]) -> String { format!("<dxfs count=\"{}\">{}</dxfs>", table.len(), table.concat()) }

// ---- reading -------------------------------------------------------------------------------------

fn sqref(e: &Element) -> Vec<Range> {
    let s = e.attr("sqref").map(String::from).or_else(|| e.child("sqref").map(|x| x.text())).unwrap_or_default();
    s.split_whitespace().filter_map(parse_range).collect()
}

/// A formula child: <formula>, <formula1>, or x14's <x14:formula1><xm:f>.
fn formula(e: &Element, name: &str) -> String {
    e.child(name).map(|f| f.child("f").map(|x| x.text()).unwrap_or_else(|| f.text())).map(|t| from_file_formula(t.trim())).unwrap_or_default()
}

fn cfvo(e: &Element) -> Cfvo {
    let val = e.attr("val").map(String::from).or_else(|| e.child("f").map(|x| x.text())).unwrap_or_default();
    Cfvo { kind: VoKind::parse(e.attr("type").unwrap_or("num")), val: from_file_formula(val.trim()), gte: e.flag("gte", true) }
}

/// One <cfRule> (or <x14:cfRule>) as a rule; None for kinds this app doesn't know.
fn rule(e: &Element, ranges: Vec<Range>, dxfs: &[Dxf], pal: &Palette) -> Option<CondFormat> {
    let fs: Vec<String> = e.all("formula").map(|f| from_file_formula(f.child("f").map(|x| x.text()).unwrap_or_else(|| f.text()).trim())).collect();
    let fs: Vec<String> = if fs.is_empty() { e.all("f").map(|f| from_file_formula(f.text().trim())).collect() } else { fs };
    let f = |i: usize| fs.get(i).cloned().unwrap_or_default();
    let colors = |p: &Element| p.all("color").filter_map(|c| pal.color(c)).collect::<Vec<_>>();
    let kind = match e.attr("type")? {
        "cellIs" => CfKind::CellIs { op: Op::parse(e.attr("operator").unwrap_or("equal")), f1: f(0), f2: f(1) },
        "expression" => CfKind::Expression(f(0)),
        "top10" => CfKind::Top { bottom: e.flag("bottom", false), percent: e.flag("percent", false), rank: e.attr_u32("rank").unwrap_or(10) },
        "aboveAverage" => CfKind::Average { below: !e.flag("aboveAverage", true), equal: e.flag("equalAverage", false), std_dev: e.attr_u32("stdDev").unwrap_or(0) },
        "duplicateValues" => CfKind::Duplicate { unique: false },
        "uniqueValues" => CfKind::Duplicate { unique: true },
        t @ ("containsText" | "notContainsText" | "beginsWith" | "endsWith") => CfKind::Text { op: TextOp::ALL.iter().find(|x| x.1 == t).unwrap().0, text: e.attr("text").unwrap_or("").into() },
        "containsBlanks" => CfKind::Blanks(true), "notContainsBlanks" => CfKind::Blanks(false),
        "containsErrors" => CfKind::Errors(true), "notContainsErrors" => CfKind::Errors(false),
        "timePeriod" => CfKind::Date(DatePeriod::parse(e.attr("timePeriod").unwrap_or("today"))),
        "colorScale" => { let c = e.child("colorScale")?; CfKind::ColorScale { vos: c.all("cfvo").map(cfvo).collect(), colors: colors(c) } }
        "dataBar" => {
            let d = e.child("dataBar")?;
            let v: Vec<Cfvo> = d.all("cfvo").map(cfvo).collect();
            CfKind::DataBar { min: v.first().cloned().unwrap_or(Cfvo::new(VoKind::Min, "")), max: v.get(1).cloned().unwrap_or(Cfvo::new(VoKind::Max, "")), color: d.child("color").or(d.child("fillColor")).and_then(|c| pal.color(c)).unwrap_or([0x63, 0x8E, 0xC6]),
                negative: d.child("negativeFillColor").and_then(|c| pal.color(c)), gradient: d.flag("gradient", true), border: d.child("borderColor").filter(|_| d.flag("border", false)).and_then(|c| pal.color(c)), show_value: d.flag("showValue", true) }
        }
        "iconSet" => {
            let i = e.child("iconSet")?;
            let set = i.attr("iconSet").unwrap_or("3TrafficLights1").to_string();
            let mut vos: Vec<Cfvo> = i.all("cfvo").map(cfvo).collect();
            if vos.len() < icon_count(&set) { vos = crate::condformat::icon_cfvos(&set); }
            CfKind::IconSet { set, vos, reverse: i.flag("reverse", false), show_value: i.flag("showValue", true) }
        }
        _ => return None,
    };
    let dxf = e.child("dxf").map(|d| read_dxf(d, pal)).or_else(|| e.attr_u32("dxfId").and_then(|i| dxfs.get(i as usize).cloned())).unwrap_or_default();
    Some(CondFormat { ranges, kind, dxf, stop: e.flag("stopIfTrue", false) })
}

fn x14_id(e: &Element) -> Option<String> {
    fn find(e: &Element) -> Option<String> { if e.local() == "id" { return Some(e.text()); } e.elements().find_map(find) }
    e.child("extLst").and_then(find)
}

fn validation(e: &Element) -> Validation {
    let s = |k: &str| e.attr(k).unwrap_or("").to_string();
    Validation {
        ranges: sqref(e), kind: DvKind::parse(e.attr("type").unwrap_or("none")), op: Op::parse(e.attr("operator").unwrap_or("between")), f1: formula(e, "formula1"), f2: formula(e, "formula2"),
        allow_blank: e.flag("allowBlank", false), dropdown: !e.flag("showDropDown", false), show_input: e.flag("showInputMessage", false), prompt_title: s("promptTitle"), prompt: s("prompt"),
        show_error: e.flag("showErrorMessage", false), style: AlertStyle::parse(e.attr("errorStyle").unwrap_or("stop")), error_title: s("errorTitle"), error: s("error"),
    }
}

/// Moves the sheet's conditional formats and validations from the carried XML into the model;
/// the extLst keeps whatever other extensions it has.
pub fn take(sheet: &mut Sheet, dxfs: &[Dxf], pal: &Palette) {
    let mut found: Vec<(u32, usize, CondFormat, Option<String>)> = vec![];
    let mut x14_bars: Vec<(String, Element)> = vec![];
    let mut rest = vec![];
    for (k, raw) in std::mem::take(&mut sheet.extra) {
        match k.as_str() {
            "conditionalFormatting" => if let Ok(b) = Element::parse(&raw) {
                let ranges = sqref(&b);
                for r in b.all("cfRule") { if let Some(cf) = rule(r, ranges.clone(), dxfs, pal) { found.push((r.attr_u32("priority").unwrap_or(u32::MAX), found.len(), cf, x14_id(r))); } }
            },
            "dataValidations" => if let Ok(b) = Element::parse(&raw) { sheet.validations.extend(b.all("dataValidation").map(validation).filter(|v| !v.ranges.is_empty())); },
            "extLst" => match Element::parse(&raw) {
                Ok(mut x) => {
                    x.children.retain(|n| {
                        let Node::Element(ext) = n else { return true };
                        match ext.attr("uri") {
                            Some(u) if u.eq_ignore_ascii_case(X14_CF) => {
                                for b in ext.elements().flat_map(|c| c.all("conditionalFormatting")) {
                                    let ranges = sqref(b);
                                    for r in b.all("cfRule") {
                                        let id = r.attr("id").map(String::from);
                                        if r.attr("type") == Some("dataBar") && id.is_some() { x14_bars.push((id.unwrap(), r.clone())); continue; }
                                        if let Some(cf) = rule(r, ranges.clone(), dxfs, pal) { found.push((r.attr_u32("priority").unwrap_or(u32::MAX), found.len(), cf, None)); }
                                    }
                                }
                                false
                            }
                            Some(u) if u.eq_ignore_ascii_case(X14_DV) => {
                                for b in ext.elements().flat_map(|c| c.all("dataValidation")) { let v = validation(b); if !v.ranges.is_empty() { sheet.validations.push(v); } }
                                false
                            }
                            _ => true,
                        }
                    });
                    if x.elements().next().is_some() { rest.push((k, x.to_xml())); }
                }
                Err(_) => rest.push((k, raw)),
            },
            _ => rest.push((k, raw)),
        }
    }
    sheet.extra = rest;
    // A 2010 data bar's settings (solid fill, negative color, border) join the rule with its id.
    for (_, _, cf, id) in &mut found {
        let (Some(id), CfKind::DataBar { min, max, negative, gradient, border, .. }) = (id.as_ref(), &mut cf.kind) else { continue };
        let Some(d) = x14_bars.iter().find(|(i, _)| i == id).and_then(|(_, r)| r.child("dataBar")) else { continue };
        let v: Vec<Cfvo> = d.all("cfvo").map(cfvo).collect();
        if let [a, b, ..] = v.as_slice() { *min = a.clone(); *max = b.clone(); }
        *gradient = d.flag("gradient", true);
        if let Some(c) = d.child("negativeFillColor").and_then(|c| pal.color(c)) { *negative = Some(c); }
        *border = d.child("borderColor").filter(|_| d.flag("border", false)).and_then(|c| pal.color(c));
    }
    found.sort_by_key(|x| (x.0, x.1));
    sheet.cond_formats = found.into_iter().map(|x| x.2).collect();
}

// ---- writing -------------------------------------------------------------------------------------

fn cfvo_xml(v: &Cfvo, x14: bool) -> String {
    let kind = match (v.kind, x14) { (VoKind::AutoMin, false) => "min", (VoKind::AutoMax, false) => "max", (k, _) => k.name() };
    let gte = if v.gte { "" } else { " gte=\"0\"" };
    let has_val = !matches!(v.kind, VoKind::Min | VoKind::Max | VoKind::AutoMin | VoKind::AutoMax);
    if x14 { if has_val { format!("<x14:cfvo type=\"{kind}\"{gte}><xm:f>{}</xm:f></x14:cfvo>", text(&to_file_formula(&v.val))) } else { format!("<x14:cfvo type=\"{kind}\"{gte}/>") } }
    else if has_val { format!("<cfvo type=\"{kind}\" val=\"{}\"{gte}/>", attr(&to_file_formula(&v.val))) } else { format!("<cfvo type=\"{kind}\"{gte}/>") }
}

fn color(c: crate::style::Rgb) -> String { format!("<color rgb=\"{}\"/>", styles::rgb_hex(c)) }

fn quote(s: &str) -> String { format!("\"{}\"", s.replace('"', "\"\"")) }

/// The formula Excel stores alongside text, blank, error and date rules.
fn implied_formula(kind: &CfKind, at: Pos) -> Option<String> {
    let a = at.a1();
    Some(match kind {
        CfKind::Text { op, text } => { let q = quote(text); match op {
            TextOp::Contains => format!("NOT(ISERROR(SEARCH({q},{a})))"), TextOp::NotContains => format!("ISERROR(SEARCH({q},{a}))"),
            TextOp::BeginsWith => format!("LEFT({a},LEN({q}))={q}"), TextOp::EndsWith => format!("RIGHT({a},LEN({q}))={q}"),
        } }
        CfKind::Blanks(b) => format!("LEN(TRIM({a})){}0", if *b { "=" } else { ">" }),
        CfKind::Errors(b) => if *b { format!("ISERROR({a})") } else { format!("NOT(ISERROR({a}))") },
        CfKind::Date(d) => match d {
            DatePeriod::Today => format!("FLOOR({a},1)=TODAY()"), DatePeriod::Yesterday => format!("FLOOR({a},1)=TODAY()-1"), DatePeriod::Tomorrow => format!("FLOOR({a},1)=TODAY()+1"),
            DatePeriod::Last7Days => format!("AND(TODAY()-FLOOR({a},1)<=6,FLOOR({a},1)<=TODAY())"),
            DatePeriod::ThisMonth => format!("AND(MONTH({a})=MONTH(TODAY()),YEAR({a})=YEAR(TODAY()))"),
            DatePeriod::LastMonth => format!("AND(MONTH({a})=MONTH(EDATE(TODAY(),0-1)),YEAR({a})=YEAR(EDATE(TODAY(),0-1)))"),
            DatePeriod::NextMonth => format!("AND(MONTH({a})=MONTH(EDATE(TODAY(),0+1)),YEAR({a})=YEAR(EDATE(TODAY(),0+1)))"),
            DatePeriod::ThisWeek => format!("AND(TODAY()-ROUNDDOWN({a},0)<=WEEKDAY(TODAY())-1,ROUNDDOWN({a},0)-TODAY()<=7-WEEKDAY(TODAY()))"),
            DatePeriod::LastWeek => format!("AND(TODAY()-ROUNDDOWN({a},0)>=(WEEKDAY(TODAY())),TODAY()-ROUNDDOWN({a},0)<(WEEKDAY(TODAY())+7))"),
            DatePeriod::NextWeek => format!("AND(ROUNDDOWN({a},0)-TODAY()>(7-WEEKDAY(TODAY())),ROUNDDOWN({a},0)-TODAY()<(15-WEEKDAY(TODAY())))"),
        },
        _ => return None,
    })
}

fn guid(si: usize, i: usize) -> String { format!("{{6A3C1D20-{:04X}-4000-8000-{:012X}}}", si & 0xFFFF, i) }

/// What a sheet's rules write: <conditionalFormatting> blocks, <dataValidations>, and the x14
/// extension (an <ext> for the worksheet's extLst) for data bars and 2010 icon sets.
pub struct RulesOut { pub cond: Vec<String>, pub validations: Option<String>, pub ext: Option<String> }

pub fn write(s: &Sheet, si: usize, dxfs: &mut Vec<String>) -> RulesOut {
    let mut cond = vec![];
    let mut ext = String::new();
    for (i, cf) in s.cond_formats.iter().enumerate() {
        let sq = cf.sqref();
        if sq.is_empty() { continue; }
        let prio = i + 1;
        let stop = if cf.stop { " stopIfTrue=\"1\"" } else { "" };
        if let CfKind::IconSet { set, vos, reverse, show_value } = &cf.kind { if X14_SETS.contains(&set.as_str()) {
            ext.push_str(&format!("<x14:conditionalFormatting xmlns:xm=\"{NS_XM}\"><x14:cfRule type=\"iconSet\" priority=\"{prio}\" id=\"{}\"><x14:iconSet iconSet=\"{set}\"{}{}>", guid(si, i), if *reverse { " reverse=\"1\"" } else { "" }, if *show_value { "" } else { " showValue=\"0\"" }));
            for v in vos { ext.push_str(&cfvo_xml(v, true)); }
            ext.push_str(&format!("</x14:iconSet></x14:cfRule><xm:sqref>{sq}</xm:sqref></x14:conditionalFormatting>"));
            continue;
        } }
        let dxf = if cf.kind.uses_dxf() { format!(" dxfId=\"{}\"", dxf_id(dxfs, &cf.dxf)) } else { String::new() };
        let fx = |f: &str| format!("<formula>{}</formula>", text(&to_file_formula(f)));
        let mut r = String::new();
        let open = |ty: &str, more: String| format!("<cfRule type=\"{ty}\"{dxf} priority=\"{prio}\"{stop}{more}>");
        match &cf.kind {
            CfKind::CellIs { op, f1, f2 } => { r.push_str(&open("cellIs", format!(" operator=\"{}\"", op.name()))); r.push_str(&fx(f1)); if op.two() { r.push_str(&fx(f2)); } }
            CfKind::Expression(f) => { r.push_str(&open("expression", String::new())); r.push_str(&fx(f)); }
            CfKind::Top { bottom, percent, rank } => r.push_str(&open("top10", format!("{}{} rank=\"{rank}\"", if *percent { " percent=\"1\"" } else { "" }, if *bottom { " bottom=\"1\"" } else { "" }))),
            CfKind::Average { below, equal, std_dev } => r.push_str(&open("aboveAverage", format!("{}{}{}", if *below { " aboveAverage=\"0\"" } else { "" }, if *equal { " equalAverage=\"1\"" } else { "" }, if *std_dev > 0 { format!(" stdDev=\"{std_dev}\"") } else { String::new() }))),
            CfKind::Duplicate { unique } => r.push_str(&open(if *unique { "uniqueValues" } else { "duplicateValues" }, String::new())),
            CfKind::Text { op, text: t } => r.push_str(&open(op.name(), format!(" operator=\"{}\" text=\"{}\"", op.operator(), attr(t)))),
            CfKind::Blanks(b) => r.push_str(&open(if *b { "containsBlanks" } else { "notContainsBlanks" }, String::new())),
            CfKind::Errors(b) => r.push_str(&open(if *b { "containsErrors" } else { "notContainsErrors" }, String::new())),
            CfKind::Date(d) => r.push_str(&open("timePeriod", format!(" timePeriod=\"{}\"", d.name()))),
            CfKind::ColorScale { vos, colors } => {
                r.push_str(&open("colorScale", String::new())); r.push_str("<colorScale>");
                for v in vos { r.push_str(&cfvo_xml(v, false)); }
                for c in colors { r.push_str(&color(*c)); }
                r.push_str("</colorScale>");
            }
            CfKind::DataBar { min, max, color: c, negative, gradient, border, show_value } => {
                r.push_str(&open("dataBar", String::new()));
                r.push_str(&format!("<dataBar{}>{}{}{}</dataBar>", if *show_value { "" } else { " showValue=\"0\"" }, cfvo_xml(min, false), cfvo_xml(max, false), color(*c)));
                r.push_str(&format!("<extLst><ext uri=\"{X14_ID}\" xmlns:x14=\"{NS_X14}\"><x14:id>{}</x14:id></ext></extLst>", guid(si, i)));
                ext.push_str(&format!("<x14:conditionalFormatting xmlns:xm=\"{NS_XM}\"><x14:cfRule type=\"dataBar\" id=\"{}\"><x14:dataBar minLength=\"0\" maxLength=\"100\"{}{} negativeBarColorSameAsPositive=\"{}\" axisPosition=\"automatic\">{}{}",
                    guid(si, i), if *gradient { "" } else { " gradient=\"0\"" }, if border.is_some() { " border=\"1\"" } else { "" }, negative.is_none() as u8, cfvo_xml(min, true), cfvo_xml(max, true)));
                if let Some(b) = border { ext.push_str(&format!("<x14:borderColor rgb=\"{}\"/>", styles::rgb_hex(*b))); }
                if let Some(n) = negative { ext.push_str(&format!("<x14:negativeFillColor rgb=\"{}\"/>", styles::rgb_hex(*n))); }
                ext.push_str(&format!("<x14:axisColor rgb=\"FF000000\"/></x14:dataBar></x14:cfRule><xm:sqref>{sq}</xm:sqref></x14:conditionalFormatting>"));
            }
            CfKind::IconSet { set, vos, reverse, show_value } => {
                r.push_str(&open("iconSet", String::new()));
                r.push_str(&format!("<iconSet iconSet=\"{}\"{}{}>", attr(set), if *reverse { " reverse=\"1\"" } else { "" }, if *show_value { "" } else { " showValue=\"0\"" }));
                for v in vos { r.push_str(&cfvo_xml(v, false)); }
                r.push_str("</iconSet>");
            }
        }
        if let Some(f) = implied_formula(&cf.kind, cf.origin()) { r.push_str(&fx(&f)); }
        r.push_str("</cfRule>");
        cond.push(format!("<conditionalFormatting sqref=\"{sq}\">{r}</conditionalFormatting>"));
    }
    let vals: Vec<&Validation> = s.validations.iter().filter(|v| !v.ranges.is_empty()).collect();
    let validations = (!vals.is_empty()).then(|| {
        let mut o = format!("<dataValidations count=\"{}\">", vals.len());
        for v in vals {
            o.push_str("<dataValidation");
            if v.kind != DvKind::Any { o.push_str(&format!(" type=\"{}\"", v.kind.name())); }
            if v.kind.has_op() && v.op != Op::Between { o.push_str(&format!(" operator=\"{}\"", v.op.name())); }
            if v.style != AlertStyle::Stop { o.push_str(&format!(" errorStyle=\"{}\"", v.style.name())); }
            if v.allow_blank { o.push_str(" allowBlank=\"1\""); } if !v.dropdown { o.push_str(" showDropDown=\"1\""); }
            if v.show_input { o.push_str(" showInputMessage=\"1\""); } if v.show_error { o.push_str(" showErrorMessage=\"1\""); }
            for (k, x) in [("errorTitle", &v.error_title), ("error", &v.error), ("promptTitle", &v.prompt_title), ("prompt", &v.prompt)] { if !x.is_empty() { o.push_str(&format!(" {k}=\"{}\"", attr(x))); } }
            o.push_str(&format!(" sqref=\"{}\">", v.sqref()));
            if !v.f1.is_empty() { o.push_str(&format!("<formula1>{}</formula1>", text(&to_file_formula(&v.f1)))); }
            if !v.f2.is_empty() && v.op.two() && v.kind.has_op() { o.push_str(&format!("<formula2>{}</formula2>", text(&to_file_formula(&v.f2)))); }
            o.push_str("</dataValidation>");
        }
        o.push_str("</dataValidations>");
        o
    });
    let ext = (!ext.is_empty()).then(|| format!("<ext uri=\"{X14_CF}\" xmlns:x14=\"{NS_X14}\"><x14:conditionalFormattings>{ext}</x14:conditionalFormattings></ext>"));
    RulesOut { cond, validations, ext }
}

/// The worksheet's extLst with the rules' extension added to the carried one (if any).
pub fn merge_ext(carried: Option<&str>, ext: &str) -> String {
    match carried.and_then(|c| c.rfind("</").map(|i| (c, i))) {
        Some((c, i)) => format!("{}{ext}{}", &c[..i], &c[i..]),
        None => format!("<extLst>{ext}</extLst>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Workbook;
    use crate::condformat::cf_at;

    fn rng(s: &str) -> Range { parse_range(s).unwrap() }

    #[test]
    fn round_trip() {
        let mut wb = Workbook::new();
        for (i, v) in [1, 5, -3, 8].iter().enumerate() { wb.set_input(0, Pos::new(i as u32, 0), &v.to_string()); }
        wb.recalc();
        let red = Dxf { fill: Some([255, 199, 206]), color: Some([156, 0, 6]), bold: Some(true), border: Some(Border { style: LineStyle::Thin, color: Some([1, 2, 3]) }), num_fmt: Some("0.0%".into()), ..Default::default() };
        let s = &mut wb.sheets[0];
        s.cond_formats = vec![
            CondFormat { stop: true, ..CondFormat::new(vec![rng("A1:A4"), rng("C1")], CfKind::CellIs { op: Op::Between, f1: "1".into(), f2: "$B$1&\"<\"".into() }, red.clone()) },
            CondFormat::new(vec![rng("A1:A4")], CfKind::Expression("AND(A1>0,XLOOKUP(1,A:A,A:A)=1)".into()), red.clone()),
            CondFormat::new(vec![rng("A1:A4")], CfKind::Top { bottom: true, percent: true, rank: 25 }, Dxf { italic: Some(true), ..Default::default() }),
            CondFormat::new(vec![rng("A1:A4")], CfKind::Average { below: true, equal: true, std_dev: 1 }, red.clone()),
            CondFormat::new(vec![rng("A1:A4")], CfKind::Duplicate { unique: true }, red.clone()),
            CondFormat::new(vec![rng("B1:B4")], CfKind::Text { op: TextOp::EndsWith, text: "x\"y".into() }, red.clone()),
            CondFormat::new(vec![rng("B1:B4")], CfKind::Blanks(false), red.clone()),
            CondFormat::new(vec![rng("B1:B4")], CfKind::Errors(true), red.clone()),
            CondFormat::new(vec![rng("B1:B4")], CfKind::Date(DatePeriod::LastMonth), red.clone()),
            CondFormat::new(vec![rng("A1:A4")], CfKind::color_scale(&[[1, 2, 3], [4, 5, 6], [7, 8, 9]]), Dxf::default()),
            CondFormat::new(vec![rng("A1:A4")], CfKind::DataBar { min: Cfvo::new(VoKind::AutoMin, ""), max: Cfvo::new(VoKind::Percentile, "90"), color: [0, 0, 255], negative: Some([255, 0, 0]), gradient: false, border: Some([0, 0, 128]), show_value: false }, Dxf::default()),
            CondFormat::new(vec![rng("A1:A4")], CfKind::IconSet { set: "4Arrows".into(), vos: crate::condformat::icon_cfvos("4Arrows"), reverse: true, show_value: true }, Dxf::default()),
            CondFormat::new(vec![rng("D1:D4")], CfKind::icon_set("3Stars"), Dxf::default()),
        ];
        s.validations = vec![
            Validation { ranges: vec![rng("E1:E9")], kind: DvKind::List, f1: "\"a,b,c\"".into(), dropdown: false, prompt_title: "Pick".into(), prompt: "One of a, b, c".into(), ..Default::default() },
            Validation { ranges: vec![rng("F1"), rng("G2:G3")], kind: DvKind::Whole, op: Op::NotBetween, f1: "1".into(), f2: "$A$1*2".into(), style: AlertStyle::Information, error_title: "No".into(), error: "<bad> & \"wrong\"".into(), allow_blank: false, show_input: false, ..Default::default() },
            Validation { ranges: vec![rng("H1")], kind: DvKind::Custom, f1: "ISNUMBER(H1)".into(), ..Default::default() },
        ];
        let bytes = super::super::write(&wb).unwrap();
        let back = super::super::read(&bytes).unwrap();
        let (a, b) = (&wb.sheets[0], &back.sheets[0]);
        assert_eq!(a.cond_formats.len(), b.cond_formats.len());
        for (x, y) in a.cond_formats.iter().zip(&b.cond_formats) {
            if let (CfKind::DataBar { min, .. }, CfKind::DataBar { min: m2, .. }) = (&x.kind, &y.kind) { assert_eq!(min, m2); }
            assert_eq!(x, y);
        }
        assert_eq!(a.validations, b.validations);
        assert!(!b.extra.iter().any(|(k, _)| k == "conditionalFormatting" || k == "dataValidations"));
        // Saving again neither duplicates the differential formats nor changes the rules.
        let again = super::super::write(&back).unwrap();
        let back2 = super::super::read(&again).unwrap();
        assert_eq!(back2.sheets[0].cond_formats, b.cond_formats);
        let styles = |bytes: &[u8]| { let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap(); let mut s = String::new(); std::io::Read::read_to_string(&mut z.by_name("xl/styles.xml").unwrap(), &mut s).unwrap(); s };
        assert_eq!(styles(&bytes).matches("<dxf>").count(), styles(&again).matches("<dxf>").count());
        assert!(cf_at(&back, 0, Pos::new(1, 0)).is_some());
    }

    #[test]
    fn reads_x14_rules_and_keeps_other_extensions() {
        let pal = Palette::new(crate::numfmt::OFFICE_THEME.to_vec(), None);
        let mut s = Sheet::new("S");
        s.extra.push(("conditionalFormatting".into(), r#"<conditionalFormatting sqref="A1:A5"><cfRule type="dataBar" priority="2"><dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar><extLst><ext uri="{B025F937-C7B1-47D3-B67F-A62EFF666E3E}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:id>{AB}</x14:id></ext></extLst></cfRule><cfRule type="cellIs" dxfId="0" priority="1" operator="greaterThan"><formula>_xlfn.XLOOKUP(1,B:B,C:C)</formula></cfRule></conditionalFormatting>"#.into()));
        s.extra.push(("extLst".into(), r#"<extLst><ext uri="{78C0D931-6437-407d-A8EE-F0AAD7539E65}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:conditionalFormattings><x14:conditionalFormatting xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main"><x14:cfRule type="dataBar" id="{AB}"><x14:dataBar gradient="0" border="1"><x14:cfvo type="autoMin"/><x14:cfvo type="autoMax"/><x14:borderColor rgb="FF000080"/><x14:negativeFillColor rgb="FF00FF00"/></x14:dataBar></x14:cfRule><xm:sqref>A1:A5</xm:sqref></x14:conditionalFormatting><x14:conditionalFormatting xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main"><x14:cfRule type="iconSet" priority="3" id="{CD}"><x14:iconSet iconSet="3Triangles"><x14:cfvo type="percent"><xm:f>0</xm:f></x14:cfvo><x14:cfvo type="percent"><xm:f>33</xm:f></x14:cfvo><x14:cfvo type="percent"><xm:f>67</xm:f></x14:cfvo></x14:iconSet></x14:cfRule><xm:sqref>B1:B5</xm:sqref></x14:conditionalFormatting></x14:conditionalFormattings></ext><ext uri="{CCE6A557-97BC-4b89-ADB6-D9C93CAAB3DF}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:dataValidations count="1" xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main"><x14:dataValidation type="list" allowBlank="1" showInputMessage="1" showErrorMessage="1"><x14:formula1><xm:f>Other!$A$1:$A$3</xm:f></x14:formula1><xm:sqref>C1:C4</xm:sqref></x14:dataValidation></x14:dataValidations></ext><ext uri="{05C60535-1F16-4fd2-B633-F4F36F0B64E0}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:sparklineGroups/></ext></extLst>"#.into()));
        take(&mut s, &[Dxf { bold: Some(true), ..Default::default() }], &pal);
        assert_eq!(s.cond_formats.len(), 3);
        assert_eq!(s.cond_formats[0].kind, CfKind::CellIs { op: Op::Greater, f1: "XLOOKUP(1,B:B,C:C)".into(), f2: String::new() });
        assert_eq!(s.cond_formats[0].dxf.bold, Some(true));
        let CfKind::DataBar { gradient, negative, border, min, .. } = &s.cond_formats[1].kind else { panic!() };
        assert_eq!((*gradient, *negative, *border, min.kind), (false, Some([0, 255, 0]), Some([0, 0, 128]), VoKind::AutoMin));
        assert!(matches!(&s.cond_formats[2].kind, CfKind::IconSet { set, vos, .. } if set == "3Triangles" && vos[2].val == "67"));
        assert_eq!(s.validations[0].f1, "Other!$A$1:$A$3");
        assert_eq!(s.validations[0].ranges, vec![rng("C1:C4")]);
        let ext = &s.extra.iter().find(|(k, _)| k == "extLst").unwrap().1;
        assert!(ext.contains("sparklineGroups") && !ext.contains("dataValidation") && !ext.contains("conditionalFormatting"));
    }
}
