// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! An ExeFS `banner` (CBMD) the HOME Menu reads: the 0x88-byte header, the common CGFX LZ11-compressed, optional
//! per-language CGFX, then a short silent BCWAV (DSP-ADPCM mono, every frame zero) at a 0x20 boundary, as real banners lay
//! it out.

use std::collections::{HashMap, VecDeque};

const HEADER: usize = 0x88;
/// Language slots in the header (0x0C..0x84).
pub const LANGUAGE_SLOTS: usize = 30;

/// A CBMD of `cgfx` alone.
pub fn write(cgfx: &[u8]) -> Vec<u8> {
    write_localized(cgfx, &[])
}

/// A CBMD of `cgfx` that every language slot also points at: one scene for every language (a title that reads the same in
/// all of them), stored once.
pub fn write_every_language(cgfx: &[u8]) -> Vec<u8> {
    let mut out = write(cgfx);
    for slot in 0..LANGUAGE_SLOTS {
        out[0xC + 4 * slot..0x10 + 4 * slot].copy_from_slice(&(HEADER as u32).to_le_bytes());
    }
    out
}

/// A CBMD of `common` plus one CGFX per `(slot, cgfx)` (slot 0..30), each LZ11-compressed in slot order.
pub fn write_localized(common: &[u8], localized: &[(usize, &[u8])]) -> Vec<u8> {
    let mut out = vec![0u8; HEADER];
    out[0..4].copy_from_slice(b"CBMD");
    out[8..12].copy_from_slice(&(HEADER as u32).to_le_bytes());
    out.extend_from_slice(&lz11(common));
    for &(slot, cgfx) in localized {
        assert!(slot < LANGUAGE_SLOTS, "language slot {slot}");
        let at = out.len();
        out[0xC + 4 * slot..0x10 + 4 * slot].copy_from_slice(&(at as u32).to_le_bytes());
        out.extend_from_slice(&lz11(cgfx));
    }
    let cwav_at = (out.len() + 0x1F) & !0x1F;
    out.resize(cwav_at, 0);
    out[0x84..0x88].copy_from_slice(&(cwav_at as u32).to_le_bytes());
    out.extend_from_slice(&silent_cwav(1400));
    out
}

/// A BCWAV of `samples` silent DSP-ADPCM samples, mono at 15 kHz: header, INFO (one channel, zero coefficients), DATA.
pub fn silent_cwav(samples: usize) -> Vec<u8> {
    let frames = samples.div_ceil(14);
    let info = 0x80usize;
    let data = 0x20 + ((frames * 8 + 0x1F) & !0x1F);
    let size = 0x40 + info + data;
    let mut b = crate::Le::new(size);
    b.bytes(0, b"CWAV").u16(4, 0xFEFF).u16(6, 0x40).u32(8, 0x0201_0000).u32(0xC, size as u32).u16(0x10, 2);
    b.u16(0x14, 0x7000).u32(0x18, 0x40).u32(0x1C, info as u32);
    b.u16(0x20, 0x7001).u32(0x24, (0x40 + info) as u32).u32(0x28, data as u32);
    let i = 0x40;
    b.bytes(i, b"INFO").u32(i + 4, info as u32);
    b.0[i + 8] = 2; // DSP-ADPCM, no loop
    b.u32(i + 0xC, 15000).u32(i + 0x10, 0).u32(i + 0x14, samples as u32);
    b.u32(i + 0x1C, 1).u16(i + 0x20, 0x7100).u32(i + 0x24, 0xC); // one channel, 0xC past the table
    let ch = i + 0x28;
    b.u16(ch, 0x1F00).u32(ch + 4, 0x18); // samples, from the DATA body
    b.u16(ch + 8, 0x0300).u32(ch + 0xC, 0x14); // ADPCM info after this channel; coefficients and contexts all zero
    let d = 0x40 + info;
    b.bytes(d, b"DATA").u32(d + 4, data as u32);
    b.0
}

/// LZ11 (the 3DS's 0x11 LZ77 variant): greedy matches of 3 to 16 bytes within a 4 KiB window, else literals; the oldest of
/// the last 32 candidates wins a tie. Padded to 4 bytes.
pub fn lz11(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + src.len() + src.len() / 8 + 8);
    let n = src.len();
    out.extend_from_slice(&[0x11, n as u8, (n >> 8) as u8, (n >> 16) as u8]);
    let key = |p: usize| (u32::from(src[p]) << 16) | (u32::from(src[p + 1]) << 8) | u32::from(src[p + 2]);
    let mut heads: HashMap<u32, VecDeque<usize>> = HashMap::new();
    let remember = |p: usize, heads: &mut HashMap<u32, VecDeque<usize>>| {
        if p + 2 >= n {
            return;
        }
        let q = heads.entry(key(p)).or_default();
        q.push_back(p);
        if q.len() > 32 {
            q.pop_front();
        }
    };
    let mut pos = 0;
    while pos < n {
        let flag_at = out.len();
        out.push(0);
        let mut flags = 0u8;
        for bit in (0..8).rev() {
            if pos >= n {
                break;
            }
            let (mut best_len, mut best_dist) = (0usize, 0usize);
            if pos + 2 < n
                && let Some(q) = heads.get(&key(pos))
            {
                for &cand in q {
                    let dist = pos - cand;
                    if !(1..=0x1000).contains(&dist) {
                        continue;
                    }
                    let mut len = 0;
                    while len < 16 && pos + len < n && src[cand + len] == src[pos + len] {
                        len += 1;
                    }
                    if len > best_len {
                        (best_len, best_dist) = (len, dist);
                    }
                }
            }
            if best_len >= 3 {
                flags |= 1 << bit;
                let (l, d) = (best_len - 1, best_dist - 1);
                out.push(((l << 4) | (d >> 8)) as u8);
                out.push(d as u8);
                for _ in 0..best_len {
                    remember(pos, &mut heads);
                    pos += 1;
                }
            } else {
                out.push(src[pos]);
                remember(pos, &mut heads);
                pos += 1;
            }
        }
        out[flag_at] = flags;
    }
    out.resize((out.len() + 3) & !3, 0);
    out
}
