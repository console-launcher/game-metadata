// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! NES header reader: iNES, NES 2.0 and archaic iNES.
mod carts_common;

use carts_common::{info, load};
use gamemeta_core::carts::CartHeader;
use gamemeta_core::carts::nes::{self, Console, Format, Mirroring, Timing};
use gamemeta_core::{GameInfo, Platform, Region};
use gamemeta_fixtures::carts::nes::Spec;

fn header(info: &GameInfo) -> &nes::Header {
    match &info.cart {
        Some(CartHeader::Nes(h)) => h,
        other => panic!("not an NES header: {other:?}"),
    }
}

fn read(spec: Spec) -> nes::Header {
    header(&load(&spec.build(), "a.nes")).clone()
}

#[test]
fn an_ines_header_gives_mapper_and_sizes() {
    let i = load(&Spec { prg: 2, chr: 1, mapper: 0x45, ..Spec::default() }.build(), "game.nes");
    let h = header(&i);
    assert_eq!(i.platform, Platform::Nes);
    assert_eq!(h.format, Format::INes);
    assert_eq!((h.mapper, h.submapper), (0x45, None));
    assert_eq!((h.prg_rom, h.chr_rom, h.prg_ram), (32768, 8192, 0));
    assert_eq!((h.mirroring, h.battery, h.trainer), (Mirroring::Horizontal, false, false));
    assert_eq!((h.timing, h.console), (Timing::Ntsc, Console::Standard));
    assert!(i.titles.is_empty() && i.title().is_none());
    assert!(i.regions.is_empty());
}

#[test]
fn ines_flags() {
    let base = Spec { prg: 1, ..Spec::default() };
    let v = read(Spec { vertical: true, battery: true, ..base.clone() });
    assert_eq!((v.mirroring, v.battery), (Mirroring::Vertical, true));
    assert_eq!(read(Spec { vertical: true, four_screen: true, ..base.clone() }).mirroring, Mirroring::FourScreen);
    assert!(read(Spec { trainer: true, ..base.clone() }).trainer);
    assert_eq!(read(Spec { prg_ram_units: 2, ..base.clone() }).prg_ram, 16384);
    let pal = load(&Spec { pal: true, ..base.clone() }.build(), "a.nes");
    assert_eq!(header(&pal).timing, Timing::Pal);
    assert_eq!(pal.regions, vec![Region::Europe, Region::Australia]);
    assert_eq!(read(Spec { console: 1, ..base.clone() }).console, Console::VsSystem);
    assert_eq!(read(Spec { console: 2, ..base }).console, Console::PlayChoice10);
}

#[test]
fn a_nes2_header_gives_twelve_bit_fields_submapper_ram_and_timing() {
    let spec = Spec {
        nes2: true,
        prg: 0x123,
        chr: 0x045,
        mapper: 0x234,
        submapper: 7,
        ram_shifts: [7, 8, 9, 10],
        timing: 3,
        console: 3,
        extended_console: 5,
        ..Spec::default()
    };
    let i = load(&spec.build(), "a.nes");
    let h = header(&i);
    assert_eq!(h.format, Format::Nes2);
    assert_eq!((h.mapper, h.submapper), (0x234, Some(7)));
    assert_eq!((h.prg_rom, h.chr_rom), (0x123 * 16384, 0x45 * 8192));
    assert_eq!((h.prg_ram, h.prg_nvram, h.chr_ram, h.chr_nvram), (64 << 7, 64 << 8, 64 << 9, 64 << 10));
    assert_eq!((h.timing, h.console), (Timing::Dendy, Console::Extended(5)));
    assert_eq!(i.regions, vec![Region::Europe, Region::Australia]);
    let zero = read(Spec { prg: 1, nes2: true, ..Spec::default() });
    assert_eq!((zero.prg_ram, zero.prg_nvram, zero.chr_ram, zero.chr_nvram), (0, 0, 0, 0));
}

#[test]
fn nes2_timing_values() {
    let at = |t| read(Spec { nes2: true, prg: 1, timing: t, ..Spec::default() }).timing;
    assert_eq!([at(0), at(1), at(2), at(3)], [Timing::Ntsc, Timing::Pal, Timing::Multi, Timing::Dendy]);
}

#[test]
fn nes2_exponent_multiplier_sizes() {
    let mut rom = Spec { nes2: true, prg: 1, chr: 1, ..Spec::default() }.build();
    rom[4] = 0b0010_1001; // exponent 10, multiplier 1: 2^10 * 3
    rom[9] = 0x0F;
    rom[5] = 0b0000_1101; // exponent 3, multiplier 1: 2^3 * 3
    rom[9] |= 0xF0;
    let h = header(&load(&rom, "a.nes")).clone();
    assert_eq!((h.prg_rom, h.chr_rom), (3072, 24));
}

#[test]
fn an_archaic_header_ignores_the_upper_mapper_nibble() {
    let mut rom = Spec { prg: 1, mapper: 1, ..Spec::default() }.build();
    rom[7..16].copy_from_slice(b"DiskDude!");
    let h = header(&load(&rom, "a.nes")).clone();
    assert_eq!((h.format, h.mapper), (Format::Archaic, 1));
}

#[test]
fn a_file_without_the_magic_or_prg_or_enough_bytes_is_nothing() {
    let mut rom = Spec { prg: 1, ..Spec::default() }.build();
    rom[3] = 0;
    assert!(info(&rom, "a.nes").is_none());
    assert!(info(&Spec::default().build(), "a.nes").is_none());
    assert!(info(&Spec { nes2: true, ..Spec::default() }.build(), "a.nes").is_none());
    assert!(info(&Spec { prg: 1, ..Spec::default() }.build()[..15], "a.nes").is_none());
    assert!(nes::parse(&Spec { prg: 1, ..Spec::default() }.build()).is_some());
}
