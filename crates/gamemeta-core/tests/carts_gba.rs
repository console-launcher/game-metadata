// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Game Boy Advance header reader.
mod carts_common;

use carts_common::{info, load};
use gamemeta_core::carts::gba;
use gamemeta_core::carts::{self, CartHeader};
use gamemeta_core::{GameInfo, Language, Platform, Region, SliceSource};
use gamemeta_fixtures::carts::gba::Spec;

fn header(info: &GameInfo) -> &gba::Header {
    match &info.cart {
        Some(CartHeader::Gba(h)) => h,
        other => panic!("not a GBA header: {other:?}"),
    }
}

#[test]
fn a_rom_gives_title_code_maker_version_and_ids() {
    let rom = Spec::default().build();
    let i = load(&rom, "game.gba");
    let h = header(&i);
    assert_eq!(i.platform, Platform::GameBoyAdvance);
    assert_eq!((h.title.as_str(), h.game_code.as_str(), h.maker_code.as_str()), ("SYNTHGAME", "ASYE", "01"));
    assert_eq!((h.version, h.complement, h.complement_valid), (1, rom[0xBD], true));
    assert_eq!(i.title(), Some("SYNTHGAME"));
    assert_eq!(i.ids.game_id.as_deref(), Some("ASYE"));
    assert_eq!(i.ids.maker_code.as_deref(), Some("01"));
    assert_eq!(i.ids.product_code.as_deref(), Some("AGB-ASYE"));
    assert_eq!(i.version, Some(1));
    assert_eq!(i.regions, vec![Region::NorthAmerica]);
    assert_eq!(gba::complement(&rom), Some(rom[0xBD]));
}

#[test]
fn the_last_letter_of_the_game_code_names_the_region() {
    let region = |letter: &str| {
        let code = format!("AXV{letter}");
        let code: &'static str = Box::leak(code.into_boxed_str());
        load(&Spec { code, ..Spec::default() }.build(), "a.gba")
    };
    assert_eq!(region("J").regions, vec![Region::Japan]);
    assert_eq!(region("J").titles[0].0, Language::Japanese);
    assert_eq!(region("E").titles[0].0, Language::English);
    for (letter, want) in [
        ("P", Region::Europe),
        ("D", Region::Europe),
        ("F", Region::Europe),
        ("Z", Region::Europe),
        ("U", Region::Australia),
        ("K", Region::Korea),
        ("C", Region::China),
    ] {
        assert_eq!(region(letter).regions, vec![want], "{letter}");
    }
    assert!(region("A").regions.is_empty());
}

#[test]
fn a_wrong_complement_is_reported_not_fatal() {
    let mut rom = Spec::default().build();
    rom[0xBD] ^= 1;
    assert!(!header(&load(&rom, "a.gba")).complement_valid);
}

#[test]
fn a_header_without_the_fixed_byte_or_a_plausible_game_code_is_nothing() {
    assert!(info(&Spec::default().build(), "a.gba").is_some());
    let mut rom = Spec::default().build();
    rom[0xB2] = 0;
    assert!(info(&rom, "a.gba").is_none());
    let mut rom = Spec::default().build();
    rom[0xAC] = b'a';
    assert!(info(&rom, "a.gba").is_none());
    assert!(info(&vec![0; 0x400], "a.gba").is_none());
}

#[test]
fn a_short_file_is_nothing() {
    let rom = Spec::default().build();
    assert!(info(&rom[..0xBF], "a.gba").is_none());
    assert!(info(&[], "a.gba").is_none());
    assert!(gba::parse(&rom[..0xC0]).is_some());
}

#[test]
fn both_extensions_and_any_case_read() {
    let rom = Spec { title: "ABC", ..Spec::default() }.build();
    for name in ["a.gba", "a.AGB", "a.Gba"] {
        assert_eq!(load(&rom, name).title(), Some("ABC"), "{name}");
    }
}

#[test]
fn there_is_no_whole_rom_checksum_to_stream() {
    let rom = Spec::default().build();
    assert!(info(&rom, "a.gba").is_some());
    assert_eq!(carts::checksum_matches(&mut SliceSource::new(&rom), "a.gba"), None);
}
