// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A U8 archive (magic 0x55AA382D): a tree of folders and files in one buffer.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::le::be32_at;

const MAGIC: u32 = 0x55AA_382D;
const MAX_NODES: u32 = 20_000;

/// A file: `offset` is an index into the buffer the archive was parsed from.
pub(crate) struct Entry {
    pub path: String,
    pub offset: usize,
    pub size: usize,
}

pub(crate) struct U8Archive<'a> {
    bytes: Cow<'a, [u8]>,
    pub entries: Vec<Entry>,
}

impl<'a> U8Archive<'a> {
    pub(crate) fn parse(bytes: Cow<'a, [u8]>, offset: usize) -> Option<Self> {
        let b: &[u8] = &bytes;
        if offset.checked_add(0x20)? > b.len() || be32_at(b, offset)? != MAGIC {
            return None;
        }
        let root_rel = be32_at(b, offset + 4)? as usize;
        if root_rel > b.len() {
            return None;
        }
        let root = offset + root_rel;
        let count = be32_at(b, root.checked_add(8)?)?;
        if !(1..=MAX_NODES).contains(&count) {
            return None;
        }
        let names = root + count as usize * 12;
        if names > b.len() {
            return None;
        }
        let mut entries = Vec::new();
        let mut dirs: Vec<(String, u32)> = Vec::new(); // folder name, end index
        for i in 1..count {
            let at = root + i as usize * 12;
            while dirs.last().is_some_and(|d| i >= d.1) {
                dirs.pop();
            }
            let name_at = names + (be32_at(b, at)? & 0xFF_FFFF) as usize;
            let tail = b.get(name_at..).filter(|t| !t.is_empty())?;
            let end = tail.iter().position(|&c| c == 0).unwrap_or(tail.len());
            let name: String = tail[..end].iter().map(|&c| char::from(c)).collect();
            if *b.get(at)? != 0 {
                dirs.push((name, be32_at(b, at + 8)?));
            } else {
                let mut path = String::new();
                for (dir, _) in &dirs {
                    path.push_str(dir);
                    path.push('/');
                }
                path.push_str(&name);
                let data_offset = u64::from(be32_at(b, at + 4)?);
                let size = u64::from(be32_at(b, at + 8)?);
                let start = offset as u64 + data_offset;
                if start + size > i32::MAX as u64 {
                    return None;
                }
                entries.push(Entry { path, offset: start as usize, size: size as usize });
            }
        }
        Some(U8Archive { bytes, entries })
    }

    /// The bytes of the file at `path` (case-insensitive, folders separated by `/`), or `None`.
    pub(crate) fn file(&self, path: &str) -> Option<&[u8]> {
        let entry = self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(path))?;
        self.bytes.get(entry.offset..entry.offset.checked_add(entry.size)?)
    }
}
