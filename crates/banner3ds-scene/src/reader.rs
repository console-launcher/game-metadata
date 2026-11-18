// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bounds-checked little-endian reads over a byte slice.

use alloc::string::String;

/// A file that is not a readable banner: truncated, inconsistent, or over a size cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatError;

impl core::fmt::Display for FormatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("not a readable 3DS banner")
    }
}

#[cfg(any(test, feature = "std"))]
impl std::error::Error for FormatError {}

/// `Result` of a read.
pub(crate) type R<T> = Result<T, FormatError>;

/// Positions are `i64` so file-supplied 32-bit offsets add up without wrapping.
#[derive(Clone, Copy)]
pub(crate) struct Reader<'a>(pub &'a [u8]);

impl Reader<'_> {
    fn check(&self, p: i64, n: i64) -> R<usize> {
        if p < 0 || n < 0 || p + n > self.0.len() as i64 { Err(FormatError) } else { Ok(p as usize) }
    }

    pub fn u8(&self, p: i64) -> R<i32> {
        Ok(i32::from(self.0[self.check(p, 1)?]))
    }

    pub fn u16(&self, p: i64) -> R<i32> {
        let i = self.check(p, 2)?;
        Ok(i32::from(u16::from_le_bytes([self.0[i], self.0[i + 1]])))
    }

    pub fn i32(&self, p: i64) -> R<i32> {
        let i = self.check(p, 4)?;
        Ok(i32::from_le_bytes([self.0[i], self.0[i + 1], self.0[i + 2], self.0[i + 3]]))
    }

    pub fn u32(&self, p: i64) -> R<u32> {
        self.i32(p).map(|v| v as u32)
    }

    pub fn f32(&self, p: i64) -> R<f32> {
        self.i32(p).map(|v| f32::from_bits(v as u32))
    }

    pub fn slice(&self, p: i64, n: i64) -> R<&[u8]> {
        let i = self.check(p, n)?;
        Ok(&self.0[i..i + n as usize])
    }

    /// A self-relative pointer at `p`: 0 stays 0, otherwise the absolute position it names (which must be inside the file).
    pub fn ptr(&self, p: i64) -> R<i64> {
        let v = self.i32(p)?;
        if v == 0 {
            return Ok(0);
        }
        let target = p + i64::from(v);
        if target < 0 || target >= self.0.len() as i64 { Err(FormatError) } else { Ok(target) }
    }

    /// A NUL-terminated string of at most 256 bytes, each byte one character (Latin-1).
    pub fn cstr(&self, p: i64) -> R<String> {
        self.check(p, 0)?;
        Ok(self.0[p as usize..].iter().take(256).take_while(|&&b| b != 0).map(|&b| char::from(b)).collect())
    }
}
