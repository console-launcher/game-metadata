// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Game Boy and Game Boy Color: the header at 0x100..0x150 (Pan Docs, "The Cartridge Header").

use alloc::string::String;
use alloc::vec::Vec;

use super::{Field, ascii, num, opt_text, size_field, text};
use crate::{GameIds, GameInfo, Language, Platform, Region, Title};

/// The header's length from 0x100.
pub const HEADER_END: usize = 0x150;

/// The Game Boy Color flag at 0x143.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cgb {
    /// Neither 0x80 nor 0xC0: a plain Game Boy cartridge.
    No,
    /// 0x80: works on a Game Boy too, with Color enhancements.
    Compatible,
    /// 0xC0: Game Boy Color only.
    Only,
}

/// A Game Boy / Game Boy Color header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The title: 16 characters, or 15 when the Color flag is set, or 11 when a manufacturer code follows.
    pub title: String,
    /// The 4-character manufacturer code at 0x13F, on Color cartridges that have one.
    pub manufacturer_code: Option<String>,
    /// The Color flag.
    pub cgb: Cgb,
    /// 0x146 is 0x03: Super Game Boy functions.
    pub sgb: bool,
    /// The old licensee code at 0x14B (0x33: the new code is used).
    pub old_licensee: u8,
    /// The new licensee code at 0x144 (two characters), when the old code is 0x33.
    pub new_licensee: Option<String>,
    /// The cartridge type (mapper and extras) at 0x147.
    pub cart_type: u8,
    /// The ROM size code at 0x148.
    pub rom_size_code: u8,
    /// The ROM size in bytes (32 KiB << code, codes 0 to 8).
    pub rom_size: Option<u64>,
    /// The RAM size code at 0x149.
    pub ram_size_code: u8,
    /// The RAM size in bytes (code 0: none, 2: 8 KiB, 3: 32 KiB, 4: 128 KiB, 5: 64 KiB).
    pub ram_size: Option<u64>,
    /// The destination code at 0x14A: 0 Japan, 1 overseas.
    pub destination: u8,
    /// The mask ROM version number at 0x14C.
    pub version: u8,
    /// The header checksum byte at 0x14D.
    pub header_checksum: u8,
    /// 0x14D matches the check over 0x134..=0x14C.
    pub header_checksum_valid: bool,
    /// The global checksum at 0x14E (big-endian); see [`super::checksum_matches`].
    pub global_checksum: u16,
    /// The 48 bytes at 0x104 are the Nintendo logo the boot ROM compares.
    pub logo_valid: bool,
}

/// SHA-256 of the 48 bytes at 0x104 that the boot ROM compares. The library holds the digest, not the bitmap.
const LOGO_SHA256: [u8; 32] = [
    0xda, 0xf4, 0xca, 0xbd, 0xc8, 0x52, 0xba, 0xa0, 0x29, 0x18, 0x49, 0x20, 0x3f, 0x0b, 0x41, 0xfd, 0x0b, 0x4e, 0xcd,
    0x58, 0xe0, 0xd7, 0xaf, 0xf4, 0xa5, 0x09, 0xf5, 0xde, 0x4d, 0x7f, 0x9a, 0x2e,
];

/// The header checksum over `rom[0x134..=0x14C]`.
pub fn header_checksum(rom: &[u8]) -> Option<u8> {
    let part = rom.get(0x134..=0x14C)?;
    Some(part.iter().fold(0u8, |x, &b| x.wrapping_sub(b).wrapping_sub(1)))
}

/// Parses the file's first bytes (at least [`HEADER_END`]). `None` when it is too short, or when neither the logo nor
/// the header checksum is right (nothing says it is a Game Boy header).
pub fn parse(rom: &[u8]) -> Option<Header> {
    let h = rom.get(..HEADER_END)?;
    let logo_valid = crate::sha256::sha256(&h[0x104..0x134]) == LOGO_SHA256;
    let header_checksum = h[0x14D];
    let header_checksum_valid = self::header_checksum(h) == Some(header_checksum);
    if !logo_valid && !header_checksum_valid {
        return None;
    }
    let cgb = match h[0x143] {
        0x80 => Cgb::Compatible,
        0xC0 => Cgb::Only,
        _ => Cgb::No,
    };
    let code = &h[0x13F..0x143];
    let has_code = cgb != Cgb::No && code.iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    let manufacturer_code = has_code.then(|| ascii(code));
    let title = match (cgb, has_code) {
        (_, true) => ascii(&h[0x134..0x13F]),
        (Cgb::No, false) => ascii(&h[0x134..0x144]),
        (_, false) => ascii(&h[0x134..0x143]),
    };
    let old_licensee = h[0x14B];
    let new_licensee =
        Some(&h[0x144..0x146]).filter(|n| old_licensee == 0x33 && n.iter().all(u8::is_ascii_alphanumeric)).map(ascii);
    let (rom_size_code, ram_size_code) = (h[0x148], h[0x149]);
    Some(Header {
        title,
        manufacturer_code,
        cgb,
        sgb: h[0x146] == 0x03,
        old_licensee,
        new_licensee,
        cart_type: h[0x147],
        rom_size_code,
        rom_size: (rom_size_code <= 8).then(|| (32 * 1024u64) << rom_size_code),
        ram_size_code,
        ram_size: match ram_size_code {
            0 => Some(0),
            2 => Some(8 << 10),
            3 => Some(32 << 10),
            4 => Some(128 << 10),
            5 => Some(64 << 10),
            _ => None,
        },
        destination: h[0x14A],
        version: h[0x14C],
        header_checksum,
        header_checksum_valid,
        global_checksum: u16::from_be_bytes([h[0x14E], h[0x14F]]),
        logo_valid,
    })
}

impl Header {
    /// The licensee as text: the new code's two characters, else the old code as two hex digits.
    pub fn licensee(&self) -> Option<String> {
        match (&self.new_licensee, self.old_licensee) {
            (Some(new), 0x33) => Some(new.clone()),
            (_, 0x33) => None,
            (_, old) => Some(alloc::format!("{old:02X}")),
        }
    }

    /// The regions the destination code names.
    pub fn regions(&self) -> Vec<Region> {
        match self.destination {
            0 => alloc::vec![Region::Japan],
            1 => alloc::vec![Region::NorthAmerica, Region::Europe],
            _ => Vec::new(),
        }
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, Field)> {
        alloc::vec![
            ("title", text(&self.title)),
            ("manufacturer_code", opt_text(&self.manufacturer_code)),
            ("cgb", num(self.cgb as i64)),
            ("sgb", Field::Flag(self.sgb)),
            ("licensee", opt_text(&self.licensee())),
            ("old_licensee", num(self.old_licensee)),
            ("new_licensee", opt_text(&self.new_licensee)),
            ("cart_type", num(self.cart_type)),
            ("rom_size", size_field(self.rom_size)),
            ("ram_size", size_field(self.ram_size)),
            ("destination", num(self.destination)),
            ("version", num(self.version)),
            ("header_checksum", num(self.header_checksum)),
            ("header_checksum_valid", Field::Flag(self.header_checksum_valid)),
            ("global_checksum", num(self.global_checksum)),
            ("logo_valid", Field::Flag(self.logo_valid)),
        ]
    }

    pub(crate) fn info(&self) -> GameInfo {
        let platform = if self.cgb == Cgb::No { Platform::GameBoy } else { Platform::GameBoyColor };
        let mut info = GameInfo::new(platform);
        if !self.title.is_empty() {
            let language = if self.destination == 0 { Language::Japanese } else { Language::English };
            info.titles
                .push((language, Title { name: self.title.clone(), short_name: self.title.clone(), publisher: None }));
        }
        info.ids =
            GameIds { game_id: self.manufacturer_code.clone(), maker_code: self.licensee(), ..GameIds::default() };
        info.regions = self.regions();
        info.version = Some(u32::from(self.version));
        info.cart = Some(super::CartHeader::GameBoy(self.clone()));
        info
    }
}
