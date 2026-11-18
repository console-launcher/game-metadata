// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Synthetic Wii U data: meta.xml, TGAs, a title's files and a ZArchive (`.wua`). Mirrors the original reader's
//! `WiiUFixture`. No commercial data.

use crate::gc::zstd;

/// The language codes of `meta.xml`, in slot order.
pub const LANGS: [&str; 12] = ["ja", "en", "fr", "de", "it", "es", "zhs", "ko", "nl", "pt", "ru", "zht"];

/// A `meta.xml`: language code to text for the long names, short names and publishers (missing codes get an empty tag).
#[derive(Debug, Clone)]
pub struct Meta<'a> {
    /// `longname_*`.
    pub longs: Vec<(&'a str, String)>,
    /// `shortname_*`.
    pub shorts: Vec<(&'a str, String)>,
    /// `publisher_*`.
    pub publishers: Vec<(&'a str, String)>,
    /// `title_id`.
    pub title_id: &'a str,
    /// `product_code`.
    pub product_code: &'a str,
    /// `region` (hex).
    pub region: &'a str,
}

impl Default for Meta<'_> {
    fn default() -> Self {
        Meta {
            longs: vec![("en", "Test Quest".into())],
            shorts: vec![("en", "TQ".into())],
            publishers: vec![("en", "Test Games".into())],
            title_id: "0005000010123456",
            product_code: "WUP-P-ABCE",
            region: "00000002",
        }
    }
}

impl Meta<'_> {
    /// The XML text.
    pub fn xml(&self) -> String {
        let get = |list: &[(&str, String)], l: &str| {
            list.iter().find(|(c, _)| *c == l).map(|(_, t)| t.clone()).unwrap_or_default()
        };
        let mut s =
            String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<menu type=\"complex\" access=\"777\">\n");
        s += &format!("  <title_id type=\"hexBinary\" length=\"8\">{}</title_id>\n", self.title_id);
        s += &format!("  <product_code type=\"string\" length=\"32\">{}</product_code>\n", self.product_code);
        s += &format!("  <region type=\"hexBinary\" length=\"4\">{}</region>\n", self.region);
        for l in LANGS {
            s += &format!("  <longname_{l} type=\"string\" length=\"512\">{}</longname_{l}>\n", get(&self.longs, l));
            s += &format!("  <shortname_{l} type=\"string\" length=\"256\">{}</shortname_{l}>\n", get(&self.shorts, l));
            s += &format!(
                "  <publisher_{l} type=\"string\" length=\"256\">{}</publisher_{l}>\n",
                get(&self.publishers, l)
            );
        }
        s += "</menu>\n";
        s
    }
}

/// An uncompressed (type 2) or RLE (type 10) TGA; `pixel` gives BGRA at (x, y) in top-down reading order.
pub fn tga(
    width: usize,
    height: usize,
    bpp: usize,
    top_down: bool,
    rle: bool,
    pixel: impl Fn(usize, usize) -> [u8; 4],
) -> Vec<u8> {
    let mut head = [0u8; 18];
    head[2] = if rle { 10 } else { 2 };
    head[12..14].copy_from_slice(&(width as u16).to_le_bytes());
    head[14..16].copy_from_slice(&(height as u16).to_le_bytes());
    head[16] = bpp as u8;
    head[17] = (if top_down { 0x20 } else { 0 }) | (if bpp == 32 { 8 } else { 0 });
    let mut out = head.to_vec();
    let bytes = bpp / 8;
    let rows: Vec<usize> = if top_down { (0..height).collect() } else { (0..height).rev().collect() };
    let mut pixels: Vec<Vec<u8>> = Vec::new();
    for y in rows {
        for x in 0..width {
            pixels.push(pixel(x, y)[..bytes].to_vec());
        }
    }
    if !rle {
        pixels.iter().for_each(|p| out.extend(p));
    } else {
        // Every packet: a run of equal pixels when 2+ follow, else a literal of one.
        let mut i = 0;
        while i < pixels.len() {
            let mut run = 1;
            while i + run < pixels.len() && run < 128 && pixels[i + run] == pixels[i] {
                run += 1;
            }
            out.push(if run > 1 { 0x80 | (run as u8 - 1) } else { 0 });
            out.extend(&pixels[i]);
            i += run;
        }
    }
    out
}

/// A 32 bit top-down TGA of one colour (BGRA).
pub fn solid_tga(width: usize, height: usize, b: u8, g: u8, r: u8) -> Vec<u8> {
    tga(width, height, 32, true, false, |_, _| [b, g, r, 255])
}

/// The files of a typical title (paths below its root).
pub fn standard_files(meta: &str) -> Vec<(String, Vec<u8>)> {
    let icon = tga(4, 2, 32, false, false, |x, y| if x == 0 && y == 0 { [0, 0, 255, 255] } else { [10, 20, 30, 128] });
    vec![
        ("meta/meta.xml".into(), meta.as_bytes().to_vec()),
        ("meta/iconTex.tga".into(), icon),
        ("meta/bootTvTex.tga".into(), solid_tga(16, 9, 1, 2, 3)),
        ("meta/bootDrcTex.tga".into(), solid_tga(8, 5, 4, 5, 6)),
        ("meta/bootLogoTex.tga".into(), solid_tga(2, 2, 7, 8, 9)),
        ("meta/bootMovie.h264".into(), vec![1; 1234]),
        ("meta/bootSound.btsnd".into(), vec![2; 99]),
        ("code/game.rpx".into(), vec![0; 16]),
        ("content/data.bin".into(), vec![0; 16]),
    ]
}

const BLOCK: usize = 64 * 1024;

struct Node {
    name: String,
    data: Option<Vec<u8>>,
    children: Vec<usize>,
}

fn put(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend(bytes);
}

/// A ZArchive of `files` (full paths with `/`): zstd blocks, raw blocks where zstd does not shrink them, big-endian tables.
pub fn zarchive(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut nodes = vec![Node { name: String::new(), data: None, children: Vec::new() }];
    for (path, data) in files {
        let parts: Vec<&str> = path.split('/').collect();
        let mut n = 0;
        for d in &parts[..parts.len() - 1] {
            let found = nodes[n].children.iter().copied().find(|&c| nodes[c].name == *d && nodes[c].data.is_none());
            n = found.unwrap_or_else(|| {
                nodes.push(Node { name: (*d).into(), data: None, children: Vec::new() });
                let id = nodes.len() - 1;
                nodes[n].children.push(id);
                id
            });
        }
        nodes.push(Node { name: parts[parts.len() - 1].into(), data: Some(data.clone()), children: Vec::new() });
        let id = nodes.len() - 1;
        nodes[n].children.push(id);
    }
    // Children sorted case-insensitively, as the format keeps them.
    for i in 0..nodes.len() {
        let mut kids = std::mem::take(&mut nodes[i].children);
        kids.sort_by_key(|&k| nodes[k].name.to_lowercase());
        nodes[i].children = kids;
    }
    // Breadth first so each folder's children are contiguous.
    let mut order = vec![0usize];
    let mut index = vec![0usize; nodes.len()];
    let mut queue = std::collections::VecDeque::from([0usize]);
    while let Some(d) = queue.pop_front() {
        for &c in &nodes[d].children.clone() {
            index[c] = order.len();
            order.push(c);
            if nodes[c].data.is_none() {
                queue.push_back(c);
            }
        }
    }
    let mut stream = Vec::new();
    let mut names = Vec::new();
    let mut tree = Vec::new();
    let mut cursor = 0u64;
    for &n in &order {
        let node = &nodes[n];
        let name_offset = |names: &mut Vec<u8>| {
            let at = names.len() as u32;
            let b = node.name.as_bytes();
            if b.len() < 0x80 {
                names.push(b.len() as u8);
            } else {
                names.push(0x80 | (b.len() & 0x7F) as u8);
                names.push((b.len() >> 7) as u8);
            }
            names.extend(b);
            at
        };
        match &node.data {
            None => {
                let first = node.children.first().map_or(0, |&c| index[c]) as u32;
                let name = if n == 0 { 0x7FFF_FFFF } else { name_offset(&mut names) };
                for v in [name, first, node.children.len() as u32, 0] {
                    put(&mut tree, &v.to_be_bytes());
                }
            }
            Some(data) => {
                let name = (1u32 << 31) | name_offset(&mut names);
                for v in [name, cursor as u32, data.len() as u32] {
                    put(&mut tree, &v.to_be_bytes());
                }
                put(&mut tree, &[0, 0, 0, 0]);
                stream.extend(data);
                cursor += data.len() as u64;
            }
        }
    }
    // Compress the stream in 64 KiB blocks (the last padded with zeros).
    let blocks = stream.len().div_ceil(BLOCK).max(1);
    let mut data = Vec::new();
    let mut records = Vec::new();
    let mut record: Vec<u8> = Vec::new();
    for i in 0..blocks {
        if i % 16 == 0 {
            if !record.is_empty() {
                record.resize(40, 0);
                records.extend(&record);
            }
            record = (data.len() as u64).to_be_bytes().to_vec();
        }
        let mut chunk = stream[(i * BLOCK).min(stream.len())..((i + 1) * BLOCK).min(stream.len())].to_vec();
        chunk.resize(BLOCK, 0);
        let packed = zstd(&chunk);
        let stored = if packed.len() < BLOCK { packed } else { chunk };
        record.extend(((stored.len() - 1) as u16).to_be_bytes());
        data.extend(stored);
    }
    record.resize(40, 0);
    records.extend(&record);
    let mut out = Vec::new();
    let mut footer = vec![0u8; 144];
    for (i, section) in [&data, &records, &names, &tree, &Vec::new(), &Vec::new()].into_iter().enumerate() {
        footer[i * 16..i * 16 + 8].copy_from_slice(&(out.len() as u64).to_be_bytes());
        footer[i * 16 + 8..i * 16 + 16].copy_from_slice(&(section.len() as u64).to_be_bytes());
        out.extend(section.iter());
    }
    footer[128..136].copy_from_slice(&(out.len() as u64 + 144).to_be_bytes());
    footer[136..140].copy_from_slice(&1u32.to_be_bytes());
    footer[140..144].copy_from_slice(&0x169f_52d6u32.to_be_bytes());
    out.extend(footer);
    out
}

/// The committed Wii U sample: a `.wua` holding one title (a base game folder).
pub fn sample_wua() -> Vec<u8> {
    let meta = Meta {
        longs: vec![("en", "Sample Title".into()), ("ja", "サンプル".into()), ("de", "Beispieltitel".into())],
        shorts: vec![("en", "Sample".into())],
        publishers: vec![("en", "Example Games".into())],
        title_id: "00050000101ABCDE",
        product_code: "WUP-P-SMPE",
        region: "00000004",
    };
    let files: Vec<(String, Vec<u8>)> =
        standard_files(&meta.xml()).into_iter().map(|(p, d)| (format!("00050000101abcde_v0/{p}"), d)).collect();
    zarchive(&files)
}
