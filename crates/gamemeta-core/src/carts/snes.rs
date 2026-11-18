// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Super Nintendo / Super Famicom: the internal header of a LoROM, HiROM or ExHiROM image, with or without a 512-byte
//! copier header in front (SNESdev wiki, "ROM header").

use alloc::string::String;
use alloc::vec::Vec;

use super::{Field, ascii, num, opt_text, size_field, text};
use crate::{GameIds, GameInfo, Language, Platform, Region, Title};

/// The farthest byte a header can sit at (ExHiROM at 0x40FFC0, plus a copier header, plus its 64 bytes).
pub const MAX_PREFIX: u64 = 0x40FFC0 + 512 + 0x40;

/// The ROM mapping, told by where a plausible header sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// The header at 0x7FC0.
    LoRom,
    /// The header at 0xFFC0.
    HiRom,
    /// The header at 0x40FFC0.
    ExHiRom,
}

/// A Super Nintendo internal header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Where the header was found.
    pub layout: Layout,
    /// The file starts with a 512-byte copier header (size is 512 over a multiple of 1024).
    pub copier_header: bool,
    /// The 21-byte title (JIS X 0201: half-width katakana kept), trailing spaces trimmed.
    pub title: String,
    /// The map mode byte (+0x15).
    pub map_mode: u8,
    /// The map mode's FastROM bit (0x10).
    pub fast_rom: bool,
    /// The cartridge type byte (+0x16): ROM, RAM, battery, coprocessor.
    pub rom_type: u8,
    /// The declared ROM size in bytes (1 KiB << code).
    pub rom_size: Option<u64>,
    /// The declared RAM size in bytes (1 KiB << code; code 0 is none).
    pub ram_size: Option<u64>,
    /// The destination (country) code (+0x19).
    pub destination: u8,
    /// The developer id (+0x1A); 0x33 means the extended header names the maker.
    pub developer_id: u8,
    /// The 2-character maker code of the extended header, when the developer id is 0x33.
    pub maker_code: Option<String>,
    /// The 4-character game code of the extended header, when the developer id is 0x33.
    pub game_code: Option<String>,
    /// The mask ROM version (+0x1B).
    pub version: u8,
    /// The checksum complement (+0x1C).
    pub complement: u16,
    /// The checksum (+0x1E).
    pub checksum: u16,
    /// The complement and the checksum are each other's bitwise inverse (the sum itself is not recomputed).
    pub checksum_valid: bool,
}

/// A header must score at least this to count.
pub const MIN_SCORE: u32 = 6;
/// The bytes `parse_window` takes: the 16 extended-header bytes, then the 64 of the header.
pub const WINDOW: usize = 0x50;

const BASES: [(Layout, u64); 3] = [(Layout::LoRom, 0x7FC0), (Layout::HiRom, 0xFFC0), (Layout::ExHiRom, 0x40FFC0)];

/// A file of `size` bytes starts with a copier header: 512 bytes over a whole number of kilobytes.
pub fn copier_header(size: u64) -> bool {
    size > 512 && size % 1024 == 512
}

/// Where each layout's window starts in a file of `size` bytes (copier header allowed for), for the layouts whose header
/// fits in the file.
pub fn windows(size: u64) -> Vec<(Layout, u64)> {
    let shift = if copier_header(size) { 512 } else { 0 };
    BASES
        .iter()
        .map(|&(layout, base)| (layout, base - 0x10 + shift))
        .filter(|&(_, start)| start + WINDOW as u64 <= size)
        .collect()
}

fn jis_title(bytes: &[u8]) -> String {
    let text: String = bytes
        .iter()
        .filter_map(|&b| match b {
            0x5C => Some('\u{A5}'),
            0x7E => Some('\u{203E}'),
            0x20..=0x7E => Some(char::from(b)),
            0xA1..=0xDF => char::from_u32(0xFF61 + u32::from(b - 0xA1)),
            _ => None,
        })
        .collect();
    text.trim().into()
}

fn code(bytes: &[u8]) -> Option<String> {
    let s = ascii(bytes);
    (!s.is_empty()).then_some(s)
}

/// Reads one candidate header out of its `window` ([`WINDOW`] bytes at [`windows`]' start) and scores how plausible it is.
/// Compare the score with [`MIN_SCORE`]; `None` only when the window is short.
pub fn parse_window(window: &[u8], layout: Layout, copier: bool) -> Option<(u32, Header)> {
    let x = window.get(..WINDOW)?;
    let h = &x[0x10..];
    let (map_mode, complement, checksum) =
        (h[0x15], u16::from_le_bytes([h[0x1C], h[0x1D]]), u16::from_le_bytes([h[0x1E], h[0x1F]]));
    let reset = u16::from_le_bytes([h[0x3C], h[0x3D]]);
    let checksum_valid = complement ^ checksum == 0xFFFF;
    let nibble_fits = match layout {
        Layout::LoRom => matches!(map_mode & 0xF, 0 | 2 | 3),
        Layout::HiRom => matches!(map_mode & 0xF, 1 | 0xA),
        Layout::ExHiRom => map_mode & 0xF == 5,
    };
    let printable = h[..21].iter().all(|b| matches!(b, 0x20..=0x7E | 0xA1..=0xDF));
    let score = 4 * u32::from(checksum_valid)
        + u32::from(map_mode & 0xE0 == 0x20)
        + 2 * u32::from(nibble_fits)
        + 2 * u32::from(reset >= 0x8000)
        + 2 * u32::from(printable)
        + u32::from((5..=0x0D).contains(&h[0x17]))
        + u32::from(h[0x19] <= 0x14);
    let developer_id = h[0x1A];
    let extended = developer_id == 0x33;
    let header = Header {
        layout,
        copier_header: copier,
        title: jis_title(&h[..21]),
        map_mode,
        fast_rom: map_mode & 0x10 != 0,
        rom_type: h[0x16],
        rom_size: (h[0x17] <= 0x10).then(|| 1024u64 << h[0x17]),
        ram_size: match h[0x18] {
            0 => Some(0),
            n if n <= 0x10 => Some(1024u64 << n),
            _ => None,
        },
        destination: h[0x19],
        developer_id,
        maker_code: if extended { code(&x[..2]) } else { None },
        game_code: if extended { code(&x[2..6]) } else { None },
        version: h[0x1B],
        complement,
        checksum,
        checksum_valid,
    };
    Some((score, header))
}

/// The highest-scoring candidate (the first on a tie) that reaches [`MIN_SCORE`].
pub fn best(candidates: impl Iterator<Item = (u32, Header)>) -> Option<Header> {
    let mut best: Option<(u32, Header)> = None;
    for (score, header) in candidates {
        if score >= MIN_SCORE && best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, header));
        }
    }
    best.map(|(_, h)| h)
}

/// Reads the ROM image in `file` (all of it, or its first bytes) of `size` bytes in total: tries every layout whose
/// header fits and keeps the best. `None` when no position scores as a header.
pub fn parse(file: &[u8], size: u64) -> Option<Header> {
    let copier = copier_header(size);
    best(windows(size).into_iter().filter_map(|(layout, start)| {
        let start = usize::try_from(start).ok()?;
        parse_window(file.get(start..start.checked_add(WINDOW)?)?, layout, copier)
    }))
}

impl Header {
    /// The regions the destination code names.
    pub fn regions(&self) -> Vec<Region> {
        match self.destination {
            0 => alloc::vec![Region::Japan],
            1 | 0xF => alloc::vec![Region::NorthAmerica],
            2..=0xA => alloc::vec![Region::Europe],
            0xB => alloc::vec![Region::China],
            0xD => alloc::vec![Region::Korea],
            0x11 => alloc::vec![Region::Australia],
            _ => Vec::new(),
        }
    }

    /// The maker as text: the extended code, else the developer id as two hex digits.
    pub fn maker(&self) -> String {
        self.maker_code.clone().unwrap_or_else(|| alloc::format!("{:02X}", self.developer_id))
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, Field)> {
        alloc::vec![
            ("layout", num(self.layout as i64)),
            ("copier_header", Field::Flag(self.copier_header)),
            ("title", text(&self.title)),
            ("map_mode", num(self.map_mode)),
            ("fast_rom", Field::Flag(self.fast_rom)),
            ("rom_type", num(self.rom_type)),
            ("rom_size", size_field(self.rom_size)),
            ("ram_size", size_field(self.ram_size)),
            ("destination", num(self.destination)),
            ("developer_id", num(self.developer_id)),
            ("maker_code", text(&self.maker())),
            ("game_code", opt_text(&self.game_code)),
            ("version", num(self.version)),
            ("complement", num(self.complement)),
            ("checksum", num(self.checksum)),
            ("checksum_valid", Field::Flag(self.checksum_valid)),
        ]
    }

    pub(crate) fn info(&self) -> GameInfo {
        let mut info = GameInfo::new(Platform::Snes);
        if !self.title.is_empty() {
            let language = if self.destination == 0 { Language::Japanese } else { Language::English };
            info.titles
                .push((language, Title { name: self.title.clone(), short_name: self.title.clone(), publisher: None }));
        }
        info.ids = GameIds { game_id: self.game_code.clone(), maker_code: Some(self.maker()), ..GameIds::default() };
        info.regions = self.regions();
        info.version = Some(u32::from(self.version));
        info.cart = Some(super::CartHeader::Snes(self.clone()));
        info
    }
}
