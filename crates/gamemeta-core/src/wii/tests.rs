// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Parity with the original reader's Wii tests (`WiiMetadataTest`, `WiiFormatsTest`): same synthetic data, same
//! expected outputs, one Rust test per reference test.

use alloc::borrow::Cow;
use alloc::vec::Vec;
use gamemeta_fixtures::gc as gcfx;
use gamemeta_fixtures::wii as fx;
use gamemeta_fixtures::wii::{PartFile, U8File};

use super::*;
use crate::{ByteSource, GameInfo, ImageKind, Language, Platform, ReadError, Region, RgbaImage, SliceSource};

fn px(i: &RgbaImage, x: usize, y: usize) -> [u8; 4] {
    let at = (y * i.width as usize + x) * 4;
    i.pixels[at..at + 4].try_into().unwrap()
}

// ---- Lz77Test

fn sample() -> Vec<u8> {
    (0..100u32).map(|i| (i * 7) as u8).collect()
}

fn lz(data: &[u8]) -> Option<Vec<u8>> {
    lz77::decompress(data, 0, lz77::MAX_SIZE)
}

#[test]
fn lz77_literals_only_stream_decodes() {
    assert_eq!(lz(&fx::lz77_literals(&sample(), 0x10)), Some(sample()));
}

#[test]
fn lz77_type_0x10_back_references_copy_earlier_output() {
    let six = [0x10, 6, 0, 0, 0x40, b'a', 0x20, 0x00];
    assert_eq!(lz(&six).as_deref(), Some(&b"aaaaaa"[..]));
}

#[test]
fn lz77_type_0x11_extended_lengths_decode() {
    let out = [0x11, 0x12, 0, 0, 0x40, b'z', 0x00, 0x00, 0x00];
    assert_eq!(lz(&out), Some(vec![b'z'; 18]));
}

#[test]
fn lz77_header_at_an_offset_is_read_from_there() {
    let mut data = vec![0u8; 8];
    data.extend(fx::lz77_literals(&sample(), 0x10));
    assert_eq!(lz77::decompress(&data, 8, lz77::MAX_SIZE), Some(sample()));
}

#[test]
fn lz77_damaged_or_oversized_streams_give_none() {
    let ok = fx::lz77_literals(&sample(), 0x10);
    assert_eq!(lz(&ok), Some(sample()));
    assert!(lz(&ok[..ok.len() - 20]).is_none(), "truncated");
    assert!(lz(&[0x30, 1, 0, 0, 0]).is_none(), "not LZ77");
    assert!(lz(&[0x10, 0, 0, 0x40, 0]).is_none(), "claims 4 MiB of nothing");
    assert!(lz77::decompress(&ok, 0, 50).is_none(), "over the limit");
    assert!(lz(&[]).is_none());
    assert!(lz(&[0x10, 5, 0, 0, 0x80, 0x20, 0x05]).is_none(), "a reference before the start of the output");
}

// ---- U8ArchiveTest

fn u8_files() -> Vec<U8File<'static>> {
    vec![
        U8File("meta/icon.bin", &[1, 2, 3]),
        U8File("meta/banner.bin", &[4, 5]),
        U8File("arc/timg/Pic.TPL", &[6]),
        U8File("top.txt", &[7, 8, 9, 10]),
    ]
}

fn parse_u8(bytes: &[u8], at: usize) -> Option<u8_archive::U8Archive<'_>> {
    u8_archive::U8Archive::parse(Cow::Borrowed(bytes), at)
}

#[test]
fn u8_files_are_found_by_path_case_insensitively_in_nested_folders() {
    let bytes = fx::u8_archive(&u8_files());
    let u8 = parse_u8(&bytes, 0).unwrap();
    assert_eq!(u8.file("meta/icon.bin"), Some(&[1, 2, 3][..]));
    assert_eq!(u8.file("META/Banner.BIN"), Some(&[4, 5][..]));
    assert_eq!(u8.file("arc/timg/pic.tpl"), Some(&[6][..]));
    assert_eq!(u8.file("top.txt"), Some(&[7, 8, 9, 10][..]));
    assert!(u8.file("meta").is_none(), "a folder is not a file");
    assert!(u8.file("meta/missing.bin").is_none());
    let mut paths: Vec<&str> = u8.entries.iter().map(|e| e.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, ["arc/timg/Pic.TPL", "meta/banner.bin", "meta/icon.bin", "top.txt"]);
}

#[test]
fn u8_archive_at_an_offset_inside_a_larger_buffer_parses() {
    let mut bytes = vec![0u8; 0x40];
    bytes.extend(fx::u8_archive(&u8_files()));
    assert_eq!(parse_u8(&bytes, 0x40).unwrap().file("meta/icon.bin"), Some(&[1, 2, 3][..]));
}

#[test]
fn u8_bad_magic_truncated_table_and_files_past_the_end_give_none() {
    let ok = fx::u8_archive(&u8_files());
    assert!(parse_u8(&ok, 0).is_some());
    assert!(parse_u8(&[0u8; 64], 0).is_none());
    assert!(parse_u8(&ok[..0x30], 0).is_none());
    assert!(parse_u8(&[], 0).is_none());
    let cut = parse_u8(&ok[..ok.len() - 40], 0);
    assert!(cut.is_some_and(|a| a.file("top.txt").is_none()), "the cut file is not served");
}

// ---- TplTest

#[test]
fn tpl_rgb5a3_decodes_opaque_rgb555_and_translucent_rgb444() {
    let data = fx::words16(8, 4, |x, _| match x {
        0 => 0xFC00,
        1 => 0x7F00,
        _ => 0x8000,
    });
    let img = tpl::decode(&fx::tpl_plain(8, 4, 5, &data)).unwrap();
    assert_eq!((img.width, img.height), (8, 4));
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
    assert_eq!(px(&img, 0, 3), [255, 0, 0, 255]);
    assert_eq!(px(&img, 2, 0), [0, 0, 0, 255]);
    assert_eq!(px(&img, 1, 1), [255, 0, 0, 255]);
    let translucent = tpl::decode(&fx::tpl_plain(4, 4, 5, &fx::words16(4, 4, |_, _| 0x3F0F))).unwrap();
    assert_eq!(px(&translucent, 0, 0), [255, 0, 255, 109]);
}

#[test]
fn tpl_rgb565_decodes() {
    let img =
        tpl::decode(&fx::tpl_plain(4, 4, 4, &fx::words16(4, 4, |x, _| if x == 0 { 0xF800 } else { 0x07E0 }))).unwrap();
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
    assert_eq!(px(&img, 1, 0), [0, 255, 0, 255]);
}

#[test]
fn tpl_rgba8_reads_alpha_and_red_from_the_first_half_of_a_tile_and_green_and_blue_from_the_second() {
    let data = fx::rgba8(8, 4, |x, y| (0x80 << 24) | ((x as u32) << 16) | ((y as u32) << 8) | 0x33);
    let img = tpl::decode(&fx::tpl_plain(8, 4, 6, &data)).unwrap();
    assert_eq!(px(&img, 5, 2), [5, 2, 0x33, 0x80]);
    assert_eq!(px(&img, 0, 0), [0, 0, 0x33, 0x80]);
}

#[test]
fn tpl_i8_and_ia8_decode_to_grey() {
    let i8 = tpl::decode(&fx::tpl_plain(8, 4, 1, &fx::bytes8(8, 4, |x, _| (x * 30) as u8))).unwrap();
    assert_eq!(px(&i8, 3, 0), [90, 90, 90, 255]);
    let ia8 = tpl::decode(&fx::tpl_plain(4, 4, 3, &fx::words16(4, 4, |_, _| 0x80C0))).unwrap();
    assert_eq!(px(&ia8, 1, 1), [0xC0, 0xC0, 0xC0, 0x80]);
}

#[test]
fn tpl_c8_uses_its_palette() {
    let mut palette = vec![0u8; 512];
    palette[2] = 0xF8;
    let data = fx::bytes8(8, 4, |x, _| u8::from(x < 4));
    let img = tpl::decode(&fx::tpl(8, 4, 9, &data, Some(&palette), 1)).unwrap();
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
    assert_eq!(px(&img, 5, 0), [0, 0, 0, 255]);
}

#[test]
fn tpl_cmpr_decodes_a_four_colour_block_and_a_transparent_three_colour_block() {
    let a = (0xF800, 0x001F, 0b00_01_10_11 << 24);
    let b = (0x001F, 0xF800, 0b11_11_11_11 << 24);
    let img = tpl::decode(&fx::tpl_plain(8, 8, 14, &fx::cmpr8x8(&[a, b, a, a]))).unwrap();
    assert_eq!(px(&img, 0, 0), [255, 0, 0, 255]);
    assert_eq!(px(&img, 1, 0), [0, 0, 255, 255]);
    assert_eq!(px(&img, 2, 0), [170, 0, 85, 255]);
    assert_eq!(px(&img, 3, 0), [85, 0, 170, 255]);
    assert_eq!(px(&img, 4, 0)[3], 0, "the second block's index 3 is transparent");
    assert_eq!(px(&img, 0, 5), [255, 0, 0, 255]);
}

#[test]
fn tpl_size_reads_the_dimensions_and_bad_input_gives_none() {
    let tpl_bytes = fx::tpl_plain(8, 4, 5, &fx::words16(8, 4, |_, _| 0));
    assert_eq!(tpl::size(&tpl_bytes), Some((8, 4)));
    assert!(tpl::decode(&tpl_bytes[..0x10]).is_none(), "truncated header");
    assert!(tpl::decode(&tpl_bytes[..tpl_bytes.len() - 10]).is_none(), "truncated pixels");
    assert!(tpl::decode(&fx::tpl_plain(8, 4, 99, &[0; 64])).is_none(), "unknown format");
    assert!(tpl::decode(&[0; 64]).is_none());
    assert!(tpl::decode(&fx::tpl_plain(0, 0, 5, &[])).is_none());
    assert!(tpl::decode(&fx::tpl_plain(9000, 9000, 5, &[0; 16])).is_none(), "absurd size");
}

// ---- WiiOpeningBnrTest

fn wide_logo() -> Vec<u8> {
    fx::tpl_plain(64, 16, 5, &fx::words16(64, 16, |_, _| 0x4F00))
}

fn solid(w: usize, h: usize, v: u16) -> Vec<u8> {
    fx::tpl_plain(w as u16, h as u16, 5, &fx::words16(w, h, |_, _| v))
}

fn dims(p: &opening::Parsed) -> Option<(u32, u32)> {
    p.logo.as_ref().map(|l| (l.width, l.height))
}

fn title_of(p: &opening::Parsed, l: Language) -> Option<&str> {
    p.titles.iter().find(|(x, _)| *x == l).map(|(_, t)| t.name.as_str())
}

#[test]
fn opening_titles_per_language_come_from_the_imet_header_in_order() {
    let bnr =
        fx::opening_plain(&[("ja", "ゼルダ"), ("en", "Test Quest"), ("de", "Testquest"), ("ko", "테스트")], None, None);
    let p = opening::parse(&bnr).unwrap();
    assert_eq!(title_of(&p, Language::English), Some("Test Quest"));
    assert_eq!(title_of(&p, Language::Japanese), Some("ゼルダ"));
    assert_eq!(title_of(&p, Language::German), Some("Testquest"));
    assert_eq!(title_of(&p, Language::Korean), Some("테스트"));
    assert_eq!(p.titles.len(), 4, "blank languages are left out");
    assert!(p.logo.is_none());
}

#[test]
fn opening_a_title_on_two_lines_is_joined_by_a_space() {
    let p = opening::parse(&fx::opening_plain(&[("en", "Test\nQuest 2")], None, None)).unwrap();
    assert_eq!(p.titles[0].1.name, "Test Quest 2");
}

#[test]
fn opening_the_logo_decodes_from_plain_lz77_and_imd5_wrapped_archives() {
    for (lz, imd) in [(false, false), (true, false), (false, true), (true, true)] {
        let icon = fx::bin_archive(&wide_logo(), "Logo.tpl", lz, imd, &[]);
        let p = opening::parse(&fx::opening_plain(&[("en", "T")], Some(&icon), None)).unwrap();
        assert_eq!(dims(&p), Some((64, 16)), "lz77={lz} imd5={imd}");
    }
}

#[test]
fn opening_a_banner_archive_is_never_read() {
    let banner = fx::bin_archive(&wide_logo(), "Logo.tpl", false, false, &[]);
    let p = opening::parse(&fx::opening_plain(&[("en", "T")], None, Some(&banner))).unwrap();
    assert!(p.logo.is_none());
}

#[test]
fn opening_the_archive_itself_may_be_lz77_compressed() {
    let icon = fx::bin_archive(&wide_logo(), "Logo.tpl", false, false, &[]);
    let bnr = fx::opening(&[("en", "T")], Some(&icon), Some(&[]), |a| {
        let mut w = b"LZ77".to_vec();
        w.extend(fx::lz77_literals(&a, 0x10));
        w
    });
    assert!(opening::parse(&bnr).unwrap().logo.is_some());
}

#[test]
fn opening_the_logo_is_the_largest_wide_texture_with_transparency_not_a_bigger_opaque_square_or_narrow_one() {
    let (opaque, square, narrow, smaller) =
        (solid(128, 32, 0xFC00), solid(64, 64, 0x4F00), solid(32, 8, 0x4F00), solid(64, 8, 0x4F00));
    let extra = [
        U8File("arc/timg/Opaque.tpl", &opaque),
        U8File("arc/timg/Square.tpl", &square),
        U8File("arc/timg/Narrow.tpl", &narrow),
        U8File("arc/timg/Smaller.tpl", &smaller),
    ];
    let bin = fx::bin_archive(&wide_logo(), "Logo.tpl", false, false, &extra);
    let p = opening::parse(&fx::opening_plain(&[("en", "T")], Some(&bin), None)).unwrap();
    assert_eq!(dims(&p), Some((64, 16)));
}

#[test]
fn opening_an_archive_with_no_wide_translucent_texture_has_no_logo() {
    let (opaque, square) = (solid(128, 32, 0xFC00), solid(64, 64, 0x4F00));
    let bin = fx::bin_archive(&opaque, "Opaque.tpl", false, false, &[U8File("arc/timg/Sq.tpl", &square)]);
    assert!(opening::parse(&fx::opening_plain(&[("en", "T")], Some(&bin), None)).unwrap().logo.is_none());
}

#[test]
fn opening_a_broken_archive_keeps_the_titles_and_a_file_without_imet_is_none() {
    let p = opening::parse(&fx::opening_plain(&[("en", "T")], Some(&[3u8; 40]), None)).unwrap();
    assert!(p.logo.is_none());
    assert_eq!(p.titles[0].1.name, "T");
    assert!(opening::parse(&[0u8; 0x700]).is_none());
    assert!(opening::parse(&[0u8; 10]).is_none());
    assert!(opening::parse(&fx::opening_plain(&[("en", "T")], None, None)[..0x100]).is_none(), "header cut short");
}

#[test]
fn opening_an_ash_compressed_archive_is_skipped_without_error() {
    let icon = fx::bin_archive(&wide_logo(), "Logo.tpl", false, false, &[]);
    let bnr = fx::opening(&[("en", "T")], Some(&icon), None, |_| {
        let mut w = b"ASH0".to_vec();
        w.extend([0u8; 40]);
        w
    });
    let p = opening::parse(&bnr).unwrap();
    assert!(p.logo.is_none());
    assert!(!p.titles.is_empty());
}

// ---- WadHeaderTest

fn wad_info(bytes: &[u8]) -> Option<wad::Info> {
    wad::read(&mut SliceSource::new(bytes)).unwrap()
}

#[test]
fn wad_title_id_game_id_and_version_come_from_the_plain_header_ticket_and_tmd() {
    let info = wad_info(&fx::wad(0x0001_0001_5245_4145, 0x0203, "Is")).unwrap();
    assert_eq!(info.title_id, "0001000152454145");
    assert_eq!(info.game_id.as_deref(), Some("REAE"));
    assert_eq!(info.version, 0x0203);
}

#[test]
fn wad_a_channel_title_keeps_a_non_ascii_low_half_as_no_game_id() {
    let info = wad_info(&fx::wad(0x0001_0002_0000_0100, 0x0101, "Is")).unwrap();
    assert_eq!(info.title_id, "0001000200000100");
    assert!(info.game_id.is_none());
}

#[test]
fn wad_the_bk_backup_type_and_garbage() {
    assert!(wad_info(&fx::wad(0x0001_0001_5245_4145, 0x0101, "ib")).is_some());
    assert!(wad_info(&[0u8; 0x100]).is_none());
    assert!(wad_info(&fx::wad(1, 0x0101, "Is")[..0x300]).is_none());
    assert!(wad_info(&[]).is_none());
    assert!(wad_info(&fx::wad(0, 0x0101, "Is")).is_none(), "a zero title id");
}

// ---- WiiMetadataTest

fn logo16() -> Vec<u8> {
    wide_logo()
}

fn opening_bnr() -> Vec<u8> {
    let icon = fx::bin_archive(&logo16(), "Logo.tpl", true, false, &[]);
    fx::opening_plain(&[("ja", "テスト"), ("en", "Test Quest"), ("fr", "Quête Test")], Some(&icon), None)
}

fn partition() -> Vec<u8> {
    fx::partition(&[PartFile("opening.bnr", &opening_bnr())], Some(&[5u8; 0x700]))
}

fn rvz_with(part: &[u8], f: impl FnOnce(&mut fx::WiiRvzOptions)) -> fx::WiiRvz {
    let mut o = fx::WiiRvzOptions::default();
    f(&mut o);
    fx::rvz(part, &o)
}

fn read(bytes: &[u8]) -> Option<GameInfo> {
    read_info(&mut SliceSource::new(bytes))
}

/// Records every read so a test can assert what a reader never touched.
struct Spy<'a> {
    bytes: &'a [u8],
    reads: Vec<(u64, u64)>,
    returned: Vec<Vec<u8>>,
}

impl<'a> Spy<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Spy { bytes, reads: Vec::new(), returned: Vec::new() }
    }
    fn touched(&self, from: u64, to: u64) -> bool {
        self.reads.iter().any(|&(s, e)| s < to && from < e)
    }
}

impl ByteSource for Spy<'_> {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        self.reads.push((offset, offset + buf.len() as u64));
        SliceSource::new(self.bytes).read_at(offset, buf)?;
        self.returned.push(buf.to_vec());
        Ok(())
    }
}

#[test]
fn a_plain_wii_iso_gives_the_header_fields_and_never_the_partitions() {
    let disc = fx::disc_header("RZDE01", "Zelda Header", 0x60000);
    let mut spy = Spy::new(&disc);
    let info = read_info(&mut spy).unwrap();
    assert_eq!(info.platform, Platform::Wii);
    assert_eq!(info.ids.game_id.as_deref(), Some("RZDE01"));
    assert_eq!(info.ids.maker_code.as_deref(), Some("01"));
    assert_eq!(info.header_title.as_deref(), Some("Zelda Header"));
    assert_eq!(info.regions, [Region::NorthAmerica]);
    assert!(info.titles.is_empty() && info.icon.is_none() && info.images.is_empty());
    assert!(!spy.touched(0x1000, 0x60000), "an encrypted ISO is read for its header only");
}

#[test]
fn regions_follow_the_game_id() {
    let r = |id: &str| read(&fx::disc_header(id, "T", 0x440)).map(|i| i.regions);
    assert_eq!(r("RZDJ01"), Some(vec![Region::Japan]));
    assert_eq!(r("RZDP01"), Some(vec![Region::Europe]));
    assert_eq!(r("RZDK01"), Some(vec![Region::Korea]));
}

#[test]
fn a_wbfs_gives_the_header_fields_from_its_disc_info() {
    let info = read(&fx::wbfs("RZDP01", "Wbfs Title")).unwrap();
    assert_eq!(info.ids.game_id.as_deref(), Some("RZDP01"));
    assert_eq!(info.header_title.as_deref(), Some("Wbfs Title"));
    assert_eq!(info.regions, [Region::Europe]);
}

#[test]
fn a_wia_with_bzip2_lzma_or_lzma2_groups_reads_like_an_rvz() {
    for method in [2, 3, 4] {
        let wia = rvz_with(&partition(), |o| {
            o.magic = b"WIA\x01";
            o.compression = method;
        });
        let info = read(&wia.bytes).unwrap_or_else(|| panic!("method {method}"));
        assert_eq!(info.ids.game_id.as_deref(), Some("RZDE01"), "method {method}");
        assert_eq!(info.header_title.as_deref(), Some("Header Title"), "method {method}");
        assert_eq!(info.title(), Some("Test Quest"), "WIA method {method} titles");
        assert!(info.icon.is_some(), "WIA method {method} logo");
    }
}

#[test]
fn a_stored_wia_reads_too_and_an_rvz_with_lzma_groups_decodes() {
    let wia = rvz_with(&partition(), |o| {
        o.magic = b"WIA\x01";
        o.compression = 0;
    });
    assert_eq!(read(&wia.bytes).unwrap().title(), Some("Test Quest"));
    for method in [3, 4, 2] {
        let rvz = rvz_with(&partition(), |o| o.compression = method);
        assert_eq!(read(&rvz.bytes).unwrap().title(), Some("Test Quest"), "RVZ method {method}");
    }
}

#[test]
fn a_wia_with_unknown_compression_or_cut_groups_gives_the_header_only() {
    let wia = rvz_with(&partition(), |o| {
        o.magic = b"WIA\x01";
        o.compression = 3;
    });
    let mut unknown = wia.bytes.clone();
    unknown[0x4C + 3] = 9; // a method no reader here knows
    let info = read(&unknown).unwrap_or_else(|| panic!("header copy still reads"));
    assert_eq!(info.ids.game_id.as_deref(), Some("RZDE01"));
    assert!(info.titles.is_empty());
    let cut = &wia.bytes[..wia.bytes.len() - 40];
    assert!(read(cut).is_none_or(|i| i.titles.is_empty()));
}

#[test]
fn a_wii_ciso_gives_the_header_fields() {
    let disc = fx::disc_header("RZDE01", "Ciso Title", 0x20000);
    assert_eq!(read(&gcfx::ciso(&disc, 0x8000)).unwrap().ids.game_id.as_deref(), Some("RZDE01"));
}

#[test]
fn a_gamecube_disc_garbage_and_nothing_are_not_wii_files() {
    assert!(read(&fx::disc_header("RZDE01", "T", 0x440)).is_some(), "a Wii header reads");
    assert!(read(&gcfx::disc("GZLE01", "Header Title", &[], None)).is_none());
    assert!(read(&[0u8; 0x500]).is_none());
    assert!(read(&[]).is_none());
    assert!(read(&fx::disc_header("RZDE01", "T", 0x30)).is_none(), "a truncated header");
}

#[test]
fn file_names_that_could_be_wii_files() {
    for n in ["a.iso", "a.WBFS", "a.rvz", "a.wia", "a.ciso", "a.gcz", "a.wad"] {
        assert!(is_candidate(n), "{n}");
    }
    for n in ["a.nds", "a.zip", "a", "a.rpx"] {
        assert!(!is_candidate(n), "{n}");
    }
}

#[test]
fn an_rvz_gives_titles_and_the_logo_from_the_decrypted_partition() {
    for chunk in [0x8000, 0x20000, 0x20_0000, 0x40_0000] {
        for compression in [0, 5] {
            let label = format!("chunk={chunk:#x} compression={compression}");
            let rvz = rvz_with(&partition(), |o| {
                o.chunk = chunk;
                o.compression = compression;
            });
            let info = read(&rvz.bytes).unwrap_or_else(|| panic!("{label}"));
            assert_eq!(info.platform, Platform::Wii, "{label}");
            assert_eq!(info.title(), Some("Test Quest"), "{label}");
            assert_eq!(info.title_in(Language::Japanese).map(|t| t.name.as_str()), Some("テスト"), "{label}");
            assert_eq!(info.title_in(Language::French).map(|t| t.name.as_str()), Some("Quête Test"), "{label}");
            assert_eq!(info.ids.game_id.as_deref(), Some("RZDE01"), "{label}");
            assert_eq!(info.icon.as_ref().map(|i| (i.width, i.height)), Some((64, 16)), "{label}");
            assert_eq!(info.images, [ImageKind::Logo], "{label}");
        }
    }
}

#[test]
fn exception_lists_before_the_data_are_skipped_stored_or_zstd_one_or_several_per_group() {
    for chunk in [0x8000, 0x20_0000, 0x40_0000] {
        for compression in [0, 5] {
            let rvz = rvz_with(&partition(), |o| {
                o.exceptions = true;
                o.chunk = chunk;
                o.compression = compression;
            });
            let info = read(&rvz.bytes).unwrap_or_else(|| panic!("chunk={chunk} c={compression}"));
            assert_eq!(info.title(), Some("Test Quest"));
            assert!(info.icon.is_some());
        }
    }
}

#[test]
fn rvz_packing_in_partition_groups_is_decoded() {
    let info = read(
        &rvz_with(&partition(), |o| {
            o.packed = true;
            o.exceptions = true;
        })
        .bytes,
    )
    .unwrap();
    assert_eq!(info.title(), Some("Test Quest"));
    assert!(info.icon.is_some());
}

#[test]
fn a_partition_spread_over_many_groups_reads_across_group_edges() {
    let filler: Vec<u8> = (0..0x30000u32).map(|i| (i % 251) as u8).collect();
    let part = fx::partition(&[PartFile("zzfiller.bin", &filler), PartFile("opening.bnr", &opening_bnr())], None);
    for chunk in [0x8000, 0x20_0000] {
        let rvz = rvz_with(&part, |o| o.chunk = chunk);
        assert_eq!(
            read(&rvz.bytes).and_then(|i| i.title().map(str::to_owned)).as_deref(),
            Some("Test Quest"),
            "chunk={chunk}"
        );
    }
}

#[test]
fn a_disc_without_an_opening_bnr_gives_the_header_fields() {
    let part = fx::partition(&[PartFile("other.bin", &[1u8; 0x500])], None);
    let info = read(&rvz_with(&part, |_| {}).bytes).unwrap();
    assert_eq!(info.ids.game_id.as_deref(), Some("RZDE01"));
    assert_eq!(info.header_title.as_deref(), Some("Header Title"));
    assert!(info.titles.is_empty() && info.icon.is_none());
}

#[test]
fn only_the_root_opening_bnr_is_used() {
    let part = fx::partition(&[], Some(&opening_bnr()));
    assert!(read(&rvz_with(&part, |_| {}).bytes).unwrap().titles.is_empty());
}

#[test]
fn the_partition_key_slot_and_encrypted_data_are_never_read() {
    let layout = rvz_with(&partition(), |_| {});
    let mut spy = Spy::new(&layout.bytes);
    assert!(read_info(&mut spy).is_some());
    let key = layout.part_table_at as u64;
    assert!(
        !spy.touched(key, key + fx::KEY_SIZE as u64),
        "the 16 byte key slot of the partition entry must not be read"
    );
    assert!(
        !spy.touched(fx::PART_OFFSET as u64, fx::PART_OFFSET as u64 + 0x2B8),
        "the partition's ticket must not be read"
    );
    let marker = [fx::KEY_MARKER; fx::KEY_SIZE];
    assert!(
        !spy.returned.iter().any(|b| b.windows(marker.len()).any(|w| w == marker)),
        "no read ever returns the marker bytes"
    );
}

#[test]
fn a_damaged_rvz_gives_what_the_header_allows_or_nothing_and_never_panics() {
    let bytes = rvz_with(&partition(), |_| {}).bytes;
    assert!(read(&bytes).is_some(), "the intact image reads");
    for cut in [0x30, 0x60, 0x100, bytes.len() / 2, bytes.len() - 5] {
        let _ = read(&bytes[..cut]);
    }
    let mut corrupted = bytes.clone();
    for b in &mut corrupted[0x300..0x400] {
        *b = 0x55;
    }
    let _ = read(&corrupted);
}

#[test]
fn an_rvz_of_a_gamecube_disc_is_not_a_wii_file() {
    assert!(read(&rvz_with(&partition(), |_| {}).bytes).is_some(), "a Wii RVZ reads");
    let disc = gcfx::disc("GZLE01", "Header Title", &[], None);
    assert!(read(&gcfx::rvz(&disc, gcfx::RvzOptions::default(), b"RVZ\x01")).is_none());
}

#[test]
fn a_wad_gives_its_title_id_and_game_id_and_no_titles_or_art() {
    let info = read_wad_info(&mut SliceSource::new(&fx::wad(0x0001_0001_5245_4145, 0x0101, "Is"))).unwrap();
    assert_eq!(info.platform, Platform::Wii);
    assert_eq!(info.ids.title_id.as_deref(), Some("0001000152454145"));
    assert_eq!(info.ids.game_id.as_deref(), Some("REAE"));
    assert!(info.titles.is_empty() && info.icon.is_none());
}
