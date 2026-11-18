// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Master System / Game Gear header reader.
mod carts_common;

use carts_common::{info, load};
use gamemeta_core::carts::sms;
use gamemeta_core::carts::{self, CartHeader};
use gamemeta_core::{GameInfo, Platform, Region, SliceSource};
use gamemeta_fixtures::carts::sms::Spec;

fn header(info: &GameInfo) -> &sms::Header {
    match &info.cart {
        Some(CartHeader::MasterSystem(h)) => h,
        other => panic!("not a Master System header: {other:?}"),
    }
}

#[test]
fn a_header_at_0x7ff0_gives_every_field() {
    let rom = Spec::default().build();
    let i = load(&rom, "game.sms");
    let h = header(&i);
    assert_eq!(i.platform, Platform::MasterSystem);
    assert_eq!(h.offset, 0x7FF0);
    assert_eq!((h.product, h.version, h.region_code, h.size_code), (26_001, 1, 4, 0xC));
    assert_eq!((h.rom_size, h.game_gear), (Some(32 << 10), false));
    assert_eq!(h.checksum, u16::from_le_bytes([rom[0x7FFA], rom[0x7FFB]]));
    assert_eq!(i.ids.product_code.as_deref(), Some("26001"));
    assert_eq!(i.version, Some(1));
    assert_eq!(i.regions, vec![Region::NorthAmerica, Region::Europe]);
    assert!(!i.region_free);
}

#[test]
fn the_header_can_sit_at_the_two_lower_offsets() {
    for (at, size) in [(0x3FF0, 0xB), (0x1FF0, 0xA)] {
        let i = load(&Spec { at, size, ..Spec::default() }.build(), "a.sms");
        assert_eq!(header(&i).offset, at as u64);
    }
}

#[test]
fn region_codes_split_master_system_and_game_gear() {
    let at = |region| load(&Spec { region, ..Spec::default() }.build(), "a.bin");
    let _ = at;
    let sms_jp = load(&Spec { region: 3, ..Spec::default() }.build(), "a.gg");
    assert_eq!((sms_jp.platform, sms_jp.regions.clone()), (Platform::MasterSystem, vec![Region::Japan]));
    let gg_jp = load(&Spec { region: 5, ..Spec::default() }.build(), "a.sms");
    assert_eq!((gg_jp.platform, gg_jp.regions.clone()), (Platform::GameGear, vec![Region::Japan]));
    let gg_export = load(&Spec { region: 6, ..Spec::default() }.build(), "a.gg");
    assert_eq!(gg_export.regions, vec![Region::NorthAmerica, Region::Europe]);
    let gg_intl = load(&Spec { region: 7, ..Spec::default() }.build(), "a.gg");
    assert_eq!(gg_intl.platform, Platform::GameGear);
    assert!(gg_intl.region_free && gg_intl.regions.is_empty());
}

#[test]
fn an_unknown_region_nibble_falls_back_to_the_extension() {
    let rom = Spec { region: 0, ..Spec::default() }.build();
    assert_eq!(load(&rom, "a.gg").platform, Platform::GameGear);
    assert_eq!(load(&rom, "a.sms").platform, Platform::MasterSystem);
    assert!(load(&rom, "a.gg").regions.is_empty());
}

#[test]
fn product_numbers_are_five_bcd_digits() {
    let at = |product| load(&Spec { product, ..Spec::default() }.build(), "a.sms").ids.product_code;
    assert_eq!(at(4).as_deref(), Some("0004"));
    assert_eq!(at(1234).as_deref(), Some("1234"));
    assert_eq!(at(99_999).as_deref(), Some("99999"));
}

#[test]
fn a_file_without_tmr_sega_or_too_short_is_nothing() {
    assert!(info(&Spec::default().build(), "a.sms").is_some());
    let mut rom = Spec::default().build();
    rom[0x7FF0] = b'X';
    assert!(info(&rom, "a.sms").is_none());
    assert!(info(&vec![0; 0x8000], "a.gg").is_none());
    assert!(info(&Spec::default().build()[..0x7FFF], "a.sms").is_none());
    assert!(info(&[], "a.sms").is_none());
}

#[test]
fn the_checksum_is_streamed_over_the_rom_around_the_header() {
    for spec in
        [Spec::default(), Spec { at: 0x3FF0, size: 0xB, ..Spec::default() }, Spec { size: 0xD, ..Spec::default() }]
    {
        let mut rom = spec.build();
        let check = |bytes: &[u8]| carts::checksum_matches(&mut SliceSource::new(bytes), "a.sms");
        assert_eq!(check(&rom), Some(true), "{spec:?}");
        rom[0x100] ^= 1;
        assert_eq!(check(&rom), Some(false), "{spec:?}");
    }
    let mut after = Spec { size: 0xD, ..Spec::default() }.build();
    after[0xB000] ^= 1;
    assert_eq!(carts::checksum_matches(&mut SliceSource::new(&after), "a.sms"), Some(false));
}

#[test]
fn an_unused_size_code_still_reads_but_has_no_checksum_to_check() {
    let mut rom = Spec::default().build();
    rom[0x7FFF] = 0x45;
    let i = load(&rom, "a.sms");
    assert_eq!(header(&i).rom_size, None);
    assert_eq!(carts::checksum_matches(&mut SliceSource::new(&rom), "a.sms"), None);
}
