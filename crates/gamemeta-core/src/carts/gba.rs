// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Game Boy Advance: the 192-byte header at the start of the ROM (GBATEK, "GBA Cartridge Header").

use alloc::string::String;
use alloc::vec::Vec;

use super::{Field, ascii, num, text};
use crate::{GameIds, GameInfo, Language, Platform, Region, Title};

/// The header's length.
pub const HEADER_END: usize = 0xC0;

/// A Game Boy Advance header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The game title: 12 upper-case characters.
    pub title: String,
    /// The 4-character game code (`AXVE`): category, two letters, destination.
    pub game_code: String,
    /// The 2-character maker code.
    pub maker_code: String,
    /// The software version at 0xBC.
    pub version: u8,
    /// The complement check byte at 0xBD.
    pub complement: u8,
    /// 0xBD matches the check over 0xA0..=0xBC.
    pub complement_valid: bool,
}

/// The complement check over `rom[0xA0..=0xBC]`.
pub fn complement(rom: &[u8]) -> Option<u8> {
    let part = rom.get(0xA0..=0xBC)?;
    Some(part.iter().fold(0u8, |x, &b| x.wrapping_sub(b)).wrapping_sub(0x19))
}

/// Parses the first [`HEADER_END`] bytes. `None` when too short, when the fixed byte 0xB2 is not 0x96, or when the game
/// code is not four upper-case letters or digits.
pub fn parse(rom: &[u8]) -> Option<Header> {
    let h = rom.get(..HEADER_END)?;
    let code = &h[0xAC..0xB0];
    if h[0xB2] != 0x96 || !code.iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
        return None;
    }
    Some(Header {
        title: ascii(&h[0xA0..0xAC]),
        game_code: ascii(code),
        maker_code: ascii(&h[0xB0..0xB2]),
        version: h[0xBC],
        complement: h[0xBD],
        complement_valid: complement(h) == Some(h[0xBD]),
    })
}

impl Header {
    /// The regions the game code's last letter names.
    pub fn regions(&self) -> Vec<Region> {
        match self.game_code.as_bytes().get(3) {
            Some(b'J') => alloc::vec![Region::Japan],
            Some(b'E') => alloc::vec![Region::NorthAmerica],
            Some(b'U') => alloc::vec![Region::Australia],
            Some(b'K') => alloc::vec![Region::Korea],
            Some(b'C') => alloc::vec![Region::China],
            Some(b'P' | b'D' | b'F' | b'I' | b'S' | b'H' | b'X' | b'Y' | b'Z') => alloc::vec![Region::Europe],
            _ => Vec::new(),
        }
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, Field)> {
        alloc::vec![
            ("title", text(&self.title)),
            ("game_code", text(&self.game_code)),
            ("maker_code", text(&self.maker_code)),
            ("version", num(self.version)),
            ("complement", num(self.complement)),
            ("complement_valid", Field::Flag(self.complement_valid)),
        ]
    }

    pub(crate) fn info(&self) -> GameInfo {
        let mut info = GameInfo::new(Platform::GameBoyAdvance);
        if !self.title.is_empty() {
            let language = if self.game_code.ends_with('J') { Language::Japanese } else { Language::English };
            info.titles
                .push((language, Title { name: self.title.clone(), short_name: self.title.clone(), publisher: None }));
        }
        info.ids = GameIds {
            game_id: Some(self.game_code.clone()),
            maker_code: (!self.maker_code.is_empty()).then(|| self.maker_code.clone()),
            product_code: Some(alloc::format!("AGB-{}", self.game_code)),
            ..GameIds::default()
        };
        info.regions = self.regions();
        info.version = Some(u32::from(self.version));
        info.cart = Some(super::CartHeader::Gba(self.clone()));
        info
    }
}
