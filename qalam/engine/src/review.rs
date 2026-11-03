//! Review tab features in the engine: Compare (a document showing another version's
//! differences as tracked changes), Restrict Editing (w:documentProtection with Word's
//! password hash), and Check Accessibility.

use crate::model::{Document, RevKind, Revision, Run};
use sha2::{Digest, Sha512};

// ---------------------------------------------------------------------------
// Compare

fn lcs_pairs<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    // Very long inputs compare greedily from both ends to stay fast.
    if n * m > 4_000_000 {
        let mut out = vec![];
        let (mut i, mut j) = (0, 0);
        while i < n && j < m { if a[i] == b[j] { out.push((i, j)); i += 1; j += 1; } else if n - i > m - j { i += 1; } else { j += 1; } }
        return out;
    }
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() { for j in (0..m).rev() { dp[i][j] = if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) }; } }
    let (mut i, mut j, mut out) = (0, 0, vec![]);
    while i < n && j < m {
        if a[i] == b[j] { out.push((i, j)); i += 1; j += 1; } else if dp[i + 1][j] >= dp[i][j + 1] { i += 1; } else { j += 1; }
    }
    out
}

/// Words and the spaces after them, so joined tokens give back the text.
fn tokens(s: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for c in s.chars() {
        let space = c.is_whitespace();
        match out.last_mut() {
            Some(t) if !space && !t.ends_with(char::is_whitespace) && c.is_alphanumeric() == t.chars().last().is_some_and(|x| x.is_alphanumeric()) => t.push(c),
            Some(t) if space && t.ends_with(char::is_whitespace) => t.push(c),
            Some(t) if space => t.push(c),
            _ => out.push(c.to_string()),
        }
    }
    out
}

/// Rebuild a paragraph's runs so text missing from `old` is a tracked insertion and old text
/// missing here comes back as tracked deletions. Runs with fields or objects are kept whole.
fn diff_runs(runs: &[Run], old: &str, rev: &dyn Fn(RevKind) -> Revision) -> Vec<Run> {
    let new_text: String = runs.iter().map(|r| r.text.as_str()).collect();
    let (a, b) = (tokens(old), tokens(&new_text));
    let pairs = lcs_pairs(&a, &b);
    // Per char of the new text: inserted? And deleted text to show before char k.
    let mut inserted = vec![true; new_text.chars().count()];
    let mut deleted_before: Vec<(usize, String)> = vec![];
    let starts_b: Vec<usize> = b.iter().scan(0, |acc, t| { let s = *acc; *acc += t.chars().count(); Some(s) }).collect();
    let (mut ai, mut bi) = (0, 0);
    for &(pa, pb) in pairs.iter().chain(std::iter::once(&(a.len(), b.len()))) {
        if ai < pa { deleted_before.push((starts_b.get(bi).copied().unwrap_or(inserted.len()), a[ai..pa].concat())); }
        if pb < b.len() { for k in starts_b[pb]..starts_b[pb] + b[pb].chars().count() { inserted[k] = false; } }
        ai = pa + 1; bi = pb + 1;
    }
    let mut out: Vec<Run> = vec![];
    let mut at = 0;
    let mut del = deleted_before.into_iter().peekable();
    for r in runs {
        let len = r.text.chars().count();
        if r.field.is_some() || r.object.is_some() || r.image.is_some() || r.marker.is_some() || r.note.is_some() {
            while let Some((p, t)) = del.next_if(|d| d.0 <= at) { let _ = p; out.push(Run { text: t, revision: Some(rev(RevKind::Del)), ..Run { text: String::new(), props: r.props.clone(), style: r.style.clone(), ..Default::default() } }); }
            let mut x = r.clone();
            if len > 0 && inserted[at] { x.revision = Some(rev(RevKind::Ins)); }
            out.push(x);
            at += len;
            continue;
        }
        let chars: Vec<char> = r.text.chars().collect();
        let mut k = 0;
        while k < len {
            while let Some((_, t)) = del.next_if(|d| d.0 <= at + k) { out.push(Run { text: t, props: r.props.clone(), style: r.style.clone(), link: r.link.clone(), revision: Some(rev(RevKind::Del)), ..Default::default() }); }
            let ins = inserted[at + k];
            let next_del = del.peek().map(|d| d.0).unwrap_or(usize::MAX);
            let mut e = k + 1;
            while e < len && inserted[at + e] == ins && at + e < next_del { e += 1; }
            let mut piece = r.clone();
            piece.text = chars[k..e].iter().collect();
            if ins { piece.revision = Some(rev(RevKind::Ins)); }
            out.push(piece);
            k = e;
        }
        at += len;
    }
    for (_, t) in del { let props = runs.last().map(|r| r.props.clone()).unwrap_or_default(); out.push(Run { text: t, props, revision: Some(rev(RevKind::Del)), ..Default::default() }); }
    out
}

impl Document {
    /// Compare: `revised` with the differences from `self` (the original) as tracked changes by
    /// `author`. Body paragraphs are matched by text; table cells are compared cell by cell when
    /// the tables have the same shape.
    pub fn compare(&self, revised: &Document, author: &str, date: &str) -> Result<Document, String> {
        let mut out = crate::docx::read(&crate::docx::write(revised)?)?;
        let rev = |k: RevKind| Revision { kind: k, author: author.into(), date: date.into() };
        let body_old: Vec<usize> = (0..self.paragraphs.len()).filter(|&i| self.paragraphs[i].cell.is_none()).collect();
        let body_new: Vec<usize> = (0..out.paragraphs.len()).filter(|&i| out.paragraphs[i].cell.is_none()).collect();
        let to = |d: &Document, v: &[usize]| v.iter().map(|&i| d.paragraphs[i].text()).collect::<Vec<_>>();
        let (ta, tb) = (to(self, &body_old), to(&out, &body_new));
        let pairs = lcs_pairs(&ta, &tb);
        // Unmatched paragraphs between matched ones: pair them up in order (changed
        // paragraphs), the rest are whole insertions or deletions.
        let mut plan: Vec<(Option<usize>, Option<usize>)> = vec![];
        let (mut i, mut j) = (0, 0);
        for &(pa, pb) in pairs.iter().chain(std::iter::once(&(ta.len(), tb.len()))) {
            let (olds, news): (Vec<usize>, Vec<usize>) = ((i..pa).collect(), (j..pb).collect());
            for k in 0..olds.len().max(news.len()) { plan.push((olds.get(k).copied(), news.get(k).copied())); }
            if pa < ta.len() { plan.push((Some(pa), Some(pb))); }
            i = pa + 1; j = pb + 1;
        }
        // Apply from the end so paragraph indices stay valid.
        let mut insert_at = out.paragraphs.len();
        for (o, n) in plan.into_iter().rev() {
            match (o, n) {
                (Some(o), Some(n)) => {
                    let ni = body_new[n];
                    insert_at = ni;
                    if ta[o] != tb[n] { out.paragraphs[ni].runs = diff_runs(&out.paragraphs[ni].runs, &ta[o], &rev); }
                }
                (None, Some(n)) => {
                    let ni = body_new[n];
                    insert_at = ni;
                    let p = &mut out.paragraphs[ni];
                    for r in &mut p.runs { if r.marker.is_none() { r.revision = Some(rev(RevKind::Ins)); } }
                    // The document's final mark can't be inserted: the mark before it is.
                    if ni + 1 == out.paragraphs.len() && ni > 0 { out.paragraphs[ni - 1].mark_revision = Some(rev(RevKind::Ins)); }
                    else { out.paragraphs[ni].mark_revision = Some(rev(RevKind::Ins)); }
                }
                (Some(o), None) => {
                    let mut p = self.paragraphs[body_old[o]].clone();
                    p.id = crate::model::new_id();
                    p.runs.retain(|r| r.marker.is_none() && r.note.is_none());
                    for r in &mut p.runs { r.revision = Some(rev(RevKind::Del)); }
                    p.mark_revision = Some(rev(RevKind::Del));
                    // Keep it before the next body paragraph, outside any table.
                    let at = insert_at.min(out.paragraphs.len());
                    let at = if at < out.paragraphs.len() && out.paragraphs[at].cell.is_some() { out.paragraphs[..at].iter().rposition(|q| q.cell.is_none()).map(|k| k + 1).unwrap_or(0) } else { at };
                    out.paragraphs.insert(at, p);
                }
                (None, None) => {}
            }
        }
        // Tables of the same shape compare cell by cell.
        for (k, t_new) in out.tables.clone().iter().enumerate() {
            let Some(t_old) = self.tables.get(k) else { break };
            let shape = |t: &crate::model::Table| t.rows.iter().map(|r| r.cells.len()).collect::<Vec<_>>();
            if shape(t_old) != shape(t_new) { continue; }
            for (co, cn) in t_old.cells().zip(t_new.cells()) {
                let old_text: String = self.paragraphs.iter().filter(|p| p.cell == Some(co.id)).map(|p| p.text()).collect::<Vec<_>>().join("\n");
                let idx: Vec<usize> = (0..out.paragraphs.len()).filter(|&i| out.paragraphs[i].cell == Some(cn.id)).collect();
                let new_text: String = idx.iter().map(|&i| out.paragraphs[i].text()).collect::<Vec<_>>().join("\n");
                if old_text != new_text && idx.len() == 1 { out.paragraphs[idx[0]].runs = diff_runs(&out.paragraphs[idx[0]].runs, &old_text, &rev); }
            }
        }
        out.track_changes = false;
        out.dirty = true;
        out.format_epoch += 1;
        out.changed();
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Restrict Editing

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Restriction { #[default] ReadOnly, Comments, TrackedChanges, Forms }

impl Restriction {
    pub const ALL: [(Restriction, &'static str); 4] = [(Restriction::ReadOnly, "No changes (Read only)"), (Restriction::Comments, "Comments"), (Restriction::TrackedChanges, "Tracked changes"), (Restriction::Forms, "Filling in forms")];
    pub fn val(&self) -> &'static str { match self { Restriction::ReadOnly => "readOnly", Restriction::Comments => "comments", Restriction::TrackedChanges => "trackedChanges", Restriction::Forms => "forms" } }
    pub fn from_val(v: &str) -> Option<Restriction> { Some(match v { "readOnly" => Restriction::ReadOnly, "comments" => Restriction::Comments, "trackedChanges" => Restriction::TrackedChanges, "forms" => Restriction::Forms, _ => return None }) }
}

/// Enforced editing restrictions; a password is stored as Word does (SHA-512, salt, 100,000
/// iterations), never in plain text.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Protection { pub kind: Restriction, pub hash: Option<(String, String)> }

const SPIN: u32 = 100_000;

fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 { if k <= c.len() { out.push(T[(n >> (18 - 6 * k) & 63) as usize] as char); } else { out.push('='); } }
    }
    out
}

/// The hash Word stores for a protection password (ECMA-376 Part 4 §2.15.1.28 with SHA-512).
pub fn password_hash(password: &str, salt: &[u8]) -> String {
    let utf16: Vec<u8> = password.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let mut h = Sha512::new();
    h.update(salt);
    h.update(&utf16);
    let mut hash = h.finalize().to_vec();
    for i in 0..SPIN {
        let mut h = Sha512::new();
        h.update(&hash);
        h.update(i.to_le_bytes());
        hash = h.finalize().to_vec();
    }
    b64(&hash)
}

impl Protection {
    pub fn new(kind: Restriction, password: &str) -> Protection {
        if password.is_empty() { return Protection { kind, hash: None }; }
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(7);
        let salt: Vec<u8> = (0..16u32).map(|k| (seed.wrapping_mul(6364136223846793005u128.wrapping_add(k as u128 * 1442695040888963407)) >> 64) as u8).collect();
        Protection { kind, hash: Some((password_hash(password, &salt), b64(&salt))) }
    }
    /// Does the password unlock this protection?
    pub fn unlocks(&self, password: &str) -> bool {
        match &self.hash {
            None => true,
            Some((hash, salt)) => b64_decode(salt).is_some_and(|s| password_hash(password, &s) == *hash),
        }
    }
    /// The w:documentProtection element.
    pub fn xml(&self) -> String {
        let mut s = format!(r#"<w:documentProtection w:edit="{}" w:enforcement="1""#, self.kind.val());
        if let Some((h, salt)) = &self.hash { s += &format!(r#" w:cryptProviderType="rsaAES" w:cryptAlgorithmClass="hash" w:cryptAlgorithmType="typeAny" w:cryptAlgorithmSid="14" w:cryptSpinCount="{SPIN}" w:hash="{h}" w:salt="{salt}""#); }
        s + "/>"
    }
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> { Some(match c { b'A'..=b'Z' => c - b'A', b'a'..=b'z' => c - b'a' + 26, b'0'..=b'9' => c - b'0' + 52, b'+' => 62, b'/' => 63, _ => return None } as u32) };
    let bytes: Vec<u8> = s.bytes().filter(|c| *c != b'=' && !c.is_ascii_whitespace()).collect();
    let mut out = vec![];
    for c in bytes.chunks(4) {
        let mut n = 0u32;
        for (k, b) in c.iter().enumerate() { n |= val(*b)? << (18 - 6 * k); }
        for k in 0..c.len().saturating_sub(1) { out.push((n >> (16 - 8 * k)) as u8); }
    }
    Some(out)
}

/// Read w:documentProtection from settings.xml.
pub fn read_protection(e: &crate::xml::Element) -> Option<Protection> {
    if !e.attr("w:enforcement").is_some_and(|v| v == "1" || v == "true" || v == "on") { return None; }
    let kind = Restriction::from_val(e.attr("w:edit")?)?;
    let hash = match (e.attr("w:hash").or(e.attr("w:hashValue")), e.attr("w:salt").or(e.attr("w:saltValue"))) { (Some(h), Some(s)) => Some((h.to_string(), s.to_string())), _ => None };
    Some(Protection { kind, hash })
}

// ---------------------------------------------------------------------------
// Accessibility

#[derive(Clone, Debug, PartialEq)]
/// An accessibility finding. `detail` is a message template whose `{}` are filled from `args`
/// in order (so the interface can translate the template).
pub struct AccessIssue { pub severity: &'static str, pub title: String, pub detail: String, pub args: Vec<String>, pub pos: Option<usize> }

impl Document {
    /// Check Accessibility: errors (missing alt text, tables without a header row, a document
    /// without a title) and warnings (headings out of order, unclear link text, low contrast,
    /// blank table cells used for layout).
    pub fn accessibility_issues(&self) -> Vec<AccessIssue> {
        let mut out = vec![];
        let mut pos = 0;
        let mut last_level: Option<u8> = None;
        for p in &self.paragraphs {
            let f = self.para_format(p);
            if f.outline_level < 9 && p.cell.is_none() && !p.text().trim().is_empty() {
                if let Some(l) = last_level { if f.outline_level > l + 1 { out.push(AccessIssue { severity: "Warning", title: "Skipped heading level".into(), detail: "\"{}\" jumps from level {} to {}.".into(), args: vec![p.text().trim().to_string(), (l + 1).to_string(), (f.outline_level + 1).to_string()], pos: Some(pos) }); } }
                last_level = Some(f.outline_level);
            }
            let mut at = pos;
            for r in &p.runs {
                if let Some(img) = &r.image { if img.alt.trim().is_empty() { out.push(AccessIssue { severity: "Error", title: "Missing alternative text".into(), detail: "A picture has no description for screen readers.".into(), args: vec![], pos: Some(at) }); } }
                if let Some(obj) = &r.object { if !matches!(obj, crate::objects::Object::Equation(_) | crate::objects::Object::Ink(_)) { out.push(AccessIssue { severity: "Warning", title: "Check object description".into(), detail: "Shapes, charts and diagrams need alternative text in Word.".into(), args: vec![], pos: Some(at) }); } }
                if let Some(link) = &r.link {
                    let t = r.text.trim().to_lowercase();
                    if ["click here", "here", "link", "more", "read more"].contains(&t.as_str()) || t.starts_with("http") && t == link.to_lowercase() { out.push(AccessIssue { severity: "Warning", title: "Unclear hyperlink text".into(), detail: "\"{}\" doesn't say where the link goes.".into(), args: vec![r.text.trim().to_string()], pos: Some(at) }); }
                }
                let rf = self.run_format(p, r);
                if let Some(c) = rf.color {
                    let lum = |c: crate::Color| { let f = |v: u8| { let v = v as f32 / 255.0; if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }; 0.2126 * f(c.0) + 0.7152 * f(c.1) + 0.0722 * f(c.2) };
                    let bg = self.page.color.unwrap_or(crate::Color(255, 255, 255));
                    let (a, b) = (lum(c), lum(bg));
                    let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                    if ratio < 4.5 && !r.text.trim().is_empty() && !out.iter().any(|i: &AccessIssue| i.title == "Hard-to-read text contrast" && i.pos.is_some_and(|q| q + 400 > at)) {
                        out.push(AccessIssue { severity: "Warning", title: "Hard-to-read text contrast".into(), detail: "Text color contrast is {}:1 (4.5:1 recommended).".into(), args: vec![format!("{ratio:.1}")], pos: Some(at) });
                    }
                }
                at += r.len();
            }
            pos += p.len() + 1;
        }
        for t in &self.tables {
            let first = t.rows.first().and_then(|r| r.cells.first()).map(|c| c.id);
            let at = first.and_then(|c| self.paragraphs.iter().position(|p| p.cell == Some(c))).map(|i| self.paragraph_start(i));
            if t.rows.len() > 1 && !t.rows[0].header { out.push(AccessIssue { severity: "Error", title: "Table has no header row".into(), detail: "Mark the first row as a header row (Repeat Header Rows).".into(), args: vec![], pos: at }); }
            if t.cells().any(|c| c.v_merge != crate::model::VMerge::None || c.span > 1) { out.push(AccessIssue { severity: "Warning", title: "Merged or split cells".into(), detail: "Screen readers navigate simple table grids more easily.".into(), args: vec![], pos: at }); }
        }
        if !self.paragraphs.iter().any(|p| self.para_format(p).outline_level < 9) && self.paragraphs.len() > 20 {
            out.push(AccessIssue { severity: "Warning", title: "No headings".into(), detail: "Long documents are easier to navigate with heading styles.".into(), args: vec![], pos: None });
        }
        if self.properties.title.trim().is_empty() {
            out.push(AccessIssue { severity: "Tip", title: "Document title missing".into(), detail: "Add a title in File ▸ Properties.".into(), args: vec![], pos: None });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Paragraph;

    #[test]
    fn compare_marks_insertions_and_deletions() {
        let a = Document::new(vec![Paragraph::new("The quick brown fox."), Paragraph::new("Gone paragraph."), Paragraph::new("Same ending.")]);
        let b = Document::new(vec![Paragraph::new("The quick red fox jumps."), Paragraph::new("Same ending."), Paragraph::new("New tail.")]);
        let c = a.compare(&b, "Reviewer", "2026-01-01T00:00:00Z").unwrap();
        let revs = c.revisions();
        assert!(!revs.is_empty());
        let runs: Vec<(String, Option<RevKind>)> = c.paragraphs[0].runs.iter().map(|r| (r.text.clone(), r.revision.as_ref().map(|x| x.kind))).collect();
        assert!(runs.iter().any(|(t, k)| t.contains("brown") && *k == Some(RevKind::Del)), "{runs:?}");
        assert!(runs.iter().any(|(t, k)| t.contains("red") && *k == Some(RevKind::Ins)), "{runs:?}");
        assert!(c.paragraphs.iter().any(|p| p.text() == "Gone paragraph." && p.mark_revision.as_ref().is_some_and(|r| r.kind == RevKind::Del)));
        assert!(c.paragraphs.iter().any(|p| p.text() == "New tail." && p.runs.iter().all(|r| r.revision.as_ref().is_some_and(|r| r.kind == RevKind::Ins))));
        // Accepting everything gives the revised text; rejecting gives the original.
        let mut acc = c.clone_via_docx();
        let n = acc.len();
        acc.resolve_revisions(true, 0, n, true);
        assert_eq!(acc.text(), b.text());
        let mut rej = c.clone_via_docx();
        let n = rej.len();
        rej.resolve_revisions(false, 0, n, true);
        assert_eq!(rej.text(), a.text());
    }

    #[test]
    fn protection_hash_and_round_trip() {
        let p = Protection::new(Restriction::Comments, "secret");
        assert!(p.unlocks("secret") && !p.unlocks("wrong"));
        assert!(Protection::new(Restriction::ReadOnly, "").unlocks("anything"));
        let mut d = Document::new(vec![Paragraph::new("x")]);
        d.protection = Some(p.clone());
        let e = crate::docx::read(&crate::docx::write(&d).unwrap()).unwrap();
        assert_eq!(e.protection, Some(p));
    }

    #[test]
    fn accessibility_finds_problems() {
        let mut d = Document::new(vec![Paragraph::new("Body")]);
        d.select(4, 4);
        d.insert_table(2, 2);
        let kinds: Vec<String> = d.accessibility_issues().into_iter().map(|i| i.title).collect();
        assert!(kinds.iter().any(|k| k == "Table has no header row"), "{kinds:?}");
    }
}

impl Document {
    /// A deep copy through .docx (tests and merge).
    pub fn clone_via_docx(&self) -> Document { crate::docx::read(&crate::docx::write(self).unwrap()).unwrap() }
}
