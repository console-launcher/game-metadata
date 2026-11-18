// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Nintendo 64: the 64-byte header of a cartridge image in any of the three byte orders (n64brew, "ROM Header").

use alloc::string::String;
use alloc::vec::Vec;

use super::{Field, ascii, num, sjis, text};
use crate::le::be32_at;
use crate::{GameIds, GameInfo, Language, Platform, Region, Title};

/// The header's length.
pub const HEADER_END: usize = 0x40;

/// How the image stores its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    /// `.z64`: the native big-endian order; the file starts `80 37 12 40`.
    BigEndian,
    /// `.v64`: every pair of bytes swapped; starts `37 80 40 12`.
    ByteSwapped,
    /// `.n64`: every word reversed (little-endian); starts `40 12 37 80`.
    LittleEndian,
}

impl ByteOrder {
    /// The order a file's first four bytes show, `None` for none of the three.
    pub fn detect(first4: &[u8]) -> Option<ByteOrder> {
        match first4.get(..4)? {
            [0x80, 0x37, 0x12, 0x40] => Some(ByteOrder::BigEndian),
            [0x37, 0x80, 0x40, 0x12] => Some(ByteOrder::ByteSwapped),
            [0x40, 0x12, 0x37, 0x80] => Some(ByteOrder::LittleEndian),
            _ => None,
        }
    }

    /// Rewrites the whole 4-byte words of `bytes` from this order to big-endian, in place. A tail shorter than a word
    /// is left as it is.
    pub fn normalize(self, bytes: &mut [u8]) {
        match self {
            ByteOrder::BigEndian => {}
            ByteOrder::ByteSwapped => bytes.chunks_exact_mut(2).for_each(|p| p.swap(0, 1)),
            ByteOrder::LittleEndian => bytes.chunks_exact_mut(4).for_each(|w| w.reverse()),
        }
    }
}

/// A Nintendo 64 header, as read from the big-endian image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The byte order the file was stored in.
    pub byte_order: ByteOrder,
    /// The 20-character title at 0x20 (Shift_JIS, space padded).
    pub title: String,
    /// The category byte at 0x3B (`N` cartridge, `C` expansion, `D`/`E` 64DD, `Z` aleck64).
    pub category: u8,
    /// The 2-character game id at 0x3C.
    pub game_id: String,
    /// The country code byte at 0x3E.
    pub country: u8,
    /// The version at 0x3F.
    pub version: u8,
    /// CRC1 at 0x10.
    pub crc1: u32,
    /// CRC2 at 0x14.
    pub crc2: u32,
}

/// Parses the first [`HEADER_END`] bytes in any byte order. `None` when too short or the first word is not one of the
/// three signatures.
pub fn parse(first: &[u8]) -> Option<Header> {
    let mut h = first.get(..HEADER_END)?.to_vec();
    let byte_order = ByteOrder::detect(&h)?;
    byte_order.normalize(&mut h);
    Some(Header {
        byte_order,
        title: sjis(&h[0x20..0x34]),
        category: h[0x3B],
        game_id: ascii(&h[0x3C..0x3E]),
        country: h[0x3E],
        version: h[0x3F],
        crc1: be32_at(&h, 0x10)?,
        crc2: be32_at(&h, 0x14)?,
    })
}

impl Header {
    /// The 4-character game code: category, game id, country (`NSME`).
    pub fn game_code(&self) -> String {
        let mut code = String::new();
        code.push(char::from(self.category));
        code.push_str(&self.game_id);
        code.push(char::from(self.country));
        code
    }

    /// The regions the country code names.
    pub fn regions(&self) -> Vec<Region> {
        match self.country {
            b'J' => alloc::vec![Region::Japan],
            b'E' | b'N' => alloc::vec![Region::NorthAmerica],
            b'U' => alloc::vec![Region::Australia],
            b'K' => alloc::vec![Region::Korea],
            b'C' => alloc::vec![Region::China],
            b'P' | b'D' | b'F' | b'H' | b'I' | b'S' | b'W' | b'X' | b'Y' | b'Z' => alloc::vec![Region::Europe],
            _ => Vec::new(),
        }
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, Field)> {
        alloc::vec![
            ("byte_order", num(self.byte_order as i64)),
            ("title", text(&self.title)),
            ("game_code", text(&self.game_code())),
            ("country", num(self.country)),
            ("version", num(self.version)),
            ("crc1", num(self.crc1)),
            ("crc2", num(self.crc2)),
        ]
    }

    pub(crate) fn info(&self) -> GameInfo {
        let mut info = GameInfo::new(Platform::Nintendo64);
        if !self.title.is_empty() {
            let language = if self.country == b'J' { Language::Japanese } else { Language::English };
            info.titles
                .push((language, Title { name: self.title.clone(), short_name: self.title.clone(), publisher: None }));
        }
        let code = self.game_code();
        info.ids =
            GameIds { game_id: code.chars().all(|c| c.is_ascii_alphanumeric()).then_some(code), ..GameIds::default() };
        info.regions = self.regions();
        info.version = Some(u32::from(self.version));
        info.cart = Some(super::CartHeader::N64(self.clone()));
        info
    }
}
