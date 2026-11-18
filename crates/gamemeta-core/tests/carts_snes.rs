// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Super Nintendo header reader: LoROM / HiROM / ExHiROM by scoring, with or without a copier header.
mod carts_common;

use carts_common::{info, load};
use gamemeta_core::carts::CartHeader;
use gamemeta_core::carts::snes::{self, Layout};
use gamemeta_core::{GameInfo, Language, Platform, Region};
use gamemeta_fixtures::carts::snes::{Layout as L, Spec};

fn header(info: &GameInfo) -> &snes::Header {
    match &info.cart {
        Some(CartHeader::Snes(h)) => h,
        other => panic!("not a SNES header: {other:?}"),
    }
}

#[test]
fn a_lorom_gives_every_field() {
    let i = load(&Spec::new(L::Lo).build(), "game.sfc");
    let h = header(&i);
    assert_eq!(i.platform, Platform::Snes);
    assert_eq!((h.layout, h.copier_header), (Layout::LoRom, false));
    assert_eq!(h.title, "SYNTH SNES TEST");
    assert_eq!((h.map_mode, h.fast_rom, h.rom_type), (0x20, false, 2));
    assert_eq!((h.rom_size, h.ram_size), (Some(512 << 10), Some(8 << 10)));
    assert_eq!((h.destination, h.developer_id, h.version), (1, 1, 0));
    assert_eq!((h.maker_code.clone(), h.game_code.clone()), (None, None));
    assert_eq!((h.checksum, h.complement, h.checksum_valid), (0xABCD, !0xABCDu16, true));
    assert_eq!(i.title(), Some("SYNTH SNES TEST"));
    assert_eq!(i.ids.maker_code.as_deref(), Some("01"));
    assert_eq!(i.version, Some(0));
    assert_eq!(i.regions, vec![Region::NorthAmerica]);
}

#[test]
fn the_layout_follows_where_a_plausible_header_sits() {
    assert_eq!(header(&load(&Spec::new(L::Hi).build(), "a.sfc")).layout, Layout::HiRom);
    let ex = Spec::new(L::ExHi).build();
    assert_eq!(ex.len(), 0x410000);
    assert_eq!(header(&load(&ex, "a.sfc")).layout, Layout::ExHiRom);
    assert_eq!(header(&load(&ex, "a.sfc")).map_mode, 0x25);
}

#[test]
fn a_copier_header_is_found_by_size_and_skipped() {
    for (layout, want) in [(L::Lo, Layout::LoRom), (L::Hi, Layout::HiRom)] {
        let plain = Spec::new(layout).build();
        let with = Spec { copier: true, ..Spec::new(layout) }.build();
        assert_eq!(with.len(), plain.len() + 512);
        let (a, b) = (load(&plain, "a.smc"), load(&with, "a.smc"));
        assert!(header(&b).copier_header && !header(&a).copier_header);
        assert_eq!(header(&b).layout, want);
        assert_eq!(header(&b).title, header(&a).title);
    }
}

#[test]
fn the_fast_rom_bit_is_the_map_mode_s() {
    let h = header(&load(&Spec { map_mode: 0x30, ..Spec::new(L::Lo) }.build(), "a.sfc")).clone();
    assert!(h.fast_rom);
    assert_eq!(h.layout, Layout::LoRom);
}

#[test]
fn the_title_is_jis_x_0201() {
    let kana = Spec { title: vec![0xB1, 0xB2, 0xB3, b' ', b'X'], ..Spec::new(L::Lo) }.build();
    assert_eq!(header(&load(&kana, "a.sfc")).title, "ｱｲｳ X");
    let yen = Spec { title: vec![0x5C, b'1', 0x7E], ..Spec::new(L::Lo) }.build();
    assert_eq!(header(&load(&yen, "a.sfc")).title, "¥1‾");
}

#[test]
fn the_destination_names_the_region() {
    let at = |d| load(&Spec { destination: d, ..Spec::new(L::Lo) }.build(), "a.sfc");
    assert_eq!(at(0).regions, vec![Region::Japan]);
    assert_eq!(at(0).titles[0].0, Language::Japanese);
    for (code, want) in [
        (1, Region::NorthAmerica),
        (0xF, Region::NorthAmerica),
        (2, Region::Europe),
        (6, Region::Europe),
        (0xA, Region::Europe),
        (0xB, Region::China),
        (0xD, Region::Korea),
        (0x11, Region::Australia),
    ] {
        assert_eq!(at(code).regions, vec![want], "destination {code:#x}");
    }
    assert!(at(0x10).regions.is_empty());
}

#[test]
fn developer_id_0x33_means_the_extended_header_names_maker_and_game() {
    let i = load(&Spec { developer: 0x33, maker: "AB", game_code: "ABCD", ..Spec::new(L::Hi) }.build(), "a.sfc");
    let h = header(&i);
    assert_eq!((h.maker_code.as_deref(), h.game_code.as_deref()), (Some("AB"), Some("ABCD")));
    assert_eq!(i.ids.maker_code.as_deref(), Some("AB"));
    assert_eq!(i.ids.game_id.as_deref(), Some("ABCD"));
}

#[test]
fn the_checksum_and_complement_must_be_each_others_inverse() {
    let h = header(&load(&Spec { complement_ok: false, ..Spec::new(L::Lo) }.build(), "a.sfc")).clone();
    assert!(!h.checksum_valid);
    assert_eq!(h.layout, Layout::LoRom);
}

#[test]
fn size_codes_map_to_bytes() {
    let h = |rom, ram| {
        let i = load(&Spec { rom_size: rom, ram_size: ram, ..Spec::new(L::Lo) }.build(), "a.sfc");
        (header(&i).rom_size, header(&i).ram_size)
    };
    assert_eq!(h(0x0C, 0), (Some(4 << 20), Some(0)));
    assert_eq!(h(0x08, 5), (Some(256 << 10), Some(32 << 10)));
    assert_eq!(h(0x1F, 0x1F), (None, None));
}

#[test]
fn junk_zeros_and_short_files_have_no_header() {
    assert!(info(&Spec::new(L::Lo).build(), "a.sfc").is_some());
    let junk: Vec<u8> = (0..0x10000usize).map(|i| ((i * 31 + 7) % 251) as u8).collect();
    assert!(info(&junk, "a.sfc").is_none());
    assert!(info(&vec![0; 0x10000], "a.sfc").is_none());
    assert!(info(&Spec::new(L::Lo).build()[..0x7FFF], "a.sfc").is_none());
    assert!(info(&[], "a.sfc").is_none());
}

#[test]
fn every_snes_extension_reads() {
    let rom = Spec::new(L::Lo).build();
    for name in ["a.sfc", "a.SMC", "a.swc", "a.fig"] {
        assert_eq!(header(&load(&rom, name)).layout, Layout::LoRom, "{name}");
    }
}
