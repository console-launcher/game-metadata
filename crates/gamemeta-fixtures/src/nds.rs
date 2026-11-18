// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Synthetic DS banners, ROMs and zips, mirroring the original reader's `NdsFixtures` byte for byte.

use crate::Le;

/// The largest banner block (version 0x0103).
pub const MAX_BANNER: usize = 0x23C0;

/// CRC-16 (modbus, initial 0xFFFF) over `bytes[from..min(to, len)]`, as the DS firmware computes it.
pub fn crc16(bytes: &[u8], from: usize, to: usize) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in bytes.get(from..to.min(bytes.len())).unwrap_or(&[]) {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

/// 16 BGR555 colours: entry i has red = i, green = 0, blue = 31 - i (entry 0 is transparent whatever it holds).
pub fn palette() -> Vec<u8> {
    let mut p = vec![0u8; 32];
    for i in 0..16u16 {
        let c = i | ((31 - i) << 10);
        p[i as usize * 2..i as usize * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    p
}

/// A 32x32 icon bitmap where pixel (x, y) has index `index_of(x, y)`, in the DS's 8x8 tile order.
pub fn bitmap_with(index_of: impl Fn(usize, usize) -> usize) -> Vec<u8> {
    let mut out = vec![0u8; 0x200];
    for ty in 0..4 {
        for tx in 0..4 {
            for row in 0..8 {
                for col in 0..8 {
                    let idx = index_of(tx * 8 + col, ty * 8 + row) as u8;
                    let at = (ty * 4 + tx) * 32 + row * 4 + col / 2;
                    out[at] |= if col % 2 == 0 { idx } else { idx << 4 };
                }
            }
        }
    }
    out
}

/// The default bitmap: index `(x + y) % 16`.
pub fn bitmap() -> Vec<u8> {
    bitmap_with(|x, y| (x + y) % 16)
}

/// A 256-byte UTF-16LE title field.
pub fn title(s: &str) -> Vec<u8> {
    let mut b = vec![0u8; 0x100];
    for (i, u) in s.encode_utf16().enumerate() {
        b[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes());
    }
    b
}

/// A banner block of `version` with English title `english`; a DSi 0x0103 one gets an animation of `sequence` tokens
/// over 8 bitmaps (bitmap n is filled with index n + 1).
pub fn banner(version: u16, english: &str, sequence: &[u16]) -> Vec<u8> {
    let titles: Vec<String> =
        ["", english, "", "", "", "", "Chinese", "Korean"].into_iter().map(String::from).collect();
    let frames: Vec<(Vec<u8>, Vec<u8>)> = (0..8).map(|n| (bitmap_with(|_, _| n + 1), palette())).collect();
    banner_with(version, &titles, &bitmap(), &palette(), &frames, sequence)
}

/// A banner block of `version` with `titles` (Japanese, English, French, German, Italian, Spanish, then Chinese from version 2
/// and Korean from version 3; missing ones are empty), the icon `bitmap` and `palette`, and, for a DSi 0x0103 one, up to 8
/// animation `frames` (bitmap, palette) played by `sequence` tokens (duration in the low byte, bitmap index in bits 8-10,
/// palette index in bits 11-13). Every CRC is filled in.
pub fn banner_with(
    version: u16,
    titles: &[String],
    bitmap: &[u8],
    palette: &[u8],
    frames: &[(Vec<u8>, Vec<u8>)],
    sequence: &[u16],
) -> Vec<u8> {
    let size = if version == 0x0103 { MAX_BANNER } else { 0xA40 };
    let mut b = Le::new(size);
    b.u16(0, version).bytes(0x20, bitmap).bytes(0x220, palette);
    let known = match version {
        1 => 6,
        2 => 7,
        _ => 8,
    };
    for lang in 0..known {
        let at = if lang < 6 { 0x240 + lang * 0x100 } else { 0x840 + (lang - 6) * 0x100 };
        b.bytes(at, &title(titles.get(lang).map_or("", String::as_str)));
    }
    if version == 0x0103 {
        for (n, (bitmap, palette)) in frames.iter().enumerate() {
            b.bytes(0x1240 + n * 0x200, bitmap);
            b.bytes(0x2240 + n * 0x20, palette);
        }
        for (i, token) in sequence.iter().enumerate() {
            b.u16(0x2340 + i * 2, *token);
        }
    }
    let crc = |from, to| crc16(&b.0, from, to);
    let (c1, c2, c3, c4) = (crc(0x20, 0x840), crc(0x20, 0x940), crc(0x20, 0xA40), crc(0x1240, 0x23C0));
    b.u16(2, c1);
    if version >= 2 {
        b.u16(4, c2);
    }
    if version >= 3 {
        b.u16(6, c3);
    }
    if version == 0x0103 {
        b.u16(8, c4);
    }
    b.0
}

/// The default test banner: version 1, English "Test Game\nSubtitle\nMaker".
pub fn default_banner() -> Vec<u8> {
    banner(1, "Test Game\nSubtitle\nMaker", &[])
}

/// A ROM: a 0x200 header whose banner offset is `banner_at`, zeros up to there, then `banner`, then `tail` bytes.
pub fn rom(banner: &[u8], banner_at: usize, tail: usize) -> Vec<u8> {
    let mut rom = Le::new(banner_at + banner.len() + tail);
    rom.u32(0x68, banner_at as u32).bytes(banner_at, banner);
    rom.0
}

/// A zip of `entry_name` (holding `content`, deflated or stored) after a `readme.txt` entry, as the original test builds it.
pub fn zip(entry_name: &str, content: &[u8], deflate: bool) -> Vec<u8> {
    crate::zip::zip(&[("readme.txt", b"hi", true), (entry_name, content, deflate)])
}

/// The committed DSi sample: game code `SMPL`, maker `01`, a DSi unit code and an animated icon of two steps.
pub fn sample_nds() -> Vec<u8> {
    let mut rom = rom(
        &banner(0x0103, "Sample Game\nSubtitle\nExample Pub", &[0x0A | (1 << 8), 0x08 | (2 << 8), 0]),
        0x4000,
        0x200,
    );
    rom[..12].copy_from_slice(b"SAMPLE GAME\0");
    rom[0x0C..0x10].copy_from_slice(b"SMPL");
    rom[0x10..0x12].copy_from_slice(b"01");
    rom[0x12] = 3;
    rom
}
