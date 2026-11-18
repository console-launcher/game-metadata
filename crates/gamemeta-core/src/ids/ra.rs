// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The RetroAchievements hash of a ROM where it falls out of one pass over the file: the platforms RA hashes whole, NES
//! (the PRG and CHR data after the iNES header), SNES (without a 512-byte copier header) and N64 (the whole file in the
//! big-endian `.z64` order, whatever order the file is in). Every other platform (discs, PC Engine, Lynx, DS, Atari 7800,
//! FDS) is skipped.

use alloc::string::String;

use super::get;
use crate::ByteSource;
use crate::carts::n64::ByteOrder;

pub(crate) const WHOLE: [&str; 13] =
    ["gb", "gbc", "gba", "md", "gen", "sms", "gg", "vb", "ws", "wsc", "ngp", "ngc", "a26"];
const SNES: [&str; 4] = ["sfc", "smc", "swc", "fig"];
const CHUNK: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Whole,
    Nes,
    Snes,
    N64,
}

/// A streaming RA hasher: feed it the file's bytes in order with [`update`](Self::update), then [`finish`](Self::finish).
pub struct RaHasher {
    kind: Kind,
    digest: md5::Context,
    header: [u8; 16],
    pos: u64,
    start: u64,
    end: u64,
    invalid: bool,
    /// N64: the bytes of the word being collected, and the order the file's first word showed.
    word: [u8; 4],
    word_len: usize,
    order: Option<ByteOrder>,
}

impl core::fmt::Debug for RaHasher {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RaHasher").field("kind", &self.kind).field("pos", &self.pos).finish_non_exhaustive()
    }
}

impl RaHasher {
    /// A hasher for `file_name` of `size` bytes (`None` when unknown), or `None` when its RA hash is not computed here.
    /// NES and SNES need the size (the copier header shows in it).
    pub fn for_file(file_name: &str, size: Option<u64>) -> Option<RaHasher> {
        let ext = file_name.rsplit_once('.').map_or("", |(_, e)| e);
        let is = |list: &[&str]| list.iter().any(|w| ext.eq_ignore_ascii_case(w));
        let (kind, size) = if is(&WHOLE) {
            (Kind::Whole, size.unwrap_or(0))
        } else if ext.eq_ignore_ascii_case("nes") {
            (Kind::Nes, size?)
        } else if is(&SNES) {
            (Kind::Snes, size?)
        } else if is(&["z64", "v64", "n64"]) {
            (Kind::N64, size.unwrap_or(0))
        } else {
            return None;
        };
        let start = if kind == Kind::Snes && size > 512 && size % 1024 == 512 { 512 } else { 0 };
        let end = if kind == Kind::Nes { 0 } else { u64::MAX };
        Some(RaHasher {
            kind,
            digest: md5::Context::new(),
            header: [0; 16],
            pos: 0,
            start,
            end,
            invalid: false,
            word: [0; 4],
            word_len: 0,
            order: None,
        })
    }

    /// The next bytes of the file.
    pub fn update(&mut self, mut buffer: &[u8]) {
        if self.kind == Kind::N64 {
            self.update_n64(buffer);
            return;
        }
        while !buffer.is_empty() && !self.invalid {
            if self.kind == Kind::Nes && self.pos < 16 {
                let n = (16 - self.pos).min(buffer.len() as u64) as usize;
                self.header[self.pos as usize..self.pos as usize + n].copy_from_slice(&buffer[..n]);
                self.pos += n as u64;
                buffer = &buffer[n..];
                if self.pos == 16 {
                    self.parse_nes_header();
                }
                continue;
            }
            let chunk = buffer.len() as u64;
            let lo = self.pos.max(self.start);
            let hi = (self.pos + chunk).min(self.end);
            if hi > lo {
                let from = (lo - self.pos) as usize;
                self.digest.consume(&buffer[from..from + (hi - lo) as usize]);
            }
            self.pos += chunk;
            buffer = &[];
        }
    }

    /// N64: whole 4-byte words are put in big-endian order (the order the first word shows) before they are hashed.
    fn update_n64(&mut self, mut buffer: &[u8]) {
        while !buffer.is_empty() && !self.invalid {
            let take = (4 - self.word_len).min(buffer.len());
            self.word[self.word_len..self.word_len + take].copy_from_slice(&buffer[..take]);
            self.word_len += take;
            self.pos += take as u64;
            buffer = &buffer[take..];
            if self.word_len < 4 {
                break;
            }
            self.word_len = 0;
            let Some(order) = self.order.or_else(|| ByteOrder::detect(&self.word)) else {
                self.invalid = true;
                break;
            };
            self.order = Some(order);
            order.normalize(&mut self.word);
            self.digest.consume(self.word);
        }
    }

    fn parse_nes_header(&mut self) {
        let h = &self.header;
        let data = u64::from(h[4]) * 16384 + u64::from(h[5]) * 8192;
        if h[..4] != [b'N', b'E', b'S', 0x1A] || data == 0 {
            self.invalid = true;
            return;
        }
        self.start = if h[6] & 4 != 0 { 16 + 512 } else { 16 };
        self.end = self.start + data;
    }

    /// The lower-case hex MD5, or `None` when the file turned out not to be hashable this way (a NES file without the
    /// header magic or cut short, an empty NES or SNES file).
    pub fn finish(mut self) -> Option<String> {
        let short_nes = self.kind == Kind::Nes && (self.pos < 16 || self.pos < self.end);
        if self.invalid || short_nes || (self.pos == 0 && self.kind != Kind::Whole) {
            return None;
        }
        if self.kind == Kind::N64 {
            // A tail shorter than a word (a ROM is always whole words): hashed as it came, after a first word that
            // named the order; a file shorter than one word has none.
            self.order?;
            self.digest.consume(&self.word[..self.word_len]);
        }
        Some(alloc::format!("{:x}", self.digest.finalize()))
    }
}

/// The RA hash of the whole of `source` for a file called `file_name`: reads it in 64 KiB pieces. `None` when the platform
/// is not hashed here, the file is not hashable, or a read failed.
pub fn ra_hash<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<String> {
    let size = source.size();
    let mut hasher = RaHasher::for_file(file_name, Some(size))?;
    let mut at = 0;
    while at < size {
        let chunk = get(source, at, (size - at).min(CHUNK))?;
        hasher.update(&chunk);
        at += chunk.len() as u64;
    }
    hasher.finish()
}
