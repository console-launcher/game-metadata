// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! RGBA pictures into PICA texture formats, the inverse of `banner3ds_scene::pica_textures::decode`: 8x8 tiles in Morton
//! order, row 0 the picture's top row. ETC1 is a plain individual-mode encoder (each 2x4 half: its mean colour, then the
//! table and per-pixel modifiers that fit best); both readers decode the same bits.

use banner3ds_scene::pica_textures::{Format, byte_size};
use gamemeta_core::RgbaImage;
use gamemeta_core::threeds::pica::morton_xy;

/// `img` encoded as `f`. Panics unless both sides are multiples of 8.
pub fn encode(img: &RgbaImage, f: Format) -> Vec<u8> {
    let (w, h) = (img.width as usize, img.height as usize);
    assert!(w.is_multiple_of(8) && h.is_multiple_of(8), "texture sides must be multiples of 8");
    let mut out = vec![0u8; byte_size(w, h, f)];
    if matches!(f, Format::Etc1 | Format::Etc1A4) {
        etc1(img, f == Format::Etc1A4, &mut out);
        return out;
    }
    let px = |x: usize, y: usize, c: usize| img.pixels[(y * w + x) * 4 + c] as i32;
    let bpp = f.bits_per_pixel();
    let step = if bpp >= 8 { bpp / 8 } else { 0 };
    let (mut i, mut nib) = (0usize, 0usize);
    for ty in (0..h).step_by(8) {
        for tx in (0..w).step_by(8) {
            for p in 0..64 {
                let (x0, y0) = morton_xy(p);
                let (x, y) = (tx + x0, ty + y0);
                let (r, g, b, a) = (px(x, y, 0), px(x, y, 1), px(x, y, 2), px(x, y, 3));
                let l = (r * 77 + g * 150 + b * 29 + 128) >> 8;
                let mut u16 = |v: i32| {
                    out[i] = v as u8;
                    out[i + 1] = (v >> 8) as u8;
                };
                match f {
                    Format::Rgba8 => {
                        out[i..i + 4].copy_from_slice(&[a as u8, b as u8, g as u8, r as u8]);
                    }
                    Format::Rgb8 => out[i..i + 3].copy_from_slice(&[b as u8, g as u8, r as u8]),
                    Format::Rgba5551 => {
                        u16(((r >> 3) << 11) | ((g >> 3) << 6) | ((b >> 3) << 1) | i32::from(a >= 128));
                    }
                    Format::Rgb565 => u16(((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)),
                    Format::Rgba4 => u16(((r >> 4) << 12) | ((g >> 4) << 8) | ((b >> 4) << 4) | (a >> 4)),
                    Format::La8 => out[i..i + 2].copy_from_slice(&[a as u8, l as u8]),
                    Format::Hilo8 => out[i..i + 2].copy_from_slice(&[g as u8, r as u8]),
                    Format::L8 => out[i] = l as u8,
                    Format::A8 => out[i] = a as u8,
                    Format::La4 => out[i] = (((l >> 4) << 4) | (a >> 4)) as u8,
                    Format::L4 | Format::A4 => {
                        let v = (if f == Format::L4 { l } else { a }) >> 4;
                        out[nib >> 1] |= (v << ((nib & 1) << 2)) as u8;
                        nib += 1;
                    }
                    Format::Etc1 | Format::Etc1A4 => unreachable!(),
                }
                i += step;
            }
        }
    }
    out
}

const LUT: [[i32; 4]; 8] = [
    [2, 8, -2, -8],
    [5, 17, -5, -17],
    [9, 29, -9, -29],
    [13, 42, -13, -42],
    [18, 60, -18, -60],
    [24, 80, -24, -80],
    [33, 106, -33, -106],
    [47, 183, -47, -183],
];

fn etc1(img: &RgbaImage, alpha: bool, out: &mut [u8]) {
    let w = img.width as usize;
    let px = |x: usize, y: usize, c: usize| img.pixels[(y * w + x) * 4 + c] as i32;
    let mut i = 0usize;
    let mut put = |v: u64, out: &mut [u8]| {
        out[i..i + 8].copy_from_slice(&v.to_le_bytes());
        i += 8;
    };
    for ty in (0..img.height as usize).step_by(8) {
        for tx in (0..w).step_by(8) {
            for t in 0..4 {
                let (bx, by) = (tx + (t & 1) * 4, ty + (t >> 1) * 4);
                if alpha {
                    let mut a = 0u64;
                    for y in 0..4 {
                        for x in 0..4 {
                            a |= ((px(bx + x, by + y, 3) >> 4) as u64) << ((x * 4 + y) * 4);
                        }
                    }
                    put(a, out);
                }
                // Individual mode, no flip: half 0 = columns 0-1, half 1 = columns 2-3.
                let (mut word, mut msb, mut lsb) = (0u64, 0u32, 0u32);
                for half in 0..2 {
                    let base: [i32; 3] = core::array::from_fn(|c| {
                        let mut sum = 0;
                        for y in 0..4 {
                            for x in 0..2 {
                                sum += px(bx + half * 2 + x, by + y, c);
                            }
                        }
                        ((sum as f32 / 8.0 / 17.0 + 0.5) as i32).clamp(0, 15)
                    });
                    let (mut best_table, mut best_err, mut best_idx) = (0usize, i64::MAX, [0usize; 8]);
                    for (table, lut) in LUT.iter().enumerate() {
                        let mut err = 0i64;
                        let mut idx = [0usize; 8];
                        for y in 0..4 {
                            for x in 0..2 {
                                let mut be = i64::MAX;
                                for (m, &d) in lut.iter().enumerate() {
                                    let mut e = 0i64;
                                    for (c, &b) in base.iter().enumerate() {
                                        let v = (b * 17 + d).clamp(0, 255) - px(bx + half * 2 + x, by + y, c);
                                        e += i64::from(v * v);
                                    }
                                    if e < be {
                                        be = e;
                                        idx[x * 4 + y] = m;
                                    }
                                }
                                err += be;
                            }
                        }
                        if err < best_err {
                            (best_err, best_table, best_idx) = (err, table, idx);
                        }
                    }
                    let shift = if half == 0 { 4 } else { 0 };
                    word |= (base[0] as u64) << (56 + shift)
                        | (base[1] as u64) << (48 + shift)
                        | (base[2] as u64) << (40 + shift);
                    word |= (best_table as u64) << if half == 0 { 37 } else { 34 };
                    for x in 0..2 {
                        for y in 0..4 {
                            let p = (half * 2 + x) * 4 + y;
                            let m = best_idx[x * 4 + y];
                            if m & 2 != 0 {
                                msb |= 1 << p;
                            }
                            if m & 1 != 0 {
                                lsb |= 1 << p;
                            }
                        }
                    }
                }
                word |= u64::from(msb) << 16 | u64::from(lsb);
                put(word, out);
            }
        }
    }
}
