// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! TGA images as Wii U titles ship them: truecolor (type 2) or RLE truecolor (type 10), 24 or 32 bit, either origin.

use alloc::vec::Vec;

use crate::RgbaImage;
use crate::le::u16_at;

const HEADER: usize = 18;
const MAX_SIDE: usize = 4096;

/// `None` for anything else, a file too short for its own header or data, or an RLE packet that overruns the image.
pub(crate) fn decode(b: &[u8]) -> Option<RgbaImage> {
    if b.len() < HEADER {
        return None;
    }
    let (id_length, colour_map, kind) = (usize::from(b[0]), b[1], b[2]);
    let width = usize::from(u16_at(b, 12)?);
    let height = usize::from(u16_at(b, 14)?);
    let bpp = b[16];
    let top_down = b[17] & 0x20 != 0;
    if colour_map != 0 || (kind != 2 && kind != 10) || (bpp != 24 && bpp != 32) {
        return None;
    }
    if width < 1 || height < 1 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let bytes = usize::from(bpp / 8);
    let count = width * height;
    let mut p = HEADER + id_length;
    let raw: Vec<u8> = if kind == 2 {
        b.get(p..p + count * bytes)?.to_vec()
    } else {
        // Grown packet by packet: a short file never allocates the full image up front.
        let mut raw = Vec::new();
        let mut n = 0;
        while n < count {
            let packet = *b.get(p)?;
            p += 1;
            let run = usize::from(packet & 0x7F) + 1;
            if n + run > count {
                return None;
            }
            if packet & 0x80 != 0 {
                let pixel = b.get(p..p + bytes)?;
                for _ in 0..run {
                    raw.extend_from_slice(pixel);
                }
                p += bytes;
            } else {
                raw.extend_from_slice(b.get(p..p + run * bytes)?);
                p += run * bytes;
            }
            n += run;
        }
        raw
    };
    let mut pixels = Vec::with_capacity(count * 4);
    for y in 0..height {
        let row = if top_down { y } else { height - 1 - y };
        for x in 0..width {
            let s = (row * width + x) * bytes;
            let alpha = if bytes == 4 { raw[s + 3] } else { 255 };
            pixels.extend_from_slice(&[raw[s + 2], raw[s + 1], raw[s], alpha]);
        }
    }
    Some(RgbaImage { width: width as u32, height: height as u32, pixels })
}
