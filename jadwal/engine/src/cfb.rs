//! A minimal compound file (MS-CFB, version 3, 512-byte sectors) writer and reader: enough for
//! OLE objects (an embedded package's `\x01Ole10Native`, `\x01CompObj` and `\x03ObjInfo`
//! streams in the root storage). Streams under 4096 bytes go to the mini stream, as the format
//! requires.

const FREE: u32 = 0xFFFF_FFFF;
const END: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const DIFSECT: u32 = 0xFFFF_FFFC;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const SECTOR: usize = 512;
const MINI: usize = 64;
const CUTOFF: usize = 4096;

fn cmp_names(a: &str, b: &str) -> std::cmp::Ordering {
    let (la, lb) = (a.encode_utf16().count(), b.encode_utf16().count());
    la.cmp(&lb).then_with(|| a.to_uppercase().cmp(&b.to_uppercase()))
}

struct Dir { name: String, kind: u8, left: u32, right: u32, child: u32, clsid: [u8; 16], start: u32, size: u64 }

fn dir_bytes(d: &Dir) -> [u8; 128] {
    let mut b = [0u8; 128];
    let mut n = 0;
    for (i, u) in d.name.encode_utf16().take(31).enumerate() { b[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes()); n = i + 1; }
    b[64..66].copy_from_slice(&(if d.kind == 0 { 0 } else { (n as u16 + 1) * 2 }).to_le_bytes());
    b[66] = d.kind;
    b[67] = 1; // black
    b[68..72].copy_from_slice(&d.left.to_le_bytes());
    b[72..76].copy_from_slice(&d.right.to_le_bytes());
    b[76..80].copy_from_slice(&d.child.to_le_bytes());
    b[80..96].copy_from_slice(&d.clsid);
    b[116..120].copy_from_slice(&d.start.to_le_bytes());
    b[120..128].copy_from_slice(&d.size.to_le_bytes());
    b
}

/// Builds a balanced tree over sorted entry indexes; returns the subtree root.
fn tree(dirs: &mut [Dir], idx: &[u32]) -> u32 {
    if idx.is_empty() { return NOSTREAM; }
    let m = idx.len() / 2;
    let l = tree(dirs, &idx[..m]);
    let r = tree(dirs, &idx[m + 1..]);
    dirs[idx[m] as usize].left = l;
    dirs[idx[m] as usize].right = r;
    idx[m]
}

/// A compound file whose root storage (class `clsid`) holds the given streams.
pub fn write(clsid: [u8; 16], streams: &[(&str, &[u8])]) -> Vec<u8> {
    let mut dirs = vec![Dir { name: "Root Entry".into(), kind: 5, left: NOSTREAM, right: NOSTREAM, child: NOSTREAM, clsid, start: END, size: 0 }];
    for (n, d) in streams { dirs.push(Dir { name: n.to_string(), kind: 2, left: NOSTREAM, right: NOSTREAM, child: NOSTREAM, clsid: [0; 16], start: END, size: d.len() as u64 }); }
    let mut order: Vec<u32> = (1..dirs.len() as u32).collect();
    order.sort_by(|a, b| cmp_names(&dirs[*a as usize].name, &dirs[*b as usize].name));
    dirs[0].child = tree(&mut dirs, &order);
    // The mini stream and its allocation table.
    let mut mini = vec![];
    let mut minifat: Vec<u32> = vec![];
    for (k, (_, d)) in streams.iter().enumerate() {
        if d.len() >= CUTOFF || d.is_empty() { continue; }
        let first = minifat.len() as u32;
        let n = d.len().div_ceil(MINI);
        for i in 0..n { minifat.push(if i + 1 == n { END } else { first + i as u32 + 1 }); }
        mini.extend_from_slice(d);
        mini.resize(minifat.len() * MINI, 0);
        dirs[k + 1].start = first;
    }
    // Regular sectors: directory, mini FAT, mini stream, large streams, then the FAT.
    let mut sectors: Vec<Vec<u8>> = vec![];
    let mut fat: Vec<u32> = vec![];
    let chain = |data: &[u8], sectors: &mut Vec<Vec<u8>>, fat: &mut Vec<u32>| -> u32 {
        if data.is_empty() { return END; }
        let first = sectors.len() as u32;
        let n = data.len().div_ceil(SECTOR);
        for i in 0..n {
            let mut s = data[i * SECTOR..((i + 1) * SECTOR).min(data.len())].to_vec();
            s.resize(SECTOR, 0);
            sectors.push(s);
            fat.push(if i + 1 == n { END } else { first + i as u32 + 1 });
        }
        first
    };
    let ndir = dirs.len().div_ceil(4);
    let dir_start = 0u32;
    for i in 0..ndir { sectors.push(vec![0; SECTOR]); fat.push(if i + 1 == ndir { END } else { i as u32 + 1 }); }
    let mf: Vec<u8> = minifat.iter().flat_map(|v| v.to_le_bytes()).collect();
    let minifat_start = chain(&mf, &mut sectors, &mut fat);
    dirs[0].start = chain(&mini, &mut sectors, &mut fat);
    dirs[0].size = mini.len() as u64;
    for (k, (_, d)) in streams.iter().enumerate() { if d.len() >= CUTOFF { dirs[k + 1].start = chain(d, &mut sectors, &mut fat); } }
    // FAT sectors (each covers 128 sectors, itself included).
    // Past 109 FAT sectors (about 7 MB) the header's list is full and DIFAT sectors, 127 FAT
    // sector numbers each plus a link to the next, hold the rest.
    let difat_for = |nfat: usize| nfat.saturating_sub(109).div_ceil(127);
    let mut nfat = 1;
    while (sectors.len() + nfat + difat_for(nfat)).div_ceil(128) > nfat { nfat += 1; }
    let ndifat = difat_for(nfat);
    let fat_start = sectors.len() as u32;
    for _ in 0..nfat { fat.push(FATSECT); }
    let difat_start = fat_start + nfat as u32;
    for _ in 0..ndifat { fat.push(DIFSECT); }
    fat.resize(nfat * 128, FREE);
    // Directory sectors.
    let mut db: Vec<u8> = dirs.iter().flat_map(|d| dir_bytes(d)).collect();
    while db.len() < ndir * SECTOR { db.extend_from_slice(&dir_bytes(&Dir { name: String::new(), kind: 0, left: NOSTREAM, right: NOSTREAM, child: NOSTREAM, clsid: [0; 16], start: 0, size: 0 })); }
    for i in 0..ndir { sectors[dir_start as usize + i] = db[i * SECTOR..(i + 1) * SECTOR].to_vec(); }
    // Header.
    let mut h = vec![0u8; SECTOR];
    h[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    h[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    h[26..28].copy_from_slice(&3u16.to_le_bytes());
    h[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    h[30..32].copy_from_slice(&9u16.to_le_bytes());
    h[32..34].copy_from_slice(&6u16.to_le_bytes());
    h[44..48].copy_from_slice(&(nfat as u32).to_le_bytes());
    h[48..52].copy_from_slice(&dir_start.to_le_bytes());
    h[56..60].copy_from_slice(&(CUTOFF as u32).to_le_bytes());
    h[60..64].copy_from_slice(&(if minifat.is_empty() { END } else { minifat_start }).to_le_bytes());
    h[64..68].copy_from_slice(&(mf.len().div_ceil(SECTOR) as u32).to_le_bytes());
    h[68..72].copy_from_slice(&(if ndifat > 0 { difat_start } else { END }).to_le_bytes());
    h[72..76].copy_from_slice(&(ndifat as u32).to_le_bytes());
    for i in 0..109 { let v = if i < nfat { fat_start + i as u32 } else { FREE }; h[76 + i * 4..80 + i * 4].copy_from_slice(&v.to_le_bytes()); }
    let mut out = h;
    for s in sectors { out.extend_from_slice(&s); }
    for c in fat.chunks(128) { for v in c { out.extend_from_slice(&v.to_le_bytes()); } }
    for d in 0..ndifat {
        for i in 0..127 { let k = 109 + d * 127 + i; let v = if k < nfat { fat_start + k as u32 } else { FREE }; out.extend_from_slice(&v.to_le_bytes()); }
        out.extend_from_slice(&(if d + 1 < ndifat { difat_start + d as u32 + 1 } else { END }).to_le_bytes());
    }
    out
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> { Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?)) }

/// The class id of the root storage and its streams (name, bytes), from a compound file.
pub fn read(data: &[u8]) -> Option<([u8; 16], Vec<(String, Vec<u8>)>)> {
    if data.get(..8)? != [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1] { return None; }
    let ss = 1usize << u16::from_le_bytes(data.get(30..32)?.try_into().ok()?);
    let ms = 1usize << u16::from_le_bytes(data.get(32..34)?.try_into().ok()?);
    if !(128..=65536).contains(&ss) { return None; }
    let sector = |n: u32| -> Option<&[u8]> { let o = ss + n as usize * ss; data.get(o..o + ss) };
    // DIFAT → FAT.
    let nfat = u32_at(data, 44)? as usize;
    let mut fat_secs: Vec<u32> = (0..109).filter_map(|i| u32_at(data, 76 + i * 4)).filter(|v| *v < FATSECT).collect();
    let mut di = u32_at(data, 68)?;
    let mut guard = 0;
    while di < FATSECT && guard < 10000 {
        let s = sector(di)?;
        for i in 0..ss / 4 - 1 { let v = u32_at(s, i * 4)?; if v < FATSECT { fat_secs.push(v); } }
        di = u32_at(s, ss - 4)?;
        guard += 1;
    }
    fat_secs.truncate(nfat.max(1));
    let fat: Vec<u32> = fat_secs.iter().filter_map(|n| sector(*n)).flat_map(|s| s.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap()))).collect();
    let follow = |start: u32, table: &[u32]| -> Vec<u32> {
        let mut v = vec![];
        let mut c = start;
        while (c as usize) < table.len() && v.len() <= table.len() { v.push(c); c = table[c as usize]; }
        v
    };
    let read_chain = |start: u32| -> Vec<u8> { follow(start, &fat).into_iter().filter_map(sector).flatten().copied().collect() };
    let dirb = read_chain(u32_at(data, 48)?);
    let entry = |i: u32| dirb.get(i as usize * 128..i as usize * 128 + 128);
    let root = entry(0)?;
    let mini = read_chain(u32_at(root, 116)?);
    let mfb = { let s = u32_at(data, 60)?; if s < FATSECT { read_chain(s) } else { vec![] } };
    let minifat: Vec<u32> = mfb.chunks(4).filter(|c| c.len() == 4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    let cutoff = u32_at(data, 56)? as u64;
    let mut out = vec![];
    // The root's children: walk the sibling tree.
    let mut stack = vec![u32_at(root, 76)?];
    let mut seen = 0;
    while let Some(i) = stack.pop() {
        if i == NOSTREAM || seen > 100000 { continue; }
        seen += 1;
        let Some(e) = entry(i) else { continue };
        stack.push(u32_at(e, 68)?);
        stack.push(u32_at(e, 72)?);
        if e[66] != 2 { continue; }
        let nl = (u16::from_le_bytes([e[64], e[65]]) as usize).min(64);
        let name = String::from_utf16_lossy(&e[..nl.saturating_sub(2)].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>());
        let size = u64::from_le_bytes(e[120..128].try_into().ok()?) & 0xFFFF_FFFF;
        let start = u32_at(e, 116)?;
        let mut bytes: Vec<u8> = if size < cutoff {
            follow(start, &minifat).into_iter().filter_map(|n| mini.get(n as usize * ms..n as usize * ms + ms)).flatten().copied().collect()
        } else { read_chain(start) };
        bytes.truncate(size as usize);
        out.push((name, bytes));
    }
    Some((root[80..96].try_into().ok()?, out))
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        let big: Vec<u8> = (0..10000u32).map(|i| (i * 7) as u8).collect();
        let small = b"hello".to_vec();
        let f = super::write([1; 16], &[("\u{1}Ole10Native", &big), ("\u{1}CompObj", &small), ("\u{3}ObjInfo", &[0, 0, 3, 0, 13, 0])]);
        assert_eq!(f.len() % 512, 0);
        let (clsid, s) = super::read(&f).unwrap();
        assert_eq!(clsid, [1; 16]);
        let get = |n: &str| s.iter().find(|x| x.0 == n).unwrap().1.clone();
        assert_eq!(get("\u{1}Ole10Native"), big);
        assert_eq!(get("\u{1}CompObj"), small);
        assert_eq!(get("\u{3}ObjInfo"), vec![0, 0, 3, 0, 13, 0]);
    }

    #[test]
    fn large_streams_use_difat_sectors() {
        let big: Vec<u8> = (0..9_000_000u32).map(|i| (i % 251) as u8).collect();
        let f = super::write([2; 16], &[("Data", &big)]);
        let (clsid, s) = super::read(&f).unwrap();
        assert_eq!(clsid, [2; 16]);
        assert_eq!(s.iter().find(|x| x.0 == "Data").unwrap().1, big);
    }
}
