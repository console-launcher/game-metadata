// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Parity with the original Kotlin reader's `GameCubeMetadataTest` and `GameCubeRvzTest`: same synthetic discs,
//! same expected outputs, one Rust test per reference test.

use gamemeta_core::{GameInfo, ImageKind, Language, Platform, Region, SliceSource, gc};
use gamemeta_fixtures::gc as fx;

fn read(bytes: &[u8]) -> Option<GameInfo> {
    gc::read_info(&mut SliceSource::new(bytes))
}
fn disc(bnr: Option<Vec<u8>>, id: &str, decoy: Option<Vec<u8>>) -> Vec<u8> {
    fx::standard_disc(bnr.as_deref(), id, decoy.as_deref())
}
fn default_disc() -> Vec<u8> {
    disc(Some(fx::bnr1("Wind Quest")), "GZLE01", None)
}
fn px(img: &gamemeta_core::RgbaImage, x: usize, y: usize) -> [u8; 4] {
    let at = (y * img.width as usize + x) * 4;
    img.pixels[at..at + 4].try_into().unwrap()
}

#[test]
fn a_bnr1_disc_gives_its_one_title_publisher_ids_and_system() {
    let info = read(&default_disc()).unwrap();
    assert_eq!(info.platform, Platform::GameCube);
    assert_eq!(info.title(), Some("Wind Quest"));
    let t = info.title_in(Language::English).unwrap();
    assert_eq!(t.name, "Wind Quest");
    assert_eq!(t.short_name, "Wind Quest");
    assert_eq!(t.publisher.as_deref(), Some("Wind Quest Maker Inc."));
    assert_eq!(info.ids.game_id.as_deref(), Some("GZLE01"));
    assert_eq!(info.ids.maker_code.as_deref(), Some("01"));
    assert_eq!(info.header_title.as_deref(), Some("Header Title"));
    assert_eq!(info.regions, [Region::NorthAmerica]);
}

#[test]
fn a_bnr1_on_a_japan_disc_is_the_japanese_title() {
    let info = read(&disc(Some(fx::bnr1("Only One")), "GZLJ01", None)).unwrap();
    assert_eq!(info.titles.iter().map(|(l, _)| *l).collect::<Vec<_>>(), [Language::Japanese]);
    assert_eq!(info.regions, [Region::Japan]);
}

#[test]
fn a_bnr2_disc_gives_six_languages_in_banner_order() {
    let texts: Vec<_> = ["English", "Deutsch", "Francais", "Espanol", "Italiano", "Nederlands"].map(fx::text).into();
    let bnr = fx::bnr(&texts, &fx::image(), "windows-1252");
    let info = read(&disc(Some(bnr), "GZLP01", None)).unwrap();
    // The original implementation lists them in banner order (a map); `titles` here is in language order.
    let langs: Vec<Language> = info.titles.iter().map(|(l, _)| *l).collect();
    assert_eq!(
        langs,
        [Language::English, Language::French, Language::German, Language::Italian, Language::Spanish, Language::Dutch]
    );
    assert_eq!(info.title_in(Language::German).unwrap().name, "Deutsch");
    assert_eq!(info.title_in(Language::Spanish).unwrap().name, "Espanol");
    assert_eq!(info.title_in(Language::French).unwrap().name, "Francais");
    assert_eq!(info.title(), Some("English"));
    assert_eq!(info.regions, [Region::Europe]);
}

#[test]
fn a_japan_disc_title_is_read_as_shift_jis() {
    let t = fx::Text {
        short_title: "ゼルダ".into(),
        short_maker: "任天堂".into(),
        long_title: "ゼルダの伝説".into(),
        long_maker: "任天堂株式会社".into(),
        description: "風のタクト".into(),
    };
    let info = read(&disc(Some(fx::bnr(&[t], &fx::image(), "Shift_JIS")), "GZLJ01", None)).unwrap();
    let ja = info.title_in(Language::Japanese).unwrap();
    assert_eq!(ja.name, "ゼルダの伝説");
    assert_eq!(ja.publisher.as_deref(), Some("任天堂株式会社"));
}

#[test]
fn a_western_title_keeps_its_windows_1252_accents() {
    let info = read(&disc(Some(fx::bnr1("Pokémon Colosseum")), "GZLE01", None)).unwrap();
    assert_eq!(info.title(), Some("Pokémon Colosseum"));
}

#[test]
fn the_banner_is_a_decoded_rgb5a3_image_with_tile_order_and_alpha() {
    let image = fx::image_with(|x, y| match (x, y) {
        (0, 0) => 0xFC00,   // opaque RGB555 red
        (5, 6) => 0x4F00,   // A3RGB4: alpha 4/7, red 15
        (95, 31) => 0x83E0, // opaque green
        _ => 0x8000,
    });
    let bnr = fx::bnr(&[fx::text("T")], &image, "windows-1252");
    let info = read(&disc(Some(bnr), "GZLE01", None)).unwrap();
    let icon = info.icon.as_ref().unwrap();
    assert_eq!((icon.width, icon.height), (96, 32));
    assert_eq!(px(icon, 0, 0), [255, 0, 0, 255]);
    assert_eq!(px(icon, 5, 6), [255, 0, 0, 146]);
    assert_eq!(px(icon, 95, 31), [0, 255, 0, 255]);
    assert_eq!(info.images, [ImageKind::Banner]);
}

#[test]
fn a_disc_with_no_opening_bnr_still_gives_ids_and_the_header_title() {
    let info = read(&disc(None, "GZLE01", None)).unwrap();
    assert!(info.titles.is_empty());
    assert!(info.icon.is_none());
    assert_eq!(info.title(), None);
    assert_eq!(info.ids.game_id.as_deref(), Some("GZLE01"));
    assert_eq!(info.header_title.as_deref(), Some("Header Title"));
}

#[test]
fn an_opening_bnr_inside_a_subfolder_is_not_the_banner() {
    let info = read(&disc(None, "GZLE01", Some(fx::bnr1("Decoy")))).unwrap();
    assert_eq!(info.title(), None);
}

#[test]
fn the_root_opening_bnr_wins_over_a_decoy_in_a_subfolder() {
    let info = read(&disc(Some(fx::bnr1("Wind Quest")), "GZLE01", Some(fx::bnr1("Decoy")))).unwrap();
    assert_eq!(info.title(), Some("Wind Quest"));
}

#[test]
fn a_bnr_with_bad_magic_gives_no_titles() {
    let mut bad = fx::bnr1("Wind Quest");
    bad[0] = b'X';
    assert!(read(&disc(Some(bad), "GZLE01", None)).unwrap().titles.is_empty());
}

#[test]
fn a_truncated_disc_keeps_what_is_intact_and_never_throws() {
    let full = default_disc();
    assert_eq!(read(&full[..0x2500]).unwrap().ids.game_id.as_deref(), Some("GZLE01")); // header + part of the FST
    assert_eq!(read(&full[..0x8010]).unwrap().title(), None); // opening.bnr is cut
    assert!(read(&full[..0x100]).is_none());
    assert!(read(&[]).is_none());
}

#[test]
fn a_hostile_fst_offset_or_size_gives_no_banner_and_no_crash() {
    let mut bytes = default_disc();
    bytes[0x424..0x428].copy_from_slice(&0x7FFF_FFF0u32.to_be_bytes());
    bytes[0x428..0x42C].copy_from_slice(&u32::MAX.to_be_bytes());
    let info = read(&bytes).unwrap();
    assert_eq!(info.title(), None);
    assert_eq!(info.ids.game_id.as_deref(), Some("GZLE01"));
}

#[test]
fn a_wii_disc_or_another_file_is_not_a_gamecube_disc() {
    let mut wii = default_disc();
    wii[0x18..0x1C].copy_from_slice(&[0x5D, 0x1C, 0x9E, 0xA3]);
    wii[0x1C..0x20].fill(0);
    assert!(read(&default_disc()).is_some());
    assert!(read(&wii).is_none());
    assert!(read(&[7u8; 0x5000]).is_none());
}

#[test]
fn a_ciso_image_reads_like_the_plain_disc() {
    let info = read(&fx::ciso(&default_disc(), 0x8000)).unwrap();
    assert_eq!(info.title(), Some("Wind Quest"));
    assert_eq!(info.ids.game_id.as_deref(), Some("GZLE01"));
    assert!(info.icon.is_some());
}

#[test]
fn a_ciso_with_the_banner_block_missing_gives_ids_and_no_title() {
    let ciso = fx::ciso(&default_disc(), 0x8000);
    let info = read(&ciso[..0x8000 + 0x8000]).unwrap();
    assert_eq!(info.title(), None);
}

#[test]
fn a_gcz_image_reads_like_the_plain_disc() {
    let info = read(&fx::gcz(&default_disc(), 0x8000)).unwrap();
    assert_eq!(info.title(), Some("Wind Quest"));
    assert_eq!(info.ids.game_id.as_deref(), Some("GZLE01"));
}

#[test]
fn a_gcz_with_corrupt_blocks_gives_nothing_and_never_throws() {
    let disc = default_disc();
    let mut gcz = fx::gcz(&disc, 0x8000);
    assert!(read(&gcz).is_some(), "the intact image reads");
    let data_start = 0x20 + 12 * disc.len().div_ceil(0x8000);
    gcz[data_start..].fill(0x55);
    assert!(read(&gcz).is_none());
    assert!(read(&gcz[..0x30]).is_none());
}

#[test]
fn candidates_are_the_disc_image_extensions() {
    for n in ["a.iso", "a.GCM", "a.ciso", "a.gcz", "a.rvz", "a.wia", "a.tgc"] {
        assert!(gc::is_candidate(n), "{n}");
    }
    for n in ["a.nds", "a.wbfs", "a.zip", "iso", "a.rpx"] {
        assert!(!gc::is_candidate(n), "{n}");
    }
}

// ---- GameCubeRvzTest ----

fn rvz_disc() -> Vec<u8> {
    let bnr = fx::bnr1("Wind Quest");
    fx::disc("GZLE01", "Header Title", &[("opening.bnr", &bnr)], None)
}
fn rvz(disc: &[u8], o: fx::RvzOptions) -> Vec<u8> {
    fx::rvz(disc, o, b"RVZ\x01")
}

#[test]
fn an_rvz_with_zstd_groups_reads_like_the_plain_disc() {
    let info = read(&rvz(&rvz_disc(), fx::RvzOptions::default())).unwrap();
    assert_eq!(info.title(), Some("Wind Quest"));
    assert_eq!(info.ids.game_id.as_deref(), Some("GZLE01"));
    assert!(info.icon.is_some());
}

#[test]
fn an_rvz_with_a_bigger_chunk_size_and_stored_groups_reads_too() {
    let o = fx::RvzOptions { compression: 0, chunk: 0x20000, ..Default::default() };
    assert_eq!(read(&rvz(&rvz_disc(), o)).unwrap().title(), Some("Wind Quest"));
}

#[test]
fn rvz_packed_groups_read_junk_runs_as_zeros() {
    let o = fx::RvzOptions { packed: true, ..Default::default() };
    assert_eq!(read(&rvz(&rvz_disc(), o)).unwrap().title(), Some("Wind Quest"));
}

#[test]
fn a_wia_an_unsupported_compression_and_a_truncated_rvz_give_nothing() {
    assert!(read(&rvz(&rvz_disc(), fx::RvzOptions::default())).is_some());
    assert!(read(&fx::rvz(&rvz_disc(), fx::RvzOptions::default(), b"WIA\x01")).is_none());
    let mut lzma = rvz(&rvz_disc(), fx::RvzOptions::default());
    lzma[0x4C + 3] = 3;
    assert!(read(&lzma).is_none());
    let good = rvz(&rvz_disc(), fx::RvzOptions::default());
    assert!(read(&good[..good.len() / 2]).is_none());
    assert!(read(&good[..0x60]).is_none());
}

#[test]
fn reading_a_title_from_a_2_mib_chunk_rvz_does_not_read_the_whole_file() {
    // The original test asserts a time budget; here the property behind it: the bytes asked of the source stay far
    // below the file size.
    use gamemeta_core::{ByteSource, ReadError};
    struct Counting<'a>(&'a [u8], u64);
    impl ByteSource for Counting<'_> {
        fn size(&self) -> u64 {
            self.0.len() as u64
        }
        fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
            self.1 += buf.len() as u64;
            SliceSource::new(self.0).read_at(offset, buf)
        }
    }
    let mut data = vec![0u8; 6 << 20];
    let mut x = 3u32;
    for b in &mut data {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *b = (x >> 24) as u8;
    }
    let bnr = fx::bnr1("Wind Quest");
    let big = fx::disc("GZLE01", "Header Title", &[("data.bin", &data), ("opening.bnr", &bnr)], None);
    let file = rvz(&big, fx::RvzOptions { chunk: 2 << 20, ..Default::default() });
    let mut src = Counting(&file, 0);
    let info = gc::read_info(&mut src).unwrap();
    assert_eq!(info.title(), Some("Wind Quest"));
    assert!(src.1 < file.len() as u64 / 2, "read {} of {} bytes", src.1, file.len());
}
