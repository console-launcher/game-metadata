// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared helpers of the cartridge reader tests.
#![allow(dead_code)]

use gamemeta_core::{ByteSource, GameInfo, ReadError, SliceSource, read};

/// What `read` gives for `bytes` named `name`.
pub fn info(bytes: &[u8], name: &str) -> Option<GameInfo> {
    read(&mut SliceSource::new(bytes), name)
}

/// The same, expecting a result.
pub fn load(bytes: &[u8], name: &str) -> GameInfo {
    info(bytes, name).unwrap_or_else(|| panic!("{name}: expected a header"))
}

/// A source that counts the bytes asked of it.
pub struct Counting<'a> {
    pub inner: SliceSource<'a>,
    pub asked: u64,
}

impl<'a> Counting<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Counting { inner: SliceSource::new(bytes), asked: 0 }
    }
}

impl ByteSource for Counting<'_> {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        self.asked += buf.len() as u64;
        self.inner.read_at(offset, buf)
    }
}

/// A source whose reads all fail.
pub struct Failing(pub u64);

impl ByteSource for Failing {
    fn size(&self) -> u64 {
        self.0
    }
    fn read_at(&mut self, _: u64, _: &mut [u8]) -> Result<(), ReadError> {
        Err(ReadError::Io)
    }
}
