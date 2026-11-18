// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A channel banner's layout (`RLYT`, NW4R): the screen size, the texture names, the materials and the pane tree. Panes with a
//! picture (`pic1`) draw; `wnd1` (window frames), `txt1` (text) and `bnd1` (bounding box) keep their place in the tree and draw
//! nothing, counted in `skipped` so a mismatch can be explained. Written from the NW4R layout format; unverified against real
//! banners (see `docs/design.md`, "Wii channel banners"): every offset and flag bit below that the format description does not
//! settle is marked "GUESS" so a later check against real files finds them in one place.

use alloc::string::String;
use alloc::vec::Vec;

use super::be::Be;

/// The most panes and materials a layout may hold, and how deep panes nest: far more than a banner has, and what keeps a
/// damaged file cheap to draw (the original implementation has no limits).
const MAX_PANES: usize = 4096;
const MAX_MATERIALS: usize = 4096;
const MAX_DEPTH: usize = 256;
/// `pan1` section size: where a `pic1`'s own fields start. GUESS: the NW4R `pic1` follows the common pane header at 0x4C.
const PAN: usize = 0x4C;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Pan,
    Pic,
    Wnd,
    Txt,
    Bnd,
}

#[derive(Debug, Clone)]
pub(super) struct TexMap {
    pub texture: u16,
    pub wrap_s: u8,
    pub wrap_t: u8,
}

/// `srt` per texture map: translate s, t, rotate degrees, scale s, t. `color` is the material color (RGBA 0..=255). `black` and
/// `white` are the first two TEV color registers (RGBA 0..=255, clamped): a texel maps to `black + (white - black) * texel`.
#[derive(Debug, Clone)]
pub(super) struct Material {
    pub name: String,
    pub tex_maps: Vec<TexMap>,
    pub srt: Vec<[f32; 5]>,
    pub color: [u8; 4],
    pub black: [u8; 4],
    pub white: [u8; 4],
}

#[derive(Debug, Clone)]
pub(super) struct Pane {
    pub name: String,
    pub kind: Kind,
    pub visible: bool,
    pub influenced_alpha: bool,
    /// 0 left, 1 center, 2 right
    pub origin_x: i32,
    /// 0 top, 1 center, 2 bottom
    pub origin_y: i32,
    pub alpha: u8,
    pub translate: [f32; 3],
    pub rotate: [f32; 3],
    pub scale: [f32; 2],
    pub size: [f32; 2],
    /// TL, TR, BL, BR as 0xRRGGBBAA, `pic1` only
    pub vertex_colors: Option<[u32; 4]>,
    pub material: i32,
    /// Per set: s, t for TL, TR, BL, BR.
    pub tex_coords: Vec<[f32; 8]>,
    pub children: Vec<usize>,
}

/// The parsed layout. `panes[0]` is a root with no name that holds the file's first pane.
#[derive(Debug)]
pub(super) struct Layout {
    pub width: f32,
    pub height: f32,
    pub textures: Vec<String>,
    pub materials: Vec<Material>,
    pub panes: Vec<Pane>,
    /// Pane kinds that were parsed but do not draw, by magic, in order of first appearance.
    pub skipped: Vec<(&'static str, u32)>,
}

impl Layout {
    pub fn parse(bytes: &[u8]) -> Option<Layout> {
        let be = Be(bytes);
        let sections = be.sections(b"RLYT")?;
        let (mut width, mut height) = (0f32, 0f32);
        let mut textures = Vec::new();
        let mut materials = Vec::new();
        let root = Pane {
            name: String::new(),
            kind: Kind::Pan,
            visible: true,
            influenced_alpha: true,
            origin_x: 1,
            origin_y: 1,
            alpha: 255,
            translate: [0.0; 3],
            rotate: [0.0; 3],
            scale: [1.0, 1.0],
            size: [0.0; 2],
            vertex_colors: None,
            material: -1,
            tex_coords: Vec::new(),
            children: Vec::new(),
        };
        let mut panes = alloc::vec![root];
        let mut stack = alloc::vec![0usize];
        let mut last = 0usize;
        let mut skipped: Vec<(&'static str, u32)> = Vec::new();
        for s in &sections {
            let at = s.start;
            match &s.magic {
                b"lyt1" => {
                    width = be.f32(at + 12)?;
                    height = be.f32(at + 16)?;
                }
                b"txl1" => textures.extend(texture_names(&be, at)?),
                b"mat1" => materials.extend(read_materials(&be, at)?),
                b"pas1" => {
                    if stack.len() >= MAX_DEPTH {
                        return None;
                    }
                    stack.push(last);
                }
                b"pae1" if stack.len() > 1 => {
                    stack.pop();
                }
                b"pae1" => {}
                m @ (b"pan1" | b"pic1" | b"wnd1" | b"txt1" | b"bnd1") => {
                    let kind = match m {
                        b"pan1" => Kind::Pan,
                        b"pic1" => Kind::Pic,
                        b"wnd1" => Kind::Wnd,
                        b"txt1" => Kind::Txt,
                        _ => Kind::Bnd,
                    };
                    if panes.len() > MAX_PANES {
                        return None;
                    }
                    let pane = read_pane(&be, at, kind)?;
                    if let Some(name) = match kind {
                        Kind::Wnd => Some("wnd1"),
                        Kind::Txt => Some("txt1"),
                        Kind::Bnd => Some("bnd1"),
                        _ => None,
                    } {
                        match skipped.iter_mut().find(|(n, _)| *n == name) {
                            Some((_, count)) => *count += 1,
                            None => skipped.push((name, 1)),
                        }
                    }
                    // The first pane is the root: its children hang from it; "pas1" nests under the last pane.
                    panes.push(pane);
                    let index = panes.len() - 1;
                    panes[*stack.last()?].children.push(index);
                    last = index;
                }
                _ => {}
            }
        }
        // NaN fails both comparisons below the way the original implementation's `<=` does not: reject it too.
        if !(width > 0.0 && height > 0.0 && width <= 4096.0 && height <= 4096.0) {
            return None;
        }
        Some(Layout { width, height, textures, materials, panes, skipped })
    }
}

fn texture_names(be: &Be, at: usize) -> Option<Vec<String>> {
    let count = usize::from(be.u16(at + 8)?);
    let table = at + 12;
    if count == 0 {
        return Some(Vec::new());
    }
    // GUESS: 8 byte entries (name offset, 0) in this layout, relative to the first entry; 4 byte entries are told by the first offset.
    let stride = if be.u32(table)? as usize == count * 4 { 4 } else { 8 };
    (0..count).map(|i| be.name(be.offset(table, table + i * stride)?, 64)).collect()
}

fn read_materials(be: &Be, at: usize) -> Option<Vec<Material>> {
    let count = usize::from(be.u16(at + 8)?);
    if count > MAX_MATERIALS {
        return None;
    }
    (0..count)
        .map(|i| {
            let start = be.offset(at, at + 12 + i * 4)?;
            // The flag word sits at 0x3C of the material (after the name and the TEV colors). Its low nibbles count the blocks that
            // follow: texture maps (bits 0-3, 4 bytes each), texture SRTs (4-7, 20 bytes), coordinate generators (8-11, 4 bytes).
            // Then, each switched by its own bit: channel control (bit 25, 4 bytes) and the material color (bit 27, 4 bytes).
            // Checked against shipped channel banners (`docs/wii-references.md`).
            let flags = be.u32(start.checked_add(0x3C)?)?;
            let mut p = start.checked_add(0x40)?;
            let mut tex_maps = Vec::new();
            for _ in 0..flags & 0xF {
                tex_maps.push(TexMap { texture: be.u16(p)?, wrap_s: be.u8(p + 2)?, wrap_t: be.u8(p + 3)? });
                p += 4;
            }
            let mut srt = Vec::new();
            for _ in 0..(flags >> 4) & 0xF {
                let mut v = [0f32; 5];
                for (k, slot) in v.iter_mut().enumerate() {
                    *slot = be.f32(p + k * 4)?;
                }
                srt.push(v);
                p += 20;
            }
            p += ((flags >> 8) & 0xF) as usize * 4; // texture coordinate generators
            if flags & (1 << 25) != 0 {
                p += 4; // channel control
            }
            let color = if flags & (1 << 27) != 0 {
                [be.u8(p)?, be.u8(p + 1)?, be.u8(p + 2)?, be.u8(p + 3)?]
            } else {
                [255; 4]
            };
            // The first two TEV color registers (0x14 and 0x1C: s16 RGBA each) are what an intensity texture maps black and white to.
            let tev = |at: usize| -> Option<[u8; 4]> {
                let mut c = [0u8; 4];
                for (k, slot) in c.iter_mut().enumerate() {
                    *slot = i32::from(be.u16(at + k * 2)? as i16).clamp(0, 255) as u8;
                }
                Some(c)
            };
            let black = tev(start.checked_add(0x14)?)?;
            let white = tev(start.checked_add(0x1C)?)?;
            // Parts after the color (TEV swap 12, indirect SRTs 13-14, indirect stages 15-17, TEV stages 18-22, alpha compare 23, blend mode 24)
            // are not read: modulating the
            // texture with the colors is the approximation.
            Some(Material { name: be.name(start, 20)?, tex_maps, srt, color, black, white })
        })
        .collect()
}

fn read_pane(be: &Be, at: usize, kind: Kind) -> Option<Pane> {
    let flags = be.u8(at + 8)?;
    let origin = i32::from(be.u8(at + 9)?);
    let f = at + 0x24;
    let floats = |n: usize, base: usize| -> Option<Vec<f32>> { (0..n).map(|i| be.f32(base + i * 4)).collect() };
    let t = floats(3, f)?;
    let r = floats(3, f + 12)?;
    let sc = floats(2, f + 24)?;
    let sz = floats(2, f + 32)?;
    let mut vertex_colors = None;
    let mut material = -1;
    let mut tex_coords = Vec::new();
    if kind == Kind::Pic {
        let p = at + PAN;
        vertex_colors = Some([be.u32(p)?, be.u32(p + 4)?, be.u32(p + 8)?, be.u32(p + 12)?]);
        material = i32::from(be.u16(p + 16)?);
        for s in 0..usize::from(be.u8(p + 18)?) {
            let mut c = [0f32; 8];
            for (i, slot) in c.iter_mut().enumerate() {
                *slot = be.f32(p + 20 + s * 32 + i * 4)?;
            }
            tex_coords.push(c);
        }
    }
    Some(Pane {
        name: be.name(at + 0x0C, 16)?,
        kind,
        visible: flags & 1 != 0,
        influenced_alpha: flags & 2 != 0,
        origin_x: origin % 3,
        origin_y: (origin / 3).min(2),
        alpha: be.u8(at + 10)?,
        translate: [t[0], t[1], t[2]],
        rotate: [r[0], r[1], r[2]],
        scale: [sc[0], sc[1]],
        size: [sz[0], sz[1]],
        vertex_colors,
        material,
        tex_coords,
        children: Vec::new(),
    })
}
