// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Writers for Wii channel banner layouts (`RLYT`) and animations (`RLAN`), the way the parsers read them, and the authored
//! banners the original renderer's tests build. Mirrors the original implementation `BrlytWriter` byte for byte (`fixtures/wii-banner/*.bin`
//! are the original implementation's own archives; a test compares these to them). No commercial data.

use crate::wii::{U8File, lz77_literals, tpl_plain, u8_archive, words16};

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// A texture map of a material: texture index and wrap (0 clamp, 1 repeat, 2 mirror) for both axes.
#[derive(Debug, Clone, Copy)]
pub struct Tex(pub u16, pub u8);

/// A material: name, texture maps, per map `[translate s, t, rotate degrees, scale s, t]`, optional RGBA color.
#[derive(Debug, Clone)]
pub struct Mat {
    /// Name (up to 20 bytes).
    pub name: &'static str,
    /// Texture maps.
    pub textures: Vec<Tex>,
    /// Texture SRTs.
    pub srt: Vec<[f32; 5]>,
    /// Material color.
    pub color: Option<[u8; 4]>,
    /// Write every optional block (see `material`).
    pub full: bool,
    /// The first two TEV color registers: the colors an intensity texture maps black and white to (s16 per channel).
    pub black: [i16; 4],
    pub white: [i16; 4],
}

impl Mat {
    /// A material with no maps and no color.
    pub fn new(name: &'static str) -> Self {
        Mat { name, textures: Vec::new(), srt: Vec::new(), color: None, full: false, black: [0; 4], white: [255; 4] }
    }
}

/// A pane. `origin` 0 is top left, 4 center, 8 bottom right; `colors` TL, TR, BL, BR as 0xRRGGBBAA.
#[derive(Debug, Clone)]
pub struct Pane {
    /// Name (up to 16 bytes).
    pub name: &'static str,
    /// `pic1`, `pan1`, `wnd1`, `txt1` or `bnd1`.
    pub kind: &'static str,
    /// Translate x, y.
    pub x: f32,
    /// Translate y.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
    /// Origin 0..=8.
    pub origin: u8,
    /// Alpha 0..=255.
    pub alpha: u8,
    /// Z rotation in degrees.
    pub rot_z: f32,
    /// Scale (both axes).
    pub scale: f32,
    /// Vertex colors.
    pub colors: [u32; 4],
    /// Material index.
    pub material: u16,
    /// Texture coordinates (s, t) of TL, TR, BL, BR.
    pub coords: [f32; 8],
    /// Visible flag.
    pub visible: bool,
    /// Alpha influenced by the parent.
    pub influenced: bool,
    /// Child panes.
    pub children: Vec<Pane>,
}

impl Pane {
    /// A white 100 x 100 picture pane at the origin.
    pub fn new(name: &'static str) -> Self {
        Pane {
            name,
            kind: "pic1",
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            origin: 4,
            alpha: 255,
            rot_z: 0.0,
            scale: 1.0,
            colors: [0xFFFF_FFFF; 4],
            material: 0,
            coords: [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            visible: true,
            influenced: true,
            children: Vec::new(),
        }
    }
}

fn name(n: &str, size: usize) -> Vec<u8> {
    let mut v = n.as_bytes().to_vec();
    v.resize(size, 0);
    v
}

fn f32b(v: f32) -> [u8; 4] {
    v.to_bits().to_be_bytes()
}

fn section(magic: &str, body: &[u8]) -> Vec<u8> {
    let mut o = name(magic, 4);
    o.extend(be32(body.len() as u32 + 8));
    o.extend(body);
    o
}

fn file(magic: &str, sections: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = sections.concat();
    let mut o = name(magic, 4);
    o.extend(be16(0xFEFF));
    o.extend(be16(8));
    o.extend(be32(body.len() as u32 + 16));
    o.extend(be16(16));
    o.extend(be16(sections.len() as u16));
    o.extend(body);
    o
}

/// A material as the real Wii menu's banners lay it out: the name, the TEV colors, then the flag word
/// at 0x3C. Counts sit in the low nibbles (texture maps 0-3, texture SRTs 4-7, coordinate generators 8-11); the optional blocks
/// follow their data in this order, each switched by its own bit: channel control (bit 25), material color (27), TEV swap (12),
/// indirect SRTs (13-14, a count), indirect stages (15-17, a count), TEV stages (18-22, a count), alpha compare (23) and blend
/// mode (24). `Mat::full` writes every optional block with the sizes the real banners show, so a reader that skips one wrongly
/// reads the color in the wrong place.
fn material(m: &Mat) -> Vec<u8> {
    let mut o = name(m.name, 20);
    for v in m.black.iter().chain(&m.white).chain(&[255i16; 4]) {
        o.extend(v.to_be_bytes()); // tev colors: black, white, third register
    }
    o.extend([255u8; 16]); // tev konst colors
    let mut flags = (m.textures.len() as u32) | ((m.srt.len() as u32) << 4) | ((m.textures.len() as u32) << 8);
    if m.color.is_some() {
        flags |= 1 << 27 | 1 << 25;
    }
    if m.full {
        flags |= 1 << 12 | 2 << 13 | 1 << 15 | 2 << 18 | 1 << 23 | 1 << 24;
    }
    o.extend(be32(flags));
    for t in &m.textures {
        o.extend(be16(t.0));
        o.extend([t.1, t.1]);
    }
    for s in &m.srt {
        for v in s {
            o.extend(f32b(*v));
        }
    }
    for _ in &m.textures {
        o.extend(be32(0x0104_1E00)); // coord gens: matrix 2x4, source texcoord 0, identity matrix
    }
    if let Some(c) = m.color {
        o.extend(be32(0x0101_0000)); // channel control
        o.extend(c);
    }
    if m.full {
        o.extend(be32(0xE4E4_E4E4)); // TEV swap
        o.extend([0u8; 2 * 20]); // indirect SRTs
        o.extend([0u8; 4]); // indirect stage
        o.extend([0u8; 2 * 16]); // TEV stages
        o.extend(be32(0x7700_0000)); // alpha compare
        o.extend(be32(0x0104_0503)); // blend mode
    }
    o
}

fn pane(p: &Pane) -> Vec<Vec<u8>> {
    let mut o = vec![u8::from(p.visible) | if p.influenced { 2 } else { 0 }, p.origin, p.alpha, 0];
    o.extend(name(p.name, 16));
    o.extend([0u8; 8]);
    for v in [p.x, p.y, 0.0, 0.0, 0.0, p.rot_z, p.scale, p.scale, p.w, p.h] {
        o.extend(f32b(v));
    }
    if p.kind == "pic1" {
        for c in p.colors {
            o.extend(be32(c));
        }
        o.extend(be16(p.material));
        o.extend([1, 0]);
        for v in p.coords {
            o.extend(f32b(v));
        }
    }
    let mut out = vec![section(p.kind, &o)];
    if !p.children.is_empty() {
        out.push(section("pas1", &[]));
        out.extend(p.children.iter().flat_map(pane));
        out.push(section("pae1", &[]));
    }
    out
}

/// A layout of `width` x `height` with the given texture names, materials and panes (under a root pane).
pub fn layout(width: f32, height: f32, textures: &[&str], materials: &[Mat], panes: &[Pane]) -> Vec<u8> {
    let mut sections = Vec::new();
    let mut lyt = vec![1, 0, 0, 0];
    lyt.extend(f32b(width));
    lyt.extend(f32b(height));
    sections.push(section("lyt1", &lyt));
    let mut txl = Vec::new();
    txl.extend(be16(textures.len() as u16));
    txl.extend(be16(0));
    let mut at = textures.len() * 8;
    for t in textures {
        txl.extend(be32(at as u32));
        txl.extend(be32(0));
        at += t.len() + 1;
    }
    for t in textures {
        txl.extend(t.as_bytes());
        txl.push(0);
    }
    while txl.len() % 4 != 0 {
        txl.push(0);
    }
    sections.push(section("txl1", &txl));
    let bodies: Vec<Vec<u8>> = materials.iter().map(material).collect();
    let mut mat = Vec::new();
    mat.extend(be16(materials.len() as u16));
    mat.extend(be16(0));
    let mut at = 8 + 4 + materials.len() * 4;
    for b in &bodies {
        mat.extend(be32(at as u32));
        at += b.len();
    }
    for b in &bodies {
        mat.extend(b);
    }
    sections.push(section("mat1", &mat));
    let root = Pane { kind: "pan1", w: width, h: height, material: 0, ..Pane::new("RootPane") };
    sections.push(pane(&root).remove(0));
    sections.push(section("pas1", &[]));
    for p in panes {
        sections.extend(pane(p));
    }
    sections.push(section("pae1", &[]));
    file("RLYT", &sections)
}

/// A keyframe: frame, value, slope (hermite only).
#[derive(Debug, Clone, Copy)]
pub struct Key(pub f32, pub f32, pub f32);

/// One track: index, type (the attribute of its tag), hermite or step, keys.
#[derive(Debug, Clone)]
pub struct Track {
    /// Texture map index for `RLTS` and `RLTP`.
    pub index: u8,
    /// Attribute of the tag.
    pub kind: u8,
    /// Hermite (else step).
    pub hermite: bool,
    /// Keys.
    pub keys: Vec<Key>,
}

/// A tag (`RLPA`, `RLVC`, ...) with tracks.
#[derive(Debug, Clone)]
pub struct Tag(pub &'static str, pub Vec<Track>);

/// A pane's or (with `material`) a material's tags.
#[derive(Debug, Clone)]
pub struct Target {
    /// Pane or material name.
    pub name: &'static str,
    /// A material target.
    pub material: bool,
    /// Tags.
    pub tags: Vec<Tag>,
}

/// A track of `kind` (the attribute of its tag) for texture map `index`.
pub fn track(index: u8, kind: u8, hermite: bool, keys: &[Key]) -> Track {
    Track { index, kind, hermite, keys: keys.to_vec() }
}

fn tag(t: &Tag) -> Vec<u8> {
    let tracks: Vec<Vec<u8>> = t.1.iter().map(track_bytes).collect();
    let mut o = name(t.0, 4);
    o.extend([tracks.len() as u8, 0, 0, 0]);
    let mut at = 8 + tracks.len() * 4;
    for b in &tracks {
        o.extend(be32(at as u32));
        at += b.len();
    }
    for b in &tracks {
        o.extend(b);
    }
    o
}

fn track_bytes(t: &Track) -> Vec<u8> {
    let mut o = vec![t.index, t.kind, if t.hermite { 2 } else { 1 }, 0];
    o.extend(be16(t.keys.len() as u16));
    o.extend(be16(0));
    o.extend(be32(12));
    for k in &t.keys {
        o.extend(f32b(k.0));
        if t.hermite {
            o.extend(f32b(k.1));
            o.extend(f32b(k.2));
        } else {
            o.extend(be16(k.1 as i32 as u16));
            o.extend(be16(0));
        }
    }
    o
}

fn target(t: &Target) -> Vec<u8> {
    let tags: Vec<Vec<u8>> = t.tags.iter().map(tag).collect();
    let mut head = name(t.name, 20);
    head.extend([tags.len() as u8, u8::from(t.material), 0, 0]);
    let mut at = 24 + tags.len() * 4;
    for b in &tags {
        head.extend(be32(at as u32));
        at += b.len();
    }
    for b in &tags {
        head.extend(b);
    }
    head
}

/// An animation of `frames` frames (looping or not) with the targets and texture names (for `RLTP`).
pub fn animation(frames: u16, looping: bool, targets: &[Target], textures: &[&str]) -> Vec<u8> {
    let mut o = Vec::new();
    o.extend(be16(frames));
    o.extend([u8::from(looping), 0]);
    o.extend(be16(textures.len() as u16));
    o.extend(be16(targets.len() as u16));
    let names: usize = textures.iter().map(|t| t.len() + 1).sum();
    o.extend(be32((8 + 12 + textures.len() * 4 + names.div_ceil(4) * 4) as u32));
    let mut at = textures.len() * 4;
    for t in textures {
        o.extend(be32(at as u32));
        at += t.len() + 1;
    }
    for t in textures {
        o.extend(t.as_bytes());
        o.push(0);
    }
    while o.len() % 4 != 0 {
        o.push(0);
    }
    let bodies: Vec<Vec<u8>> = targets.iter().map(target).collect();
    let mut start = 8 + o.len() + targets.len() * 4;
    for b in &bodies {
        o.extend(be32(start as u32));
        start += b.len();
    }
    for b in &bodies {
        o.extend(b);
    }
    file("RLAN", &[section("pai1", &o)])
}

/// A banner archive as `meta/banner.bin` holds it: the layout as `arc/blyt/banner.brlyt`, animations in `arc/anim`, textures in
/// `arc/timg`.
pub fn archive(layout: &[u8], animations: &[(&str, &[u8])], textures: &[(&str, &[u8])]) -> Vec<u8> {
    let paths: Vec<String> = animations
        .iter()
        .map(|(n, _)| format!("arc/anim/{n}"))
        .chain(textures.iter().map(|(n, _)| format!("arc/timg/{n}")))
        .collect();
    let datas = animations.iter().map(|(_, d)| *d).chain(textures.iter().map(|(_, d)| *d));
    let mut files = vec![U8File("arc/blyt/banner.brlyt", layout)];
    files.extend(paths.iter().zip(datas).map(|(p, d)| U8File(p, d)));
    u8_archive(&files)
}

/// `data` wrapped as the LZ77 archive a banner may ship in: `LZ77` then literals.
pub fn lz77_wrapped(data: &[u8]) -> Vec<u8> {
    let mut w = b"LZ77".to_vec();
    w.extend(lz77_literals(data, 0x10));
    w
}

/// A 5 (RGB5A3) TPL of `width` x `height` from `word(x, y)`.
pub fn tpl_words(width: usize, height: usize, word: impl Fn(usize, usize) -> u16) -> Vec<u8> {
    let mut tpl = tpl_plain(width as u16, height as u16, 5, &words16(width, height, word));
    // The original writer also sets the linear min and mag filters (offsets 0x28, 0x2C of the image header).
    tpl[0x28..0x30].copy_from_slice(&[0, 0, 0, 1, 0, 0, 0, 1]);
    tpl
}

/// An opaque RGB555 TPL from `color(x, y)` (5 bit channels, no top bit).
pub fn tpl_opaque(width: usize, height: usize, color: impl Fn(usize, usize) -> u16) -> Vec<u8> {
    tpl_words(width, height, |x, y| 0x8000 | color(x, y))
}

/// RGB555 words used by the cases.
pub const RED5: u16 = 31 << 10;
/// Green.
pub const GREEN5: u16 = 31 << 5;
/// Blue.
pub const BLUE5: u16 = 31;
/// White.
pub const WHITE5: u16 = 0x7FFF;

/// The 4 x 4 texture of red, green, blue, white quadrants.
pub fn quadrants() -> Vec<u8> {
    tpl_opaque(4, 4, |x, y| match (y < 2, x < 2) {
        (true, true) => RED5,
        (true, false) => GREEN5,
        (false, true) => BLUE5,
        (false, false) => WHITE5,
    })
}
