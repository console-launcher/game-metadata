// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Parity with the original Kotlin reader's tests: `NdsBannerReaderTest`, `NdsExtractorTest` (the parts that are the
//! reader, not PNG caching) and the DS cases of `DsGameMetadataTest`. Same synthetic inputs, same expected outputs.

use gamemeta_core::nds::{self, Read};
use gamemeta_core::{ByteSource, Language, Platform, ReadError, Region, RgbaImage, SliceSource, read};
use gamemeta_fixtures::nds as fx;

fn found(r: Read) -> nds::Banner {
    match r {
        Read::Found { banner, .. } => banner,
        Read::NoBanner(why) => panic!("expected a banner, got NoBanner({why})"),
    }
}
fn no_banner(r: Read, what: &str) {
    assert!(matches!(r, Read::NoBanner(_)), "{what}: expected no banner");
}
fn rom_banner(rom: &[u8]) -> Read {
    nds::read_banner(&mut SliceSource::new(rom))
}
fn px(img: &RgbaImage, x: usize, y: usize) -> [u8; 4] {
    let at = (y * img.width as usize + x) * 4;
    img.pixels[at..at + 4].try_into().unwrap()
}
fn expand(v: u32) -> u8 {
    ((v << 3) | (v >> 2)) as u8
}

// ---- NdsBannerReaderTest ----

#[test]
fn icon_pixels_and_palette_match_the_known_values() {
    let banner = found(rom_banner(&fx::rom(&fx::default_banner(), 0x8000, 0x400)));
    assert_eq!(banner.icon.width, 32);
    // (x + y) % 16 = 0 is transparent: pixel (0, 0).
    assert_eq!(px(&banner.icon, 0, 0)[3], 0);
    // Pixel (9, 3) crosses tiles and odd/even nibbles: index 12 = red 12, blue 19.
    assert_eq!(px(&banner.icon, 9, 3), [expand(12), 0, expand(19), 255]);
    // Pixel (31, 31): index 62 % 16 = 14.
    assert_eq!(px(&banner.icon, 31, 31), [expand(14), 0, expand(17), 255]);
}

#[test]
fn english_title_is_used_and_lines_are_kept() {
    let banner = found(nds::parse_banner(&fx::banner(3, "Test Game\nSubtitle\nMaker", &[])));
    assert_eq!(banner.title(), "Test Game Subtitle Maker");
    assert_eq!(banner.title_lines(), ["Test Game", "Subtitle", "Maker"]);
    assert_eq!(banner.titles.len(), 8);
    assert_eq!(banner.titles[7], "Korean");
}

#[test]
fn title_falls_back_to_the_first_non_empty_one() {
    let banner = found(nds::parse_banner(&fx::banner(1, "", &[])));
    assert_eq!(banner.title(), "");
    let mut b = fx::banner(1, "", &[]);
    b[0x240..0x340].copy_from_slice(&fx::title("Nihongo"));
    let crc = fx::crc16(&b, 0x20, 0x840);
    b[2..4].copy_from_slice(&crc.to_le_bytes());
    assert_eq!(found(nds::parse_banner(&b)).title(), "Nihongo");
}

#[test]
fn dsi_animation_follows_the_sequence_with_flips_and_durations() {
    // bitmap 2, 10 frames; bitmap 5 flipped both ways, 4 frames; then the end.
    let sequence = [(2 << 8) | 10, 0x8000 | 0x4000 | (5 << 8) | 4, 0];
    let banner = found(nds::parse_banner(&fx::banner(0x0103, "T", &sequence)));
    assert!(banner.animated());
    let durations: Vec<u32> = banner.animation.iter().map(|s| s.duration_frames).collect();
    assert_eq!(durations, [10, 4]);
    // Bitmap n is all index n + 1: bitmap 2 -> index 3, bitmap 5 -> index 6.
    assert_eq!(px(&banner.animation[0].frame, 10, 10)[0], expand(3));
    assert_eq!(px(&banner.animation[1].frame, 10, 10)[0], expand(6));
}

#[test]
fn a_still_icon_in_a_dsi_banner_is_not_animated() {
    let banner = found(nds::parse_banner(&fx::banner(0x0103, "T", &[0x0001, 0x0100])));
    assert!(!banner.animated());
    assert_eq!(banner.animation.len(), 1);
}

#[test]
fn flip_horizontally_mirrors_the_columns() {
    let flipped = found(nds::parse_banner(&fx::banner(0x0103, "T", &[0x4000 | 1, 0]))).animation.remove(0).frame;
    assert_eq!(px(&flipped, 0, 0), px(&flipped, 31, 0)); // bitmap 0 is a flat index 1 fill
}

#[test]
fn truncated_files_give_no_banner_and_never_throw() {
    let rom = fx::rom(&fx::default_banner(), 0x8000, 0x400);
    for cut in [0usize, 10, 0x1FF, 0x200, 0x8000, 0x8100, 0x8200, 0x8800] {
        no_banner(rom_banner(&rom[..cut]), &format!("cut at {cut}"));
    }
}

#[test]
fn no_banner_offset_unknown_version_bad_checksum_and_a_garbage_offset_give_no_banner() {
    let banner = fx::default_banner();
    let mut r = fx::rom(&banner, 0x8000, 0x400);
    r[0x68..0x6C].copy_from_slice(&0u32.to_le_bytes());
    no_banner(rom_banner(&r), "offset 0");
    let mut r = fx::rom(&banner, 0x8000, 0x400);
    r[0x68..0x6C].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
    no_banner(rom_banner(&r), "garbage offset");
    let mut b = banner.clone();
    b[0..2].copy_from_slice(&0x0042u16.to_le_bytes());
    no_banner(nds::parse_banner(&b), "unknown version");
    let mut b = banner.clone();
    b[0x30] = b[0x30].wrapping_add(1);
    no_banner(nds::parse_banner(&b), "bad checksum");
    // A "scrambled" banner (an encrypted file) fails the checksum.
    let mut b: Vec<u8> = (0..0xA40usize).map(|i| (i * 31 + 7) as u8).collect();
    b[0..2].copy_from_slice(&1u16.to_le_bytes());
    no_banner(nds::parse_banner(&b), "scrambled");
}

#[test]
fn a_zip_streams_its_ds_entry() {
    let rom = fx::rom(&fx::default_banner(), 0x20000, 0x100000);
    for deflate in [true, false] {
        let zip = fx::zip("Game (USA).nds", &rom, deflate);
        let banner = found(nds::read_zip_banner(&mut SliceSource::new(&zip)));
        assert_eq!(banner.title(), "Test Game Subtitle Maker", "deflate={deflate}");
    }
}

#[test]
fn a_zip_with_no_ds_file_or_garbage_gives_no_banner() {
    no_banner(nds::read_zip_banner(&mut SliceSource::new(&fx::zip("game.sfc", &[0; 100], true))), "no DS file");
    no_banner(nds::read_zip_banner(&mut SliceSource::new(&[7u8; 300])), "garbage");
    let zip = fx::zip("game.nds", &fx::rom(&fx::default_banner(), 0x8000, 0x400), true);
    no_banner(nds::read_zip_banner(&mut SliceSource::new(&zip[..zip.len() / 3])), "truncated");
}

#[test]
fn seven_z_is_not_a_candidate_and_zip_is() {
    for ok in ["A.NDS", "a.zip", "a.dsi"] {
        assert!(nds::is_candidate(ok), "{ok}");
    }
    assert!(!nds::is_candidate("a.7z") && nds::is_unsupported_archive("a.7z"));
}

// ---- NdsExtractorTest (the reader's part: the raw block reads back; PNG caching is the app's) ----

#[test]
fn a_rom_and_a_zip_give_the_banner_and_the_raw_block_reads_back() {
    let rom = fx::rom(&fx::default_banner(), 0x8000, 0x400);
    let zip = fx::zip("g.nds", &rom, true);
    for r in [rom_banner(&rom), nds::read_zip_banner(&mut SliceSource::new(&zip))] {
        let Read::Found { banner, raw } = r else { panic!("no banner") };
        assert_eq!(banner.title(), "Test Game Subtitle Maker");
        assert_eq!(found(nds::parse_banner(&raw)).title(), "Test Game Subtitle Maker");
    }
}

// ---- DsGameMetadataTest, the DS cases ----

fn ds(game_code: &str, maker: &str, unit_code: u8, region_byte: u8, banner: &[u8]) -> Vec<u8> {
    let mut rom = fx::rom(banner, 0x8000, 0x400);
    rom[..8].copy_from_slice(b"TESTGAME");
    rom[0x0C..0x10].copy_from_slice(game_code.as_bytes());
    rom[0x10..0x12].copy_from_slice(maker.as_bytes());
    rom[0x12] = unit_code;
    rom[0x1D] = region_byte;
    rom
}
fn default_ds() -> Vec<u8> {
    ds("ABCE", "01", 0, 0, &fx::default_banner())
}

#[test]
fn a_ds_rom_gives_its_platform_titles_with_the_publisher_split_off_the_icon_and_no_animation() {
    let info = read(&mut SliceSource::new(&default_ds()), "game.nds").unwrap();
    assert_eq!(info.platform, Platform::Ds);
    let en = info.title_in(Language::English).unwrap();
    assert_eq!(en.short_name, "Test Game");
    assert_eq!(en.name, "Test Game Subtitle");
    assert_eq!(en.publisher.as_deref(), Some("Maker"));
    assert_eq!(info.title(), Some("Test Game Subtitle"));
    assert_eq!(info.icon.as_ref().map(|i| i.width), Some(32));
    assert!(info.animated_icon.is_empty());
    assert!(info.banner.is_some_and(|b| b.offset == 0x8000 && b.size == 0x840));
}

#[test]
fn a_ds_rom_gives_its_game_code_and_maker_code() {
    let info = read(&mut SliceSource::new(&ds("AMCE", "01", 0, 0, &fx::default_banner())), "game.nds").unwrap();
    assert_eq!(info.ids.game_id.as_deref(), Some("AMCE"));
    assert_eq!(info.ids.maker_code.as_deref(), Some("01"));
    assert_eq!(info.ids.title_id, None);
}

#[test]
fn a_ds_headers_region_byte_gives_china_or_korea_and_a_normal_one_gives_no_region() {
    let regions =
        |b| read(&mut SliceSource::new(&ds("ABCE", "01", 0, b, &fx::default_banner())), "a.nds").unwrap().regions;
    assert_eq!(regions(0x80), [Region::China]);
    assert_eq!(regions(0x40), [Region::Korea]);
    assert_eq!(regions(0), []);
}

#[test]
fn a_dsi_only_unit_code_is_a_dsi_game_and_its_animated_icon_comes_through() {
    let animated = fx::banner(0x0103, "T", &[0x0010, 0x0110]);
    let info = read(&mut SliceSource::new(&ds("ABCE", "01", 3, 0, &animated)), "game.dsi").unwrap();
    assert_eq!(info.platform, Platform::Dsi);
    assert_eq!(info.animated_icon.len(), 2);
    let plain = read(&mut SliceSource::new(&ds("ABCE", "01", 2, 0, &fx::default_banner())), "game.nds").unwrap();
    assert_eq!(plain.platform, Platform::Ds);
}

#[test]
fn the_ds_rom_in_a_zip_gives_the_same_titles_and_ids() {
    let zip = fx::zip("game.nds", &ds("AZLE", "01", 0, 0, &fx::default_banner()), true);
    let info = read(&mut SliceSource::new(&zip), "game.zip").unwrap();
    assert_eq!(info.ids.game_id.as_deref(), Some("AZLE"));
    assert_eq!(info.title(), Some("Test Game Subtitle"));
}

struct Failing;
impl ByteSource for Failing {
    fn size(&self) -> u64 {
        1 << 20
    }
    fn read_at(&mut self, _: u64, _: &mut [u8]) -> Result<(), ReadError> {
        Err(ReadError::Io)
    }
}

#[test]
fn an_unknown_empty_damaged_or_unreadable_ds_file_is_none_and_never_throws() {
    assert!(read(&mut SliceSource::new(&[]), "game.nds").is_none());
    assert!(read(&mut SliceSource::new(&default_ds()), "notes.txt").is_none());
    assert!(read(&mut Failing, "game.nds").is_none());
    assert!(read(&mut Failing, "game.zip").is_none());
}

// ---- reading a big zip must not cost a read per few KiB ----

/// A source that counts what is asked of it: a browser pays for every call, not only for every byte.
struct Counting<'a> {
    inner: SliceSource<'a>,
    calls: usize,
    bytes: u64,
}
impl<'a> Counting<'a> {
    fn new(data: &'a [u8]) -> Self {
        Counting { inner: SliceSource::new(data), calls: 0, bytes: 0 }
    }
}
impl ByteSource for Counting<'_> {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        self.calls += 1;
        self.bytes += buf.len() as u64;
        self.inner.read_at(offset, buf)
    }
}

#[test]
fn a_stored_zip_entry_seeks_to_its_banner_instead_of_reading_up_to_it() {
    let rom = ds("AZLE", "01", 0, 0, &fx::default_banner());
    let mut big = fx::rom(&fx::default_banner(), 4 << 20, 0x400);
    big[..rom.len().min(0x1000)].copy_from_slice(&rom[..rom.len().min(0x1000)]);
    big[0x68..0x6C].copy_from_slice(&(4u32 << 20).to_le_bytes());
    let zip = fx::zip("game.nds", &big, false);
    let mut source = Counting::new(&zip);
    let info = nds::read_zip_info(&mut source).expect("info");
    assert_eq!(info.title(), Some("Test Game Subtitle"));
    assert!(source.bytes < 256 * 1024, "read {} bytes of a stored entry to reach a banner 4 MiB in", source.bytes);
    assert!(source.calls < 40, "{} reads", source.calls);
}

#[test]
fn a_deflated_zip_entry_is_read_in_large_pieces() {
    let mut big = fx::rom(&fx::default_banner(), 4 << 20, 0x400);
    let mut x = 0x1234_5678u32;
    for b in &mut big[0x1000..3 << 20] {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *b = ((x & 3) * 17) as u8;
    }
    let zip = fx::zip("game.nds", &big, true);
    let mut source = Counting::new(&zip);
    let info = nds::read_zip_info(&mut source).expect("info");
    assert_eq!(info.title(), Some("Test Game Subtitle"));
    // A 16 KiB read per call would take zip.len() / 16 KiB reads.
    assert!(source.calls < zip.len() / (128 * 1024) + 16, "{} reads of a {} byte zip", source.calls, zip.len());
}

// ---- a zip of several ROMs ----

fn rom_with_code(code: &str) -> Vec<u8> {
    let mut rom = ds("AZLE", "01", 0, 0, &fx::default_banner());
    rom[0x0C..0x10].copy_from_slice(code.as_bytes());
    rom
}

#[test]
fn a_zip_lists_every_ds_rom_with_its_crc_and_size_and_each_can_be_read() {
    let (a, b) = (rom_with_code("AAAA"), rom_with_code("BBBB"));
    let zip = gamemeta_fixtures::zip::zip(&[("a.nds", &a, true), ("notes.txt", b"hi", true), ("b.nds", &b, false)]);
    let roms = nds::zip_roms(&mut SliceSource::new(&zip));
    assert_eq!(roms.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["a.nds", "b.nds"]);
    assert_eq!(roms[1].size, b.len() as u64);
    assert_eq!(roms[1].crc, gamemeta_fixtures::zip::crc32(&b));
    for (name, code) in [("a.nds", "AAAA"), ("b.nds", "BBBB")] {
        let info = nds::read_zip_info_of(&mut SliceSource::new(&zip), name).expect(name);
        assert_eq!(info.ids.game_id.as_deref(), Some(code));
    }
    assert!(nds::read_zip_info_of(&mut SliceSource::new(&zip), "c.nds").is_none());
    assert!(nds::zip_roms(&mut SliceSource::new(&[7u8; 300])).is_empty());
}
