// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Nintendo 64 header reader: three byte orders, one header.
mod carts_common;

use carts_common::{info, load};
use gamemeta_core::carts::CartHeader;
use gamemeta_core::carts::n64::{self, ByteOrder};
use gamemeta_core::{GameInfo, Language, Platform, Region};
use gamemeta_fixtures::carts::n64::{Spec, to_n64, to_v64};

fn header(info: &GameInfo) -> &n64::Header {
    match &info.cart {
        Some(CartHeader::N64(h)) => h,
        other => panic!("not an N64 header: {other:?}"),
    }
}

#[test]
fn a_z64_image_gives_every_field() {
    let i = load(&Spec::default().build(), "game.z64");
    let h = header(&i);
    assert_eq!(i.platform, Platform::Nintendo64);
    assert_eq!(h.byte_order, ByteOrder::BigEndian);
    assert_eq!(h.title, "SYNTH 64 TEST");
    assert_eq!((h.category, h.game_id.as_str(), h.country, h.version), (b'N', "ST", b'E', 1));
    assert_eq!((h.crc1, h.crc2), (0x1122_3344, 0x5566_7788));
    assert_eq!(h.game_code(), "NSTE");
    assert_eq!(i.title(), Some("SYNTH 64 TEST"));
    assert_eq!(i.ids.game_id.as_deref(), Some("NSTE"));
    assert_eq!(i.version, Some(1));
    assert_eq!(i.regions, vec![Region::NorthAmerica]);
}

#[test]
fn byte_swapped_and_little_endian_images_read_as_the_same_header() {
    let z = Spec::default().build();
    let reference = header(&load(&z, "a.z64")).clone();
    let v = load(&to_v64(&z), "a.v64");
    let n = load(&to_n64(&z), "a.n64");
    assert_eq!(header(&v).byte_order, ByteOrder::ByteSwapped);
    assert_eq!(header(&n).byte_order, ByteOrder::LittleEndian);
    for got in [header(&v), header(&n)] {
        let mut same = got.clone();
        same.byte_order = ByteOrder::BigEndian;
        assert_eq!(same, reference);
    }
}

#[test]
fn the_magic_decides_not_the_extension() {
    let z = Spec::default().build();
    assert_eq!(header(&load(&z, "a.n64")).byte_order, ByteOrder::BigEndian);
    assert_eq!(header(&load(&to_n64(&z), "a.z64")).byte_order, ByteOrder::LittleEndian);
}

#[test]
fn an_unknown_first_word_or_a_short_file_is_nothing() {
    assert!(info(&Spec::default().build(), "a.z64").is_some());
    let mut z = Spec::default().build();
    z[0] = 0x81;
    assert!(info(&z, "a.z64").is_none());
    assert!(info(&Spec::default().build()[..0x3F], "a.z64").is_none());
    assert!(info(&[], "a.z64").is_none());
    assert_eq!(ByteOrder::detect(&[0x80, 0x37, 0x12]), None);
}

#[test]
fn the_country_code_names_the_region() {
    let at = |country: u8| load(&Spec { country, ..Spec::default() }.build(), "a.z64");
    assert_eq!(at(b'J').regions, vec![Region::Japan]);
    assert_eq!(at(b'J').titles[0].0, Language::Japanese);
    assert_eq!(at(b'N').regions, vec![Region::NorthAmerica]);
    assert_eq!(at(b'P').regions, vec![Region::Europe]);
    assert_eq!(at(b'U').regions, vec![Region::Australia]);
    assert!(at(b'A').regions.is_empty());
}

#[test]
fn normalizing_swaps_whole_words_and_leaves_a_tail() {
    let z = Spec::default().build();
    let mut v = to_v64(&z);
    ByteOrder::ByteSwapped.normalize(&mut v);
    assert_eq!(v, z);
    let mut n = to_n64(&z);
    ByteOrder::LittleEndian.normalize(&mut n);
    assert_eq!(n, z);
    let mut tail = [1u8, 2, 3, 4, 5, 6];
    ByteOrder::LittleEndian.normalize(&mut tail);
    assert_eq!(tail, [4, 3, 2, 1, 5, 6]);
}

#[test]
fn whatever_bytes_the_title_holds_it_comes_back_trimmed() {
    // Found by the fuzzer: a Shift_JIS byte that decodes to a Unicode space left it on the end of the title.
    let mut z = Spec::default().build();
    z[0x20..0x34].copy_from_slice(&[
        0xFE, 0xFF, b'S', b'Y', b'N', b'T', 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, b'H', b' ', b'6', b'4', b' ', 3, 0, 0,
    ]);
    let found = load(&z, "a.z64");
    assert_eq!(found.title(), Some(header(&found).title.trim()));
    for byte in 0..=255u8 {
        let mut z = Spec::default().build();
        z[0x20..0x34].fill(byte);
        z[0x20] = b'A';
        z[0x33] = byte;
        let i = load(&z, "a.z64");
        assert_eq!(header(&i).title, header(&i).title.trim(), "byte {byte:#x}");
        if let Some((_, t)) = i.titles.first() {
            assert_eq!(t.name, t.name.trim(), "byte {byte:#x}");
        }
    }
}
