// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! LZ11, the compression 3DS banners use for their CGFX.

use crate::reader::{FormatError, R};
use alloc::vec::Vec;

/// Whether `data` at `offset` starts with the LZ11 type byte.
pub fn is_compressed(data: &[u8], offset: usize) -> bool {
    data.get(offset) == Some(&0x11)
}

/// Decompresses the LZ11 stream at `offset`. `max_size` caps the output so a hostile header cannot ask for gigabytes; the
/// output grows only as the stream produces it.
pub fn decompress(data: &[u8], offset: usize, max_size: usize) -> R<Vec<u8>> {
    if !is_compressed(data, offset) || data.len() < offset + 4 {
        return Err(FormatError);
    }
    let byte = |i: usize| data.get(i).map(|&b| usize::from(b)).ok_or(FormatError);
    let mut p = offset + 4;
    let mut n = byte(offset + 1)? | (byte(offset + 2)? << 8) | (byte(offset + 3)? << 16);
    if n == 0 {
        if data.len() < offset + 8 {
            return Err(FormatError);
        }
        let v = i32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]);
        p += 4;
        n = usize::try_from(v).map_err(|_| FormatError)?;
    }
    if n > max_size {
        return Err(FormatError);
    }
    let mut out: Vec<u8> = Vec::new();
    while out.len() < n {
        let flags = byte(p)?;
        p += 1;
        for bit in 0..8 {
            if out.len() >= n {
                break;
            }
            if flags & (0x80 >> bit) == 0 {
                out.push(byte(p)? as u8);
                p += 1;
                continue;
            }
            let b0 = byte(p)?;
            let (length, disp);
            match b0 >> 4 {
                0 => {
                    length = (((b0 & 15) << 4) | (byte(p + 1)? >> 4)) + 0x11;
                    disp = (((byte(p + 1)? & 15) << 8) | byte(p + 2)?) + 1;
                    p += 3;
                }
                1 => {
                    length = (((b0 & 15) << 12) | (byte(p + 1)? << 4) | (byte(p + 2)? >> 4)) + 0x111;
                    disp = (((byte(p + 2)? & 15) << 8) | byte(p + 3)?) + 1;
                    p += 4;
                }
                _ => {
                    length = (b0 >> 4) + 1;
                    disp = (((b0 & 15) << 8) | byte(p + 1)?) + 1;
                    p += 2;
                }
            }
            if disp > out.len() {
                return Err(FormatError);
            }
            for _ in 0..length {
                if out.len() >= n {
                    break;
                }
                out.push(out[out.len() - disp]);
            }
        }
    }
    Ok(out)
}
