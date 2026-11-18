// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The header-only readers that are a few lines each: CHD, a one-ROM zip, a Switch NSP.

use alloc::string::String;
use alloc::vec::Vec;

use super::get;
use crate::ByteSource;
use crate::le::{u16_at, u32_at};

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| alloc::format!("{b:02x}")).collect()
}

/// The raw SHA-1 of the data inside a CHD v5, from its header (offset 0x40, lower-case hex); `None` for other versions
/// or an all-zero field.
pub(crate) fn chd_sha1<S: ByteSource + ?Sized>(source: &mut S) -> Option<String> {
    let head = get(source, 0, 124)?;
    if &head[..8] != b"MComprHD" {
        return None;
    }
    let big = |at: usize| i32::from_be_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]);
    if big(8) < 124 || big(12) != 5 {
        return None;
    }
    let sha = &head[0x40..0x54];
    (!sha.iter().all(|&b| b == 0)).then(|| hex_lower(sha))
}

/// The CRC-32 (eight hex digits) and size of the one ROM in a zip, from the central directory; `None` for zero or several
/// files, a zip64 size, or a damaged directory.
pub(crate) fn zip_entry<S: ByteSource + ?Sized>(source: &mut S) -> Option<(String, u64)> {
    let size = source.size();
    let tail_len = size.min(66_000);
    if tail_len < 22 {
        return None;
    }
    let tail = get(source, size - tail_len, tail_len)?;
    let eocd = (0..=tail.len() - 22).rev().find(|&i| u32_at(&tail, i) == Some(0x0605_4B50))?;
    let count = u16_at(&tail, eocd + 10)?;
    let cd_size = u64::from(u32_at(&tail, eocd + 12)?);
    let cd_offset = u64::from(u32_at(&tail, eocd + 16)?);
    if count == 0 || cd_size > 1 << 20 || cd_offset + cd_size > size {
        return None;
    }
    let cd = get(source, cd_offset, cd_size)?;
    let mut at = 0usize;
    let mut found = None;
    for _ in 0..count {
        if at + 46 > cd.len() || u32_at(&cd, at)? != 0x0201_4B50 {
            return None;
        }
        let (crc, rom_size) = (u32_at(&cd, at + 16)?, u32_at(&cd, at + 24)?);
        let name_len = usize::from(u16_at(&cd, at + 28)?);
        let skip = name_len + usize::from(u16_at(&cd, at + 30)?) + usize::from(u16_at(&cd, at + 32)?);
        if at + 46 + name_len > cd.len() {
            return None;
        }
        let directory = name_len > 0 && cd[at + 46 + name_len - 1] == b'/';
        if !directory {
            if found.is_some() || rom_size == u32::MAX {
                return None;
            }
            found = Some((alloc::format!("{crc:08x}"), u64::from(rom_size)));
        }
        at += 46 + skip;
    }
    found
}

fn hex_run(s: &str, from: usize, len: usize) -> Option<&str> {
    let part = s.get(from..from + len)?;
    part.bytes().all(|b| b.is_ascii_hexdigit()).then_some(part)
}

/// The title id (16 upper-case hex digits) of a Switch NSP from the file names in its PFS0 table: a ticket's rights id
/// (`<title id><key generation>.tik`), else a `.cnmt.nca` / `.cnmt.xml` named by title id. An XCI is encrypted and not read.
pub(crate) fn nsp_title_id<S: ByteSource + ?Sized>(source: &mut S) -> Option<String> {
    let head = get(source, 0, 16)?;
    if &head[..4] != b"PFS0" {
        return None;
    }
    let count = u32_at(&head, 4)? as i32;
    let table = u32_at(&head, 8)? as i32;
    if !(1..=4096).contains(&count) || !(1..=1 << 20).contains(&table) {
        return None;
    }
    let all = get(source, 0, 16 + 24 * count as u64 + table as u64)?;
    let strings = 16 + 24 * count as usize;
    let names: Vec<String> = (0..count as usize)
        .filter_map(|i| {
            let off = strings as i64 + i64::from(u32_at(&all, 16 + 24 * i + 16)? as i32);
            if off < strings as i64 || off >= all.len() as i64 {
                return None;
            }
            let tail = &all[off as usize..];
            let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
            Some(tail[..end].iter().map(|&b| char::from(b)).collect())
        })
        .collect();
    let ticket = names.iter().find_map(|n| {
        let id = hex_run(n, 0, 16)?;
        hex_run(n, 16, 16)?;
        (n.len() == 36 && n.ends_with(".tik")).then_some(id)
    });
    let cnmt = || {
        names.iter().find_map(|n| {
            let id = hex_run(n, 0, 16)?;
            matches!(&n[16..], ".cnmt.nca" | ".cnmt.xml").then_some(id)
        })
    };
    ticket.or_else(cnmt).map(str::to_uppercase)
}
