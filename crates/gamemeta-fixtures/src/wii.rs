// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Synthetic Wii data: TPL, U8, LZ77, IMET `opening.bnr`, a partition's file system, discs, WBFS, WADs and an RVZ of a
//! Wii disc. Mirrors the original reader's `WiiFixture`/`WiiRvzFixture`. No commercial data.

use crate::gc::{compress as compress_with, group_entry, put_compression};

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn put(b: &mut [u8], at: usize, bytes: &[u8]) {
    b[at..at + bytes.len()].copy_from_slice(bytes);
}

/// Type 0x10 (or `type_`) with literals only: a flag byte of 0 before every 8 bytes.
pub fn lz77_literals(data: &[u8], type_: u8) -> Vec<u8> {
    let mut out = vec![type_, data.len() as u8, (data.len() >> 8) as u8, (data.len() >> 16) as u8];
    for chunk in data.chunks(8) {
        out.push(0);
        out.extend(chunk);
    }
    out
}

/// One image TPL. `data` is the encoded texture, `palette` (RGB565 or RGB5A3 words with `palette_format`) is for the C formats.
pub fn tpl(width: u16, height: u16, format: u32, data: &[u8], palette: Option<&[u8]>, palette_format: u32) -> Vec<u8> {
    let palette_at = 0x44;
    let data_at = palette_at + palette.map_or(0, <[u8]>::len);
    let mut out = vec![0u8; data_at + data.len()];
    put(&mut out, 0, &be32(0x0020_AF30));
    put(&mut out, 4, &be32(1));
    put(&mut out, 8, &be32(0x0C));
    put(&mut out, 0x0C, &be32(0x14));
    put(&mut out, 0x10, &be32(if palette.is_some() { 0x38 } else { 0 }));
    put(&mut out, 0x14, &height.to_be_bytes());
    put(&mut out, 0x16, &width.to_be_bytes());
    put(&mut out, 0x18, &be32(format));
    put(&mut out, 0x1C, &be32(data_at as u32));
    if let Some(p) = palette {
        put(&mut out, 0x38, &((p.len() / 2) as u16).to_be_bytes());
        put(&mut out, 0x3C, &be32(palette_format));
        put(&mut out, 0x40, &be32(palette_at as u32));
        put(&mut out, palette_at, p);
    }
    put(&mut out, data_at, data);
    out
}

/// A one-image TPL with no palette.
pub fn tpl_plain(width: u16, height: u16, format: u32, data: &[u8]) -> Vec<u8> {
    tpl(width, height, format, data, None, 2)
}

fn tiles(
    width: usize,
    height: usize,
    tile_w: usize,
    tile_h: usize,
    mut put: impl FnMut(usize, usize, &mut Vec<u8>),
) -> Vec<u8> {
    let mut out = Vec::new();
    for ty in 0..height / tile_h {
        for tx in 0..width / tile_w {
            for py in 0..tile_h {
                for px in 0..tile_w {
                    put(tx * tile_w + px, ty * tile_h + py, &mut out);
                }
            }
        }
    }
    out
}

/// RGB5A3 (or any 16-bit format) in 4x4 tiles from `pixel(x, y)` -> 16-bit word.
pub fn words16(width: usize, height: usize, pixel: impl Fn(usize, usize) -> u16) -> Vec<u8> {
    tiles(width, height, 4, 4, |x, y, out| out.extend(pixel(x, y).to_be_bytes()))
}

/// One byte per pixel in 8x4 tiles (I8, C8).
pub fn bytes8(width: usize, height: usize, pixel: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    tiles(width, height, 8, 4, |x, y, out| out.push(pixel(x, y)))
}

/// RGBA8 (format 6) in 4x4 tiles: 32 bytes per tile, AR for 16 pixels, then GB; `pixel` gives 0xAARRGGBB.
pub fn rgba8(width: usize, height: usize, pixel: impl Fn(usize, usize) -> u32) -> Vec<u8> {
    let mut out = Vec::new();
    for ty in 0..height / 4 {
        for tx in 0..width / 4 {
            let (mut ar, mut gb) = ([0u8; 32], [0u8; 32]);
            for py in 0..4 {
                for px in 0..4 {
                    let v = pixel(tx * 4 + px, ty * 4 + py);
                    let i = (py * 4 + px) * 2;
                    ar[i] = (v >> 24) as u8;
                    ar[i + 1] = (v >> 16) as u8;
                    gb[i] = (v >> 8) as u8;
                    gb[i + 1] = v as u8;
                }
            }
            out.extend(ar);
            out.extend(gb);
        }
    }
    out
}

/// CMPR for an 8x8 image: four 4x4 blocks in one tile, each two RGB565 colors and 16 two-bit indices (row 0 in the top byte).
pub fn cmpr8x8(blocks: &[(u16, u16, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    for &(c0, c1, idx) in blocks {
        out.extend(c0.to_be_bytes());
        out.extend(c1.to_be_bytes());
        out.extend(idx.to_be_bytes());
    }
    out
}

/// A file in a U8 archive: path (`/` makes folders) and data.
#[derive(Debug)]
pub struct U8File<'a>(pub &'a str, pub &'a [u8]);

struct Node {
    name: String,
    dir: bool,
    data: Vec<u8>,
    children: Vec<usize>,
    parent: usize,
}

/// A U8 archive holding `files` (paths with `/` make folders); data is 32 byte aligned.
pub fn u8_archive(files: &[U8File]) -> Vec<u8> {
    let mut nodes = vec![Node { name: String::new(), dir: true, data: Vec::new(), children: Vec::new(), parent: 0 }];
    for U8File(path, data) in files {
        let parts: Vec<&str> = path.split('/').collect();
        let mut dir = 0;
        for p in &parts[..parts.len() - 1] {
            let found = nodes[dir].children.iter().copied().find(|&c| nodes[c].dir && nodes[c].name == *p);
            dir = found.unwrap_or_else(|| {
                nodes.push(Node { name: (*p).into(), dir: true, data: Vec::new(), children: Vec::new(), parent: dir });
                let id = nodes.len() - 1;
                nodes[dir].children.push(id);
                id
            });
        }
        nodes.push(Node {
            name: parts[parts.len() - 1].into(),
            dir: false,
            data: data.to_vec(),
            children: Vec::new(),
            parent: dir,
        });
        let id = nodes.len() - 1;
        nodes[dir].children.push(id);
    }
    // Depth-first order, with each folder's end index.
    fn walk(nodes: &[Node], n: usize, order: &mut Vec<usize>, ends: &mut Vec<usize>) {
        order.push(n);
        for &c in &nodes[n].children {
            walk(nodes, c, order, ends);
        }
        ends[n] = order.len();
    }
    let mut order = Vec::new();
    let mut ends = vec![0; nodes.len()];
    walk(&nodes, 0, &mut order, &mut ends);
    let mut names = Vec::new();
    let mut name_at = vec![0usize; nodes.len()];
    for &n in &order {
        name_at[n] = names.len();
        names.extend(nodes[n].name.as_bytes());
        names.push(0);
    }
    let table_size = order.len() * 12 + names.len();
    let data_start = (0x20 + table_size + 0x1F) & !0x1F;
    let mut cursor = data_start;
    let mut offsets = vec![0usize; nodes.len()];
    for &n in &order {
        if !nodes[n].dir {
            offsets[n] = cursor;
            cursor = (cursor + nodes[n].data.len() + 0x1F) & !0x1F;
        }
    }
    let mut out = vec![0u8; cursor];
    put(&mut out, 0, &be32(0x55AA_382D));
    put(&mut out, 4, &be32(0x20));
    put(&mut out, 8, &be32(table_size as u32));
    put(&mut out, 12, &be32(data_start as u32));
    let index: Vec<usize> = {
        let mut v = vec![0; nodes.len()];
        for (i, &n) in order.iter().enumerate() {
            v[n] = i;
        }
        v
    };
    for (i, &n) in order.iter().enumerate() {
        let at = 0x20 + i * 12;
        let node = &nodes[n];
        put(&mut out, at, &be32((if node.dir { 0x0100_0000 } else { 0 }) | name_at[n] as u32));
        if node.dir {
            put(&mut out, at + 4, &be32(if n == 0 { 0 } else { index[node.parent] as u32 }));
            put(&mut out, at + 8, &be32(ends[n] as u32));
        } else {
            put(&mut out, at + 4, &be32(offsets[n] as u32));
            put(&mut out, at + 8, &be32(node.data.len() as u32));
            put(&mut out, offsets[n], &node.data);
        }
    }
    put(&mut out, 0x20 + order.len() * 12, &names);
    out
}

/// `IMD5`, 0x18 bytes of nothing, then `payload`.
pub fn imd5(payload: &[u8]) -> Vec<u8> {
    let mut out = b"IMD5".to_vec();
    out.extend(be32(payload.len() as u32));
    out.extend([0u8; 0x18]);
    out.extend(payload);
    out
}

const IMET_LANGUAGES: [&str; 10] = ["ja", "en", "de", "fr", "es", "it", "nl", "zhs", "zht", "ko"];

/// An `opening.bnr`: 0x40 zero bytes, `IMET`, the titles by language code (UTF-16BE, 42 characters each), padded to
/// 0x600, then a U8 archive holding `meta/icon.bin` and `meta/banner.bin` (each as built by [`bin_archive`]).
pub fn opening(
    titles: &[(&str, &str)],
    icon: Option<&[u8]>,
    banner: Option<&[u8]>,
    wrap: impl Fn(Vec<u8>) -> Vec<u8>,
) -> Vec<u8> {
    let mut head = vec![0u8; 0x600];
    put(&mut head, 0x40, b"IMET");
    put(&mut head, 0x44, &be32(0x600));
    put(&mut head, 0x48, &be32(3));
    put(&mut head, 0x4C, &be32(icon.map_or(0, <[u8]>::len) as u32));
    put(&mut head, 0x50, &be32(banner.map_or(0, <[u8]>::len) as u32));
    for (i, code) in IMET_LANGUAGES.iter().enumerate() {
        let text = titles.iter().find(|(c, _)| c == code).map_or("", |(_, t)| t);
        let mut bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        bytes.resize(0x54, 0);
        put(&mut head, 0x40 + 0x1C + i * 0x54, &bytes);
    }
    let mut files = Vec::new();
    if let Some(i) = icon {
        files.push(U8File("meta/icon.bin", i));
    }
    if let Some(b) = banner {
        files.push(U8File("meta/banner.bin", b));
    }
    if !files.is_empty() {
        head.extend(wrap(u8_archive(&files)));
    }
    head
}

/// `opening` with the archive left as it is.
pub fn opening_plain(titles: &[(&str, &str)], icon: Option<&[u8]>, banner: Option<&[u8]>) -> Vec<u8> {
    opening(titles, icon, banner, |a| a)
}

/// An `icon.bin` / `banner.bin`: a U8 archive with one TPL under `arc/timg`, as a channel stores it, optionally LZ77
/// and IMD5 wrapped; `extra` files (full paths) go in too.
pub fn bin_archive(tpl: &[u8], name: &str, lz77: bool, with_imd5: bool, extra: &[U8File]) -> Vec<u8> {
    let path = format!("arc/timg/{name}");
    let mut files: Vec<U8File> = extra.iter().map(|U8File(p, d)| U8File(p, d)).collect();
    files.push(U8File(&path, tpl));
    let mut bytes = u8_archive(&files);
    if lz77 {
        let mut w = b"LZ77".to_vec();
        w.extend(lz77_literals(&bytes, 0x10));
        bytes = w;
    }
    if with_imd5 {
        bytes = imd5(&bytes);
    }
    bytes
}

/// A file in a partition's root folder.
#[derive(Debug)]
pub struct PartFile<'a>(pub &'a str, pub &'a [u8]);

/// The decrypted partition data: boot.bin (id, Wii magic, title), an FST (offsets and the FST location stored shifted
/// right by 2) and `files` in the root; `decoy` is an `opening.bnr` in a subfolder that must never be used.
pub fn partition_data(game_id: &str, title: &str, files: &[PartFile], decoy: Option<&[u8]>, pad: usize) -> Vec<u8> {
    let fst_offset = 0x2440usize;
    let mut names = Vec::new();
    let mut name_at = |n: &str| {
        let at = names.len() as u32;
        names.extend(n.as_bytes());
        names.push(0);
        at
    };
    let mut rows: Vec<[u32; 3]> = vec![[0x0100_0000, 0, 0]];
    let mut data_start = 0x8000usize;
    let mut blocks: Vec<(usize, &[u8])> = Vec::new();
    if let Some(d) = decoy {
        rows.push([0x0100_0000 | name_at("sound"), 0, 3]);
        rows.push([name_at("opening.bnr"), (data_start >> 2) as u32, d.len() as u32]);
        blocks.push((data_start, d));
        data_start += (d.len() + 0x1F) & !0x1F;
    }
    for PartFile(name, data) in files {
        rows.push([name_at(name), (data_start >> 2) as u32, data.len() as u32]);
        blocks.push((data_start, data));
        data_start += (data.len() + 0x1F) & !0x1F;
    }
    rows[0][2] = rows.len() as u32;
    while names.len() % 4 != 0 {
        names.push(0);
    }
    let mut fst: Vec<u8> = rows.iter().flat_map(|r| r.iter().flat_map(|v| v.to_be_bytes())).collect();
    fst.extend(&names);
    let total = data_start.max(fst_offset + fst.len()).div_ceil(pad) * pad;
    let mut out = vec![0u8; total];
    put(&mut out, 0, &game_id.as_bytes()[..6]);
    put(&mut out, 0x18, &be32(0x5D1C_9EA3));
    put(&mut out, 0x20, title.as_bytes());
    put(&mut out, 0x424, &be32((fst_offset >> 2) as u32));
    put(&mut out, 0x428, &be32((fst.len() >> 2) as u32));
    put(&mut out, 0x42C, &be32((fst.len() >> 2) as u32));
    put(&mut out, fst_offset, &fst);
    for (at, data) in blocks {
        put(&mut out, at, data);
    }
    out
}

/// A partition with the usual defaults (`RZDE01`, `Header Title`, 0x7C00 padding).
pub fn partition(files: &[PartFile], decoy: Option<&[u8]>) -> Vec<u8> {
    partition_data("RZDE01", "Header Title", files, decoy, 0x7C00)
}

/// A plain Wii disc header region: `size` bytes with `game_id`, the Wii magic and `title`.
pub fn disc_header(game_id: &str, title: &str, size: usize) -> Vec<u8> {
    let mut b = vec![0u8; size];
    let n = size.min(6);
    put(&mut b, 0, &game_id.as_bytes()[..n]);
    if size >= 0x1C {
        put(&mut b, 0x18, &be32(0x5D1C_9EA3));
    }
    let t = title.as_bytes();
    if size > 0x20 {
        let n = t.len().min(size - 0x20);
        put(&mut b, 0x20, &t[..n]);
    }
    b
}

/// Where the first partition sits and where its data starts.
pub const PART_OFFSET: usize = 0x50000;
/// Offset of the data inside the partition.
pub const DATA_OFFSET: usize = 0x20000;
/// Where the data starts on the disc.
pub const DATA_START: usize = PART_OFFSET + DATA_OFFSET;

/// The plain-disc part of a Wii image up to the first partition's data: header, one data partition in the table, its header.
pub fn disc_prefix(game_id: &str, title: &str) -> Vec<u8> {
    let mut b = vec![0u8; DATA_START];
    put(&mut b, 0, &disc_header(game_id, title, 0x440));
    put(&mut b, 0x40000, &be32(1));
    put(&mut b, 0x40004, &be32(0x40020 >> 2));
    put(&mut b, 0x40020, &be32((PART_OFFSET >> 2) as u32));
    put(&mut b, PART_OFFSET + 0x2B8, &be32((DATA_OFFSET >> 2) as u32));
    b
}

/// WBFS: 'WBFS' head sector (512 byte sectors), then the disc info whose first 0x100 bytes copy the disc header.
pub fn wbfs(game_id: &str, title: &str) -> Vec<u8> {
    let mut b = vec![0u8; 0x2000];
    put(&mut b, 0, b"WBFS");
    put(&mut b, 4, &be32(0x1000));
    b[8] = 9;
    b[9] = 21;
    put(&mut b, 0x200, &disc_header(game_id, title, 0x100));
    b
}

/// A WAD: header (0x20), certificates, ticket (title id at 0x1DC), TMD (title id at 0x18C, version at 0x1DC), each 64
/// byte aligned.
pub fn wad(title_id: u64, version: u16, kind: &str) -> Vec<u8> {
    let (cert, ticket, tmd) = (0xA00usize, 0x2A4usize, 0x208usize);
    let align = |v: usize| (v + 0x3F) & !0x3F;
    let ticket_at = align(0x20) + align(cert);
    let tmd_at = ticket_at + align(ticket);
    let mut out = vec![0u8; tmd_at + align(tmd) + 0x40];
    put(&mut out, 0, &be32(0x20));
    put(&mut out, 4, kind.as_bytes());
    put(&mut out, 8, &be32(cert as u32));
    put(&mut out, 0x10, &be32(ticket as u32));
    put(&mut out, 0x14, &be32(tmd as u32));
    put(&mut out, 0x18, &be32(0x40));
    put(&mut out, ticket_at + 0x1DC, &title_id.to_be_bytes());
    put(&mut out, tmd_at + 0x18C, &title_id.to_be_bytes());
    put(&mut out, tmd_at + 0x1DC, &version.to_be_bytes());
    out
}

/// What the fixture writes in the partition's key slot: tests assert it is never read.
pub const KEY_MARKER: u8 = 0x6B;
/// The key slot's size.
pub const KEY_SIZE: usize = 16;

/// How a Wii RVZ is laid out.
#[derive(Debug, Clone)]
pub struct WiiRvzOptions<'a> {
    /// The disc's game id.
    pub game_id: &'a str,
    /// The disc header's title.
    pub title: &'a str,
    /// 0 stored, 2 bzip2, 3 LZMA, 4 LZMA2, 5 zstd (the WIA magic and any method make a WIA: 8 byte group entries).
    pub compression: u32,
    /// Group size.
    pub chunk: usize,
    /// RVZ-pack groups that end in 0x40 zero bytes.
    pub packed: bool,
    /// Put hash exception lists (two entries in the first list) before partition data.
    pub exceptions: bool,
    /// The first four bytes.
    pub magic: &'a [u8; 4],
}

impl Default for WiiRvzOptions<'_> {
    fn default() -> Self {
        WiiRvzOptions {
            game_id: "RZDE01",
            title: "Header Title",
            compression: 5,
            chunk: 0x8000,
            packed: false,
            exceptions: false,
            magic: b"RVZ\x01",
        }
    }
}

/// A Wii RVZ's bytes and where its partition table entry (key slot first) sits.
#[derive(Debug, Clone)]
pub struct WiiRvz {
    /// The file.
    pub bytes: Vec<u8>,
    /// Offset of the partition entry (its first 16 bytes are the marker key slot).
    pub part_table_at: usize,
}

/// An RVZ of a Wii disc: raw data up to the first partition's data, then that partition's decrypted data in groups.
pub fn rvz(partition: &[u8], o: &WiiRvzOptions) -> WiiRvz {
    let prefix = disc_prefix(o.game_id, o.title);
    let compress = |b: &[u8]| compress_with(o.compression, b);
    let chunk = o.chunk;
    let raw_groups = prefix.len().div_ceil(chunk);
    let sectors_per_group = chunk / 0x8000;
    let group_data = sectors_per_group * 0x7C00;
    let n_sectors = partition.len().div_ceil(0x7C00);
    let part_groups = n_sectors.div_ceil(sectors_per_group);
    let lists = (chunk / 0x20_0000).max(1);
    let mut stored: Vec<Vec<u8>> = Vec::new();
    let mut flags: Vec<u32> = Vec::new();
    let mut packed_sizes: Vec<u32> = Vec::new();
    let mut add = |raw: &[u8], with_exceptions: bool| {
        if raw.iter().all(|&b| b == 0) && (!with_exceptions || !o.exceptions) {
            stored.push(Vec::new());
            flags.push(0);
            packed_sizes.push(0);
            return;
        }
        let mut ex = Vec::new();
        if with_exceptions {
            for l in 0..lists {
                if l == 0 && o.exceptions {
                    ex.extend(2u16.to_be_bytes());
                    for _ in 0..2 {
                        ex.extend(0x10u16.to_be_bytes());
                        ex.extend([7u8; 20]);
                    }
                } else {
                    ex.extend([0u8; 2]);
                }
            }
        }
        let mut body = raw.to_vec();
        let mut packed_size = 0;
        if o.packed && raw.len() >= 0x100 && raw[raw.len() - 0x40..].iter().all(|&b| b == 0) {
            let lit = raw.len() - 0x40;
            let mut out = (lit as u32).to_be_bytes().to_vec();
            out.extend(&raw[..lit]);
            out.extend((0x40u32 | (1 << 31)).to_be_bytes());
            out.extend([9u8; 68]);
            packed_size = out.len() as u32;
            body = out;
        }
        if o.compression == 0 {
            let mut head = ex;
            while head.len() % 4 != 0 {
                head.push(0);
            }
            head.extend(body);
            stored.push(head);
            flags.push(0);
        } else {
            let mut all = ex;
            all.extend(body);
            stored.push(compress_with(o.compression, &all));
            flags.push(1 << 31);
        }
        packed_sizes.push(packed_size);
    };
    for g in 0..raw_groups {
        let mut slice = prefix[(g * chunk).min(prefix.len())..((g + 1) * chunk).min(prefix.len())].to_vec();
        slice.resize(chunk, 0);
        add(&slice, false);
    }
    for g in 0..part_groups {
        let from = g * group_data;
        add(&partition[from..partition.len().min(from + group_data)], true);
    }
    let groups = raw_groups + part_groups;
    let mut raw_entries = Vec::new();
    raw_entries.extend(0x80u64.to_be_bytes());
    raw_entries.extend((prefix.len() as u64 - 0x80).to_be_bytes());
    raw_entries.extend(0u32.to_be_bytes());
    raw_entries.extend((raw_groups as u32).to_be_bytes());
    let raw_stored = compress(&raw_entries);
    // wia_part_t: a 16 byte key slot (a marker, never read), then two wia_part_data_t of first sector, n_sectors,
    // group_index, n_groups.
    let mut part = vec![KEY_MARKER; KEY_SIZE];
    part.extend(((DATA_START / 0x8000) as u32).to_be_bytes());
    part.extend((n_sectors as u32).to_be_bytes());
    part.extend((raw_groups as u32).to_be_bytes());
    part.extend((part_groups as u32).to_be_bytes());
    part.resize(48, 0);
    let body_start = 0x48 + 0xDC;
    let raw_at = body_start;
    let part_at = (raw_at + raw_stored.len() + 3) & !3;
    let groups_at = (part_at + 48 + 3) & !3;
    let slot = {
        let worst = match o.compression {
            0 => groups * 12,
            5 => groups * 12 + groups * 12 / 8 + 64,
            _ => groups * 24 + 256,
        };
        (worst + 3) & !3
    };
    let mut data_at = groups_at + slot;
    let mut entries = Vec::new();
    let mut offsets = Vec::new();
    for g in 0..groups {
        let empty = stored[g].is_empty();
        let (at, len) = if empty { (0, 0) } else { ((data_at >> 2) as u32, stored[g].len() as u32) };
        entries.extend(group_entry(o.magic, at, len, flags[g], packed_sizes[g]));
        offsets.push(data_at);
        data_at += (stored[g].len() + 3) & !3;
    }
    let groups_stored = compress(&entries);
    assert!(groups_stored.len() <= slot, "the group table must fit its slot");
    let mut out = vec![0u8; data_at + 16];
    put(&mut out, 0, o.magic);
    put(&mut out, 4, &be32(0x0100_0000));
    put(&mut out, 8, &be32(0x0003_0000));
    put(&mut out, 12, &be32(0xDC));
    put(&mut out, 0x24, &0x100_0000u64.to_be_bytes());
    put(&mut out, 0x2C, &(data_at as u64).to_be_bytes());
    put(&mut out, 0x48, &be32(2));
    put(&mut out, 0x50, &be32(5));
    put(&mut out, 0x54, &be32(chunk as u32));
    put(&mut out, 0x58, &prefix[..0x80]);
    put(&mut out, 0x48 + 0x90, &be32(1));
    put(&mut out, 0x48 + 0x94, &be32(48));
    put(&mut out, 0x48 + 0x98, &(part_at as u64).to_be_bytes());
    put(&mut out, 0x48 + 0xB4, &be32(1));
    put(&mut out, 0x48 + 0xB8, &(raw_at as u64).to_be_bytes());
    put(&mut out, 0x48 + 0xC0, &be32(raw_stored.len() as u32));
    put(&mut out, 0x48 + 0xC4, &be32(groups as u32));
    put(&mut out, 0x48 + 0xC8, &(groups_at as u64).to_be_bytes());
    put(&mut out, 0x48 + 0xD0, &be32(groups_stored.len() as u32));
    put_compression(&mut out, 0x48, o.compression);
    put(&mut out, raw_at, &raw_stored);
    put(&mut out, part_at, &part);
    put(&mut out, groups_at, &groups_stored);
    for g in 0..groups {
        put(&mut out, offsets[g], &stored[g]);
    }
    WiiRvz { bytes: out, part_table_at: part_at }
}

/// The committed Wii sample: an RVZ with a decrypted partition holding an `opening.bnr` (titles and a logo).
pub fn sample_rvz() -> Vec<u8> {
    let logo = tpl_plain(64, 16, 5, &words16(64, 16, |x, _| if x < 32 { 0x4F00 } else { 0x40F0 }));
    let icon = bin_archive(&logo, "Logo.tpl", true, false, &[]);
    let bnr = opening_plain(
        &[("ja", "サンプル"), ("en", "Sample Channel"), ("de", "Beispielkanal"), ("fr", "Chaîne exemple")],
        Some(&icon),
        None,
    );
    let part = partition_data("RSMP01", "Header Title", &[PartFile("opening.bnr", &bnr)], None, 0x7C00);
    rvz(&part, &WiiRvzOptions { game_id: "RSMP01", ..WiiRvzOptions::default() }).bytes
}

/// The committed WAD sample: title id `0001000152534D50` (`RSMP`).
pub fn sample_wad() -> Vec<u8> {
    wad(0x0001_0001_5253_4D50, 0x0102, "Is")
}
