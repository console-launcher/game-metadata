// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ISO 9660 as the id readers need it: the data sectors of a plain ISO or of a raw 2352-byte image, the primary volume
//! descriptor, and a path walk to one file. One implementation behind the PlayStation serial, the PSP ids and the SFO.

use alloc::vec::Vec;

use super::{get, get_upto};
use crate::ByteSource;
use crate::le::u32_at;

const SECTOR: u64 = 2048;
const RAW_SECTOR: u64 = 2352;
const MAX_DIRECTORY: u64 = 256 * 1024;

/// How the 2048-byte data sectors sit in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    /// Back to back (a plain `.iso`).
    Plain,
    /// 2352-byte sectors with `head` bytes (sync, header, subheader) before the data: a `.bin` track.
    Raw { head: u64 },
}

/// The layout the file's first bytes say: the 12-byte sync pattern means raw sectors, whose mode byte (2) gives the
/// 24-byte head (else 16).
pub(crate) fn layout_of<S: ByteSource + ?Sized>(source: &mut S) -> Layout {
    match get(source, 0, 16) {
        Some(lead) if lead[0] == 0 && lead[1..=10].iter().all(|&b| b == 0xFF) && lead[11] == 0 => {
            Layout::Raw { head: if lead[15] == 2 { 24 } else { 16 } }
        }
        _ => Layout::Plain,
    }
}

/// `len` bytes from logical block `lba` (blocks of `block` bytes in a plain image; always 2048-byte sectors in a raw one),
/// fewer where the source ends; `None` when nothing is there.
fn read_blocks<S: ByteSource + ?Sized>(
    source: &mut S,
    layout: Layout,
    lba: u64,
    len: u64,
    block: u64,
) -> Option<Vec<u8>> {
    match layout {
        Layout::Plain => get_upto(source, lba * block, len),
        Layout::Raw { head } => {
            let mut out = Vec::new();
            let mut sector = lba;
            while (out.len() as u64) < len {
                let want = (len - out.len() as u64).min(SECTOR);
                let Some(chunk) = get_upto(source, sector * RAW_SECTOR + head, want) else { break };
                let short = (chunk.len() as u64) < want;
                out.extend(chunk);
                if short {
                    break;
                }
                sector += 1;
            }
            (!out.is_empty()).then_some(out)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    extent: u32,
    size: u32,
    directory: bool,
}

/// The record named `name` (a trailing `;1` ignored, ASCII case ignored) in the directory `dir`.
fn find_entry<S: ByteSource + ?Sized>(
    source: &mut S,
    layout: Layout,
    block: u64,
    dir: Entry,
    name: &str,
) -> Option<Entry> {
    if !dir.directory || dir.size as i32 <= 0 {
        return None;
    }
    let want = u64::from(dir.size).min(MAX_DIRECTORY);
    let bytes = read_blocks(source, layout, u64::from(dir.extent), want, block)?;
    let mut at = 0usize;
    while at < bytes.len() {
        let len = usize::from(bytes[at]);
        if len == 0 {
            // Records never span a block: the rest of it is padding.
            at = (at / block as usize + 1) * block as usize;
            continue;
        }
        if len < 34 || at + len > bytes.len() {
            return None;
        }
        let name_len = usize::from(bytes[at + 32]);
        if 33 + name_len <= len {
            let raw = &bytes[at + 33..at + 33 + name_len];
            let raw = raw.split(|&b| b == b';').next().unwrap_or(raw);
            if raw.eq_ignore_ascii_case(name.as_bytes()) {
                return Some(Entry {
                    extent: u32_at(&bytes, at + 2)?,
                    size: u32_at(&bytes, at + 10)?,
                    directory: bytes[at + 25] & 2 != 0,
                });
            }
        }
        at += len;
    }
    None
}

/// A file found by [`locate`]: where it starts and how big it is.
pub(crate) struct Found {
    extent: u32,
    pub size: u64,
    block: u64,
}

/// The file at `path` (`SYSTEM.CNF`, `PSP_GAME/PARAM.SFO`: any depth) in the ISO 9660 image `source` holds in `layout`, without
/// reading it. `None` for a missing file, a folder, an empty file or anything damaged.
pub(crate) fn locate<S: ByteSource + ?Sized>(source: &mut S, layout: Layout, path: &str) -> Option<Found> {
    let pvd = read_blocks(source, layout, 16, SECTOR, SECTOR)?;
    if pvd.len() < 190 || pvd[0] != 1 || &pvd[1..6] != b"CD001" {
        return None;
    }
    let size = u16::from_le_bytes([pvd[128], pvd[129]]);
    let block = if layout == Layout::Plain && (512..=8192).contains(&size) { u64::from(size) } else { SECTOR };
    let mut entry = Entry { extent: u32_at(&pvd, 156 + 2)?, size: u32_at(&pvd, 156 + 10)?, directory: true };
    for part in path.split('/') {
        entry = find_entry(source, layout, block, entry, part)?;
    }
    if entry.directory || entry.size as i32 <= 0 || entry.extent as i32 <= -1 {
        return None;
    }
    Some(Found { extent: entry.extent, size: u64::from(entry.size), block })
}

/// The first `limit` bytes of the file at `path` (see [`locate`]).
pub(crate) fn read_file<S: ByteSource + ?Sized>(
    source: &mut S,
    layout: Layout,
    path: &str,
    limit: u64,
) -> Option<Vec<u8>> {
    let found = locate(source, layout, path)?;
    let size = found.size.min(limit);
    let mut data = read_blocks(source, layout, u64::from(found.extent), size, found.block)?;
    data.truncate(size as usize);
    Some(data)
}
