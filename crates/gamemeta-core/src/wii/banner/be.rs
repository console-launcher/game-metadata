// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Big-endian reads over a buffer for the layout and animation parsers. Every read past the end is `None`, which the parsers
//! pass up: a damaged file is no banner, never a panic. Offsets are unsigned here; the original implementation reads them as signed 32 bit
//! numbers, which only matters for offsets above 2 GiB (garbage in a buffer that is at most a few megabytes).

use alloc::string::String;
use alloc::vec::Vec;

use crate::le::{be16_at, be32_at};

/// The longest a file's sections may be listed: more than a layout or animation ever has.
const MAX_SECTIONS: usize = 4096;

#[derive(Clone, Copy)]
pub(super) struct Be<'a>(pub &'a [u8]);

/// A section of a layout or animation file.
pub(super) struct Section {
    pub magic: [u8; 4],
    pub start: usize,
}

impl Be<'_> {
    pub fn u8(&self, at: usize) -> Option<u8> {
        self.0.get(at).copied()
    }

    pub fn u16(&self, at: usize) -> Option<u16> {
        be16_at(self.0, at)
    }

    pub fn u32(&self, at: usize) -> Option<u32> {
        be32_at(self.0, at)
    }

    pub fn f32(&self, at: usize) -> Option<f32> {
        self.u32(at).map(f32::from_bits)
    }

    pub fn magic(&self, at: usize) -> Option<[u8; 4]> {
        let s = self.0.get(at..at.checked_add(4)?)?;
        Some([s[0], s[1], s[2], s[3]])
    }

    /// `base` plus the offset stored at `at`.
    pub fn offset(&self, base: usize, at: usize) -> Option<usize> {
        base.checked_add(self.u32(at)? as usize)
    }

    /// A NUL terminated (or `max` long) name, as Latin-1. `None` when `at` is past the end.
    pub fn name(&self, at: usize, max: usize) -> Option<String> {
        let tail = self.0.get(at..)?;
        let end = tail.iter().take(max).position(|&c| c == 0).unwrap_or(tail.len().min(max));
        Some(tail[..end].iter().map(|&c| char::from(c)).collect())
    }

    /// The sections after the 0x10 byte file header of a layout or animation file. `None` for a wrong file magic or a section that
    /// does not fit (or more than [`MAX_SECTIONS`]).
    pub fn sections(&self, file_magic: &[u8; 4]) -> Option<Vec<Section>> {
        if self.0.len() < 0x10 || self.magic(0)? != *file_magic {
            return None;
        }
        let count = usize::from(self.u16(0x0E)?);
        let mut at = usize::from(self.u16(0x0C)?).max(0x10);
        let mut out = Vec::new();
        for _ in 0..count {
            let size = self.u32(at.checked_add(4)?)? as usize;
            if size < 8 || at.checked_add(size)? > self.0.len() || out.len() >= MAX_SECTIONS {
                return None;
            }
            out.push(Section { magic: self.magic(at)?, start: at });
            at += size;
        }
        Some(out)
    }
}
