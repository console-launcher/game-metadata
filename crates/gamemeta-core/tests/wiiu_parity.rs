// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Parity with the original reader's `WiiUMetadataTest`, `TgaDecoderTest` and `WiiUOpenerFilesTest`: same synthetic
//! titles, same expected outputs, one Rust test per reference test.

use std::collections::HashMap;

use gamemeta_core::wiiu::{self, OpenerFiles, TitleFiles};
use gamemeta_core::{ByteSource, GameInfo, ImageKind, Language, Platform, Region, RgbaImage, SliceSource};
use gamemeta_fixtures::wiiu as fx;

struct MapFiles(HashMap<String, Vec<u8>>);

impl MapFiles {
    fn new(files: Vec<(String, Vec<u8>)>) -> Self {
        MapFiles(files.into_iter().collect())
    }
    fn standard() -> Self {
        Self::new(fx::standard_files(&fx::Meta::default().xml()))
    }
    fn with_meta(meta: &fx::Meta) -> Self {
        Self::new(fx::standard_files(&meta.xml()))
    }
}

impl TitleFiles for MapFiles {
    fn size(&mut self, path: &str) -> Option<u64> {
        self.0.get(path).map(|d| d.len() as u64)
    }
    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        self.0.get(path).filter(|d| d.len() <= max).cloned()
    }
}

fn px(img: &RgbaImage, x: usize, y: usize) -> [u8; 4] {
    let at = (y * img.width as usize + x) * 4;
    img.pixels[at..at + 4].try_into().unwrap()
}

fn read(files: &mut MapFiles) -> Option<GameInfo> {
    wiiu::read_title(files)
}

fn name_in(info: &GameInfo, l: Language) -> Option<&str> {
    info.title_in(l).map(|t| t.name.as_str())
}

// ---- WiiUMetadataTest

#[test]
fn an_unpacked_title_gives_titles_ids_region_icon_and_boot_assets() {
    let info = read(&mut MapFiles::standard()).unwrap();
    assert_eq!(info.platform, Platform::WiiU);
    assert_eq!(info.title(), Some("Test Quest"));
    let t = info.title_in(Language::English).unwrap();
    assert_eq!(t.short_name, "TQ");
    assert_eq!(t.publisher.as_deref(), Some("Test Games"));
    assert_eq!(info.ids.title_id.as_deref(), Some("0005000010123456"));
    assert_eq!(info.ids.product_code.as_deref(), Some("WUP-P-ABCE"));
    assert_eq!(info.regions, [Region::NorthAmerica]);
    assert!(!info.region_free);
    let icon = info.icon.as_ref().unwrap();
    assert_eq!((icon.width, icon.height), (4, 2));
    assert_eq!(px(icon, 0, 0), [255, 0, 0, 255]);
    assert_eq!(px(icon, 3, 1), [30, 20, 10, 128]);
    assert_eq!(info.images, [ImageKind::Icon, ImageKind::TvSplash, ImageKind::GamePadSplash, ImageKind::BootLogo]);
    assert_eq!(info.boot_movie, Some(1234));
    assert_eq!(info.boot_sound, Some(99));
}

#[test]
fn splashes_decode_on_demand() {
    let mut files = MapFiles::standard();
    let tv = wiiu::image(&mut files, ImageKind::TvSplash).unwrap();
    assert_eq!((tv.width, tv.height), (16, 9));
    assert_eq!(px(&tv, 0, 0), [3, 2, 1, 255]);
    assert_eq!(wiiu::image(&mut files, ImageKind::GamePadSplash).unwrap().height, 5);
    assert_eq!(wiiu::image(&mut files, ImageKind::BootLogo).unwrap().width, 2);
    assert!(wiiu::image(&mut files, ImageKind::Banner).is_none());
}

#[test]
fn titles_per_language_missing_ones_skipped_long_falls_back_to_short() {
    let meta = fx::Meta {
        longs: vec![("ja", "日本語の題名".into()), ("fr", "Quête\ntest".into())],
        shorts: vec![("en", "Short Only".into()), ("ja", "短".into()), ("de", "Kurz".into())],
        publishers: vec![("ja", "会社".into())],
        ..Default::default()
    };
    let info = read(&mut MapFiles::with_meta(&meta)).unwrap();
    let langs: Vec<Language> = info.titles.iter().map(|(l, _)| *l).collect();
    assert_eq!(langs, [Language::Japanese, Language::English, Language::French, Language::German]);
    assert_eq!(name_in(&info, Language::Japanese), Some("日本語の題名"));
    assert_eq!(info.title_in(Language::Japanese).unwrap().publisher.as_deref(), Some("会社"));
    assert_eq!(name_in(&info, Language::French), Some("Quête test"));
    assert_eq!(name_in(&info, Language::English), Some("Short Only"));
    assert_eq!(info.title_in(Language::German).unwrap().publisher, None);
    assert_eq!(info.title(), Some("Short Only"));
}

#[test]
fn entities_and_cdata_in_titles_are_decoded() {
    let meta = fx::Meta {
        longs: vec![("en", "Fish &amp; Chips &#233; &#x41;".into()), ("fr", "<![CDATA[A <b> & C]]>".into())],
        ..Default::default()
    };
    let info = read(&mut MapFiles::with_meta(&meta)).unwrap();
    assert_eq!(name_in(&info, Language::English), Some("Fish & Chips é A"));
    assert_eq!(name_in(&info, Language::French), Some("A <b> & C"));
}

#[test]
fn the_region_bitmask_maps_to_regions_and_all_of_them_is_region_free() {
    let info = |region: &str| read(&mut MapFiles::with_meta(&fx::Meta { region, ..Default::default() })).unwrap();
    assert_eq!(info("00000005").regions, [Region::Japan, Region::Europe]);
    assert_eq!(info("00000070").regions, [Region::China, Region::Korea, Region::Taiwan]);
    let free = info("7FFFFFFF");
    assert!(free.region_free);
    assert!(free.regions.is_empty());
}

#[test]
fn a_huge_title_text_does_not_overflow_the_scanner() {
    let meta = fx::Meta { longs: vec![("en", "x".repeat(900_000))], ..Default::default() };
    assert_eq!(read(&mut MapFiles::with_meta(&meta)).unwrap().title().map(str::len), Some(900_000));
}

#[test]
fn ids_are_normalized_to_upper_case() {
    let meta = fx::Meta { title_id: "000500001010abcd", product_code: "WUP-P-abcd", ..Default::default() };
    let info = read(&mut MapFiles::with_meta(&meta)).unwrap();
    assert_eq!(info.ids.title_id.as_deref(), Some("000500001010ABCD"));
    assert_eq!(info.ids.product_code.as_deref(), Some("WUP-P-ABCD"));
}

#[test]
fn a_title_with_no_images_or_boot_media_still_reads() {
    let mut files = MapFiles::new(vec![("meta/meta.xml".into(), fx::Meta::default().xml().into_bytes())]);
    let info = read(&mut files).unwrap();
    assert_eq!(info.title(), Some("Test Quest"));
    assert!(info.icon.is_none() && info.images.is_empty() && info.boot_movie.is_none() && info.boot_sound.is_none());
}

#[test]
fn an_unreadable_icon_is_left_out_of_the_images() {
    let mut files = MapFiles::standard();
    files.0.insert("meta/iconTex.tga".into(), vec![0; 5]);
    let info = read(&mut files).unwrap();
    assert!(info.icon.is_none());
    assert!(!info.images.contains(&ImageKind::Icon));
    assert!(info.images.contains(&ImageKind::TvSplash));
}

#[test]
fn no_meta_xml_a_broken_one_or_one_with_nothing_in_it_gives_none() {
    assert!(read(&mut MapFiles::standard()).is_some());
    assert!(read(&mut MapFiles::new(vec![("code/game.rpx".into(), vec![0; 4])])).is_none());
    assert!(
        read(&mut MapFiles::new(vec![("meta/meta.xml".into(), b"<menu><longname_en>unclosed".to_vec())])).is_none()
    );
    assert!(read(&mut MapFiles::new(vec![("meta/meta.xml".into(), Vec::new())])).is_none());
}

#[test]
fn wud_and_wux_are_encrypted_and_give_nothing_never_asking_for_a_key() {
    let junk: Vec<u8> = (0..0x8000u32).map(|i| (i % 251) as u8).collect();
    assert!(read(&mut MapFiles::standard()).is_some());
    for name in ["game.wud", "game.wux", "game.rpx"] {
        assert!(gamemeta_core::read(&mut SliceSource::new(&junk), name).is_none(), "{name}");
    }
    assert!(!wiiu::is_candidate("game.wud"));
}

#[test]
fn candidates_are_rpx_and_wua() {
    assert!(wiiu::is_candidate("code/game.RPX"));
    assert!(wiiu::is_candidate("games.wua"));
    for n in ["a.iso", "a.wux", "rpx", "a.nds"] {
        assert!(!wiiu::is_candidate(n), "{n}");
    }
}

// ---- .wua

type FileList = Vec<(String, Vec<u8>)>;

fn wua(titles: &[(&str, FileList)]) -> Vec<u8> {
    let files: Vec<(String, Vec<u8>)> = titles
        .iter()
        .flat_map(|(dir, files)| files.iter().map(move |(p, d)| (format!("{dir}/{p}"), d.clone())))
        .collect();
    fx::zarchive(&files)
}

fn read_wua(bytes: &[u8]) -> Option<GameInfo> {
    gamemeta_core::read(&mut SliceSource::new(bytes), "games.wua")
}

#[test]
fn a_wua_gives_the_same_fields_as_the_unpacked_title() {
    let bytes = wua(&[("0005000010123456_v0", fx::standard_files(&fx::Meta::default().xml()))]);
    let info = read_wua(&bytes).unwrap();
    assert_eq!(info.platform, Platform::WiiU);
    assert_eq!(info.title(), Some("Test Quest"));
    assert_eq!(info.ids.title_id.as_deref(), Some("0005000010123456"));
    assert_eq!(info.boot_movie, Some(1234));
    assert_eq!(info.icon.as_ref().unwrap().width, 4);
    let mut src = SliceSource::new(&bytes);
    let mut files = wiiu::open_wua(&mut src).unwrap();
    assert_eq!(wiiu::image(&mut files, ImageKind::TvSplash).unwrap().width, 16);
}

#[test]
fn a_wua_with_a_big_file_in_front_still_reads_later_files_raw_and_zstd_blocks() {
    let mut x = 7u32;
    let noise: Vec<u8> = (0..300_000)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 24) as u8
        })
        .collect();
    let mut files = fx::standard_files(&fx::Meta::default().xml());
    files.push(("aaa/big.bin".into(), noise.clone()));
    let bytes = wua(&[("0005000010123456_v0", files)]);
    assert_eq!(read_wua(&bytes).unwrap().title(), Some("Test Quest"));
    let mut src = SliceSource::new(&bytes);
    let mut files = wiiu::open_wua(&mut src).unwrap();
    assert_eq!(files.read("aaa/big.bin", 1 << 20).unwrap()[..1000], noise[..1000]);
}

#[test]
fn a_wua_with_an_update_and_a_game_reads_the_base_title() {
    let update = fx::standard_files(
        &fx::Meta { longs: vec![("en", "Update Name".into())], title_id: "0005000E10123456", ..Default::default() }
            .xml(),
    );
    let game = fx::standard_files(&fx::Meta { longs: vec![("en", "Base Game".into())], ..Default::default() }.xml());
    let bytes = wua(&[("0005000E10123456_v32", update), ("0005000010123456_v0", game)]);
    assert_eq!(read_wua(&bytes).unwrap().title(), Some("Base Game"));
}

#[test]
fn a_wua_holding_only_an_update_reads_that_title() {
    let update =
        fx::standard_files(&fx::Meta { longs: vec![("en", "Update Name".into())], ..Default::default() }.xml());
    assert_eq!(read_wua(&wua(&[("0005000E10123456_v32", update)])).unwrap().title(), Some("Update Name"));
}

#[test]
fn a_wua_without_meta_xml_a_truncated_one_and_junk_give_none_and_never_panic() {
    let no_meta = wua(&[("0005000010123456_v0", vec![("code/game.rpx".into(), vec![0; 10])])]);
    let good = wua(&[("0005000010123456_v0", fx::standard_files(&fx::Meta::default().xml()))]);
    assert!(read_wua(&good).is_some());
    assert!(read_wua(&no_meta).is_none());
    assert!(read_wua(&good[..good.len() - 50]).is_none());
    assert!(read_wua(&good[..100]).is_none());
    assert!(read_wua(&[3u8; 5000]).is_none());
    let mut bad = good.clone();
    bad[..200].fill(0x55);
    assert!(read_wua(&bad).is_none(), "corrupt compressed data: unreadable, not a crash");
}

// ---- TgaDecoderTest

fn pattern(x: usize, y: usize) -> [u8; 4] {
    [(x * 10) as u8, (y * 10) as u8, 200, (100 + x + y) as u8]
}

#[test]
fn a_bottom_up_32_bit_tga_comes_out_top_down_with_alpha() {
    let img = wiiu::decode_tga(&fx::tga(3, 2, 32, false, false, pattern)).unwrap();
    assert_eq!((img.width, img.height), (3, 2));
    assert_eq!(px(&img, 0, 0), [200, 0, 0, 100]);
    assert_eq!(px(&img, 2, 1), [200, 10, 20, 103]);
}

#[test]
fn a_top_down_32_bit_tga_keeps_its_row_order() {
    let img = wiiu::decode_tga(&fx::tga(3, 2, 32, true, false, pattern)).unwrap();
    assert_eq!(px(&img, 0, 0), [200, 0, 0, 100]);
    assert_eq!(px(&img, 2, 1), [200, 10, 20, 103]);
}

#[test]
fn a_24_bit_tga_is_opaque() {
    let img = wiiu::decode_tga(&fx::tga(2, 2, 24, false, false, pattern)).unwrap();
    assert_eq!(px(&img, 1, 1)[3], 255);
    assert_eq!(px(&img, 1, 1), [200, 10, 10, 255]);
}

#[test]
fn an_rle_tga_decodes_like_the_plain_one() {
    let f = |_: usize, y: usize| [1, 2, y as u8, 255];
    let plain = wiiu::decode_tga(&fx::tga(6, 4, 32, false, false, f)).unwrap();
    let rle = wiiu::decode_tga(&fx::tga(6, 4, 32, false, true, f)).unwrap();
    assert_eq!(plain.pixels, rle.pixels);
}

#[test]
fn short_truncated_unsupported_or_oversized_files_give_none() {
    let ok = fx::tga(3, 2, 32, false, false, pattern);
    assert!(wiiu::decode_tga(&ok).is_some());
    assert!(wiiu::decode_tga(&[]).is_none());
    assert!(wiiu::decode_tga(&ok[..10]).is_none());
    assert!(wiiu::decode_tga(&ok[..ok.len() - 1]).is_none());
    let patch = |at: usize, v: u8| {
        let mut b = ok.clone();
        b[at] = v;
        b
    };
    assert!(wiiu::decode_tga(&patch(16, 8)).is_none());
    assert!(wiiu::decode_tga(&patch(1, 1)).is_none());
    let mut wide = ok.clone();
    wide[12] = 0xFF;
    wide[13] = 0xFF;
    assert!(wiiu::decode_tga(&wide).is_none());
    let mut bad = fx::tga(3, 2, 32, false, true, pattern);
    bad[18] = 0x7F;
    assert!(wiiu::decode_tga(&bad).is_none());
}

// ---- WiiUOpenerFilesTest

struct Owned(Vec<u8>);

impl ByteSource for Owned {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_at(&mut self, o: u64, b: &mut [u8]) -> Result<(), gamemeta_core::ReadError> {
        SliceSource::new(&self.0).read_at(o, b)
    }
}

type Opener = OpenerFiles<Box<dyn FnMut(&str) -> Option<Box<dyn ByteSource>>>>;
type Opened = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

fn opener() -> (Opener, Opened) {
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = opened.clone();
    let files: HashMap<&str, Vec<u8>> =
        HashMap::from([("../meta/meta.xml", b"<a>b</a>".to_vec()), ("../meta/big.bin", vec![0; 100])]);
    let open = move |name: &str| -> Option<Box<dyn ByteSource>> {
        log.borrow_mut().push(name.to_owned());
        let bytes = files.get(name)?.clone();
        Some(Box::new(Owned(bytes)))
    };
    (OpenerFiles(Box::new(open)), opened)
}

#[test]
fn a_title_path_is_opened_relative_to_the_rpxs_folder() {
    let (mut subject, opened) = opener();
    assert_eq!(subject.read("meta/meta.xml", 1000).as_deref(), Some(&b"<a>b</a>"[..]));
    assert_eq!(subject.size("meta/big.bin"), Some(100));
    assert_eq!(*opened.borrow(), ["../meta/meta.xml", "../meta/big.bin"]);
}

#[test]
fn a_missing_file_or_one_over_the_cap_is_none() {
    let (mut subject, _) = opener();
    assert_eq!(subject.read("meta/big.bin", 100).map(|d| d.len()), Some(100), "positive control");
    assert!(subject.read("meta/none.tga", 1000).is_none());
    assert!(subject.size("meta/none.tga").is_none());
    assert!(subject.read("meta/big.bin", 50).is_none());
}

#[test]
fn a_whole_title_reads_through_the_opener_as_the_rpxs_sibling_folders() {
    let files: HashMap<String, Vec<u8>> =
        fx::standard_files(&fx::Meta::default().xml()).into_iter().map(|(p, d)| (format!("../{p}"), d)).collect();
    let mut subject = OpenerFiles(move |name: &str| -> Option<Box<dyn ByteSource>> {
        files.get(name).map(|d| Box::new(Owned(d.clone())) as Box<dyn ByteSource>)
    });
    let info = wiiu::read_title(&mut subject).unwrap();
    assert_eq!(info.title(), Some("Test Quest"));
    assert_eq!(info.ids.title_id.as_deref(), Some("0005000010123456"));
    assert!(info.icon.is_some());
}
