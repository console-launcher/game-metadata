// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! PICA200 texture formats and their decoders (Morton-tiled 8x8, ETC1 in 4x4 blocks), to top-down RGBA.

use alloc::vec;
use gamemeta_core::RgbaImage;
use gamemeta_core::threeds::pica::morton_xy;

/// A hardware texture format, by register value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// 32 bits.
    Rgba8,
    /// 24 bits.
    Rgb8,
    /// 16 bits.
    Rgba5551,
    /// 16 bits.
    Rgb565,
    /// 16 bits.
    Rgba4,
    /// 16 bits: luminance, alpha.
    La8,
    /// 16 bits: two channels (normal maps).
    Hilo8,
    /// 8 bits luminance.
    L8,
    /// 8 bits alpha.
    A8,
    /// 8 bits: 4 luminance, 4 alpha.
    La4,
    /// 4 bits luminance.
    L4,
    /// 4 bits alpha.
    A4,
    /// ETC1, 4 bits per pixel.
    Etc1,
    /// ETC1 with a 4-bit alpha block, 8 bits per pixel.
    Etc1A4,
}

impl Format {
    const ALL: [Format; 14] = [
        Format::Rgba8,
        Format::Rgb8,
        Format::Rgba5551,
        Format::Rgb565,
        Format::Rgba4,
        Format::La8,
        Format::Hilo8,
        Format::L8,
        Format::A8,
        Format::La4,
        Format::L4,
        Format::A4,
        Format::Etc1,
        Format::Etc1A4,
    ];

    /// The format of register value `id`.
    pub fn of(id: i32) -> Option<Format> {
        usize::try_from(id).ok().and_then(|i| Self::ALL.get(i).copied())
    }

    /// Bits per pixel.
    pub fn bits_per_pixel(self) -> usize {
        match self {
            Format::Rgba8 => 32,
            Format::Rgb8 => 24,
            Format::Rgba5551 | Format::Rgb565 | Format::Rgba4 | Format::La8 | Format::Hilo8 => 16,
            Format::L8 | Format::A8 | Format::La4 | Format::Etc1A4 => 8,
            Format::L4 | Format::A4 | Format::Etc1 => 4,
        }
    }
}

/// Bytes the base level of a `width` x `height` image takes.
pub fn byte_size(width: usize, height: usize, format: Format) -> usize {
    width * height * format.bits_per_pixel() / 8
}

/// Decodes the base level of `data`. Storage row 0 is the picture's top row (as authored) and so is the result's, so the
/// picture reads upright; texture coordinates then run with v = 0 at the bottom. `None` when `data` is shorter than the
/// image needs or the size is not a multiple of 8 (or over 1024).
pub fn decode(data: &[u8], width: usize, height: usize, format: Format) -> Option<RgbaImage> {
    if width == 0
        || height == 0
        || !width.is_multiple_of(8)
        || !height.is_multiple_of(8)
        || width > 1024
        || height > 1024
    {
        return None;
    }
    if data.len() < byte_size(width, height, format) {
        return None;
    }
    let mut out = vec![0u8; width * height * 4];
    if matches!(format, Format::Etc1 | Format::Etc1A4) {
        etc1(data, width, height, format == Format::Etc1A4, &mut out);
    } else {
        linear(data, width, height, format, &mut out);
    }
    Some(RgbaImage { width: width as u32, height: height as u32, pixels: out })
}

fn e5(v: i32) -> u8 {
    ((v << 3) | (v >> 2)) as u8
}

fn e4(v: i32) -> u8 {
    ((v << 4) | v) as u8
}

fn linear(d: &[u8], w: usize, h: usize, f: Format, out: &mut [u8]) {
    let b = |o: usize| i32::from(d[o]);
    let u16 = |o: usize| b(o) | (b(o + 1) << 8);
    let step = if f.bits_per_pixel() >= 8 { f.bits_per_pixel() / 8 } else { 0 };
    let nibble = |px: usize, tx: usize, ty: usize| -> i32 {
        let index = ((ty / 8) * (w / 8) + tx / 8) * 64 + px;
        (i32::from(d[index >> 1]) >> ((index & 1) << 2)) & 15
    };
    let mut i = 0;
    for ty in (0..h).step_by(8) {
        for tx in (0..w).step_by(8) {
            for px in 0..64 {
                let (x, y) = morton_xy(px);
                let o = ((ty + y) * w + tx + x) * 4;
                let rgba: [u8; 4] = match f {
                    Format::Rgba8 => [b(i + 3) as u8, b(i + 2) as u8, b(i + 1) as u8, b(i) as u8],
                    Format::Rgb8 => [b(i + 2) as u8, b(i + 1) as u8, b(i) as u8, 0xff],
                    Format::Rgba5551 => {
                        let v = u16(i);
                        [e5((v >> 11) & 31), e5((v >> 6) & 31), e5((v >> 1) & 31), if v & 1 != 0 { 0xff } else { 0 }]
                    }
                    Format::Rgb565 => {
                        let v = u16(i);
                        let g = (v >> 5) & 63;
                        [e5((v >> 11) & 31), ((g << 2) | (g >> 4)) as u8, e5(v & 31), 0xff]
                    }
                    Format::Rgba4 => {
                        let v = u16(i);
                        [e4((v >> 12) & 15), e4((v >> 8) & 15), e4((v >> 4) & 15), e4(v & 15)]
                    }
                    Format::La8 => [b(i + 1) as u8, b(i + 1) as u8, b(i + 1) as u8, b(i) as u8],
                    Format::Hilo8 => [b(i + 1) as u8, b(i) as u8, 0, 0xff],
                    Format::L8 => [b(i) as u8, b(i) as u8, b(i) as u8, 0xff],
                    Format::A8 => [0xff, 0xff, 0xff, b(i) as u8],
                    Format::La4 => {
                        let v = b(i);
                        [e4(v >> 4), e4(v >> 4), e4(v >> 4), e4(v & 15)]
                    }
                    Format::L4 => {
                        let l = e4(nibble(px, tx, ty));
                        [l, l, l, 0xff]
                    }
                    Format::A4 => [0xff, 0xff, 0xff, e4(nibble(px, tx, ty))],
                    Format::Etc1 | Format::Etc1A4 => [0; 4],
                };
                out[o..o + 4].copy_from_slice(&rgba);
                i += step;
            }
        }
    }
}

const XT: [usize; 4] = [0, 4, 0, 4];
const YT: [usize; 4] = [0, 0, 4, 4];
const ETC1_LUT: [[i32; 4]; 8] = [
    [2, 8, -2, -8],
    [5, 17, -5, -17],
    [9, 29, -9, -29],
    [13, 42, -13, -42],
    [18, 60, -18, -60],
    [24, 80, -24, -80],
    [33, 106, -33, -106],
    [47, 183, -47, -183],
];

fn etc1(d: &[u8], w: usize, h: usize, alpha: bool, out: &mut [u8]) {
    let u64le = |o: usize| u64::from_le_bytes(d[o..o + 8].try_into().unwrap_or([0; 8]));
    let mut i = 0;
    for ty in (0..h).step_by(8) {
        for tx in (0..w).step_by(8) {
            for t in 0..4 {
                let mut alpha_block = u64::MAX;
                if alpha {
                    alpha_block = u64le(i);
                    i += 8;
                }
                let color = u64le(i).swap_bytes();
                i += 8;
                let tile = etc1_tile(color);
                let mut n = 0;
                for py in YT[t]..4 + YT[t] {
                    for px in XT[t]..4 + XT[t] {
                        let o = ((ty + py) * w + tx + px) * 4;
                        out[o..o + 3].copy_from_slice(&tile[n..n + 3]);
                        let shift = (((px & 3) * 4 + (py & 3)) << 2) as u32;
                        let a = ((alpha_block >> shift) & 0xf) as u8;
                        out[o + 3] = (a << 4) | a;
                        n += 4;
                    }
                }
            }
        }
    }
}

/// A 4x4 ETC1 block as R, G, B (+1 pad) per pixel, row-major. `block` is the big-endian block read as a number.
fn etc1_tile(block: u64) -> [u8; 64] {
    let low = (block >> 32) as u32;
    let high = block as u32 as i32;
    let flip = high & 0x100_0000 != 0;
    let diff = high & 0x200_0000 != 0;
    let (mut r1, mut g1, mut b1, mut r2, mut g2, mut b2);
    let sext3 = |v: i32| i32::from(v as u8 as i8) >> 5;
    if diff {
        b1 = high & 0xf8;
        g1 = (high & 0xf800) >> 8;
        r1 = (high & 0xf8_0000) >> 16;
        b2 = (b1 >> 3) + sext3((high & 7) << 5);
        g2 = (g1 >> 3) + sext3((high & 0x700) >> 3);
        r2 = (r1 >> 3) + sext3((high & 0x7_0000) >> 11);
        b1 |= b1 >> 5;
        g1 |= g1 >> 5;
        r1 |= r1 >> 5;
        b2 = ((b2 << 3) | (b2 >> 2)) & 0xff;
        g2 = ((g2 << 3) | (g2 >> 2)) & 0xff;
        r2 = ((r2 << 3) | (r2 >> 2)) & 0xff;
    } else {
        b1 = high & 0xf0;
        g1 = (high & 0xf000) >> 8;
        r1 = (high & 0xf0_0000) >> 16;
        b2 = (high & 0xf) << 4;
        g2 = (high & 0xf00) >> 4;
        r2 = (high & 0xf_0000) >> 12;
        b1 |= b1 >> 4;
        g1 |= g1 >> 4;
        r1 |= r1 >> 4;
        b2 |= b2 >> 4;
        g2 |= g2 >> 4;
        r2 |= r2 >> 4;
    }
    let table1 = ((high as u32 >> 29) & 7) as usize;
    let table2 = ((high as u32 >> 26) & 7) as usize;
    let mut out = [0u8; 64];
    let msb = low << 1;
    let mut put = |x: usize, y: usize, r: i32, g: i32, b: i32, table: usize| {
        let index = (x * 4 + y) as u32;
        let p = if index < 8 {
            ETC1_LUT[table][(((low >> (index + 24)) & 1) + ((msb >> (index + 8)) & 2)) as usize]
        } else {
            ETC1_LUT[table][(((low >> (index + 8)) & 1) + ((msb >> (index - 8)) & 2)) as usize]
        };
        let o = (y * 4 + x) * 4;
        // The block's first colour byte (named b here, from the low bits of the swapped word) is red.
        out[o] = (b + p).clamp(0, 255) as u8;
        out[o + 1] = (g + p).clamp(0, 255) as u8;
        out[o + 2] = (r + p).clamp(0, 255) as u8;
    };
    if !flip {
        for y in 0..4 {
            for x in 0..2 {
                put(x, y, r1, g1, b1, table1);
                put(x + 2, y, r2, g2, b2, table2);
            }
        }
    } else {
        for y in 0..2 {
            for x in 0..4 {
                put(x, y, r1, g1, b1, table1);
                put(x, y + 2, r2, g2, b2, table2);
            }
        }
    }
    out
}
