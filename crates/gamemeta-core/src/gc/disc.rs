// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What GameCube and Wii discs share: the region a game id names and the file system table (FST).

use alloc::vec::Vec;

use crate::le::be32_at;
use crate::source::fetch_opt;
use crate::{ByteSource, ReadError, Region};

/// The disc region a game id's 4th character names (GameCube and Wii alike).
pub(crate) fn regions(code: Option<u8>) -> Vec<Region> {
    match code {
        Some(b'J') => alloc::vec![Region::Japan],
        Some(b'E') => alloc::vec![Region::NorthAmerica],
        Some(b'P' | b'D' | b'F' | b'I' | b'S' | b'H' | b'X' | b'Y' | b'Z' | b'L' | b'M') => alloc::vec![Region::Europe],
        Some(b'U') => alloc::vec![Region::Australia],
        Some(b'K') => alloc::vec![Region::Korea],
        Some(b'W') => alloc::vec![Region::Taiwan],
        Some(b'C') => alloc::vec![Region::China],
        _ => Vec::new(),
    }
}

/// The 6-character game id at the start of a disc header, when all of it is `A-Z` or `0-9`.
pub(crate) fn game_id(header: &[u8]) -> Option<&[u8]> {
    header.get(..6).filter(|s| s.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
}

const MAX_FST: u64 = 8 * 1024 * 1024;

/// The first `max_size` bytes of the file `name` in the root folder (a same-named file in a subfolder is never used).
/// `disc` is the plain disc, or the partition's data; `header` its first 0x440 bytes. `shift` is 0 for GameCube and 2
/// for Wii (offsets stored shifted right by 2).
pub(crate) fn read_root_file<S: ByteSource + ?Sized>(
    disc: &mut S,
    header: &[u8],
    name: &str,
    max_size: u64,
    shift: u32,
) -> Result<Option<Vec<u8>>, ReadError> {
    let fst_offset = u64::from(be32_at(header, 0x424).unwrap_or(0)) << shift;
    let fst_size = u64::from(be32_at(header, 0x428).unwrap_or(0)) << shift;
    if fst_offset == 0 || !(12..=MAX_FST).contains(&fst_size) {
        return Ok(None);
    }
    let Some(fst) = fetch_opt(disc, fst_offset, fst_size)? else { return Ok(None) };
    let count = u64::from(be32_at(&fst, 8).unwrap_or(0));
    if count < 1 || count * 12 > fst.len() as u64 {
        return Ok(None);
    }
    let names = count as usize * 12;
    let mut i = 1u64;
    while i < count {
        let at = i as usize * 12;
        if fst[at] != 0 {
            // A folder: its end index is the next entry to look at, so its files are skipped.
            let next = u64::from(be32_at(&fst, at + 8).unwrap_or(0));
            if next <= i {
                return Ok(None);
            }
            i = next;
            continue;
        }
        let name_at = names + (be32_at(&fst, at).unwrap_or(0) & 0xFF_FFFF) as usize;
        if let Some(tail) = fst.get(name_at..) {
            let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
            if tail[..end].eq_ignore_ascii_case(name.as_bytes()) {
                let offset = u64::from(be32_at(&fst, at + 4).unwrap_or(0)) << shift;
                let length = u64::from(be32_at(&fst, at + 8).unwrap_or(0)).min(max_size);
                return fetch_opt(disc, offset, length);
            }
        }
        i += 1;
    }
    Ok(None)
}
