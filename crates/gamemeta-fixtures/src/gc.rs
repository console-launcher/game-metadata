// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Synthetic GameCube data: a minimal disc (header, FST, files) and its container wrappers (CISO, GCZ, RVZ). Mirrors
//! the original reader's `GcFixture`/`RvzFixture`. No commercial data.

/// One language's banner text.
#[derive(Debug, Clone)]
pub struct Text {
    /// Short title (32 bytes).
    pub short_title: String,
    /// Short maker (32 bytes).
    pub short_maker: String,
    /// Long title (64 bytes).
    pub long_title: String,
    /// Long maker (64 bytes).
    pub long_maker: String,
    /// Description (128 bytes).
    pub description: String,
}

/// A text the way the original fixtures make one from a name.
pub fn text(name: &str) -> Text {
    Text {
        short_title: name.chars().take(31).collect(),
        short_maker: "Maker".into(),
        long_title: name.into(),
        long_maker: format!("{name} Maker Inc."),
        description: format!("About {name}"),
    }
}

/// The default banner image: 96x32 RGB5A3 in 4x4 tiles; `pixel(x, y)` gives the 16-bit value.
pub fn image_with(pixel: impl Fn(usize, usize) -> u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(0x1800);
    for ty in 0..8 {
        for tx in 0..24 {
            for py in 0..4 {
                for px in 0..4 {
                    out.extend(pixel(tx * 4 + px, ty * 4 + py).to_be_bytes());
                }
            }
        }
    }
    out
}

/// The default image: a red 4x4 corner on an opaque black field.
pub fn image() -> Vec<u8> {
    image_with(|x, y| if x < 4 && y < 4 { 0xFC00 } else { 0x8000 })
}

/// `s` in a legacy charset (`windows-1252` or `Shift_JIS`), zero padded to `length` with at least one NUL.
pub fn fixed(s: &str, length: usize, charset: &str) -> Vec<u8> {
    let enc = encoding_rs::Encoding::for_label(charset.as_bytes()).expect("known charset");
    let (bytes, _, _) = enc.encode(s);
    let mut out = vec![0u8; length];
    let n = bytes.len().min(length - 1);
    out[..n].copy_from_slice(&bytes[..n]);
    out
}

/// `BNR1` with one text, or `BNR2` with six.
pub fn bnr(texts: &[Text], image: &[u8], charset: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(if texts.len() > 1 { b"BNR2" } else { b"BNR1" });
    out.extend([0u8; 0x1C]);
    out.extend(image);
    for t in texts {
        out.extend(fixed(&t.short_title, 0x20, charset));
        out.extend(fixed(&t.short_maker, 0x20, charset));
        out.extend(fixed(&t.long_title, 0x40, charset));
        out.extend(fixed(&t.long_maker, 0x40, charset));
        out.extend(fixed(&t.description, 0x80, charset));
    }
    out
}

/// A one-text `BNR1` with the default image and windows-1252.
pub fn bnr1(name: &str) -> Vec<u8> {
    bnr(&[text(name)], &image(), "windows-1252")
}

fn put_be32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

/// A GameCube disc: the header (id `game_id`, GameCube magic, `title`), an FST with `files` in the root (and one
/// subfolder `sound` holding `decoy_bnr` as its own `opening.bnr`, which must never be used) and the file data.
pub fn disc(game_id: &str, title: &str, files: &[(&str, &[u8])], decoy_bnr: Option<&[u8]>) -> Vec<u8> {
    let fst_offset = 0x2440usize;
    let mut names: Vec<u8> = Vec::new();
    let mut name_at = |n: &str| {
        let at = names.len() as u32;
        names.extend(n.as_bytes());
        names.push(0);
        at
    };
    let align = |v: usize| (v + 0x7FFF) & !0x7FFF;
    let mut cursor = 0x8000usize;
    let mut offsets = Vec::new();
    for (_, data) in files {
        offsets.push(cursor);
        cursor += align(data.len());
    }
    let decoy_offset = cursor;
    if let Some(d) = decoy_bnr {
        cursor += align(d.len());
    }
    let total = 1 + files.len() + if decoy_bnr.is_some() { 2 } else { 0 };
    let mut rows: Vec<[u32; 3]> = vec![[1 << 24, 0, total as u32]];
    // The decoy folder first, so a search that does not skip folders meets it before the real file.
    if let Some(d) = decoy_bnr {
        rows.push([(1 << 24) | name_at("sound"), 0, 3]);
        rows.push([name_at("opening.bnr"), decoy_offset as u32, d.len() as u32]);
    }
    for (i, (name, data)) in files.iter().enumerate() {
        rows.push([name_at(name), offsets[i] as u32, data.len() as u32]);
    }
    let mut fst: Vec<u8> = rows.iter().flat_map(|r| r.iter().flat_map(|v| v.to_be_bytes())).collect();
    fst.extend(&names);
    let mut image = vec![0u8; cursor.max(fst_offset + fst.len())];
    image[..game_id.len()].copy_from_slice(game_id.as_bytes());
    image[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
    image[0x20..0x60].copy_from_slice(&fixed(title, 0x40, "windows-1252"));
    put_be32(&mut image, 0x424, fst_offset as u32);
    put_be32(&mut image, 0x428, fst.len() as u32);
    put_be32(&mut image, 0x42C, fst.len() as u32);
    image[fst_offset..fst_offset + fst.len()].copy_from_slice(&fst);
    for (i, (_, data)) in files.iter().enumerate() {
        image[offsets[i]..offsets[i] + data.len()].copy_from_slice(data);
    }
    if let Some(d) = decoy_bnr {
        image[decoy_offset..decoy_offset + d.len()].copy_from_slice(d);
    }
    image
}

/// The usual test disc: `boot.bin` (40 zero bytes) and `bnr` as `opening.bnr` when given.
pub fn standard_disc(bnr: Option<&[u8]>, id: &str, decoy: Option<&[u8]>) -> Vec<u8> {
    let boot = [0u8; 40];
    let mut files: Vec<(&str, &[u8])> = vec![("boot.bin", &boot)];
    if let Some(b) = bnr {
        files.push(("opening.bnr", b));
    }
    disc(id, "Header Title", &files, decoy)
}

fn chunk_of(disc: &[u8], i: usize, block: usize) -> Vec<u8> {
    let mut chunk = disc[i * block..disc.len().min((i + 1) * block)].to_vec();
    chunk.resize(block, 0);
    chunk
}

/// `disc` as a CISO image: `block_size` blocks, the all-zero ones left out.
pub fn ciso(disc: &[u8], block_size: usize) -> Vec<u8> {
    let blocks = disc.len().div_ceil(block_size);
    let mut head = vec![0u8; 0x8000];
    head[..4].copy_from_slice(b"CISO");
    head[4..8].copy_from_slice(&(block_size as u32).to_le_bytes());
    let mut body = Vec::new();
    for i in 0..blocks {
        let chunk = chunk_of(disc, i, block_size);
        if chunk.iter().any(|&b| b != 0) {
            head[8 + i] = 1;
            body.extend(chunk);
        }
    }
    head.extend(body);
    head
}

/// `disc` as a GCZ image: `block_size` blocks, zlib-deflated unless that does not shrink them.
pub fn gcz(disc: &[u8], block_size: usize) -> Vec<u8> {
    let blocks = disc.len().div_ceil(block_size);
    let mut data = Vec::new();
    let mut pointers = Vec::new();
    for i in 0..blocks {
        let chunk = chunk_of(disc, i, block_size);
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&chunk, 6);
        let mut pointer = data.len() as u64;
        if packed.len() < block_size {
            data.extend(packed);
        } else {
            pointer |= 1 << 63;
            data.extend(chunk);
        }
        pointers.push(pointer);
    }
    let mut head = Vec::new();
    head.extend(0xB10B_C001u32.to_le_bytes());
    head.extend(0u32.to_le_bytes());
    head.extend((data.len() as u64).to_le_bytes());
    head.extend((disc.len() as u64).to_le_bytes());
    head.extend((block_size as u32).to_le_bytes());
    head.extend((blocks as u32).to_le_bytes());
    for p in &pointers {
        head.extend(p.to_le_bytes());
    }
    head.extend(vec![0u8; blocks * 4]); // the hash table (12 bytes per block in all)
    head.extend(data);
    head
}

/// Zstd as the RVZ groups use it.
pub fn zstd(bytes: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(bytes, ruzstd::encoding::CompressionLevel::Fastest)
}

/// `bytes` compressed the way a WIA or RVZ names it: 0 stored, 2 bzip2, 3 LZMA (raw, no end marker), 4 LZMA2, 5 zstd.
pub fn compress(method: u32, bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    match method {
        0 => bytes.to_vec(),
        2 => {
            let mut w = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
            w.write_all(bytes).expect("bzip2 into memory");
            w.finish().expect("bzip2 into memory")
        }
        3 => {
            let mut w =
                lzma_rust2::LzmaWriter::new_no_header(Vec::new(), &lzma_rust2::LzmaOptions::with_preset(1), false)
                    .expect("lzma options");
            w.write_all(bytes).expect("lzma into memory");
            w.finish().expect("lzma into memory")
        }
        4 => {
            let mut w = lzma_rust2::Lzma2Writer::new(Vec::new(), lzma_rust2::Lzma2Options::with_preset(1));
            w.write_all(bytes).expect("lzma2 into memory");
            w.finish().expect("lzma2 into memory")
        }
        5 => zstd(bytes),
        _ => panic!("no such WIA compression {method}"),
    }
}

/// The `compr_data` bytes header 2 keeps for `method`: LZMA's properties byte and dictionary size, LZMA2's dictionary byte.
pub fn compr_data(method: u32) -> Vec<u8> {
    let preset = lzma_rust2::LzmaOptions::with_preset(1);
    match method {
        3 => {
            let props = (preset.pb * 5 + preset.lp) * 9 + preset.lc;
            let mut v = vec![props as u8];
            v.extend(preset.dict_size.to_le_bytes());
            v
        }
        4 => {
            // The smallest dictionary byte whose size covers the preset's.
            let size = |b: u32| (2u64 | u64::from(b & 1)) << (b / 2 + 11);
            vec![(0..40).find(|&b| size(b) >= u64::from(preset.dict_size)).unwrap_or(40) as u8]
        }
        _ => Vec::new(),
    }
}

/// Group table entries are 12 bytes in an RVZ (offset, size with the compressed flag, packed size) and 8 in a WIA (offset,
/// size).
pub fn group_entry(magic: &[u8; 4], offset: u32, size: u32, flag: u32, packed: u32) -> Vec<u8> {
    let mut e = offset.to_be_bytes().to_vec();
    if magic == b"WIA\x01" {
        e.extend(size.to_be_bytes());
    } else {
        e.extend((size | flag).to_be_bytes());
        e.extend(packed.to_be_bytes());
    }
    e
}

/// Writes the compression method and its `compr_data` into header 2 at `h2` (the offset of header 2 in `out`).
pub fn put_compression(out: &mut [u8], h2: usize, method: u32) {
    out[h2 + 4..h2 + 8].copy_from_slice(&method.to_be_bytes());
    let data = compr_data(method);
    out[h2 + 0xD4] = data.len() as u8;
    out[h2 + 0xD5..h2 + 0xD5 + data.len()].copy_from_slice(&data);
}

/// How an RVZ is laid out for a test.
#[derive(Debug, Clone, Copy)]
pub struct RvzOptions {
    /// 0 (stored), 2 (bzip2), 3 (LZMA), 4 (LZMA2) or 5 (zstd).
    pub compression: u32,
    /// Group size in bytes.
    pub chunk: usize,
    /// Use RVZ packing with the second half of each group as a junk run.
    pub packed: bool,
    /// 1 GameCube, 2 Wii.
    pub disc_type: u32,
}

impl Default for RvzOptions {
    fn default() -> Self {
        RvzOptions { compression: 5, chunk: 0x8000, packed: false, disc_type: 1 }
    }
}

/// `disc` as an RVZ: one raw data entry from 0x80 (the first 0x80 bytes live in header 2), groups of `chunk` bytes
/// counted from 0. A group whose bytes are all zero is stored empty. `magic` is the first four bytes (`RVZ\x01`).
pub fn rvz(disc: &[u8], o: RvzOptions, magic: &[u8; 4]) -> Vec<u8> {
    let compress = |b: &[u8]| compress(o.compression, b);
    let chunk = o.chunk;
    let groups = disc.len().div_ceil(chunk);
    let body_start = 0x48 + 0xDC;
    let mut group_data: Vec<Vec<u8>> = Vec::new();
    let mut flags: Vec<u32> = Vec::new();
    let mut packed_sizes: Vec<u32> = Vec::new();
    for g in 0..groups {
        let bytes = chunk_of(disc, g, chunk);
        if bytes.iter().all(|&b| b == 0) {
            group_data.push(Vec::new());
            flags.push(0);
            packed_sizes.push(0);
            continue;
        }
        let flag = if o.compression == 0 { 0 } else { 1 << 31 };
        if o.packed {
            let half = bytes.len() / 2;
            let mut stream = Vec::new();
            stream.extend((half as u32).to_be_bytes());
            stream.extend(&bytes[..half]);
            stream.extend(((bytes.len() - half) as u32 | (1 << 31)).to_be_bytes());
            stream.extend([9u8; 68]);
            group_data.push(compress(&stream));
            packed_sizes.push(chunk as u32);
        } else {
            group_data.push(compress(&bytes));
            packed_sizes.push(0);
        }
        flags.push(flag);
    }
    let mut raw_entry = Vec::new();
    raw_entry.extend(0x80u64.to_be_bytes());
    raw_entry.extend((disc.len() as u64 - 0x80).to_be_bytes());
    raw_entry.extend(0u32.to_be_bytes());
    raw_entry.extend((groups as u32).to_be_bytes());
    let raw_stored = compress(&raw_entry);
    // Group entries are compressed as one block whose size depends on their offsets: lay the group data after a
    // slot big enough for the worst case.
    let slot = {
        let worst = match o.compression {
            0 => groups * 12,
            5 => groups * 12 + groups * 12 / 8 + 64,
            _ => groups * 24 + 256,
        };
        (worst + 3) & !3
    };
    let groups_start = (body_start + raw_stored.len() + 3) & !3;
    let mut data_at = groups_start + slot;
    let mut entries = Vec::new();
    let mut offsets = Vec::new();
    for g in 0..groups {
        let empty = group_data[g].is_empty();
        let (at, len) = if empty { (0, 0) } else { ((data_at >> 2) as u32, group_data[g].len() as u32) };
        entries.extend(group_entry(magic, at, len, flags[g], packed_sizes[g]));
        offsets.push(data_at);
        data_at += (group_data[g].len() + 3) & !3;
    }
    let groups_stored = compress(&entries);
    assert!(groups_stored.len() <= slot, "the group table must fit its slot");
    let mut out = vec![0u8; data_at + 16];
    out[..4].copy_from_slice(magic);
    out[4..8].copy_from_slice(&0x0100_0000u32.to_be_bytes());
    out[8..12].copy_from_slice(&0x0003_0000u32.to_be_bytes());
    out[12..16].copy_from_slice(&0xDCu32.to_be_bytes());
    out[0x24..0x2C].copy_from_slice(&(disc.len() as u64).to_be_bytes());
    out[0x2C..0x34].copy_from_slice(&(data_at as u64).to_be_bytes());
    let h2 = |out: &mut Vec<u8>, at: usize, v: &[u8]| out[0x48 + at..0x48 + at + v.len()].copy_from_slice(v);
    h2(&mut out, 0, &o.disc_type.to_be_bytes());
    h2(&mut out, 8, &5u32.to_be_bytes());
    h2(&mut out, 0xC, &(chunk as u32).to_be_bytes());
    h2(&mut out, 0x10, &disc[..0x80]);
    h2(&mut out, 0xB4, &1u32.to_be_bytes());
    h2(&mut out, 0xB8, &(body_start as u64).to_be_bytes());
    h2(&mut out, 0xC0, &(raw_stored.len() as u32).to_be_bytes());
    h2(&mut out, 0xC4, &(groups as u32).to_be_bytes());
    h2(&mut out, 0xC8, &(groups_start as u64).to_be_bytes());
    h2(&mut out, 0xD0, &(groups_stored.len() as u32).to_be_bytes());
    out[body_start..body_start + raw_stored.len()].copy_from_slice(&raw_stored);
    put_compression(&mut out, 0x48, o.compression);
    out[groups_start..groups_start + groups_stored.len()].copy_from_slice(&groups_stored);
    for g in 0..groups {
        out[offsets[g]..offsets[g] + group_data[g].len()].copy_from_slice(&group_data[g]);
    }
    out
}

/// The committed GameCube sample: a six-language BNR2 disc as a zstd RVZ.
pub fn sample_rvz() -> Vec<u8> {
    let texts: Vec<Text> = [
        "Sample Quest",
        "Beispiel Quest",
        "Quete Exemple",
        "Aventura Muestra",
        "Avventura Esempio",
        "Voorbeeld Queeste",
    ]
    .into_iter()
    .map(text)
    .collect();
    let bnr = bnr(&texts, &image_with(|x, y| if (x / 8 + y / 8) % 2 == 0 { 0xFC00 } else { 0x83E0 }), "windows-1252");
    let boot = [0u8; 40];
    let disc = disc("GSMP01", "Sample Disc", &[("boot.bin", &boot), ("opening.bnr", &bnr)], None);
    rvz(&disc, RvzOptions::default(), b"RVZ\x01")
}
