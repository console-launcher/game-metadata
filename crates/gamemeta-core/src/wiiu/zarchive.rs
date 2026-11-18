// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A read-only view of a ZArchive (a `.wua`): a footer of six sections, zstd blocks of 64 KiB addressed through
//! offset records (16 blocks each), a name table and a file tree (16 byte entries, a folder's children contiguous),
//! all big-endian. Reads only the tables and the blocks a file needs. A bad archive is `None`.

use alloc::string::String;
use alloc::vec::Vec;

use crate::le::{be16_at, be32_at, be64_at};
use crate::source::fetch_opt;
use crate::{ByteSource, zstd};

const BLOCK: usize = 64 * 1024;
const BLOCKS_PER_RECORD: u64 = 16;
const RECORD: usize = 8 + 2 * BLOCKS_PER_RECORD as usize;
const ENTRY: usize = 16;
const FOOTER: u64 = 144;
const MAGIC: u32 = 0x169f_52d6;
const MAX_TABLE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub name: String,
    pub is_file: bool,
    /// A file's offset in the uncompressed stream; a folder's first child index.
    pub first: u64,
    /// A file's size; a folder's child count.
    pub size_or_count: u64,
}

pub(crate) struct Archive<'a, S: ?Sized> {
    source: &'a mut S,
    data_offset: u64,
    data_size: u64,
    records: Vec<u8>,
    names: Vec<u8>,
    tree: Vec<u8>,
    cache: Option<(u64, Option<Vec<u8>>)>,
}

impl<'a, S: ByteSource + ?Sized> Archive<'a, S> {
    pub(crate) fn open(source: &'a mut S) -> Option<Self> {
        let size = source.size();
        if size < FOOTER {
            return None;
        }
        let footer = fetch_opt(source, size - FOOTER, FOOTER).ok()??;
        if be32_at(&footer, 140)? != MAGIC {
            return None;
        }
        let body = size - FOOTER;
        let section = |i: usize| Some((be64_at(&footer, i * 16)?, be64_at(&footer, i * 16 + 8)?));
        let fits = |offset: u64, len: u64| offset.checked_add(len).is_some_and(|end| end <= body);
        let (data_offset, data_size) = section(0)?;
        if !fits(data_offset, data_size) {
            return None;
        }
        let mut table = |i: usize| {
            let (offset, len) = section(i)?;
            if len > MAX_TABLE || !fits(offset, len) {
                return None;
            }
            fetch_opt(source, offset, len).ok()?
        };
        let (records, names, tree) = (table(1)?, table(2)?, table(3)?);
        Some(Archive { source, data_offset, data_size, records, names, tree, cache: None })
    }

    fn entry_count(&self) -> usize {
        self.tree.len() / ENTRY
    }

    fn entry(&self, index: usize) -> Option<Entry> {
        if index >= self.entry_count() {
            return None;
        }
        let at = index * ENTRY;
        let type_and_name = be32_at(&self.tree, at)?;
        let is_file = type_and_name & 0x8000_0000 != 0;
        let name = if index == 0 { String::new() } else { self.name_at((type_and_name & 0x7FFF_FFFF) as usize)? };
        let word = |o: usize| be32_at(&self.tree, at + o).map(u64::from);
        Some(if is_file {
            let offset = word(4)? | u64::from(be16_at(&self.tree, at + 14)?) << 32;
            let size = word(8)? | u64::from(be16_at(&self.tree, at + 12)?) << 32;
            Entry { name, is_file, first: offset, size_or_count: size }
        } else {
            Entry { name, is_file, first: word(4)?, size_or_count: word(8)? }
        })
    }

    fn name_at(&self, at: usize) -> Option<String> {
        let first = usize::from(*self.names.get(at)?);
        let (len, start) = if first & 0x80 != 0 {
            ((first & 0x7F) | usize::from(*self.names.get(at + 1)?) << 7, at + 2)
        } else {
            (first, at + 1)
        };
        let bytes = self.names.get(start..start.checked_add(len)?)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    /// The entries inside folder `dir` (`None` = the root), in tree order; empty for a file or a missing folder.
    pub(crate) fn children(&self, dir: Option<&Entry>) -> Vec<Entry> {
        let (first, count) = match dir {
            None => match self.entry(0) {
                Some(root) if !root.is_file => (root.first, root.size_or_count),
                _ => return Vec::new(),
            },
            Some(d) if !d.is_file => (d.first, d.size_or_count),
            Some(_) => return Vec::new(),
        };
        let total = self.entry_count() as u64;
        if count > total || first < 1 || first + count > total {
            return Vec::new();
        }
        (0..count as usize).filter_map(|i| self.entry(first as usize + i)).collect()
    }

    /// The entry below `folder` at `path` (names separated by `/`, ASCII case-insensitive).
    pub(crate) fn find(&self, folder: Option<&Entry>, path: &str) -> Option<Entry> {
        let mut dir = folder.cloned();
        let mut found = None;
        for part in path.split('/') {
            let next = self.children(dir.as_ref()).into_iter().find(|e| e.name.eq_ignore_ascii_case(part))?;
            dir = Some(next.clone());
            found = Some(next);
        }
        found
    }

    fn decode_block(&mut self, index: u64) -> Option<Vec<u8>> {
        let record = usize::try_from(index / BLOCKS_PER_RECORD).ok()?;
        let slot = (index % BLOCKS_PER_RECORD) as usize;
        let base = record.checked_mul(RECORD)?;
        let record_bytes = self.records.get(base..base.checked_add(RECORD)?)?;
        let mut offset = be64_at(record_bytes, 0)?;
        for i in 0..slot {
            offset = offset.checked_add(u64::from(be16_at(record_bytes, 8 + i * 2)?) + 1)?;
        }
        let size = usize::from(be16_at(record_bytes, 8 + slot * 2)?) + 1;
        if offset.checked_add(size as u64)? > self.data_size {
            return None;
        }
        let stored = fetch_opt(self.source, self.data_offset + offset, size as u64).ok()??;
        if size == BLOCK { Some(stored) } else { zstd::decompress(&stored, BLOCK) }
    }

    fn block(&mut self, index: u64) -> Option<&[u8]> {
        if self.cache.as_ref().is_none_or(|(i, _)| *i != index) {
            let block = self.decode_block(index);
            self.cache = Some((index, block));
        }
        self.cache.as_ref()?.1.as_deref()
    }

    /// The whole of `file`; `None` when it is over `limit` or a block cannot be read.
    pub(crate) fn read(&mut self, file: &Entry, limit: usize) -> Option<Vec<u8>> {
        if !file.is_file || file.size_or_count > limit as u64 {
            return None;
        }
        let size = file.size_or_count as usize;
        let mut out = Vec::with_capacity(size);
        while out.len() < size {
            let position = file.first.checked_add(out.len() as u64)?;
            let in_block = (position % BLOCK as u64) as usize;
            let want = size - out.len();
            let block = self.block(position / BLOCK as u64)?;
            let n = want.min(block.len().checked_sub(in_block)?);
            if n == 0 {
                return None;
            }
            out.extend_from_slice(&block[in_block..in_block + n]);
        }
        Some(out)
    }

    /// The first title folder with a `meta/meta.xml`, preferring one named `00050000...` (the base game).
    pub(crate) fn pick_title(&self) -> Option<Entry> {
        let mut best: Option<(bool, Entry)> = None;
        for folder in self.children(None).into_iter().filter(|e| !e.is_file) {
            if self.find(Some(&folder), "meta/meta.xml").is_some_and(|e| e.is_file) {
                let base = folder.name.starts_with("00050000");
                if best.as_ref().is_none_or(|(b, _)| base && !b) {
                    best = Some((base, folder));
                }
            }
        }
        best.map(|(_, e)| e)
    }
}
