// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! An RVZ image (`docs/WiaAndRvz.md`) as the plain disc: header 1 (0x48 bytes), header 2 (the first 0x80 disc
//! bytes, the raw data and group tables), raw data entries naming runs of fixed-size groups, groups stored or zstd
//! compressed, or RVZ packed (zero runs and "junk" the format regenerates; junk reads as zeros here, which is fine for
//! the header, the FST and `opening.bnr`). Only stored or zstd images: a WIA, or another compression, is not opened.
//!
//! A Wii image keeps its game partition decrypted, in 0x7C00-byte sectors; [`Rvz::partition_data`] reads those. Nothing
//! here decrypts anything.

use alloc::vec::Vec;

use super::containers::{Block, copy_out};
use crate::le::{be32_at, be64_at};
use crate::source::{fetch, fetch_opt};
use crate::{ByteSource, ReadError, zstd};

const GROUP: usize = 12;
const RAW: usize = 0x18;
const SEED: usize = 68;
const EXTRA: usize = 0x10000;
const MAX_TABLE: u64 = 32 * 1024 * 1024;
const EXCEPTION_SPAN: usize = 0x20_0000;
const LIST_MAX: usize = 2 + 3328 * 22; // the format caps a list at 52 x 64 exceptions
const PART: u32 = 48;
const SECTOR: u64 = 0x7C00;

struct Raw {
    start: u64,
    end: u64,
    first_group: u64,
    count: u64,
}

/// A run of a Wii partition's sectors (0x8000 bytes on disc, 0x7C00 stored) and the groups that hold them.
#[derive(Clone, Copy)]
pub(crate) struct PartData {
    first_sector: u64,
    sectors: u64,
    first_group: u64,
    group_count: u64,
}

pub(crate) struct Rvz<'a, S: ?Sized> {
    base: &'a mut S,
    size: u64,
    chunk: usize,
    disc_header: Vec<u8>,
    entries: Vec<Raw>,
    groups: Vec<u8>,
    partitions: Vec<Vec<PartData>>,
    cache: Option<(u64, Block)>,
    part_cache: Option<(u64, Block)>,
}

fn table<S: ByteSource + ?Sized>(
    base: &mut S,
    compression: u32,
    offset: u64,
    stored: u64,
    expected: u64,
) -> Result<Option<Vec<u8>>, ReadError> {
    if expected > MAX_TABLE {
        return Ok(None);
    }
    let Some(bytes) = fetch_opt(base, offset, stored)? else { return Ok(None) };
    if compression == 0 {
        return Ok(Some(bytes).filter(|b| b.len() as u64 >= expected));
    }
    Ok(zstd::decompress(&bytes, expected as usize).filter(|o| o.len() as u64 == expected))
}

impl<'a, S: ByteSource + ?Sized> Rvz<'a, S> {
    /// Opens the image; `Ok(None)` for a WIA, a non-zstd compression, or tables that do not hold up.
    pub(crate) fn open(base: &'a mut S) -> Result<Option<Self>, ReadError> {
        let Some(h1) = fetch_opt(base, 0, 0x48)? else { return Ok(None) };
        if &h1[..4] != b"RVZ\x01" {
            return Ok(None);
        }
        let Some(h2) = fetch_opt(base, 0x48, 0xDC)? else { return Ok(None) };
        let iso_size = be64_at(&h1, 0x24).unwrap_or(0) as i64;
        let disc_type = be32_at(&h2, 0).unwrap_or(0);
        let compression = be32_at(&h2, 4).unwrap_or(99);
        let chunk = be32_at(&h2, 0xC).unwrap_or(0) as i32;
        if (disc_type != 1 && disc_type != 2) || (compression != 0 && compression != 5) {
            return Ok(None);
        }
        if !(0x8000..=64 * 1024 * 1024).contains(&chunk) || iso_size < 0x440 {
            return Ok(None);
        }
        let raw_count = be32_at(&h2, 0xB4).unwrap_or(0) as i32;
        let raw_offset = be64_at(&h2, 0xB8).unwrap_or(0);
        let raw_stored = be32_at(&h2, 0xC0).unwrap_or(0) as i32;
        let group_count = be32_at(&h2, 0xC4).unwrap_or(0) as i32;
        let group_offset = be64_at(&h2, 0xC8).unwrap_or(0);
        let group_stored = be32_at(&h2, 0xD0).unwrap_or(0) as i32;
        if !(1..=4096).contains(&raw_count)
            || !(1..=2_000_000).contains(&group_count)
            || raw_stored <= 0
            || group_stored <= 0
        {
            return Ok(None);
        }
        let Some(raw) = table(base, compression, raw_offset, raw_stored as u64, raw_count as u64 * RAW as u64)? else {
            return Ok(None);
        };
        let Some(groups) =
            table(base, compression, group_offset, group_stored as u64, group_count as u64 * GROUP as u64)?
        else {
            return Ok(None);
        };
        let mut entries = Vec::new();
        for i in 0..raw_count as usize {
            let offset = be64_at(&raw, i * RAW).unwrap_or(0) as i64;
            let length = be64_at(&raw, i * RAW + 8).unwrap_or(0) as i64;
            if length <= 0 || offset < 0 {
                continue;
            }
            // The data starts where it says, but its groups are counted from the 32 KiB boundary below it.
            entries.push(Raw {
                start: offset as u64 - offset as u64 % 0x8000,
                end: (offset as u64).saturating_add(length as u64),
                first_group: u64::from(be32_at(&raw, i * RAW + 16).unwrap_or(0)),
                count: u64::from(be32_at(&raw, i * RAW + 20).unwrap_or(0)),
            });
        }
        // wia_part_t entries: a 16 byte key slot (skipped: never read) then two runs of first sector, sectors, first group, groups.
        let part_count = be32_at(&h2, 0x90).unwrap_or(0) as i32;
        let part_size = be32_at(&h2, 0x94).unwrap_or(0);
        let part_offset = be64_at(&h2, 0x98).unwrap_or(0);
        let mut partitions = Vec::new();
        if (1..=64).contains(&part_count) && part_size >= PART {
            for i in 0..part_count as u64 {
                let at = part_offset.saturating_add(i * u64::from(part_size)).saturating_add(16);
                let Some(runs) = fetch_opt(base, at, 32)? else { continue };
                let mut parts = Vec::new();
                for r in 0..2 {
                    let f = |k: usize| u64::from(be32_at(&runs, r * 16 + k).unwrap_or(0));
                    let run = PartData { first_sector: f(0), sectors: f(4), first_group: f(8), group_count: f(12) };
                    if run.sectors > 0 && run.group_count > 0 {
                        parts.push(run);
                    }
                }
                partitions.push(parts);
            }
        }
        Ok(Some(Rvz {
            base,
            size: iso_size as u64,
            chunk: chunk as usize,
            disc_header: h2[0x10..0x90].to_vec(),
            entries,
            groups,
            partitions,
            cache: None,
            part_cache: None,
        }))
    }

    /// The group entry at `index`: (file offset, stored length, zstd, packed size).
    fn group_entry(&self, index: u64) -> Result<(u64, usize, bool, usize), ReadError> {
        let at = usize::try_from(index).ok().and_then(|i| i.checked_mul(GROUP)).ok_or(ReadError::OutOfRange)?;
        let (Some(offset), Some(raw), Some(packed)) =
            (be32_at(&self.groups, at), be32_at(&self.groups, at + 4), be32_at(&self.groups, at + 8))
        else {
            return Err(ReadError::OutOfRange);
        };
        let length = (raw & 0x7FFF_FFFF) as usize;
        // A group never stores more than a chunk plus its packing and exception overhead.
        let cap = 2 * self.chunk + 0x2_0000;
        if length > cap || packed as usize > cap {
            return Err(ReadError::OutOfRange);
        }
        Ok((u64::from(offset) << 2, length, raw & 0x8000_0000 != 0, packed as usize))
    }

    fn decode_group(&mut self, index: u64) -> Result<Block, ReadError> {
        let (offset, length, compressed, packed) = self.group_entry(index)?;
        if length == 0 {
            return Ok(Block::Zero);
        }
        let stored = fetch(self.base, offset, length as u64)?;
        let data = if compressed {
            zstd::decompress(&stored, self.chunk + EXTRA).ok_or(ReadError::OutOfRange)?
        } else {
            stored
        };
        let data = if packed != 0 { unpack(&data, self.chunk).ok_or(ReadError::OutOfRange)? } else { data };
        Ok(Block::Data(data))
    }

    /// A group of Wii partition data: `expected` bytes of decrypted, hash-free sector data (less in a run's last
    /// group), after the hash exception lists.
    fn decode_part_group(&mut self, index: u64, expected: usize) -> Result<Block, ReadError> {
        let (offset, length, compressed, packed) = self.group_entry(index)?;
        if length == 0 {
            return Ok(Block::Zero);
        }
        let stored = fetch(self.base, offset, length as u64)?;
        let lists = (self.chunk / EXCEPTION_SPAN).max(1);
        let body = if compressed {
            let limit = lists * LIST_MAX + if packed != 0 { packed } else { expected };
            zstd::decompress(&stored, limit).ok_or(ReadError::OutOfRange)?
        } else {
            stored
        };
        // One exception list per 2 MiB (per chunk, when smaller): a u16 count and 22 bytes each, skipped.
        let mut p = 0usize;
        for _ in 0..lists {
            let count = usize::from(crate::le::be16_at(&body, p).ok_or(ReadError::OutOfRange)?);
            p += 2 + count * 22;
            if p > body.len() {
                return Err(ReadError::OutOfRange);
            }
        }
        if !compressed {
            p = (p + 3) & !3; // stored data starts on a 4 byte boundary
        }
        let rest = body.get(p..).ok_or(ReadError::OutOfRange)?;
        Ok(Block::Data(if packed != 0 { unpack(rest, expected).ok_or(ReadError::OutOfRange)? } else { rest.to_vec() }))
    }

    /// The decrypted data of the disc's data partition (type 0) as a source whose offset 0 is the partition's
    /// `boot.bin`; `Ok(None)` for a GameCube image, a disc without that partition or one whose entries do not match.
    /// The key slot of the partition entry and the ticket are never read: only the partition table, the data offset
    /// and the sector runs.
    pub(crate) fn partition_data(&mut self) -> Result<Option<RvzPartition<'_, 'a, S>>, ReadError> {
        let Some(table) = fetch_opt(self, 0x40000, 8)? else { return Ok(None) };
        let count = u64::from(be32_at(&table, 0).unwrap_or(0));
        let table_at = u64::from(be32_at(&table, 4).unwrap_or(0)) << 2;
        if !(1..=4).contains(&count) || table_at == 0 {
            return Ok(None);
        }
        let Some(rows) = fetch_opt(self, table_at, count * 8)? else { return Ok(None) };
        let Some(part_offset) = (0..count as usize)
            .find(|&i| be32_at(&rows, i * 8 + 4) == Some(0))
            .and_then(|i| be32_at(&rows, i * 8))
            .map(|o| u64::from(o) << 2)
        else {
            return Ok(None);
        };
        let Some(data_offset) = fetch_opt(self, part_offset + 0x2B8, 4)? else { return Ok(None) };
        let data_start = part_offset + (u64::from(be32_at(&data_offset, 0).unwrap_or(0)) << 2);
        if data_start % 0x8000 != 0 {
            return Ok(None);
        }
        let Some(runs) =
            self.partitions.iter().find(|p| p.first().is_some_and(|r| r.first_sector == data_start / 0x8000)).cloned()
        else {
            return Ok(None);
        };
        let sectors_per_group = (self.chunk / 0x8000) as u64;
        let first = runs.iter().map(|r| r.first_sector).min().unwrap_or(0);
        let end = runs.iter().map(|r| r.first_sector + r.sectors).max().unwrap_or(0);
        Ok(Some(RvzPartition { rvz: self, runs, sectors_per_group, first, size: (end - first) * SECTOR }))
    }
}

/// RVZ packing: `u32 size` (bit 31: a junk run, followed by its 68 byte seed, read as zeros) or `size` literal bytes.
fn unpack(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 4 <= data.len() && out.len() < limit {
        let word = be32_at(data, p)?;
        p += 4;
        let n = (word & 0x7FFF_FFFF) as usize;
        if out.len() + n > limit {
            return None;
        }
        if word & 0x8000_0000 != 0 {
            p += SEED;
            out.resize(out.len() + n, 0);
        } else {
            out.extend_from_slice(data.get(p..p.checked_add(n)?)?);
            p += n;
        }
    }
    Some(out)
}

impl<S: ByteSource + ?Sized> ByteSource for Rvz<'_, S> {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        if offset.checked_add(buf.len() as u64).is_none_or(|e| e > self.size) {
            return Err(ReadError::OutOfRange);
        }
        let mut done = 0usize;
        while done < buf.len() {
            let position = offset + done as u64;
            if position < self.disc_header.len() as u64 {
                let at = position as usize;
                let n = (buf.len() - done).min(self.disc_header.len() - at);
                buf[done..done + n].copy_from_slice(&self.disc_header[at..at + n]);
                done += n;
                continue;
            }
            let entry =
                self.entries.iter().find(|e| position >= e.start && position < e.end).ok_or(ReadError::OutOfRange)?;
            let n = (position - entry.start) / self.chunk as u64;
            if n >= entry.count {
                return Err(ReadError::OutOfRange);
            }
            let index = entry.first_group + n;
            let in_group = ((position - entry.start) % self.chunk as u64) as usize;
            if self.cache.as_ref().is_none_or(|(i, _)| *i != index) {
                let block = self.decode_group(index)?;
                self.cache = Some((index, block));
            }
            let want = (buf.len() - done).min(self.chunk - in_group);
            done += match &self.cache {
                Some((_, Block::Data(d))) => copy_out(d, in_group, &mut buf[done..done + want])?,
                _ => {
                    buf[done..done + want].fill(0);
                    want
                }
            };
        }
        Ok(())
    }
}

/// A Wii partition's decrypted data (0x7C00 bytes per sector, hashes stripped) read through an RVZ's sector runs;
/// offset 0 is `boot.bin`.
pub(crate) struct RvzPartition<'r, 'a, S: ?Sized> {
    rvz: &'r mut Rvz<'a, S>,
    runs: Vec<PartData>,
    sectors_per_group: u64,
    first: u64,
    size: u64,
}

impl<S: ByteSource + ?Sized> ByteSource for RvzPartition<'_, '_, S> {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        if offset.checked_add(buf.len() as u64).is_none_or(|e| e > self.size) {
            return Err(ReadError::OutOfRange);
        }
        let group_data = (self.sectors_per_group * SECTOR) as usize;
        let mut done = 0usize;
        while done < buf.len() {
            let position = offset + done as u64;
            let sector = position / SECTOR;
            let run = *self
                .runs
                .iter()
                .find(|r| sector >= r.first_sector - self.first && sector < r.first_sector - self.first + r.sectors)
                .ok_or(ReadError::OutOfRange)?;
            let in_run = sector - (run.first_sector - self.first);
            let index = run.first_group + in_run / self.sectors_per_group;
            if self.rvz.part_cache.as_ref().is_none_or(|(i, _)| *i != index) {
                let block = self.rvz.decode_part_group(index, group_data)?;
                self.rvz.part_cache = Some((index, block));
            }
            let in_group = ((in_run % self.sectors_per_group) * SECTOR + position % SECTOR) as usize;
            let want = (buf.len() - done).min(group_data.saturating_sub(in_group));
            if want == 0 {
                return Err(ReadError::OutOfRange);
            }
            done += match &self.rvz.part_cache {
                Some((_, Block::Data(d))) => copy_out(d, in_group, &mut buf[done..done + want])?,
                _ => {
                    buf[done..done + want].fill(0);
                    want
                }
            };
        }
        Ok(())
    }
}
