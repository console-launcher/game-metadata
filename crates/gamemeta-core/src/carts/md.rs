// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sega Genesis / Mega Drive and 32X: the header at 0x100..0x200, in a plain image or a Super Magic Drive `.smd`
//! (a 512-byte header, then 16 KiB blocks with the odd bytes first) (Plutiedev "Mega Drive header", Sega Retro).

use alloc::string::String;
use alloc::vec::Vec;

use super::{Field, ascii, num, opt_num, opt_text, sjis, squeeze, text};
use crate::le::{be16_at, be32_at};
use crate::{GameIds, GameInfo, Language, Platform, Region, Title};

/// An SMD file's copier header, and each interleaved block.
pub const SMD_HEADER: u64 = 512;
/// The size of an interleaved block.
pub const SMD_BLOCK: u64 = 16 * 1024;
/// The bytes a header read needs: the SMD header and the first block.
pub const MAX_PREFIX: u64 = SMD_HEADER + SMD_BLOCK;
/// The header's end (the vectors precede it, the code starts here).
pub const HEADER_END: usize = 0x200;

/// How the image is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// A plain image (`.md .gen .bin .32x`).
    Plain,
    /// Super Magic Drive: copier header and 16 KiB blocks, odd bytes first.
    Smd,
}

/// A Mega Drive header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// How the file is stored.
    pub layout: Layout,
    /// The system string (`SEGA MEGA DRIVE`, `SEGA GENESIS`, `SEGA 32X`), trimmed.
    pub system: String,
    /// The system string says 32X.
    pub is_32x: bool,
    /// The copyright field (`(C)SEGA 1991.APR`), trimmed.
    pub copyright: String,
    /// The copyright holder: the text after `(C)` up to the first space (`SEGA`, `T-76`).
    pub copyright_holder: Option<String>,
    /// The rest of the copyright field after the holder (`1991.APR`).
    pub copyright_date: Option<String>,
    /// The domestic (Japanese) title, Shift_JIS decoded, spaces squeezed.
    pub domestic_title: String,
    /// The overseas title, spaces squeezed.
    pub overseas_title: String,
    /// The serial (`GM 00001009-00`), trimmed.
    pub serial: String,
    /// The 8 digits of a serial in the `XX NNNNNNNN-RR` form.
    pub serial_number: Option<String>,
    /// The revision digits after the dash of such a serial.
    pub revision: Option<u8>,
    /// The checksum at 0x18E (big-endian); see [`super::checksum_matches`].
    pub checksum: u16,
    /// The I/O support field, trimmed (`J`, `6`, `M`...).
    pub io_support: String,
    /// The ROM start address at 0x1A0.
    pub rom_start: u32,
    /// The ROM end address at 0x1A4 (inclusive).
    pub rom_end: u32,
    /// The region field, trimmed (`JUE`, `U`, `4`...).
    pub region_codes: String,
    /// The regions as bits: 1 Japan, 2 Asia, 4 Americas, 8 Europe (letters `J`, `U`, `E` map to 1, 4, 8).
    pub region_bits: u8,
}

/// Parses the header out of the image's first block, de-interleaved (a plain image's first [`HEADER_END`] bytes work too).
/// `None` unless `SEGA` starts the system string at 0x100 (or 0x101).
pub fn parse(rom: &[u8], layout: Layout) -> Option<Header> {
    let h = rom.get(..HEADER_END)?;
    if h[0x100..0x104] != *b"SEGA" && h[0x101..0x105] != *b"SEGA" {
        return None;
    }
    let system = ascii(&h[0x100..0x110]);
    let copyright = ascii(&h[0x110..0x120]);
    let (copyright_holder, copyright_date) = match copyright.strip_prefix("(C)") {
        Some(rest) => {
            let (holder, date) = rest.split_once(' ').unwrap_or((rest, ""));
            let non_empty = |s: &str| (!s.trim().is_empty()).then(|| String::from(s.trim()));
            (non_empty(holder), non_empty(date))
        }
        None => (None, None),
    };
    let serial = ascii(&h[0x180..0x18E]);
    let s = serial.as_bytes();
    let standard = s.len() == 14
        && s[2] == b' '
        && s[11] == b'-'
        && s[3..11].iter().all(u8::is_ascii_digit)
        && s[12..].iter().all(u8::is_ascii_digit);
    let region_codes = ascii(&h[0x1F0..0x200]);
    Some(Header {
        layout,
        is_32x: system.contains("32X"),
        system,
        copyright,
        copyright_holder,
        copyright_date,
        domestic_title: sjis(&h[0x120..0x150]),
        overseas_title: squeeze(&ascii(&h[0x150..0x180])),
        serial_number: standard.then(|| serial[3..11].into()),
        revision: standard.then(|| (s[12] - b'0') * 10 + (s[13] - b'0')),
        serial,
        checksum: be16_at(h, 0x18E)?,
        io_support: ascii(&h[0x190..0x1A0]),
        rom_start: be32_at(h, 0x1A0)?,
        rom_end: be32_at(h, 0x1A4)?,
        region_bits: region_bits(&region_codes),
        region_codes,
    })
}

/// The region field as bits: the old letters `J` `U` `E` (1, 4, 8), else one hex digit holding the bits.
fn region_bits(codes: &str) -> u8 {
    if codes.is_empty() {
        return 0;
    }
    if codes.chars().all(|c| matches!(c, 'J' | 'U' | 'E')) {
        return codes.chars().fold(0, |bits, c| {
            bits | match c {
                'J' => 1,
                'U' => 4,
                _ => 8,
            }
        });
    }
    let mut chars = codes.chars();
    match (chars.next().and_then(|c| c.to_digit(16)), chars.next()) {
        (Some(d), None) => d as u8,
        _ => 0,
    }
}

/// Whether a file's first bytes and size say it is a Super Magic Drive file.
pub fn is_smd(first: &[u8], size: u64) -> bool {
    first.len() >= 10
        && first[8] == 0xAA
        && first[9] == 0xBB
        && first.get(10) == Some(&6)
        && size % SMD_BLOCK == SMD_HEADER
}

/// Un-interleaves one 16 KiB SMD block: the first half holds the odd bytes, the second half the even ones.
pub fn deinterleave(block: &[u8]) -> Vec<u8> {
    let half = block.len() / 2;
    let mut out = alloc::vec![0u8; half * 2];
    for i in 0..half {
        out[2 * i + 1] = block[i];
        out[2 * i] = block[half + i];
    }
    out
}

impl Header {
    /// The regions the region field names.
    pub fn regions(&self) -> Vec<Region> {
        let bits = self.region_bits;
        let mut out = Vec::new();
        if bits & 1 != 0 {
            out.push(Region::Japan);
        }
        if bits & 4 != 0 {
            out.push(Region::NorthAmerica);
        }
        if bits & 8 != 0 {
            out.push(Region::Europe);
        }
        out
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, Field)> {
        alloc::vec![
            ("layout", num(self.layout as i64)),
            ("system", text(&self.system)),
            ("is_32x", Field::Flag(self.is_32x)),
            ("copyright", text(&self.copyright)),
            ("copyright_holder", opt_text(&self.copyright_holder)),
            ("copyright_date", opt_text(&self.copyright_date)),
            ("domestic_title", text(&self.domestic_title)),
            ("overseas_title", text(&self.overseas_title)),
            ("serial", text(&self.serial)),
            ("serial_number", opt_text(&self.serial_number)),
            ("revision", opt_num(self.revision)),
            ("checksum", num(self.checksum)),
            ("io_support", text(&self.io_support)),
            ("rom_start", num(self.rom_start)),
            ("rom_end", num(self.rom_end)),
            ("region_codes", text(&self.region_codes)),
            ("region_bits", num(self.region_bits)),
        ]
    }

    pub(crate) fn info(&self) -> GameInfo {
        let mut info = GameInfo::new(if self.is_32x { Platform::Sega32x } else { Platform::MegaDrive });
        for (language, name) in [(Language::Japanese, &self.domestic_title), (Language::English, &self.overseas_title)]
        {
            if !name.is_empty() {
                let publisher = self.copyright_holder.clone().filter(|h| !h.starts_with("T-"));
                info.titles.push((language, Title { name: name.clone(), short_name: name.clone(), publisher }));
            }
        }
        info.ids = GameIds {
            product_code: (!self.serial.is_empty()).then(|| self.serial.clone()),
            game_id: self.serial_number.clone(),
            maker_code: self.copyright_holder.clone(),
            ..GameIds::default()
        };
        info.regions = self.regions();
        info.version = self.revision.map(u32::from);
        info.cart = Some(super::CartHeader::MegaDrive(self.clone()));
        info
    }
}
