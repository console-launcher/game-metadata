// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Plain PNG for RGBA8 pictures: [`encode`] writes 8-bit RGBA, no interlace (what a rendered banner is saved as), and
//! [`decode`] reads 8-bit RGBA or RGB, non-interlaced, any filter (enough for the pictures this project writes and its goldens).

use alloc::vec::Vec;

use crate::RgbaImage;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// `image` as a PNG file (8-bit RGBA, filter 0, zlib level 6); `None` when its pixels are not `width * height * 4` bytes.
pub fn encode(image: &RgbaImage) -> Option<Vec<u8>> {
    let (w, h) = (image.width as usize, image.height as usize);
    if w.checked_mul(h)?.checked_mul(4)? != image.pixels.len() {
        return None;
    }
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for row in image.pixels.chunks(w * 4).take(h) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut out = Vec::from(SIGNATURE);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&image.width.to_be_bytes());
    ihdr.extend_from_slice(&image.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    Some(out)
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

/// Most pixels [`decode`] accepts (64 Mpx).
const MAX_PIXELS: usize = 1 << 26;

/// A PNG's pixels as RGBA8; `None` for anything but 8-bit RGBA or RGB without interlace, or damaged data. CRCs are not checked.
pub fn decode(bytes: &[u8]) -> Option<RgbaImage> {
    if bytes.get(..8)? != SIGNATURE {
        return None;
    }
    let mut at = 8;
    let (mut width, mut height, mut channels) = (0usize, 0usize, 0usize);
    let mut idat = Vec::new();
    while at + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().ok()?) as usize;
        let kind = &bytes[at + 4..at + 8];
        let data = bytes.get(at + 8..at.checked_add(8)?.checked_add(len)?)?;
        match kind {
            b"IHDR" => {
                let d = data.get(..13)?;
                width = u32::from_be_bytes(d[0..4].try_into().ok()?) as usize;
                height = u32::from_be_bytes(d[4..8].try_into().ok()?) as usize;
                channels = match (d[8], d[9], d[12]) {
                    (8, 6, 0) => 4,
                    (8, 2, 0) => 3,
                    _ => return None,
                };
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    if channels == 0 || width == 0 || height == 0 || width.checked_mul(height)? > MAX_PIXELS {
        return None;
    }
    let stride = width * channels;
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&idat, (stride + 1) * height).ok()?;
    if raw.len() != (stride + 1) * height {
        return None;
    }
    let mut cur = alloc::vec![0u8; stride];
    let mut prev = alloc::vec![0u8; stride];
    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in raw.chunks(stride + 1) {
        let filter = row[0];
        for i in 0..stride {
            let a = if i >= channels { cur[i - channels] } else { 0 };
            let b = prev[i];
            let c = if i >= channels { prev[i - channels] } else { 0 };
            let x = row[1 + i];
            cur[i] = match filter {
                0 => x,
                1 => x.wrapping_add(a),
                2 => x.wrapping_add(b),
                3 => x.wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8),
                4 => x.wrapping_add(paeth(a, b, c)),
                _ => return None,
            };
        }
        for px in cur.chunks(channels) {
            pixels.extend_from_slice(px);
            if channels == 3 {
                pixels.push(255);
            }
        }
        core::mem::swap(&mut cur, &mut prev);
    }
    Some(RgbaImage { width: width as u32, height: height as u32, pixels })
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = ((p - i16::from(a)).abs(), (p - i16::from(b)).abs(), (p - i16::from(c)).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_survives_encode_and_decode() {
        let pixels: Vec<u8> = (0..3 * 2 * 4).map(|i| (i * 37 % 256) as u8).collect();
        let image = RgbaImage { width: 3, height: 2, pixels };
        let png = encode(&image).unwrap();
        assert_eq!(&png[..8], &SIGNATURE);
        assert_eq!(decode(&png), Some(image));
    }

    #[test]
    fn the_crc_is_the_png_one() {
        // The IEND chunk's CRC every PNG ends with.
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn a_wrong_pixel_count_does_not_encode_and_garbage_does_not_decode() {
        assert_eq!(encode(&RgbaImage { width: 2, height: 2, pixels: alloc::vec![0; 15] }), None);
        assert_eq!(decode(b"\x89PNG\r\n\x1a\nnot a png"), None);
        assert_eq!(decode(&[]), None);
    }
}
