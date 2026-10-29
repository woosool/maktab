//! Equations as Excel saves them: a text box whose paragraph holds Office Math
//! (`a14:m` / `m:oMathPara`) inside `mc:AlternateContent` Requires="a14", with a fallback text
//! box filled with the equation's picture. In Jadwal an equation is a picture whose description
//! is `EQ_TAG` + its linear form (app/src/ins_pictures.rs); the math is converted with the word
//! engine's OMML code both ways. Reading accepts the frame inside an anchor or wrapping whole
//! anchors (top level), and renders the picture when no fallback image is at hand.

use super::native_objects::Sink;
use super::xml::{attr, text, Element};
use crate::book::Workbook;
use crate::drawing::{ObjKind, Object, Rel};
use std::sync::Arc;

/// The description prefix of equation pictures (as app/src/ins_pictures.rs EQ_TAG).
pub const EQ_TAG: &str = "qalam-equation:";
pub const NS_A14: &str = "http://schemas.microsoft.com/office/drawing/2010/main";
pub const NS_M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const RT_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const RUN_PR: &str = r#"<w:rPr><w:rFonts w:ascii="Cambria Math" w:hAnsi="Cambria Math"/></w:rPr>"#;
const RUN_PR_A: &str = r#"<a:rPr lang="en-US" sz="1600" i="1"><a:latin typeface="Cambria Math" panose="02040503050406030204" pitchFamily="18" charset="0"/></a:rPr>"#;

fn emu(px: f32) -> i64 { (px as f64 * crate::drawing::EMU as f64).round() as i64 }

/// The `m:oMath` of a linear-format equation, runs styled the DrawingML way.
pub fn omml(linear: &str) -> String {
    word_engine::equation::omml(&word_engine::equation::Equation::new(linear)).replace(RUN_PR, RUN_PR_A)
}

/// The linear form of an OMML container (oMathPara, oMath), whatever its prefix.
pub fn linear(e: &Element) -> String {
    // The word engine matches "m:" names: rename math-namespace elements, drop the others.
    fn norm(e: &Element, math: &[String], out: &mut String) {
        let (pre, local) = match e.name.split_once(':') { Some((p, l)) => (p, l), None => ("", e.name.as_str()) };
        if !math.iter().any(|m| m == pre) { return; }
        out.push_str(&format!("<m:{local}"));
        for (k, v) in &e.attrs { if k.starts_with("xmlns") { continue; } let l = super::xml::local(k); out.push_str(&format!(" m:{l}=\"{}\"", attr(v))); }
        out.push('>');
        for c in &e.children { match c { super::xml::Node::Element(x) => norm(x, math, out), super::xml::Node::Text(t) => out.push_str(&text(t)), #[allow(unreachable_patterns)] _ => {} } }
        out.push_str(&format!("</m:{local}>"));
    }
    let mut math = vec!["m".to_string()];
    if let Some((k, _)) = e.attrs.iter().find(|(k, v)| k.starts_with("xmlns:") && v == NS_M) { math.push(k[6..].to_string()); }
    if let Some(p) = e.name.split_once(':').map(|x| x.0) { math.push(p.to_string()); }
    let mut s = format!("<m:root xmlns:m=\"{NS_M}\">");
    let maths: Vec<&Element> = if e.local() == "oMathPara" { e.all("oMath").collect() } else { vec![e] };
    for m in maths { norm(m, &math, &mut s); }
    s.push_str("</m:root>");
    let Ok(w) = word_engine::xml::parse(&s) else { return String::new() };
    let lines: Vec<String> = w.children.iter().filter_map(|n| if let word_engine::xml::Node::Element(x) = n { Some(word_engine::equation::read_omml(x, true).linear) } else { None }).collect();
    lines.join("\n")
}

/// The equation picture rendered as the app renders it (PNG at 2×).
pub fn render(linear: &str) -> Option<Vec<u8>> {
    let eq = word_engine::equation::Equation { linear: linear.into(), size: 16.0, color: word_engine::Color::BLACK, display: true };
    word_engine::objects::object_png(&word_engine::objects::Object::Equation(eq))
}

/// The native frame of an equation picture.
pub fn write(w: &mut Workbook, o: &Object, sink: &mut Sink) -> Option<String> {
    let lin = o.descr.strip_prefix(EQ_TAG)?;
    let ObjKind::Picture { data, ext } = &o.kind else { return None };
    // The picture: the part it came from while unchanged, else a new media part.
    let target = match o.rels.iter().find(|r| r.kind == RT_IMAGE && !r.external && w.parts.get(&r.target).is_some_and(|b| b.as_slice() == data.as_slice())) {
        Some(r) => r.target.clone(),
        None => {
            let p = (sink.free)("xl/media/image", &format!(".{ext}"));
            w.parts.insert(p.clone(), (**data).clone());
            sink.defaults.push((ext.clone(), if ext == "png" { "image/png".into() } else { format!("image/{ext}") }));
            p
        }
    };
    let rid = (sink.add_rel)(RT_IMAGE, target, false);
    let (cx, cy) = match &o.anchor { crate::drawing::Anchor::One { w, h, .. } | crate::drawing::Anchor::Abs { w, h, .. } => (emu(*w), emu(*h)), _ => (0, 0) };
    let paras: String = lin.split('\n').map(|l| format!("<a:p><a:pPr algn=\"ctr\"/><a14:m><m:oMathPara xmlns:m=\"{NS_M}\"><m:oMathParaPr><m:jc m:val=\"centerGroup\"/></m:oMathParaPr>{}</m:oMathPara></a14:m><a:endParaRPr lang=\"en-US\" sz=\"1600\"/></a:p>", omml(l))).collect();
    let nv = |extra: &str| format!("<xdr:nvSpPr><xdr:cNvPr id=\"{}\" name=\"{}\" descr=\"{}\"/><xdr:cNvSpPr txBox=\"1\"{extra}/></xdr:nvSpPr>", o.id, attr(&o.name), attr(&o.descr));
    let xfrm = format!("<a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>");
    let body = "<a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\" wrap=\"none\" rtlCol=\"0\" anchor=\"ctr\" anchorCtr=\"1\"/><a:lstStyle/>";
    Some(format!("<mc:AlternateContent xmlns:mc=\"{NS_MC}\"><mc:Choice xmlns:a14=\"{NS_A14}\" Requires=\"a14\"><xdr:sp macro=\"\" textlink=\"\">{}<xdr:spPr>{xfrm}<a:noFill/><a:ln><a:noFill/></a:ln></xdr:spPr><xdr:txBody>{body}{paras}</xdr:txBody></xdr:sp></mc:Choice>\
<mc:Fallback><xdr:sp macro=\"\" textlink=\"\">{}<xdr:spPr>{xfrm}<a:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></a:blipFill><a:ln><a:noFill/></a:ln></xdr:spPr><xdr:txBody>{body}<a:p><a:endParaRPr lang=\"en-US\" sz=\"1100\"/></a:p></xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent>",
        nv(""), nv("")))
}

fn find<'a>(e: &'a Element, l: &str) -> Option<&'a Element> { e.elements().find_map(|c| if c.local() == l { Some(c) } else { find(c, l) }) }

/// The `a14:m` holders of a drawing element (a text box's paragraphs).
fn math_of(e: &Element) -> Option<&Element> {
    fn walk(e: &Element) -> Option<&Element> {
        for c in e.elements() {
            if c.local() == "m" && c.elements().any(|x| matches!(x.local(), "oMathPara" | "oMath")) { return Some(c); }
            if c.local() == "Fallback" { continue; }
            if let Some(x) = walk(c) { return Some(x); }
        }
        None
    }
    walk(e)
}

/// Whether drawing content is an Excel equation.
pub fn is_equation(content: &Element) -> bool { math_of(content).is_some() }

/// A read equation text box (or frame) as the equation picture.
pub fn read(wb: &Workbook, content: &Element, o: &mut Object) -> bool {
    let Some(m) = math_of(content) else { return false };
    let lines: Vec<String> = m.elements().map(linear).filter(|l| !l.is_empty()).collect();
    // Further paragraphs with math.
    let mut all = lines;
    if let Some(tx) = find(content, "txBody") {
        let ms: Vec<&Element> = tx.all("p").filter_map(|p| p.elements().find(|c| c.local() == "m")).collect();
        if ms.len() > 1 { all = ms.iter().flat_map(|m| m.elements().map(linear)).filter(|l| !l.is_empty()).collect(); }
    }
    let lin = all.join("\n");
    if lin.is_empty() { return false; }
    // The fallback's picture when there is one, else rendered.
    let blip = content.child("Fallback").and_then(|f| find(f, "blip")).and_then(|b| b.attrs.iter().find(|(k, _)| k.ends_with(":embed")).map(|(_, v)| v.clone()));
    let img = blip.and_then(|id| o.rels.iter().find(|r| r.id == id && !r.external).cloned()).and_then(|r| wb.parts.get(&r.target).map(|b| (r, b.clone())));
    let (data, ext, rels) = match img {
        Some((r, b)) if !b.is_empty() => { let ext = r.target.rsplit('.').next().unwrap_or("png").to_ascii_lowercase(); (b, ext, vec![Rel { id: r.id, kind: RT_IMAGE.into(), target: r.target, external: false }]) }
        _ => match render(&lin) { Some(p) => (p, "png".into(), vec![]), None => return false },
    };
    o.kind = ObjKind::Picture { data: Arc::new(data), ext };
    o.descr = format!("{EQ_TAG}{lin}");
    if o.name.is_empty() || o.name.starts_with("TextBox") { o.name = o.name.replacen("TextBox", "Equation", 1); }
    o.rels = rels;
    o.raw = None;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{add_object, Anchor, Mark};
    use crate::xlsx::pkgcheck::{check_package, unzip};

    #[test]
    fn linear_omml_round_trip() {
        for l in ["x=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}", "a^2+b^2=c^2"] {
            let x = format!("<m:oMathPara xmlns:m=\"{NS_M}\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">{}</m:oMathPara>", omml(l));
            let e = Element::parse(&x).unwrap();
            let back = linear(&e);
            assert_eq!(omml(&back), omml(l), "{l} -> {back}");
        }
    }

    #[test]
    fn equation_round_trips() {
        let mut wb = Workbook::new();
        let lin = "a^2+b^2=c^2";
        let png = render(lin).unwrap();
        let mut o = Object::new(ObjKind::Picture { data: Arc::new(png.clone()), ext: "png".into() }, Anchor::One { from: Mark { col: 1, row: 1, ..Default::default() }, w: 120.0, h: 30.0 });
        o.descr = format!("{EQ_TAG}{lin}");
        add_object(&mut wb.sheets[0].objects, o, "Equation");
        let bytes = crate::xlsx::write(&wb).unwrap();
        if let Ok(d) = std::env::var("JADWAL_DUMP") { std::fs::write(format!("{d}/equation_parts.xlsx"), &bytes).unwrap(); }
        let files = unzip(&bytes);
        check_package(&files);
        let d = &files["xl/drawings/drawing1.xml"];
        assert!(d.contains("Requires=\"a14\"") && d.contains("<a14:m><m:oMathPara") && d.contains("<m:sSup>") && !d.contains("w:rPr"), "{d}");
        let back = crate::xlsx::read(&bytes).unwrap();
        let ob = &back.sheets[0].objects[0];
        assert_eq!(ob.descr, format!("{EQ_TAG}{lin}"));
        assert!(matches!(&ob.kind, ObjKind::Picture { data, .. } if data.as_slice() == png.as_slice()));
        // Saved again: still native.
        let again = unzip(&crate::xlsx::write(&back).unwrap());
        check_package(&again);
        assert!(again["xl/drawings/drawing1.xml"].contains("<a14:m>"));
    }
}
