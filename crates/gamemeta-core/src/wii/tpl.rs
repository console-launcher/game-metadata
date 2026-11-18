// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! GameCube and Wii texture palette files (TPL): the first image decoded to RGBA. Formats I4, I8, IA4, IA8, RGB565,
//! RGB5A3, RGBA32, C4, C8, C14X2 (palette IA8, RGB565, RGB5A3) and CMPR; I4 and I8 come out opaque grey. `None` for
//! anything else, a truncated file or an absurd size.

use alloc::vec;
use alloc::vec::Vec;

use crate::RgbaImage;
use crate::image::{rgb5a3, rgb565};
use crate::le::{be16_at, be32_at};

const MAGIC: u32 = 0x0020_AF30;
const MAX_SIDE: u32 = 4096;
const MAX_PIXELS: u32 = 8 << 20;

struct Header {
    width: u32,
    height: u32,
    format: u32,
    data_at: usize,
    palette_at: usize,
}

fn header(b: &[u8]) -> Option<Header> {
    if b.len() < 0x14 || be32_at(b, 0)? != MAGIC {
        return None;
    }
    let count = be32_at(b, 4)?;
    let table = u64::from(be32_at(b, 8)?);
    if count == 0 || table + 8 > b.len() as u64 {
        return None;
    }
    let image = u64::from(be32_at(b, table as usize)?);
    let palette = u64::from(be32_at(b, table as usize + 4)?);
    if image + 0x14 > b.len() as u64 {
        return None;
    }
    let image = image as usize;
    let (width, height) = (u32::from(be16_at(b, image + 2)?), u32::from(be16_at(b, image)?));
    if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) || width * height > MAX_PIXELS {
        return None;
    }
    let data = be32_at(b, image + 8)? as usize;
    if data > b.len() {
        return None;
    }
    let palette_at = if (1..b.len() as u64).contains(&palette) { palette as usize } else { 0 };
    Some(Header { width, height, format: be32_at(b, image + 4)?, data_at: data, palette_at })
}

/// Width and height of the first image without decoding it.
pub(crate) fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    header(bytes).map(|h| (h.width, h.height))
}

/// Tile width, height and bytes per tile by format.
fn tile(format: u32) -> Option<(usize, usize, usize)> {
    Some(match format {
        0 | 8 | 14 => (8, 8, 32),
        1 | 2 | 9 => (8, 4, 32),
        3..=5 | 10 => (4, 4, 32),
        6 => (4, 4, 64),
        _ => return None,
    })
}

/// The palette as RGBA words, or `None` when its header or data is unreadable.
fn read_palette(b: &[u8], h: &Header) -> Option<Vec<[u8; 4]>> {
    if h.palette_at == 0 || h.palette_at + 12 > b.len() {
        return None;
    }
    let entries = usize::from(be16_at(b, h.palette_at)?);
    let format = be32_at(b, h.palette_at + 4)?;
    let data = be32_at(b, h.palette_at + 8)? as usize;
    if entries == 0 || data + entries * 2 > b.len() {
        return None;
    }
    (0..entries)
        .map(|i| {
            let v = u32::from(be16_at(b, data + i * 2)?);
            match format {
                0 => Some([(v & 0xFF) as u8, (v & 0xFF) as u8, (v & 0xFF) as u8, (v >> 8) as u8]),
                1 => Some(rgb565(v)),
                2 => Some(rgb5a3(v)),
                _ => None,
            }
        })
        .collect()
}

/// One 4x4 CMPR (DXT1) block: two RGB565 colors then 16 two-bit indices, the top row first, the leftmost pixel in the
/// high bits.
fn cmpr_block(b: &[u8], at: usize, mut put: impl FnMut(usize, usize, [u8; 4])) -> Option<()> {
    let (c0, c1) = (be16_at(b, at)?, be16_at(b, at + 2)?);
    let (a, z) = (rgb565(u32::from(c0)), rgb565(u32::from(c1)));
    let mix = |wa: u32, wz: u32, div: u32| {
        let ch = |i: usize| ((u32::from(a[i]) * wa + u32::from(z[i]) * wz) / div) as u8;
        [ch(0), ch(1), ch(2), 255]
    };
    let colors = if c0 > c1 { [a, z, mix(2, 1, 3), mix(1, 2, 3)] } else { [a, z, mix(1, 1, 2), [0; 4]] };
    for py in 0..4 {
        let row = usize::from(*b.get(at + 4 + py)?);
        for px in 0..4 {
            put(px, py, colors[(row >> (6 - px * 2)) & 3]);
        }
    }
    Some(())
}

/// A banner texture: like [`decode`], but an I4 or I8 texel is (I, I, I, I) as the GX texture unit expands it, so intensity is also
/// the alpha (the black texels of an intensity checker are transparent). [`decode`] keeps them opaque grey, which is what a
/// channel logo wants.
pub(crate) fn decode_gx(b: &[u8]) -> Option<RgbaImage> {
    let mut image = decode(b)?;
    if matches!(header(b)?.format, 0 | 1) {
        for px in image.pixels.chunks_exact_mut(4) {
            px[3] = px[0];
        }
    }
    Some(image)
}

pub(crate) fn decode(b: &[u8]) -> Option<RgbaImage> {
    let h = header(b)?;
    let (tw, th, tile_bytes) = tile(h.format)?;
    let (width, height) = (h.width as usize, h.height as usize);
    let (tiles_x, tiles_y) = (width.div_ceil(tw), height.div_ceil(th));
    if h.data_at + tiles_x * tiles_y * tile_bytes > b.len() {
        return None;
    }
    let palette = if matches!(h.format, 8..=10) { Some(read_palette(b, &h)?) } else { None };
    let pal = |i: usize| palette.as_ref().and_then(|p| p.get(i).copied()).unwrap_or([0; 4]);
    let mut out = vec![0u8; width * height * 4];
    let mut set = |x: usize, y: usize, c: [u8; 4]| {
        if x < width && y < height {
            let o = (y * width + x) * 4;
            out[o..o + 4].copy_from_slice(&c);
        }
    };
    let byte = |i: usize| usize::from(b[i]);
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            let base = h.data_at + (ty * tiles_x + tx) * tile_bytes;
            let (x0, y0) = (tx * tw, ty * th);
            let word = |i: usize| u32::from(u16::from_be_bytes([b[i], b[i + 1]]));
            match h.format {
                0 | 8 => {
                    for py in 0..8 {
                        for px in 0..8 {
                            let v = byte(base + py * 4 + px / 2);
                            let n = if px % 2 == 0 { v >> 4 } else { v & 15 };
                            let c = if h.format == 0 {
                                [(n * 17) as u8, (n * 17) as u8, (n * 17) as u8, 255]
                            } else {
                                pal(n)
                            };
                            set(x0 + px, y0 + py, c);
                        }
                    }
                }
                1 | 9 | 2 => {
                    for py in 0..4 {
                        for px in 0..8 {
                            let v = byte(base + py * 8 + px);
                            let c = match h.format {
                                1 => [v as u8, v as u8, v as u8, 255],
                                2 => [
                                    ((v & 15) * 17) as u8,
                                    ((v & 15) * 17) as u8,
                                    ((v & 15) * 17) as u8,
                                    ((v >> 4) * 17) as u8,
                                ],
                                _ => pal(v),
                            };
                            set(x0 + px, y0 + py, c);
                        }
                    }
                }
                3..=5 | 10 => {
                    for py in 0..4 {
                        for px in 0..4 {
                            let at = base + (py * 4 + px) * 2;
                            let c = match h.format {
                                3 => [b[at + 1], b[at + 1], b[at + 1], b[at]],
                                4 => rgb565(word(at)),
                                5 => rgb5a3(word(at)),
                                _ => pal((word(at) & 0x3FFF) as usize),
                            };
                            set(x0 + px, y0 + py, c);
                        }
                    }
                }
                6 => {
                    for py in 0..4 {
                        for px in 0..4 {
                            let i = (py * 4 + px) * 2;
                            set(
                                x0 + px,
                                y0 + py,
                                [b[base + i + 1], b[base + 32 + i], b[base + 32 + i + 1], b[base + i]],
                            );
                        }
                    }
                }
                _ => {
                    for block in 0..4 {
                        cmpr_block(b, base + block * 8, |px, py, c| {
                            set(x0 + (block % 2) * 4 + px, y0 + (block / 2) * 4 + py, c)
                        })?;
                    }
                }
            }
        }
    }
    Some(RgbaImage { width: h.width, height: h.height, pixels: out })
}
