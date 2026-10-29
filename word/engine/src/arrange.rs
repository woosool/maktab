//! Arrange: stacking order of floating objects (Bring Forward, Send Backward) and the list the
//! Selection Pane shows.

use crate::model::Document;

/// A drawing in the document for the Selection Pane: position, name, floating z-order.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawingEntry { pub pos: usize, pub name: String, pub z: Option<i64> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stack { Forward, ToFront, Backward, ToBack }

impl Document {
    /// Every picture and object, with a name like Word's ("Picture 1", "Chart 2").
    pub fn drawings(&self) -> Vec<DrawingEntry> {
        let mut out = vec![];
        let mut counts = std::collections::HashMap::new();
        let mut pos = 0;
        for p in &self.paragraphs {
            for r in &p.runs {
                let kind = if r.image.is_some() { Some("Picture") } else { r.object.as_ref().map(|o| match o { crate::objects::Object::Shape(_) => "Shape", crate::objects::Object::Chart(_) => "Chart", crate::objects::Object::Diagram(_) => "Diagram", crate::objects::Object::Equation(_) => "Equation", crate::objects::Object::Ink(_) => "Ink" }) };
                if let Some(k) = kind {
                    let n = counts.entry(k).or_insert(0);
                    *n += 1;
                    out.push(DrawingEntry { pos, name: format!("{k} {n}"), z: r.anchor.as_ref().map(|a| a.z) });
                }
                pos += r.len();
            }
            pos += 1;
        }
        out
    }

    /// Move the floating object at `pos` in the stacking order.
    pub fn restack(&mut self, pos: usize, how: Stack) {
        let Some(mut a) = self.anchor_at(pos) else { return };
        let others: Vec<i64> = self.drawings().into_iter().filter(|d| d.pos != pos).filter_map(|d| d.z).collect();
        a.z = match how {
            Stack::ToFront => others.iter().max().map(|m| m + 1).unwrap_or(a.z).max(a.z),
            Stack::ToBack => others.iter().min().map(|m| m - 1).unwrap_or(a.z).min(a.z),
            Stack::Forward => others.iter().filter(|z| **z >= a.z).min().map(|z| z + 1).unwrap_or(a.z),
            Stack::Backward => others.iter().filter(|z| **z <= a.z).max().map(|z| z - 1).unwrap_or(a.z),
        };
        // Keep z non-negative (relativeHeight is unsigned).
        let shift = -a.z.min(0);
        if shift > 0 {
            let all: Vec<usize> = self.drawings().into_iter().filter(|d| d.pos != pos && d.z.is_some()).map(|d| d.pos).collect();
            for p in all { if let Some(mut b) = self.anchor_at(p) { b.z += shift; self.set_anchor(p, Some(b)); } }
            a.z += shift;
        }
        self.set_anchor(pos, Some(a));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Anchor, Paragraph, Wrap};

    #[test]
    fn restacking_orders_floats() {
        let mut d = Document::new(vec![Paragraph::new("x")]);
        for _ in 0..3 { d.select(0, 0); d.insert_image(vec![0x89, b'P', b'N', b'G'], "image/png", 10.0, 10.0); }
        let ds = d.drawings();
        for (k, e) in ds.iter().enumerate() { d.set_anchor(e.pos, Some(Anchor { z: k as i64, ..Anchor::new(Wrap::Square, 0.0, 0.0) })); }
        let first = d.drawings()[0].pos;
        d.restack(first, Stack::ToFront);
        assert_eq!(d.drawings()[0].z, Some(3));
        d.restack(first, Stack::ToBack);
        let zs: Vec<i64> = d.drawings().iter().filter_map(|x| x.z).collect();
        assert!(zs[0] < zs[1] && zs[0] < zs[2] && zs.iter().all(|z| *z >= 0), "{zs:?}");
        assert_eq!(d.drawings()[1].name, "Picture 2");
    }
}
