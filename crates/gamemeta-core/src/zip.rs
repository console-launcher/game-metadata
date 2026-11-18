// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Zip central directory walk: find an entry and stream its decompressed bytes. Stored and deflated entries only.

use alloc::string::String;
use alloc::vec::Vec;

use miniz_oxide::DataFormat;

use crate::deflate::{self, Failure, Flow};
use crate::le::{u16_at, u32_at};
use crate::source::fetch;
use crate::{ByteSource, ReadError};

const EOCD_SIG: u32 = 0x0605_4B50;
const CD_SIG: u32 = 0x0201_4B50;
const LOCAL_SIG: u32 = 0x0403_4B50;
const MAX_TAIL: u64 = 22 + 0xFFFF;
const MAX_DIRECTORY: u64 = 1 << 20;

/// A central directory entry.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub name: String,
    pub method: u16,
    local_offset: u64,
    compressed: u64,
    /// The CRC-32 of the decompressed bytes, as the directory records it.
    pub crc: u32,
    /// The uncompressed size the directory declares.
    pub size: u64,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

/// The first `limit` entries of the archive, in directory order. `Ok(None)` when the source is not a zip.
pub(crate) fn entries<S: ByteSource + ?Sized>(source: &mut S, limit: usize) -> Result<Option<Vec<Entry>>, ReadError> {
    let size = source.size();
    let tail_len = size.min(MAX_TAIL);
    let tail = fetch(source, size - tail_len, tail_len)?;
    let Some(eocd) = (0..=tail.len().saturating_sub(22)).rev().find(|&i| u32_at(&tail, i) == Some(EOCD_SIG)) else {
        return Ok(None);
    };
    let (Some(count), Some(cd_size), Some(cd_at)) =
        (u16_at(&tail, eocd + 10), u32_at(&tail, eocd + 12), u32_at(&tail, eocd + 16))
    else {
        return Ok(None);
    };
    let (cd_size, cd_at) = (u64::from(cd_size), u64::from(cd_at));
    if cd_at.checked_add(cd_size).is_none_or(|e| e > size) {
        return Ok(None);
    }
    let cd = fetch(source, cd_at, cd_size.min(MAX_DIRECTORY))?;
    let mut out = Vec::new();
    let mut at = 0usize;
    for _ in 0..usize::from(count).min(limit) {
        if u32_at(&cd, at) != Some(CD_SIG) {
            return Ok(if out.is_empty() { None } else { Some(out) });
        }
        let (
            Some(method),
            Some(crc),
            Some(compressed),
            Some(size),
            Some(name_len),
            Some(extra),
            Some(comment),
            Some(local),
        ) = (
            u16_at(&cd, at + 10),
            u32_at(&cd, at + 16),
            u32_at(&cd, at + 20),
            u32_at(&cd, at + 24),
            u16_at(&cd, at + 28),
            u16_at(&cd, at + 30),
            u16_at(&cd, at + 32),
            u32_at(&cd, at + 42),
        )
        else {
            break;
        };
        let Some(name) = cd.get(at + 46..at + 46 + usize::from(name_len)) else { break };
        out.push(Entry {
            name: String::from_utf8_lossy(name).into_owned(),
            method,
            local_offset: u64::from(local),
            compressed: u64::from(compressed),
            crc,
            size: u64::from(size),
        });
        at += 46 + usize::from(name_len) + usize::from(extra) + usize::from(comment);
    }
    Ok(Some(out))
}

impl Entry {
    /// True when the entry's bytes are stored as they are, so any range of them can be read without reading the rest.
    pub fn is_stored(&self) -> bool {
        self.method == 0
    }
}

/// Reads `buf.len()` bytes at `at` inside a stored `entry`, without reading anything before them.
pub(crate) fn read_stored<S: ByteSource + ?Sized>(
    source: &mut S,
    entry: &Entry,
    at: u64,
    buf: &mut [u8],
) -> Result<(), Failure> {
    let data_at = data_start(source, entry)?;
    at.checked_add(buf.len() as u64).filter(|&e| e <= entry.compressed).ok_or(Failure::Source)?;
    source.read_at(data_at.checked_add(at).ok_or(Failure::Source)?, buf).map_err(|_| Failure::Source)
}

/// Where the entry's data starts: after its local header.
fn data_start<S: ByteSource + ?Sized>(source: &mut S, entry: &Entry) -> Result<u64, Failure> {
    let local = fetch(source, entry.local_offset, 30).map_err(|_| Failure::Source)?;
    if u32_at(&local, 0) != Some(LOCAL_SIG) {
        return Err(Failure::Corrupt);
    }
    let skip = 30 + u64::from(u16_at(&local, 26).unwrap_or(0)) + u64::from(u16_at(&local, 28).unwrap_or(0));
    Ok(entry.local_offset + skip)
}

/// Streams the decompressed bytes of `entry` to `sink`. A stored or deflated entry; anything else is corrupt here.
pub(crate) fn stream<S: ByteSource + ?Sized>(
    source: &mut S,
    entry: &Entry,
    mut sink: impl FnMut(&[u8]) -> Flow,
) -> Result<(), Failure> {
    let data_at = data_start(source, entry)?;
    match entry.method {
        0 => {
            let end = data_at.checked_add(entry.compressed).filter(|&e| e <= source.size()).ok_or(Failure::Source)?;
            let mut at = data_at;
            while at < end {
                let chunk = fetch(source, at, (end - at).min(256 * 1024)).map_err(|_| Failure::Source)?;
                at += chunk.len() as u64;
                if sink(&chunk) == Flow::Stop {
                    break;
                }
            }
            Ok(())
        }
        8 => deflate::inflate_range(source, data_at, entry.compressed, DataFormat::Raw, sink),
        _ => Err(Failure::Corrupt),
    }
}
