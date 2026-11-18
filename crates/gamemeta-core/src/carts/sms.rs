// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sega Master System and Game Gear: the `TMR SEGA` header at 0x7FF0, 0x3FF0 or 0x1FF0 (SMS Power, "ROM header").

use alloc::vec::Vec;

use super::{Field, num, size_field};
use crate::{GameIds, GameInfo, Platform, Region};

/// The header's length.
pub const HEADER_LEN: usize = 16;
/// Where the header can sit, in the order they are tried.
pub const OFFSETS: [u64; 3] = [0x7FF0, 0x3FF0, 0x1FF0];

/// A `TMR SEGA` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Where the header was found (one of [`OFFSETS`]).
    pub offset: u64,
    /// The checksum at +0x0A (little-endian); see [`super::checksum_matches`].
    pub checksum: u16,
    /// The product number: 5 BCD digits (the fifth is the high nibble of +0x0E).
    pub product: u32,
    /// The version: the low nibble of +0x0E.
    pub version: u8,
    /// The region code: the high nibble of +0x0F (3 SMS Japan, 4 SMS export, 5 GG Japan, 6 GG export, 7 GG international).
    pub region_code: u8,
    /// The ROM size code: the low nibble of +0x0F.
    pub size_code: u8,
    /// The ROM size in bytes the code names (0xA 8 KiB ... 0x2 1 MiB), `None` for an unused code.
    pub rom_size: Option<u64>,
    /// The Game Gear (region code 5 to 7), not the Master System.
    pub game_gear: bool,
}

/// Parses the 16 header bytes found at `offset`. `None` unless they start with `TMR SEGA`. `extension_gg` decides
/// Master System versus Game Gear only when the region code is not 3 to 7.
pub fn parse(bytes: &[u8], offset: u64, extension_gg: bool) -> Option<Header> {
    let h = bytes.get(..HEADER_LEN)?;
    if h[..8] != *b"TMR SEGA" {
        return None;
    }
    let bcd = |b: u8| u32::from(b >> 4) * 10 + u32::from(b & 0xF);
    let (region_code, size_code) = (h[0xF] >> 4, h[0xF] & 0xF);
    Some(Header {
        offset,
        checksum: u16::from_le_bytes([h[0xA], h[0xB]]),
        product: u32::from(h[0xE] >> 4) * 10_000 + bcd(h[0xD]) * 100 + bcd(h[0xC]),
        version: h[0xE] & 0xF,
        region_code,
        size_code,
        rom_size: Header::size_for(size_code),
        game_gear: match region_code {
            3 | 4 => false,
            5..=7 => true,
            _ => extension_gg,
        },
    })
}

impl Header {
    /// The ROM size a code names.
    pub fn size_for(code: u8) -> Option<u64> {
        Some(match code {
            0xA => 8 << 10,
            0xB => 16 << 10,
            0xC => 32 << 10,
            0xD => 48 << 10,
            0xE => 64 << 10,
            0xF => 128 << 10,
            0x0 => 256 << 10,
            0x1 => 512 << 10,
            0x2 => 1 << 20,
            _ => return None,
        })
    }

    /// The regions the region code names; 7 (international) is none, with [`GameInfo::region_free`] set.
    pub fn regions(&self) -> Vec<Region> {
        match self.region_code {
            3 | 5 => alloc::vec![Region::Japan],
            4 | 6 => alloc::vec![Region::NorthAmerica, Region::Europe],
            _ => Vec::new(),
        }
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, Field)> {
        alloc::vec![
            ("offset", num(self.offset as i64)),
            ("checksum", num(self.checksum)),
            ("product", num(self.product)),
            ("version", num(self.version)),
            ("region_code", num(self.region_code)),
            ("size_code", num(self.size_code)),
            ("rom_size", size_field(self.rom_size)),
            ("game_gear", Field::Flag(self.game_gear)),
        ]
    }

    pub(crate) fn info(&self) -> GameInfo {
        let mut info = GameInfo::new(if self.game_gear { Platform::GameGear } else { Platform::MasterSystem });
        info.ids = GameIds { product_code: Some(alloc::format!("{:04}", self.product)), ..GameIds::default() };
        info.regions = self.regions();
        info.region_free = self.region_code == 7;
        info.version = Some(u32::from(self.version));
        info.cart = Some(super::CartHeader::MasterSystem(self.clone()));
        info
    }
}
