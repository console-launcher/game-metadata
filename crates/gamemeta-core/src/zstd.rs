// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Zstd decoding of one frame into a bounded buffer (pure Rust, `ruzstd`).

use alloc::vec;
use alloc::vec::Vec;

use ruzstd::decoding::FrameDecoder;

/// Decodes `input` (one or more frames) into at most `max` bytes; `None` for corrupt data or output over `max`.
pub(crate) fn decompress(input: &[u8], max: usize) -> Option<Vec<u8>> {
    if input.is_empty() || max == 0 {
        return None;
    }
    let mut out = vec![0u8; max];
    let n = FrameDecoder::new().decode_all(input, &mut out).ok()?;
    out.truncate(n);
    Some(out)
}
