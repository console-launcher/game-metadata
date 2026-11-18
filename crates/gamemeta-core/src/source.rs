// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use alloc::vec;
use alloc::vec::Vec;

/// Why a [`ByteSource`] read gave no bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// Some of the requested range lies outside the source.
    OutOfRange,
    /// The source itself failed (an I/O error, a closed descriptor, a callback that reported failure).
    Io,
}

impl core::fmt::Display for ReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            ReadError::OutOfRange => "read outside the source",
            ReadError::Io => "source read failed",
        })
    }
}

#[cfg(any(test, feature = "std"))]
impl std::error::Error for ReadError {}

/// Random-access reads of one file, performed by the caller. Readers ask for small ranges (headers, an icon) and
/// never for more than a few megabytes in total.
pub trait ByteSource {
    /// The total length in bytes.
    fn size(&self) -> u64;

    /// Fills `buf` with the bytes at `offset`. Callers only ask for ranges inside [`size`](Self::size); an
    /// implementation that finds the range outside the source (it shrank) returns [`ReadError::OutOfRange`].
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError>;
}

impl<S: ByteSource + ?Sized> ByteSource for &mut S {
    fn size(&self) -> u64 {
        (**self).size()
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        (**self).read_at(offset, buf)
    }
}

/// A [`ByteSource`] over bytes in memory.
#[derive(Debug, Clone, Copy)]
pub struct SliceSource<'a> {
    bytes: &'a [u8],
}

impl<'a> SliceSource<'a> {
    /// A source over `bytes`.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl ByteSource for SliceSource<'_> {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        let start = usize::try_from(offset).map_err(|_| ReadError::OutOfRange)?;
        let end = start.checked_add(buf.len()).ok_or(ReadError::OutOfRange)?;
        let src = self.bytes.get(start..end).ok_or(ReadError::OutOfRange)?;
        buf.copy_from_slice(src);
        Ok(())
    }
}

/// `len` bytes at `offset`, after checking the range against the source's size (so a lying header never allocates
/// more than the file holds).
pub(crate) fn fetch<S: ByteSource + ?Sized>(source: &mut S, offset: u64, len: u64) -> Result<Vec<u8>, ReadError> {
    let end = offset.checked_add(len).ok_or(ReadError::OutOfRange)?;
    if end > source.size() {
        return Err(ReadError::OutOfRange);
    }
    let len = usize::try_from(len).map_err(|_| ReadError::OutOfRange)?;
    let mut buf = vec![0u8; len];
    source.read_at(offset, &mut buf)?;
    Ok(buf)
}

/// Like [`fetch`], but a range outside the source is `Ok(None)` (the original readers' "null"); only a source that
/// itself failed is an error.
pub(crate) fn fetch_opt<S: ByteSource + ?Sized>(
    source: &mut S,
    offset: u64,
    len: u64,
) -> Result<Option<Vec<u8>>, ReadError> {
    match fetch(source, offset, len) {
        Ok(v) => Ok(Some(v)),
        Err(ReadError::OutOfRange) => Ok(None),
        Err(e) => Err(e),
    }
}
