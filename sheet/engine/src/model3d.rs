//! 3D models (Insert ▸ 3D Models): .obj and .glb meshes, rendered to a picture with flat
//! shading from a view turned by yaw and pitch. The picture is what the sheet shows and saves;
//! the model file travels with it (`embed`), so it can be turned again.

/// A triangle mesh: vertices and triangles (indices), with one base color.
#[derive(Clone, Debug, Default)]
pub struct Mesh { pub verts: Vec<[f32; 3]>, pub tris: Vec<[u32; 3]>, pub color: [u8; 3] }

/// Reads a model by extension ("obj" or "glb"/"gltf" binary).
pub fn load(bytes: &[u8], ext: &str) -> Result<Mesh, String> {
    let m = match ext.to_ascii_lowercase().as_str() { "obj" => obj(bytes), _ if bytes.starts_with(b"glTF") => glb(bytes), "glb" => Err("Not a binary glTF file.".into()), _ => obj(bytes) }?;
    if m.tris.is_empty() { return Err("The model has no triangles.".into()); }
    Ok(m)
}

fn obj(bytes: &[u8]) -> Result<Mesh, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut m = Mesh { color: [0x9E, 0xB4, 0xD8], ..Default::default() };
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => { let v: Vec<f32> = it.take(3).filter_map(|x| x.parse().ok()).collect(); if v.len() == 3 { m.verts.push([v[0], v[1], v[2]]); } }
            Some("f") => {
                let n = m.verts.len() as i64;
                let idx: Vec<u32> = it.filter_map(|x| x.split('/').next()?.parse::<i64>().ok()).map(|i| if i < 0 { n + i } else { i - 1 }).filter(|&i| i >= 0 && i < n).map(|i| i as u32).collect();
                for k in 1..idx.len().saturating_sub(1) { m.tris.push([idx[0], idx[k], idx[k + 1]]); }
            }
            _ => {}
        }
    }
    Ok(m)
}

fn glb(b: &[u8]) -> Result<Mesh, String> {
    let u32_at = |o: usize| -> Option<u32> { b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])) };
    let bad = || "The glTF file couldn't be read.".to_string();
    let mut off = 12;
    let (mut json, mut bin): (Option<&[u8]>, &[u8]) = (None, &[]);
    while off + 8 <= b.len() {
        let len = u32_at(off).ok_or_else(bad)? as usize;
        let kind = u32_at(off + 4).ok_or_else(bad)?;
        let data = b.get(off + 8..off + 8 + len).ok_or_else(bad)?;
        if kind == 0x4E4F534A { json = Some(data); } else if kind == 0x004E4942 { bin = data; }
        off += 8 + len;
    }
    let j: serde_json::Value = serde_json::from_slice(json.ok_or_else(bad)?).map_err(|_| bad())?;
    let views = j["bufferViews"].as_array().cloned().unwrap_or_default();
    let accs = j["accessors"].as_array().cloned().unwrap_or_default();
    // An accessor's elements as f32 or u32 tuples of `n` components.
    let read = |ai: usize| -> Option<(Vec<f64>, usize)> {
        let a = accs.get(ai)?;
        let v = views.get(a["bufferView"].as_u64()? as usize)?;
        let n = match a["type"].as_str()? { "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, _ => return None };
        let ct = a["componentType"].as_u64()?;
        let cs = match ct { 5120 | 5121 => 1, 5122 | 5123 => 2, _ => 4 };
        let count = a["count"].as_u64()? as usize;
        let stride = v["byteStride"].as_u64().map(|x| x as usize).unwrap_or(n * cs);
        let base = v["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let mut out = Vec::with_capacity(count * n);
        for i in 0..count { for c in 0..n {
            let o = base + i * stride + c * cs;
            let s = bin.get(o..o + cs)?;
            out.push(match ct { 5126 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64, 5125 => u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64, 5123 => u16::from_le_bytes([s[0], s[1]]) as f64, 5122 => i16::from_le_bytes([s[0], s[1]]) as f64, 5121 => s[0] as f64, _ => s[0] as i8 as f64 });
        } }
        Some((out, n))
    };
    let mut m = Mesh { color: [0x9E, 0xB4, 0xD8], ..Default::default() };
    if let Some(c) = j["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"].as_array() {
        let f = |i: usize| (c.get(i).and_then(|x| x.as_f64()).unwrap_or(0.7).clamp(0.0, 1.0) * 255.0) as u8;
        m.color = [f(0), f(1), f(2)];
    }
    for mesh in j["meshes"].as_array().cloned().unwrap_or_default() {
        for p in mesh["primitives"].as_array().cloned().unwrap_or_default() {
            let Some((pos, 3)) = p["attributes"]["POSITION"].as_u64().and_then(|a| read(a as usize)) else { continue };
            let base = m.verts.len() as u32;
            for v in pos.chunks(3) { m.verts.push([v[0] as f32, v[1] as f32, v[2] as f32]); }
            let nv = (pos.len() / 3) as u32;
            let idx: Vec<u32> = match p["indices"].as_u64().and_then(|a| read(a as usize)) { Some((i, _)) => i.into_iter().map(|x| x as u32).collect(), None => (0..nv).collect() };
            for t in idx.chunks(3) { if t.len() == 3 && t.iter().all(|&k| k < nv) { m.tris.push([base + t[0], base + t[1], base + t[2]]); } }
        }
    }
    Ok(m)
}

/// The mesh drawn at w × h pixels turned by yaw and pitch (degrees): RGBA, transparent background.
pub fn render(m: &Mesh, w: u32, h: u32, yaw: f32, pitch: f32) -> Vec<u8> {
    let (w, h) = (w.max(1), h.max(1));
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut zb = vec![f32::INFINITY; (w * h) as usize];
    if m.verts.is_empty() { return px; }
    // Center and scale to fit.
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for v in &m.verts { for k in 0..3 { lo[k] = lo[k].min(v[k]); hi[k] = hi[k].max(v[k]); } }
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let rad = m.verts.iter().map(|v| ((v[0] - c[0]).powi(2) + (v[1] - c[1]).powi(2) + (v[2] - c[2]).powi(2)).sqrt()).fold(0.0f32, f32::max).max(1e-6);
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    let scale = w.min(h) as f32 * 0.48 / rad;
    let pts: Vec<[f32; 3]> = m.verts.iter().map(|v| {
        let (x, y, z) = (v[0] - c[0], v[1] - c[1], v[2] - c[2]);
        let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
        let (y, z) = (y * cp - z * sp, y * sp + z * cp);
        [w as f32 / 2.0 + x * scale, h as f32 / 2.0 - y * scale, -z]
    }).collect();
    let light = { let l = [-0.4f32, 0.6, 0.7]; let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt(); [l[0] / n, l[1] / n, l[2] / n] };
    for t in &m.tris {
        let (a, b, d) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
        // Normal in view space (screen y points down, so flip it back).
        let u = [b[0] - a[0], -(b[1] - a[1]), b[2] - a[2]];
        let v = [d[0] - a[0], -(d[1] - a[1]), d[2] - a[2]];
        let mut n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len < 1e-9 { continue; }
        n = [n[0] / len, n[1] / len, n[2] / len];
        // Two-sided lighting (models don't agree on winding); depth is -z, nearer is smaller.
        let lit = (n[0] * light[0] + n[1] * light[1] - n[2] * light[2]).abs();
        let k = 0.35 + 0.65 * lit;
        let col = m.color.map(|x| (x as f32 * k).min(255.0) as u8);
        let minx = a[0].min(b[0]).min(d[0]).floor().max(0.0) as i32;
        let maxx = a[0].max(b[0]).max(d[0]).ceil().min(w as f32 - 1.0) as i32;
        let miny = a[1].min(b[1]).min(d[1]).floor().max(0.0) as i32;
        let maxy = a[1].max(b[1]).max(d[1]).ceil().min(h as f32 - 1.0) as i32;
        let area = (b[0] - a[0]) * (d[1] - a[1]) - (b[1] - a[1]) * (d[0] - a[0]);
        if area.abs() < 1e-9 { continue; }
        for y in miny..=maxy { for x in minx..=maxx {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((b[0] - fx) * (d[1] - fy) - (b[1] - fy) * (d[0] - fx)) / area;
            let w1 = ((d[0] - fx) * (a[1] - fy) - (d[1] - fy) * (a[0] - fx)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 { continue; }
            let z = w0 * a[2] + w1 * b[2] + w2 * d[2];
            let i = (y as u32 * w + x as u32) as usize;
            if z < zb[i] { zb[i] = z; px[i * 4..i * 4 + 4].copy_from_slice(&[col[0], col[1], col[2], 255]); }
        } }
    }
    px
}

/// The rendered mesh as a PNG.
pub fn render_png(m: &Mesh, w: u32, h: u32, yaw: f32, pitch: f32) -> Option<Vec<u8>> {
    let px = render(m, w, h, yaw, pitch);
    let img = image::RgbaImage::from_raw(w.max(1), h.max(1), px)?;
    let mut out = std::io::Cursor::new(vec![]);
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// The view a 3D model picture was rendered with, from its description ("jadwal-3d:yaw,pitch").
pub fn view_of(descr: &str) -> Option<(f32, f32)> {
    let r = descr.strip_prefix(TAG)?;
    let (a, b) = r.split_once(',')?;
    Some((a.trim().parse().ok()?, b.split_whitespace().next()?.parse().ok()?))
}
pub const TAG: &str = "jadwal-3d:";

/// A unit cube (the sample model).
pub fn cube() -> Mesh {
    let verts = vec![[-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0], [-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0]];
    let q = [[0, 1, 2, 3], [4, 5, 6, 7], [0, 1, 5, 4], [2, 3, 7, 6], [1, 2, 6, 5], [0, 3, 7, 4]];
    Mesh { verts, tris: q.iter().flat_map(|f: &[u32; 4]| [[f[0], f[1], f[2]], [f[0], f[2], f[3]]]).collect(), color: [0x9E, 0xB4, 0xD8] }
}

/// The cube as .obj text.
pub fn cube_obj() -> String {
    let m = cube();
    let mut s = String::new();
    for v in &m.verts { s.push_str(&format!("v {} {} {}\n", v[0], v[1], v[2])); }
    for t in &m.tris { s.push_str(&format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1)); }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn obj_loads_and_renders() {
        let m = load(cube_obj().as_bytes(), "obj").unwrap();
        assert_eq!(m.tris.len(), 12);
        let px = render(&m, 40, 40, 30.0, 20.0);
        assert!(px[(20 * 40 + 20) * 4 + 3] == 255, "center is drawn");
        assert_eq!(px[3], 0, "corner stays clear");
        assert_eq!(view_of("jadwal-3d:30,-15 model.obj"), Some((30.0, -15.0)));
    }
    #[test]
    fn glb_loads() {
        // A one-triangle glTF binary.
        let mut bin = vec![];
        for f in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] { bin.extend_from_slice(&f.to_le_bytes()); }
        let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}]}"#;
        let mut j = json.as_bytes().to_vec(); while j.len() % 4 != 0 { j.push(b' '); }
        let mut b = b"glTF".to_vec();
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(&((12 + 8 + j.len() + 8 + bin.len()) as u32).to_le_bytes());
        b.extend_from_slice(&(j.len() as u32).to_le_bytes()); b.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); b.extend_from_slice(&j);
        b.extend_from_slice(&(bin.len() as u32).to_le_bytes()); b.extend_from_slice(&0x004E4942u32.to_le_bytes()); b.extend_from_slice(&bin);
        let m = load(&b, "glb").unwrap();
        assert_eq!(m.tris.len(), 1);
    }
}
