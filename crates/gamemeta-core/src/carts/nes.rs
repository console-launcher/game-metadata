// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! NES / Famicom: the 16-byte iNES or NES 2.0 header (NESdev wiki, "INES" and "NES 2.0").

use alloc::vec::Vec;

use super::{Field, num, opt_num, size_field};
use crate::{GameInfo, Platform, Region};

/// The header's length.
pub const HEADER_END: usize = 16;

/// Which header dialect the file uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// iNES (byte 7 bits 2-3 are 0, bytes 12-15 are 0).
    INes,
    /// NES 2.0 (byte 7 bits 2-3 are 2).
    Nes2,
    /// An old iNES dump with junk in the upper header bytes: the upper mapper nibble is not trusted.
    Archaic,
}

/// How the cartridge wires the nametables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    /// Horizontal (vertical arrangement; byte 6 bit 0 clear).
    Horizontal,
    /// Vertical (byte 6 bit 0 set).
    Vertical,
    /// The cartridge supplies four-screen VRAM (byte 6 bit 3).
    FourScreen,
}

/// CPU/PPU timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    /// NTSC (RP2C02).
    Ntsc,
    /// PAL (RP2C07).
    Pal,
    /// Works in several regions.
    Multi,
    /// Dendy (UA6538).
    Dendy,
}

/// The console the ROM is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Console {
    /// NES, Famicom or Dendy.
    Standard,
    /// Vs. System.
    VsSystem,
    /// PlayChoice-10.
    PlayChoice10,
    /// NES 2.0 extended console type (byte 13's low nibble).
    Extended(u8),
}

/// An iNES / NES 2.0 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The header dialect.
    pub format: Format,
    /// The mapper number (8 bits in iNES, 12 in NES 2.0).
    pub mapper: u16,
    /// The NES 2.0 submapper.
    pub submapper: Option<u8>,
    /// PRG ROM size in bytes.
    pub prg_rom: u64,
    /// CHR ROM size in bytes (0: the board has CHR RAM).
    pub chr_rom: u64,
    /// PRG RAM in bytes: NES 2.0's volatile size; iNES's byte 8 in 8 KiB units as written (0: not stated).
    pub prg_ram: u64,
    /// PRG NVRAM (battery-backed) in bytes, NES 2.0 only.
    pub prg_nvram: u64,
    /// CHR RAM in bytes, NES 2.0 only.
    pub chr_ram: u64,
    /// CHR NVRAM in bytes, NES 2.0 only.
    pub chr_nvram: u64,
    /// Nametable arrangement.
    pub mirroring: Mirroring,
    /// Battery-backed memory (byte 6 bit 1).
    pub battery: bool,
    /// A 512-byte trainer follows the header (byte 6 bit 2).
    pub trainer: bool,
    /// The timing: NES 2.0 byte 12; iNES byte 9 bit 0 (NTSC or PAL).
    pub timing: Timing,
    /// The console type.
    pub console: Console,
}

/// Parses the 16-byte header. `None` for a file that is too short, lacks `NES\x1A`, or declares no PRG ROM.
pub fn parse(bytes: &[u8]) -> Option<Header> {
    let h = bytes.get(..HEADER_END)?;
    if h[..4] != *b"NES\x1A" {
        return None;
    }
    let format = match h[7] & 0x0C {
        0x08 => Format::Nes2,
        0 if h[12..16].iter().all(|&b| b == 0) => Format::INes,
        _ => Format::Archaic,
    };
    let low = u16::from(h[6] >> 4);
    let nes2 = format == Format::Nes2;
    let mapper = match format {
        Format::Archaic => low,
        Format::INes => low | u16::from(h[7] & 0xF0),
        Format::Nes2 => low | u16::from(h[7] & 0xF0) | u16::from(h[8] & 0xF) << 8,
    };
    let (prg_rom, chr_rom) = if nes2 {
        (rom_size(h[4], h[9] & 0xF, 16384)?, rom_size(h[5], h[9] >> 4, 8192)?)
    } else {
        (u64::from(h[4]) * 16384, u64::from(h[5]) * 8192)
    };
    if prg_rom == 0 {
        return None;
    }
    let ram = |shift: u8| if shift == 0 { 0 } else { 64u64 << shift };
    let (prg_ram, prg_nvram, chr_ram, chr_nvram) = if nes2 {
        (ram(h[10] & 0xF), ram(h[10] >> 4), ram(h[11] & 0xF), ram(h[11] >> 4))
    } else if format == Format::INes {
        (u64::from(h[8]) * 8192, 0, 0, 0)
    } else {
        (0, 0, 0, 0)
    };
    let timing = if nes2 {
        [Timing::Ntsc, Timing::Pal, Timing::Multi, Timing::Dendy][usize::from(h[12] & 3)]
    } else if format == Format::INes && h[9] & 1 != 0 {
        Timing::Pal
    } else {
        Timing::Ntsc
    };
    let console = match (format, h[7] & 3) {
        (Format::Archaic, _) | (_, 0) => Console::Standard,
        (_, 1) => Console::VsSystem,
        (_, 2) => Console::PlayChoice10,
        (Format::Nes2, _) => Console::Extended(h[13] & 0xF),
        _ => Console::Standard,
    };
    Some(Header {
        format,
        mapper,
        submapper: nes2.then_some(h[8] >> 4),
        prg_rom,
        chr_rom,
        prg_ram,
        prg_nvram,
        chr_ram,
        chr_nvram,
        mirroring: if h[6] & 8 != 0 {
            Mirroring::FourScreen
        } else if h[6] & 1 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        },
        battery: h[6] & 2 != 0,
        trainer: h[6] & 4 != 0,
        timing,
        console,
    })
}

/// A NES 2.0 ROM size: `lsb` with the 4-bit `msb` as a count of `unit`s, or (msb 0xF) the exponent-multiplier form
/// `2^E * (2M + 1)` bytes with `lsb` = `EEEEEEMM`.
fn rom_size(lsb: u8, msb: u8, unit: u64) -> Option<u64> {
    if msb == 0xF {
        1u64.checked_shl(u32::from(lsb >> 2))?.checked_mul(u64::from(lsb & 3) * 2 + 1)
    } else {
        Some((u64::from(msb) << 8 | u64::from(lsb)) * unit)
    }
}

impl Header {
    /// PAL ROMs are for Europe and Australia; an NTSC header names no region.
    pub fn regions(&self) -> Vec<Region> {
        match self.timing {
            Timing::Pal | Timing::Dendy => alloc::vec![Region::Europe, Region::Australia],
            _ => Vec::new(),
        }
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, Field)> {
        let console = match self.console {
            Console::Standard => 0,
            Console::VsSystem => 1,
            Console::PlayChoice10 => 2,
            Console::Extended(n) => 0x10 + i64::from(n),
        };
        alloc::vec![
            ("format", num(self.format as i64)),
            ("mapper", num(self.mapper)),
            ("submapper", opt_num(self.submapper)),
            ("prg_rom", size_field(Some(self.prg_rom))),
            ("chr_rom", size_field(Some(self.chr_rom))),
            ("prg_ram", size_field(Some(self.prg_ram))),
            ("prg_nvram", size_field(Some(self.prg_nvram))),
            ("chr_ram", size_field(Some(self.chr_ram))),
            ("chr_nvram", size_field(Some(self.chr_nvram))),
            ("mirroring", num(self.mirroring as i64)),
            ("battery", Field::Flag(self.battery)),
            ("trainer", Field::Flag(self.trainer)),
            ("timing", num(self.timing as i64)),
            ("console", num(console)),
        ]
    }

    pub(crate) fn info(&self) -> GameInfo {
        let mut info = GameInfo::new(Platform::Nes);
        info.regions = self.regions();
        info.cart = Some(super::CartHeader::Nes(self.clone()));
        info
    }
}
