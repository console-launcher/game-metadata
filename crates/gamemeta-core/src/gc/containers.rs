// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A disc image seen as the plain disc: reads take logical offsets, whatever the container (a plain ISO/GCM is the
//! file itself). [`open`] sniffs the container from its magic, never its extension.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use miniz_oxide::DataFormat;

use super::rvz::Rvz;
use crate::deflate::{self, Flow};
use crate::le::{le64_at, u32_at};
use crate::source::fetch;
use crate::source::fetch_opt;
use crate::{ByteSource, ReadError};

/// The plain disc behind any container this reader opens.
pub(crate) type Disc<'a> = Box<dyn ByteSource + 'a>;

const GCZ_MAGIC: [u8; 4] = [0x01, 0xC0, 0x0B, 0xB1];

/// The disc inside `source`; `Ok(None)` for a container this module cannot read (WIA included).
pub(crate) fn open<'a, S: ByteSource + ?Sized>(source: &'a mut S) -> Result<Option<Disc<'a>>, ReadError> {
    let Some(magic) = fetch_opt(source, 0, 4)? else { return Ok(None) };
    Ok(match &magic[..] {
        b"CISO" => Ciso::open(source)?.map(|d| Box::new(d) as Disc<'a>),
        m if m == GCZ_MAGIC => Gcz::open(source)?.map(|d| Box::new(d) as Disc<'a>),
        b"RVZ\x01" => Rvz::open(source)?.map(|d| Box::new(d) as Disc<'a>),
        b"WIA\x01" => None,
        _ => Some(Box::new(source) as Disc<'a>),
    })
}

/// One block of a block-stored image.
pub(crate) enum Block {
    /// All zero (a block left out of the image); never allocated.
    Zero,
    /// The block's bytes (shorter than the block size for the last block).
    Data(Vec<u8>),
}

/// Copies what `data` holds from `in_block` into `out`; `OutOfRange` when it holds none (a short or damaged block).
pub(crate) fn copy_out(data: &[u8], in_block: usize, out: &mut [u8]) -> Result<usize, ReadError> {
    let n = out.len().min(data.len().saturating_sub(in_block));
    if n == 0 {
        return Err(ReadError::OutOfRange);
    }
    out[..n].copy_from_slice(&data[in_block..in_block + n]);
    Ok(n)
}

/// Where the blocks of an image come from.
pub(crate) trait BlockProvider {
    fn block(&mut self, index: u64) -> Result<Block, ReadError>;
}

/// A disc stored as equal blocks, with the last block read cached.
pub(crate) struct Blocks<P> {
    provider: P,
    block_size: u64,
    size: u64,
    cache: Option<(u64, Vec<u8>)>,
}

impl<P: BlockProvider> ByteSource for Blocks<P> {
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
            let index = position / self.block_size;
            let in_block = (position % self.block_size) as usize;
            let want = (buf.len() - done).min(self.block_size as usize - in_block);
            if self.cache.as_ref().is_none_or(|(i, _)| *i != index) {
                self.cache = match self.provider.block(index)? {
                    Block::Zero => {
                        buf[done..done + want].fill(0);
                        done += want;
                        continue;
                    }
                    Block::Data(v) => Some((index, v)),
                };
            }
            let data = self.cache.as_ref().map_or(&[][..], |(_, v)| v);
            done += copy_out(data, in_block, &mut buf[done..done + want])?;
        }
        Ok(())
    }
}

/// CISO: a 0x8000 byte header (magic, little-endian block size, one present-flag byte per block), then the present
/// blocks packed.
pub(crate) struct CisoBlocks<'a, S: ?Sized> {
    base: &'a mut S,
    block_size: u64,
    rank: Vec<i32>,
}

const CISO_HEADER: u64 = 0x8000;

impl<S: ByteSource + ?Sized> BlockProvider for CisoBlocks<'_, S> {
    fn block(&mut self, index: u64) -> Result<Block, ReadError> {
        let r =
            *self.rank.get(usize::try_from(index).map_err(|_| ReadError::OutOfRange)?).ok_or(ReadError::OutOfRange)?;
        // A block left out of the image is all zero.
        if r < 0 {
            return Ok(Block::Zero);
        }
        fetch(self.base, CISO_HEADER + r as u64 * self.block_size, self.block_size).map(Block::Data)
    }
}

pub(crate) struct Ciso;

impl Ciso {
    fn open<'a, S: ByteSource + ?Sized>(base: &'a mut S) -> Result<Option<Blocks<CisoBlocks<'a, S>>>, ReadError> {
        let Some(head) = fetch_opt(base, 0, CISO_HEADER)? else { return Ok(None) };
        let block_size = u64::from(u32_at(&head, 4).unwrap_or(0));
        if !(0x800..=64 * 1024 * 1024).contains(&block_size) {
            return Ok(None);
        }
        let mut rank = vec![-1i32; head.len() - 8];
        let (mut present, mut last) = (0i32, None);
        for (i, slot) in rank.iter_mut().enumerate() {
            if head[8 + i] == 1 {
                *slot = present;
                present += 1;
                last = Some(i);
            }
        }
        let Some(last) = last else { return Ok(None) };
        let size = (last as u64 + 1) * block_size;
        Ok(Some(Blocks { provider: CisoBlocks { base, block_size, rank }, block_size, size, cache: None }))
    }
}

/// GCZ: a 0x20 byte header, a table of block pointers (high bit: stored raw) and hashes, then zlib blocks.
pub(crate) struct GczBlocks<'a, S: ?Sized> {
    base: &'a mut S,
    block_size: u64,
    blocks: u64,
    compressed_size: u64,
    size: u64,
}

const GCZ_HEADER: u64 = 0x20;
const RAW: u64 = 1 << 63;

impl<S: ByteSource + ?Sized> GczBlocks<'_, S> {
    fn pointer(&mut self, index: u64) -> Result<u64, ReadError> {
        let b = fetch(self.base, GCZ_HEADER + index * 8, 8)?;
        le64_at(&b, 0).ok_or(ReadError::OutOfRange)
    }
}

impl<S: ByteSource + ?Sized> BlockProvider for GczBlocks<'_, S> {
    fn block(&mut self, index: u64) -> Result<Block, ReadError> {
        if index >= self.blocks {
            return Err(ReadError::OutOfRange);
        }
        let pointer = self.pointer(index)?;
        let raw = pointer & RAW != 0;
        let start = pointer & !RAW;
        let end = if index + 1 < self.blocks { self.pointer(index + 1)? & !RAW } else { self.compressed_size };
        let length = end
            .checked_sub(start)
            .filter(|&l| l > 0 && l <= 2 * self.block_size + 1024)
            .ok_or(ReadError::OutOfRange)?;
        let data_start = GCZ_HEADER + self.blocks * 12;
        let at = data_start.checked_add(start).ok_or(ReadError::OutOfRange)?;
        if at.checked_add(length).is_none_or(|e| e > self.base.size()) {
            return Err(ReadError::OutOfRange);
        }
        let expected = self.block_size.min(self.size - index * self.block_size) as usize;
        if raw {
            let mut data = fetch(self.base, at, length)?;
            data.resize(expected, 0);
            return Ok(Block::Data(data));
        }
        let mut out = Vec::with_capacity(expected);
        let walked = deflate::inflate_range(self.base, at, length, DataFormat::Zlib, |chunk| {
            let take = (expected - out.len()).min(chunk.len());
            out.extend_from_slice(&chunk[..take]);
            if out.len() == expected { Flow::Stop } else { Flow::Continue }
        });
        match walked {
            Err(deflate::Failure::Source) => Err(ReadError::Io),
            _ if out.len() == expected => Ok(Block::Data(out)),
            _ => Err(ReadError::OutOfRange),
        }
    }
}

pub(crate) struct Gcz;

impl Gcz {
    fn open<'a, S: ByteSource + ?Sized>(base: &'a mut S) -> Result<Option<Blocks<GczBlocks<'a, S>>>, ReadError> {
        let Some(head) = fetch_opt(base, 0, GCZ_HEADER)? else { return Ok(None) };
        let compressed = le64_at(&head, 8).unwrap_or(0);
        let data_size = le64_at(&head, 16).unwrap_or(0);
        let block_size = u64::from(u32_at(&head, 24).unwrap_or(0));
        let blocks = u64::from(u32_at(&head, 28).unwrap_or(0));
        if !(0x800..=16 * 1024 * 1024).contains(&block_size) || blocks == 0 || data_size == 0 {
            return Ok(None);
        }
        if blocks.checked_mul(block_size).is_none_or(|t| t < data_size) || blocks > base.size() {
            return Ok(None);
        }
        let provider = GczBlocks { base, block_size, blocks, compressed_size: compressed, size: data_size };
        Ok(Some(Blocks { provider, block_size, size: data_size, cache: None }))
    }
}
