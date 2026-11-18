// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Genesis / Mega Drive / 32X header reader, plain and Super Magic Drive.
mod carts_common;

use carts_common::{info, load};
use gamemeta_core::carts::md::{self, Layout};
use gamemeta_core::carts::{self, CartHeader};
use gamemeta_core::{GameInfo, Language, Platform, Region, SliceSource};
use gamemeta_fixtures::carts::md::{Spec, to_smd};

fn header(info: &GameInfo) -> &md::Header {
    match &info.cart {
        Some(CartHeader::MegaDrive(h)) => h,
        other => panic!("not a Mega Drive header: {other:?}"),
    }
}

fn read(spec: Spec) -> md::Header {
    header(&load(&spec.build(), "a.md")).clone()
}

#[test]
fn a_plain_image_gives_every_field() {
    let rom = Spec::default().build();
    let i = load(&rom, "game.md");
    let h = header(&i);
    assert_eq!(i.platform, Platform::MegaDrive);
    assert_eq!(h.layout, Layout::Plain);
    assert_eq!((h.system.as_str(), h.is_32x), ("SEGA MEGA DRIVE", false));
    assert_eq!(h.copyright, "(C)SEGA 1991.APR");
    assert_eq!((h.copyright_holder.as_deref(), h.copyright_date.as_deref()), (Some("SEGA"), Some("1991.APR")));
    assert_eq!((h.domestic_title.as_str(), h.overseas_title.as_str()), ("シンセゲーム", "SYNTH DRIVE GAME"));
    assert_eq!(h.serial, "GM 00001009-02");
    assert_eq!((h.serial_number.as_deref(), h.revision), (Some("00001009"), Some(2)));
    assert_eq!((h.io_support.as_str(), h.rom_start, h.rom_end), ("J", 0, 0x7FFF));
    assert_eq!((h.region_codes.as_str(), h.region_bits), ("JUE", 13));
    assert_eq!(h.checksum, u16::from_be_bytes([rom[0x18E], rom[0x18F]]));
    assert_eq!(i.title(), Some("SYNTH DRIVE GAME"));
    assert_eq!(i.title_in(Language::Japanese).map(|t| t.name.as_str()), Some("シンセゲーム"));
    assert_eq!(i.title_in(Language::English).and_then(|t| t.publisher.as_deref()), Some("SEGA"));
    assert_eq!(i.ids.product_code.as_deref(), Some("GM 00001009-02"));
    assert_eq!(i.ids.game_id.as_deref(), Some("00001009"));
    assert_eq!(i.ids.maker_code.as_deref(), Some("SEGA"));
    assert_eq!(i.version, Some(2));
    assert_eq!(i.regions, vec![Region::Japan, Region::NorthAmerica, Region::Europe]);
}

#[test]
fn a_super_magic_drive_file_reads_as_the_same_header() {
    let plain = Spec::default().build();
    let smd = to_smd(&plain);
    let reference = header(&load(&plain, "a.md")).clone();
    for name in ["a.smd", "a.bin", "a.md"] {
        let got = header(&load(&smd, name)).clone();
        assert_eq!(got.layout, Layout::Smd, "{name}");
        assert_eq!(md::Header { layout: Layout::Plain, ..got }, reference, "{name}");
    }
}

#[test]
fn region_codes_in_both_styles() {
    let bits = |region: &'static str| read(Spec { region, ..Spec::default() }).region_bits;
    // Old style: the letters J, U, E.
    assert_eq!([bits("J"), bits("U"), bits("E"), bits("JE"), bits("JUE")], [1, 4, 8, 9, 13]);
    // New style: one hex digit.
    assert_eq!([bits("1"), bits("4"), bits("8"), bits("5"), bits("A"), bits("D")], [1, 4, 8, 5, 10, 13]);
    assert_eq!(bits(""), 0);
    let at = |region: &'static str| load(&Spec { region, ..Spec::default() }.build(), "a.md").regions;
    assert_eq!(at("U"), vec![Region::NorthAmerica]);
    assert_eq!(at("8"), vec![Region::Europe]);
    assert!(at("").is_empty());
}

#[test]
fn a_32x_system_string_makes_a_32x_game() {
    let x = load(&Spec { system: "SEGA 32X", ..Spec::default() }.build(), "a.32x");
    assert_eq!(x.platform, Platform::Sega32x);
    assert!(header(&x).is_32x);
    let genesis = load(&Spec { system: "SEGA GENESIS", ..Spec::default() }.build(), "a.gen");
    assert_eq!((genesis.platform, header(&genesis).system.as_str()), (Platform::MegaDrive, "SEGA GENESIS"));
}

#[test]
fn a_system_string_with_a_leading_space_still_reads() {
    let h = read(Spec { system: " SEGA MEGA DRIVE", ..Spec::default() });
    assert_eq!(h.system, "SEGA MEGA DRIVE");
}

#[test]
fn a_t_series_copyright_is_a_maker_code_not_a_publisher() {
    let i = load(&Spec { copyright: "(C)T-76 1990.OCT", ..Spec::default() }.build(), "a.md");
    assert_eq!(header(&i).copyright_holder.as_deref(), Some("T-76"));
    assert_eq!(i.ids.maker_code.as_deref(), Some("T-76"));
    assert!(i.titles.iter().all(|(_, t)| t.publisher.is_none()));
}

#[test]
fn an_odd_serial_or_blank_fields_give_none_not_guesses() {
    let h = read(Spec { serial: "SOMETHING ODD", copyright: "", ..Spec::default() });
    assert_eq!((h.serial_number, h.revision, h.copyright_holder), (None, None, None));
    let i = load(&Spec { serial: "SOMETHING ODD", ..Spec::default() }.build(), "a.md");
    assert_eq!((i.version, i.ids.game_id.clone()), (None, None));
    assert_eq!(i.ids.product_code.as_deref(), Some("SOMETHING ODD"));
    let only_jp = load(&Spec { overseas: "", ..Spec::default() }.build(), "a.md");
    assert_eq!(only_jp.titles.len(), 1);
    let none = load(&Spec { overseas: "", domestic: "", ..Spec::default() }.build(), "a.md");
    assert!(none.titles.is_empty() && none.title().is_none());
}

#[test]
fn only_a_sega_header_reads_so_a_bin_is_never_guessed() {
    assert!(info(&Spec::default().build(), "a.bin").is_some());
    let junk: Vec<u8> = (0..0x8000usize).map(|i| ((i * 31 + 7) % 251) as u8).collect();
    for name in ["a.bin", "a.md", "a.smd", "a.gen"] {
        assert!(info(&junk, name).is_none(), "{name}");
        assert!(info(&vec![0; 0x8000], name).is_none(), "{name}");
    }
    let mut not_sega = Spec::default().build();
    not_sega[0x103] = b'B';
    assert!(info(&not_sega, "a.bin").is_none());
    assert!(info(&Spec::default().build()[..0x1FF], "a.bin").is_none());
    assert!(info(&[], "a.bin").is_none());
    assert!(info(&junk, "a.smd").is_none());
}

#[test]
fn the_checksum_is_checked_by_streaming_the_rom_even_through_smd_interleave() {
    let mut plain = Spec::default().build();
    let check = |bytes: &[u8], name| carts::checksum_matches(&mut SliceSource::new(bytes), name);
    assert_eq!(check(&plain, "a.md"), Some(true));
    assert_eq!(check(&to_smd(&plain), "a.smd"), Some(true));
    plain[0x5000] ^= 0x44;
    assert_eq!(check(&plain, "a.md"), Some(false));
    assert_eq!(check(&to_smd(&plain), "a.smd"), Some(false));
    assert_eq!(check(&[0; 0x400], "a.md"), None);
}

#[test]
fn deinterleave_restores_one_block() {
    let plain: Vec<u8> = (0..0x4000usize).map(|i| (i % 251) as u8).collect();
    assert_eq!(md::deinterleave(&to_smd(&plain)[512..]), plain);
    assert!(md::is_smd(&to_smd(&plain)[..16], 0x4200));
    assert!(!md::is_smd(&to_smd(&plain)[..16], 0x4000));
}
