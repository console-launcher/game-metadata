// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Nintendo LZ77 (types 0x10 and 0x11) as used for Wii banner and icon archives.

use alloc::vec::Vec;

/// The most an LZ77 stream may expand to.
pub(crate) const MAX_SIZE: usize = 16 << 20;

/// The data after the 4 byte header at `offset` (type byte, 24 bit size); `None` when it is not LZ77 or is damaged.
pub(crate) fn decompress(data: &[u8], offset: usize, max_size: usize) -> Option<Vec<u8>> {
    let head = data.get(offset..offset.checked_add(4)?)?;
    let kind = head[0];
    if kind != 0x10 && kind != 0x11 {
        return None;
    }
    let mut p = offset + 4;
    let mut size = usize::from(head[1]) | usize::from(head[2]) << 8 | usize::from(head[3]) << 16;
    if size == 0 {
        // The size is in the next 4 bytes.
        let big = u64::from(crate::le::u32_at(data, p)?);
        if big > max_size as u64 {
            return None;
        }
        size = big as usize;
        p += 4;
    }
    if size > max_size {
        return None;
    }
    let at = |i: usize| data.get(i).copied().map(usize::from);
    let mut out: Vec<u8> = Vec::with_capacity(size);
    while out.len() < size {
        let flags = at(p)?;
        p += 1;
        for bit in (0..8).rev() {
            if out.len() >= size {
                break;
            }
            if flags & (1 << bit) == 0 {
                out.push(*data.get(p)?);
                p += 1;
                continue;
            }
            let b0 = at(p)?;
            let b1 = at(p + 1)?;
            let (length, distance) = if kind == 0x10 || b0 >> 4 > 1 {
                p += 2;
                (if kind == 0x10 { (b0 >> 4) + 3 } else { (b0 >> 4) + 1 }, (((b0 & 0xF) << 8) | b1) + 1)
            } else if b0 >> 4 == 0 {
                let b2 = at(p + 2)?;
                p += 3;
                ((((b0 & 0xF) << 4) | (b1 >> 4)) + 0x11, (((b1 & 0xF) << 8) | b2) + 1)
            } else {
                let (b2, b3) = (at(p + 2)?, at(p + 3)?);
                p += 4;
                ((((b0 & 0xF) << 12) | (b1 << 4) | (b2 >> 4)) + 0x111, (((b2 & 0xF) << 8) | b3) + 1)
            };
            if distance > out.len() {
                return None;
            }
            for _ in 0..length.min(size - out.len()) {
                out.push(out[out.len() - distance]);
            }
        }
    }
    Some(out)
}
