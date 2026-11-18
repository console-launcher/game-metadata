// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! PICA200 command buffers as the CGFX stores them: a parameter word, then a header word (register in the low 16 bits).

use crate::reader::{R, Reader};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Walks the `words`-word buffer at `at`, calling `write(register, value)` for every parameter. A header's bits 20-30
/// count the parameters that follow it; with bit 31 set they go to the next registers (consecutive writing), otherwise all
/// to the same one. A command pads to an even word count. `skip_empty` ignores an all-zero (padding) command.
fn walk(r: &Reader<'_>, at: i64, words: i64, skip_empty: bool, mut write: impl FnMut(i32, i32)) -> R<()> {
    let mut p = 0i64;
    while p + 2 <= words {
        let param = r.i32(at + 4 * p)?;
        let header = r.i32(at + 4 * p + 4)?;
        if skip_empty && header == 0 && param == 0 {
            p += 2;
            continue;
        }
        let register = header & 0xFFFF;
        let extra = i64::from(((header as u32) >> 20) & 0x7FF);
        let consecutive = (header as u32) >> 31 != 0;
        write(register, param);
        for k in 1..=extra {
            if p + 2 + k > words {
                break;
            }
            let reg = if consecutive { register + k as i32 } else { register };
            write(reg, r.i32(at + 4 * (p + 1 + k))?);
        }
        let used = 2 + extra;
        p += if used % 2 == 0 { used } else { used + 1 };
    }
    Ok(())
}

/// Reads the buffer, returning register to its last value.
pub(crate) fn read(r: &Reader<'_>, at: i64, words: i64) -> R<BTreeMap<i32, i32>> {
    let mut out = BTreeMap::new();
    walk(r, at, words, true, |reg, v| {
        out.insert(reg, v);
    })?;
    Ok(out)
}

/// Every value written to a register in `lo..=hi`, in order (what a LUT upload is).
pub(crate) fn writes(r: &Reader<'_>, at: i64, words: i64, lo: i32, hi: i32) -> R<Vec<i32>> {
    let mut out = Vec::new();
    walk(r, at, words, false, |reg, v| {
        if (lo..=hi).contains(&reg) {
            out.push(v);
        }
    })?;
    Ok(out)
}
