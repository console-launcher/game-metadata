// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Game Boy / Game Boy Color header reader.
mod carts_common;

use carts_common::{info, load};
use gamemeta_core::carts::gb::{self, Cgb};
use gamemeta_core::carts::{self, CartHeader};
use gamemeta_core::{GameInfo, Language, Platform, Region, SliceSource};
use gamemeta_fixtures::carts::gb::Spec;

fn header(info: &GameInfo) -> &gb::Header {
    match &info.cart {
        Some(CartHeader::GameBoy(h)) => h,
        other => panic!("not a Game Boy header: {other:?}"),
    }
}

#[test]
fn a_plain_rom_gives_every_field() {
    let rom = Spec::default().build();
    let i = load(&rom, "game.gb");
    let h = header(&i);
    assert_eq!(i.platform, Platform::GameBoy);
    assert_eq!(h.title, "SYNTH TEST");
    assert_eq!(h.manufacturer_code, None);
    assert_eq!(h.cgb, Cgb::No);
    assert!(!h.sgb);
    assert_eq!((h.old_licensee, h.new_licensee.clone()), (1, None));
    assert_eq!(h.cart_type, 0x13);
    assert_eq!((h.rom_size_code, h.rom_size), (0, Some(32 * 1024)));
    assert_eq!((h.ram_size_code, h.ram_size), (3, Some(32 * 1024)));
    assert_eq!((h.destination, h.version), (1, 2));
    assert!(h.header_checksum_valid && h.logo_valid);
    assert_eq!(h.header_checksum, rom[0x14D]);
    assert_eq!(h.global_checksum, u16::from_be_bytes([rom[0x14E], rom[0x14F]]));
    assert_eq!(i.title(), Some("SYNTH TEST"));
    assert_eq!(i.version, Some(2));
    assert_eq!(i.ids.maker_code.as_deref(), Some("01"));
    assert_eq!(i.regions, vec![Region::NorthAmerica, Region::Europe]);
}

#[test]
fn the_color_flag_shortens_the_title_and_reveals_the_manufacturer_code() {
    let rom = Spec { cgb: 0x80, manufacturer: Some("BXTE"), title: "LONGERTITLE12345", ..Spec::default() }.build();
    let i = load(&rom, "game.gbc");
    let h = header(&i);
    assert_eq!(i.platform, Platform::GameBoyColor);
    assert_eq!(h.cgb, Cgb::Compatible);
    assert_eq!(h.title, "LONGERTITLE");
    assert_eq!(h.manufacturer_code.as_deref(), Some("BXTE"));
    assert_eq!(i.ids.game_id.as_deref(), Some("BXTE"));
}

#[test]
fn a_color_only_cartridge_without_a_manufacturer_code_keeps_a_15_character_title() {
    let rom = Spec { cgb: 0xC0, title: "FOURTEEN CHARS", ..Spec::default() }.build();
    let i = load(&rom, "game.gbc");
    let h = header(&i);
    assert_eq!((h.cgb, h.manufacturer_code.clone()), (Cgb::Only, None));
    assert_eq!(h.title, "FOURTEEN CHARS");
    assert_eq!(i.platform, Platform::GameBoyColor);
}

#[test]
fn the_super_game_boy_flag_is_read() {
    assert!(header(&load(&Spec { sgb: true, ..Spec::default() }.build(), "a.sgb")).sgb);
    assert!(!header(&load(&Spec::default().build(), "a.sgb")).sgb);
}

#[test]
fn the_licensee_is_the_new_code_only_when_the_old_one_is_0x33() {
    let new = load(&Spec { old_licensee: 0x33, new_licensee: "7F", ..Spec::default() }.build(), "a.gb");
    assert_eq!(header(&new).new_licensee.as_deref(), Some("7F"));
    assert_eq!(header(&new).licensee().as_deref(), Some("7F"));
    assert_eq!(new.ids.maker_code.as_deref(), Some("7F"));
    let old = load(&Spec { old_licensee: 0xA4, new_licensee: "7F", ..Spec::default() }.build(), "a.gb");
    assert_eq!(header(&old).licensee().as_deref(), Some("A4"));
    assert_eq!(old.ids.maker_code.as_deref(), Some("A4"));
    let none = load(&Spec { old_licensee: 0x33, new_licensee: "", ..Spec::default() }.build(), "a.gb");
    assert_eq!((header(&none).new_licensee.clone(), none.ids.maker_code.clone()), (None, None));
}

#[test]
fn rom_and_ram_size_codes_map_to_bytes() {
    let sizes = |rom, ram| {
        let i = load(&Spec { rom_size: rom, ram_size: ram, ..Spec::default() }.build(), "a.gb");
        (header(&i).rom_size, header(&i).ram_size)
    };
    assert_eq!(sizes(8, 0), (Some(8 << 20), Some(0)));
    assert_eq!(sizes(4, 2), (Some(512 << 10), Some(8 << 10)));
    assert_eq!(sizes(0, 4), (Some(32 << 10), Some(128 << 10)));
    assert_eq!(sizes(1, 5), (Some(64 << 10), Some(64 << 10)));
    assert_eq!(sizes(9, 1), (None, None));
    assert_eq!(sizes(0x52, 6), (None, None));
}

#[test]
fn the_destination_names_the_region_and_the_title_language() {
    let at = |d| load(&Spec { destination: d, ..Spec::default() }.build(), "a.gb");
    assert_eq!(at(0).regions, vec![Region::Japan]);
    assert_eq!(at(0).titles[0].0, Language::Japanese);
    assert_eq!(at(1).titles[0].0, Language::English);
    assert!(at(2).regions.is_empty());
}

#[test]
fn a_wrong_header_checksum_is_reported_not_fatal() {
    let mut rom = Spec::default().build();
    rom[0x14D] ^= 1;
    let h = header(&load(&rom, "a.gb")).clone();
    assert!(!h.header_checksum_valid && h.logo_valid);
}

#[test]
fn the_logo_is_judged_by_its_digest_byte_by_byte() {
    for i in 0..48 {
        let mut rom = Spec::default().build();
        rom[0x104 + i] ^= 0x01;
        let h = header(&load(&rom, "a.gb")).clone();
        assert!(!h.logo_valid, "byte {i}");
    }
}

#[test]
fn a_header_needs_the_logo_or_the_checksum() {
    let mut no_logo = Spec::default().build();
    no_logo[0x110] ^= 0xFF;
    let h = header(&load(&no_logo, "a.gb")).clone();
    assert!(!h.logo_valid && h.header_checksum_valid);
    let mut neither = no_logo.clone();
    neither[0x14D] ^= 1;
    assert!(info(&neither, "a.gb").is_none());
    assert!(info(&vec![0; 0x8000], "a.gb").is_none());
}

#[test]
fn a_short_file_is_nothing() {
    let rom = Spec::default().build();
    assert!(info(&rom[..0x14F], "a.gb").is_none());
    assert!(info(&[], "a.gb").is_none());
    assert!(gb::parse(&rom[..0x14F]).is_none());
    assert!(gb::parse(&rom[..0x150]).is_some());
}

#[test]
fn the_header_decides_not_the_extension() {
    let color = Spec { cgb: 0x80, ..Spec::default() }.build();
    for name in ["a.gb", "a.GBC", "a.sgb"] {
        assert_eq!(load(&color, name).platform, Platform::GameBoyColor, "{name}");
    }
}

#[test]
fn the_global_checksum_is_checked_by_streaming_the_rom() {
    let mut rom = Spec::default().build();
    assert_eq!(carts::checksum_matches(&mut SliceSource::new(&rom), "a.gb"), Some(true));
    rom[0x4000] ^= 0x10;
    assert_eq!(carts::checksum_matches(&mut SliceSource::new(&rom), "a.gb"), Some(false));
    assert_eq!(carts::checksum_matches(&mut SliceSource::new(&rom), "a.txt"), None);
}

#[test]
fn the_header_is_available_as_json() {
    let json = header_json(&Spec::default().build());
    assert!(json.starts_with("{\"family\":\"gb\",\"title\":\"SYNTH TEST\","), "{json}");
    assert!(json.contains("\"logo_valid\":true") && json.contains("\"manufacturer_code\":null"), "{json}");
}

fn header_json(rom: &[u8]) -> String {
    load(rom, "a.gb").cart.expect("a header").to_json()
}
