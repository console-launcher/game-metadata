// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cases beyond the original suite: icon pixels and tile order, the small icon, a plain CIA, the sample fixture.

#![allow(clippy::unusual_byte_groupings)]

use gamemeta_core::threeds::{self, smdh::Smdh};
use gamemeta_core::{Language, RatingBoard, Region, SliceSource, read};
use gamemeta_fixtures::{SmdhSlot, cia, exefs, plain_ncch, sample_3ds, smdh, smdh_with, with_program_id};

fn pixel(img: &gamemeta_core::RgbaImage, x: usize, y: usize) -> [u8; 4] {
    let o = (y * img.width as usize + x) * 4;
    img.pixels[o..o + 4].try_into().unwrap()
}

#[test]
fn rgb565_expands_to_full_range_rgba() {
    let s = Smdh::parse(&smdh("a", "b", "c", 0xF800)).unwrap();
    assert_eq!(pixel(&s.large_icon, 0, 0), [255, 0, 0, 255]);
    let s = Smdh::parse(&smdh("a", "b", "c", 0x07E0)).unwrap();
    assert_eq!(pixel(&s.large_icon, 47, 47), [0, 255, 0, 255]);
    let s = Smdh::parse(&smdh("a", "b", "c", 0b00001_000010_00011)).unwrap();
    assert_eq!(pixel(&s.large_icon, 5, 5), [0b00001_000, 0b000010_00, 0b00011_000, 255]);
}

#[test]
fn icon_tiles_are_8x8_morton_blocks_in_row_major_tile_order() {
    // Storage pixel i carries i as its value: tile 1 (x 8..16) starts at storage index 64, and inside a tile index 2
    // is (0, 1) and index 4 is (2, 0).
    let s = Smdh::parse(&smdh_with(&[], |i| i as u16, |i| i as u16, 0, &[])).unwrap();
    let value = |img: &gamemeta_core::RgbaImage, x, y| {
        let [r, g, b, _] = pixel(img, x, y);
        ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3)
    };
    assert_eq!(value(&s.large_icon, 0, 1), 2);
    assert_eq!(value(&s.large_icon, 2, 0), 4);
    assert_eq!(value(&s.large_icon, 8, 0), 64);
    assert_eq!(value(&s.large_icon, 0, 8), 64 * 6);
    assert_eq!(value(&s.small_icon, 0, 8), 64 * 3);
    assert_eq!((s.small_icon.width, s.small_icon.pixels.len()), (24, 24 * 24 * 4));
}

#[test]
fn a_short_or_unmarked_smdh_is_none() {
    assert!(Smdh::parse(&smdh("a", "b", "c", 0)[..0x36BF]).is_none());
    let mut bad = smdh("a", "b", "c", 0);
    bad[0] = b'X';
    assert!(Smdh::parse(&bad).is_none());
}

#[test]
fn a_plain_cia_reads_through_to_its_ncch() {
    let icon = smdh("CIA", "CIA Title", "P", 0x001F);
    let file = cia(&with_program_id(plain_ncch(&exefs(&[("icon", &icon)])), 0x0004_0000_0000_0042));
    let info = read(&mut SliceSource::new(&file), "x.CIA").unwrap();
    assert_eq!(info.title(), Some("CIA Title"));
    assert_eq!(info.ids.title_id.as_deref(), Some("0004000000000042"));
    assert_eq!(threeds::read(&mut SliceSource::new(&file)), Err(threeds::Reason::NoBannerInExeFs));
}

#[test]
fn the_sample_gives_every_field() {
    let info = read(&mut SliceSource::new(&sample_3ds()), "sample.3ds").unwrap();
    let langs: Vec<Language> = info.titles.iter().map(|t| t.0).collect();
    assert_eq!(langs, [Language::Japanese, Language::English, Language::French]);
    let ja = info.title_in(Language::Japanese).unwrap();
    assert_eq!((ja.short_name.as_str(), ja.name.as_str()), ("サンプル", "サンプル\u{3000}ゲーム"));
    let en = info.title_in(Language::English).unwrap();
    assert_eq!((en.name.as_str(), en.publisher.as_deref()), ("Sample Game Subtitle", Some("Example Pub")));
    let fr = info.title_in(Language::French).unwrap();
    assert_eq!((fr.name.as_str(), fr.publisher.as_deref()), ("Exemple", None));
    assert_eq!(info.title(), Some("Sample Game Subtitle"));
    assert_eq!(info.regions, [Region::NorthAmerica, Region::Europe]);
    assert!(!info.region_free);
    assert_eq!(info.ratings, [(RatingBoard::Cero, 12), (RatingBoard::Esrb, 10), (RatingBoard::Pegi, 0)]);
    assert_eq!(info.ids.title_id.as_deref(), Some("0004000000123400"));
    assert_eq!(info.ids.product_code.as_deref(), Some("CTR-P-SMPL"));
    assert_eq!(info.small_icon.as_ref().unwrap().width, 24);
    assert!(info.banner.is_some());
}

#[test]
fn the_preferred_title_falls_back_to_the_lowest_language() {
    let s = smdh_with(&[SmdhSlot { slot: 3, short: "Kurz", long: "", publisher: "" }], |_| 0, |_| 0, 0, &[]);
    let file = plain_ncch(&exefs(&[("icon", &s)]));
    let info = read(&mut SliceSource::new(&file), "a.cxi").unwrap();
    assert_eq!(info.preferred_title().map(|t| t.0), Some(Language::German));
    assert_eq!(info.title(), Some("Kurz"));
}
