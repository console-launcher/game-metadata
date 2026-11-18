// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Inflate over a [`ByteSource`] range, streamed: the compressed bytes are read in small chunks and the output is
//! handed to a sink as it appears, so nothing larger than a window is ever held (a zip's ROM entry is hundreds of MB).

use alloc::vec;
use alloc::vec::Vec;

use miniz_oxide::DataFormat;
use miniz_oxide::MZFlush;
use miniz_oxide::MZStatus;
use miniz_oxide::inflate::stream::{InflateState, inflate};

use crate::ByteSource;
use crate::source::fetch;

/// Why a stream could not be inflated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    /// Not valid deflate data, or it ended early.
    Corrupt,
    /// The source failed or the range is outside it.
    Source,
}

// Large pieces: a browser pays per read call (a file read each), not per byte.
const IN_CHUNK: u64 = 256 * 1024;
const OUT_CHUNK: usize = 128 * 1024;

/// What a sink wants next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    Continue,
    Stop,
}

/// Inflates the raw deflate stream (no zlib header) at `source[offset..offset + len]`, passing each piece of output
/// to `sink` until the stream ends or the sink says [`Flow::Stop`].
pub(crate) fn inflate_range<S: ByteSource + ?Sized>(
    source: &mut S,
    offset: u64,
    len: u64,
    format: DataFormat,
    mut sink: impl FnMut(&[u8]) -> Flow,
) -> Result<(), Failure> {
    let end = offset.checked_add(len).filter(|&e| e <= source.size()).ok_or(Failure::Source)?;
    let mut state = InflateState::new_boxed(format);
    let mut out = vec![0u8; OUT_CHUNK];
    let mut at = offset;
    let mut input: Vec<u8> = Vec::new();
    let mut consumed_in_input = 0usize;
    let mut stalls = 0u32;
    loop {
        if consumed_in_input == input.len() && at < end {
            input = fetch(source, at, (end - at).min(IN_CHUNK)).map_err(|_| Failure::Source)?;
            at += input.len() as u64;
            consumed_in_input = 0;
        }
        // Never `Finish`: on a first call it demands the whole output fit in one buffer. A truncated stream shows as
        // calls that make no progress once the input is used up.
        let r = inflate(&mut state, &input[consumed_in_input..], &mut out, MZFlush::None);
        consumed_in_input += r.bytes_consumed;
        stalls = if r.bytes_consumed == 0 && r.bytes_written == 0 { stalls + 1 } else { 0 };
        if stalls > 4 {
            return Err(Failure::Corrupt);
        }
        if r.bytes_written > 0 && sink(&out[..r.bytes_written]) == Flow::Stop {
            return Ok(());
        }
        match r.status {
            Ok(MZStatus::StreamEnd) => return Ok(()),
            Ok(_) => {}
            Err(miniz_oxide::MZError::Buf) => {}
            Err(_) => return Err(Failure::Corrupt),
        }
    }
}

#[allow(dead_code)] // used by the GCZ reader
/// Inflates the raw deflate stream at `source[offset..offset + len]` into at most `max` bytes. `None` when the data is
/// corrupt, truncated, or longer than `max`.
pub(crate) fn inflate_to_vec<S: ByteSource + ?Sized>(
    source: &mut S,
    offset: u64,
    len: u64,
    format: DataFormat,
    max: usize,
) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut too_long = false;
    inflate_range(source, offset, len, format, |chunk| {
        if out.len() + chunk.len() > max {
            too_long = true;
            return Flow::Stop;
        }
        out.extend_from_slice(chunk);
        Flow::Continue
    })
    .ok()?;
    (!too_long).then_some(out)
}
