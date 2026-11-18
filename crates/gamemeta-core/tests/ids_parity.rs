// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Parity with the original id readers' suites, one Rust test per reference test, on the same synthetic images:
//! `HeaderIdReaderTest` (19), `RaHasherTest` (6), `PspParamSfoTest` (10) and the frontend `DiscIdTest` (12). Where the
//! reference asserts on the platform a caller passed (a `.rpx` on "switch", a `.wad` on "gc"), the Rust entry has no
//! platform argument: those assertions are dropped and noted at the test. Reference `titleId` is `Ids::primary_id`.

use std::collections::HashMap;

use gamemeta_core::ids::{self, DiscIdRefusal, DiscIdResult, DiscRegion, DiscSystem, Ids, RaHasher, psp};
use gamemeta_core::{ByteSource, ReadError, SliceSource};
use gamemeta_fixtures::ids as fx;

fn read(name: &str, bytes: &[u8]) -> Option<Ids> {
    ids::read(&mut SliceSource::new(bytes), name)
}

/// The original implementation's `titleId`: what `HeaderIdReader` calls the id.
fn id(name: &str, bytes: &[u8]) -> Option<String> {
    read(name, bytes)?.primary_id().map(String::from)
}

struct Owned(Vec<u8>);

impl ByteSource for Owned {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_at(&mut self, o: u64, b: &mut [u8]) -> Result<(), ReadError> {
        SliceSource::new(&self.0).read_at(o, b)
    }
}

/// An opener serving `files` by the path the reader asks for.
fn opener(files: Vec<(&str, Vec<u8>)>) -> impl FnMut(&str) -> Option<Box<dyn ByteSource>> {
    let map: HashMap<String, Vec<u8>> = files.into_iter().map(|(n, b)| (n.to_owned(), b)).collect();
    move |name: &str| map.get(name).map(|b| Box::new(Owned(b.clone())) as Box<dyn ByteSource>)
}

// ---- HeaderIdReaderTest: Wii U

#[test]
fn a_wii_u_rpx_gives_the_title_id_from_the_meta_folder_beside_its_code_folder() {
    let mut open = opener(vec![("../meta/meta.xml", fx::wiiu_meta("000500001010abcd"))]);
    let got = ids::read_with(&mut SliceSource::new(&[0u8; 16]), "game.rpx", &mut open);
    assert_eq!(got.and_then(|i| i.title_id).as_deref(), Some("000500001010ABCD"));
}

// The original implementation's last assertion (a `.rpx` on platform "switch" gives nothing) is the caller's platform gate: dropped.
#[test]
fn a_wii_u_file_with_no_meta_folder_an_encrypted_image_or_a_junk_archive_has_no_id() {
    let mut open = opener(vec![("../meta/meta.xml", fx::wiiu_meta("000500001010abcd"))]);
    assert!(ids::read_with(&mut SliceSource::new(&[0u8; 16]), "game.rpx", &mut open).is_some());
    assert!(read("game.rpx", &[0u8; 16]).is_none());
    assert!(ids::read_with(&mut SliceSource::new(&vec![0u8; 0x8000]), "game.wud", &mut open).is_none());
    assert!(read("g.wua", &[1u8; 500]).is_none());
}

// ---- Wii WAD (the "gc" and "switch" platform assertions are the caller's gate: dropped)

#[test]
fn a_wii_wad_gives_its_sixteen_digit_title_id() {
    let wad = fx::wad(0x0001_0001_5245_4145);
    assert_eq!(read("g.wad", &wad).and_then(|i| i.title_id).as_deref(), Some("0001000152454145"));
    assert_eq!(id("g.wad", &wad).as_deref(), Some("0001000152454145"));
    assert!(read("g.wad", &[0u8; 0x1000]).is_none());
}

// ---- GameCube / Wii

#[test]
fn a_gamecube_or_wii_image_gives_its_six_character_game_id() {
    assert_eq!(id("g.iso", &fx::game_cube("GZZE01")).as_deref(), Some("GZZE01"));
    assert_eq!(id("g.gcm", &fx::game_cube("GZZP99")).as_deref(), Some("GZZP99"));
    assert_eq!(id("w.iso", &fx::wii("RZZE01")).as_deref(), Some("RZZE01"));
    assert_eq!(read("g.iso", &fx::game_cube("GZZE01")).and_then(|i| i.game_id).as_deref(), Some("GZZE01"));
}

#[test]
fn rvz_wia_wbfs_and_ciso_carry_the_id_in_their_own_header() {
    assert_eq!(id("g.rvz", &fx::rvz(&fx::game_cube("GZZE01"), b"RVZ\x01")).as_deref(), Some("GZZE01"));
    assert_eq!(id("w.wia", &fx::rvz(&fx::wii("RZZE01"), b"WIA\x01")).as_deref(), Some("RZZE01"));
    assert_eq!(id("w.wbfs", &fx::wbfs(&fx::wii("RZZP01"))).as_deref(), Some("RZZP01"));
    assert_eq!(id("g.ciso", &fx::ciso(&fx::game_cube("GZZJ01"))).as_deref(), Some("GZZJ01"));
}

// "nes" platform + gc iso is the caller's gate: dropped.
#[test]
fn a_disc_image_without_the_disc_magic_or_with_a_bad_id_has_no_id() {
    assert!(id("g.iso", &fx::game_cube("GZZE01")).is_some()); // control
    assert!(read("g.iso", &[0u8; 0x440]).is_none());
    assert!(read("g.iso", &fx::game_cube("GZ\u{0}E01")).is_none());
    assert_eq!(id("g.iso", &fx::game_cube("gzze01")), None);
    assert!(read("g.iso", &fx::game_cube("GZZE01")[..0x10]).is_none());
}

// ---- PlayStation

#[test]
fn a_ps1_image_gives_the_serial_from_system_cnf() {
    let iso = fx::iso9660(&[("SYSTEM.CNF;1", &fx::system_cnf("BOOT = cdrom:\\SLUS_000.01;1"))]);
    assert_eq!(id("g.iso", &iso).as_deref(), Some("SLUS00001"));
    assert_eq!(read("g.iso", &iso).and_then(|i| i.serial).as_deref(), Some("SLUS00001"));
}

#[test]
fn a_ps2_image_gives_the_serial_from_boot2_with_or_without_the_backslash_and_spaces() {
    let a = fx::iso9660(&[("SYSTEM.CNF;1", &fx::system_cnf("BOOT2 = cdrom0:\\SLES_000.02;1"))]);
    let b = fx::iso9660(&[("SYSTEM.CNF;1", &fx::system_cnf("BOOT2=cdrom0:SCUS_000.03"))]);
    assert_eq!(id("g.iso", &a).as_deref(), Some("SLES00002"));
    assert_eq!(id("g.iso", &b).as_deref(), Some("SCUS00003"));
}

#[test]
fn a_cue_sheet_reads_the_serial_from_its_first_data_track_in_a_raw_2352_byte_bin() {
    let iso = fx::iso9660(&[("SYSTEM.CNF;1", &fx::system_cnf("BOOT = cdrom:\\SCES_000.04;1"))]);
    let cue = b"FILE \"game (Track 1).bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\nFILE \"game (Track 2).bin\" BINARY\r\n  TRACK 02 AUDIO\r\n";
    let mut open = opener(vec![("game (Track 1).bin", fx::raw2352(&iso, 2))]);
    let got = ids::read_with(&mut SliceSource::new(cue), "game.cue", &mut open);
    assert_eq!(got.and_then(|i| i.serial).as_deref(), Some("SCES00004"));
    let cue1 = b"FILE \"a.bin\" BINARY\n TRACK 01 MODE1/2352\n";
    let mut any = |_: &str| -> Option<Box<dyn ByteSource>> { Some(Box::new(Owned(fx::raw2352(&iso, 1)))) };
    let got = ids::read_with(&mut SliceSource::new(cue1), "a.cue", &mut any);
    assert_eq!(got.and_then(|i| i.serial).as_deref(), Some("SCES00004"));
}

#[test]
fn a_disc_with_no_system_cnf_a_damaged_one_or_a_cue_whose_track_is_missing_has_no_serial() {
    assert!(id("g.iso", &fx::iso9660(&[("SYSTEM.CNF;1", &fx::system_cnf("BOOT = cdrom:\\SLUS_000.01;1"))])).is_some()); // control
    assert!(read("g.iso", &fx::iso9660(&[("README.TXT;1", &[0u8; 10])])).is_none());
    assert!(read("g.iso", &[0u8; 40_000]).is_none());
    assert!(read("g.iso", &fx::iso9660(&[("SYSTEM.CNF;1", &fx::system_cnf("TCB = 4"))])).is_none());
    assert!(read("a.cue", b"FILE \"a.bin\" BINARY\n TRACK 01 MODE2/2352\n").is_none());
}

// ---- PSP

#[test]
fn a_psp_iso_gives_disc_id_from_param_sfo_else_the_id_in_umd_data_bin() {
    let sfo = fx::sfo(&[("TITLE", "Made Up"), ("DISC_ID", "ULUS00001")]);
    assert_eq!(id("g.iso", &fx::iso9660(&[("PSP_GAME/PARAM.SFO;1", &sfo)])).as_deref(), Some("ULUS00001"));
    let umd = fx::iso9660(&[("UMD_DATA.BIN;1", b"ULES-00002|0000012345|0000|G")]);
    assert_eq!(id("g.iso", &umd).as_deref(), Some("ULES00002"));
    // The original implementation gives no HeaderIds here; the Rust result still carries the SFO title, but no id.
    let title_only = fx::iso9660(&[("PSP_GAME/PARAM.SFO;1", &fx::sfo(&[("TITLE", "x")]))]);
    assert_eq!(id("g.iso", &title_only), None);
}

// ---- CHD, zip

#[test]
fn a_chd_v5_gives_the_raw_sha1_from_its_header_other_versions_and_bad_files_give_none() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    assert_eq!(read("g.chd", &fx::chd(sha, 5)).and_then(|i| i.chd_sha1).as_deref(), Some(sha));
    assert_eq!(read("g.chd", &fx::chd(&sha.to_uppercase(), 5)).and_then(|i| i.chd_sha1).as_deref(), Some(sha));
    assert!(read("g.chd", &fx::chd(sha, 4)).is_none());
    assert!(read("g.chd", &[0u8; 124]).is_none());
    assert!(read("g.chd", &fx::chd(sha, 5)[..60]).is_none());
    assert!(read("g.chd", &fx::chd(&"00".repeat(20), 5)).is_none());
}

#[test]
fn a_zip_with_one_rom_gives_its_crc_and_size_without_unzipping_it() {
    let rom = fx::sample_rom(4096);
    let got = read("g.zip", &fx::zip(&[("game.sfc", &rom)])).unwrap();
    assert_eq!(got.zip_crc.as_deref(), Some(fx::crc_hex(&rom).as_str()));
    assert_eq!(got.zip_size, Some(4096));
    let with_folder = read("g.zip", &fx::zip(&[("readme/", &[]), ("game.sfc", &rom)])).unwrap();
    assert_eq!(with_folder.zip_crc.as_deref(), Some(fx::crc_hex(&rom).as_str()));
}

#[test]
fn a_zip_with_several_files_none_or_a_damaged_directory_gives_none() {
    assert!(read("g.zip", &fx::zip(&[("a.sfc", &[0u8; 10])])).is_some()); // control
    assert!(read("g.zip", &fx::zip(&[("a.sfc", &[0u8; 10]), ("b.sfc", &[0u8; 11])])).is_none());
    assert!(read("g.zip", &fx::zip(&[])).is_none());
    assert!(read("g.zip", &fx::zip(&[("a.sfc", &[0u8; 10])])[..20]).is_none());
    assert!(read("g.zip", &[0u8; 100]).is_none());
}

// ---- Switch

#[test]
fn an_nsp_gives_the_title_id_from_its_ticket_or_its_cnmt_name() {
    let ticket = fx::pfs0(&["01000000000010000000000000000000.tik", "x.nca"]);
    assert_eq!(read("g.nsp", &ticket).and_then(|i| i.title_id).as_deref(), Some("0100000000001000"));
    let cnmt = fx::pfs0(&["0100000000002000.cnmt.nca", "y.nca"]);
    assert_eq!(read("g.nsp", &cnmt).and_then(|i| i.title_id).as_deref(), Some("0100000000002000"));
}

#[test]
fn an_nsp_with_only_hash_named_files_an_xci_and_a_bad_header_have_no_id() {
    assert!(read("g.nsp", &fx::pfs0(&["0100000000001000.cnmt.nca"])).is_some()); // control
    assert!(read("g.nsp", &fx::pfs0(&["0123456789abcdef0123456789abcdef.cnmt.nca", "z.nca"])).is_none());
    assert!(read("g.xci", &[0u8; 0x300]).is_none());
    assert!(read("g.nsp", &[0u8; 8]).is_none());
    assert!(read("g.nsp", &fx::pfs0(&["0100000000001000.cnmt.nca"])[..30]).is_none());
}

// ---- DS, 3DS and the dispatch

#[test]
fn a_3ds_file_gives_its_title_id() {
    let got = read("g.3ds", &fx::ncch(0x0004_0000_0003_0800));
    assert_eq!(got.and_then(|i| i.title_id).as_deref(), Some("0004000000030800"));
    assert!(read("g.cia", &[0u8; 0x400]).is_none());
}

#[test]
fn a_ds_rom_gives_no_title_id_since_homebrew_shares_placeholder_codes_and_hacks_keep_the_originals() {
    assert!(id("g.iso", &fx::game_cube("GZZE01")).is_some()); // control: the reader itself works
    assert!(read("g.nds", &fx::ds_header("AZZE")).is_none());
    assert!(read("g.nds", &fx::ds_header("####")).is_none());
}

#[test]
fn an_unknown_extension_reads_nothing_and_no_read_ever_panics() {
    assert!(id("g.iso", &fx::game_cube("GZZE01")).is_some()); // control
    assert!(read("g.gb", &[0u8; 200]).is_none());
    assert!(read("g.txt", &fx::game_cube("GZZE01")).is_none());
    assert!(read("g.3ds", &[0u8; 5]).is_none());
    assert!(read("", &[]).is_none());
}

// ---- RaHasherTest

fn md5_hex(bytes: &[u8]) -> String {
    format!("{:x}", md5::compute(bytes))
}

fn hash(name: &str, bytes: &[u8], chunk: usize) -> Option<String> {
    let mut hasher = RaHasher::for_file(name, Some(bytes.len() as u64))?;
    for part in bytes.chunks(chunk) {
        hasher.update(part);
    }
    hasher.finish()
}

fn body() -> Vec<u8> {
    (0..40_000usize).map(|i| (i * 7 + 3) as u8).collect()
}

#[test]
fn platforms_ra_hashes_whole_are_the_md5_of_the_whole_file() {
    let body = body();
    for ext in ["gb", "gbc", "gba", "md", "gen", "sms", "gg", "vb", "ws", "wsc", "ngp", "ngc", "a26"] {
        assert_eq!(hash(&format!("g.{ext}"), &body, 4096), Some(md5_hex(&body)), "{ext}");
    }
}

#[test]
fn nes_skips_the_16_byte_header_and_a_trainer_and_hashes_the_prg_and_chr_data() {
    let prg: Vec<u8> = (0..16384 * 2).map(|i| (i % 251) as u8).collect();
    let chr: Vec<u8> = (0..8192).map(|i| (i % 241) as u8).collect();
    let data = [prg.clone(), chr.clone()].concat();
    let plain = [fx::ines_header(2, 1, 0), data.clone(), vec![0u8; 100]].concat();
    assert_eq!(hash("g.nes", &plain, 4096), Some(md5_hex(&data)));
    let trained = [fx::ines_header(2, 1, 4), vec![9u8; 512], data.clone()].concat();
    assert_eq!(hash("g.nes", &trained, 100), Some(md5_hex(&data)));
}

#[test]
fn nes_without_the_header_magic_or_with_a_cut_short_file_is_not_hashed() {
    assert!(hash("g.nes", &fx::sample_nes(), 4096).is_some()); // control
    assert_eq!(hash("g.nes", &[0u8; 40_000], 4096), None);
    let cut = [fx::ines_header(2, 1, 0), vec![0u8; 1000]].concat();
    assert_eq!(hash("g.nes", &cut, 4096), None);
}

#[test]
fn snes_skips_a_512_byte_copier_header_only_when_the_size_says_there_is_one() {
    let rom: Vec<u8> = (0..1024 * 64).map(|i| (i % 199) as u8).collect();
    assert_eq!(hash("g.sfc", &rom, 4096), Some(md5_hex(&rom)));
    let headered = [vec![1u8; 512], rom.clone()].concat();
    assert_eq!(hash("g.smc", &headered, 1000), Some(md5_hex(&rom)));
}

#[test]
fn any_other_extension_is_not_hashed() {
    assert!(RaHasher::for_file("g.gb", Some(100)).is_some()); // control
    for name in ["g.bin", "g.iso", "g.cue", "g.zip", "g.chd", "g.nds", "g", "g.PCE"] {
        assert!(RaHasher::for_file(name, Some(100)).is_none(), "{name}");
    }
}

#[test]
fn the_file_name_case_does_not_matter_and_an_unknown_size_is_not_hashed_for_the_header_platforms() {
    let body = body();
    assert_eq!(hash("G.GBA", &body, 4096), Some(md5_hex(&body)));
    assert!(RaHasher::for_file("g.sfc", None).is_none());
    assert!(RaHasher::for_file("g.nes", None).is_none());
    assert!(RaHasher::for_file("g.gb", None).is_some());
}

// ---- PspParamSfoTest

fn sfo_of(entries: &[(&str, &str)]) -> Vec<u8> {
    fx::sfo(entries)
}

#[test]
fn reads_the_title_and_disc_id_from_a_param_sfo() {
    let parsed = psp::parse(&sfo_of(&[("DISC_ID", "ULUS10041"), ("TITLE", "Daxter")])).unwrap();
    assert_eq!(parsed.title.as_deref(), Some("Daxter"));
    assert_eq!(parsed.disc_id.as_deref(), Some("ULUS10041"));
}

#[test]
fn an_eboot_pbp_gives_its_sfo_title() {
    let pbp = fx::pbp(&sfo_of(&[("TITLE", "Locoroco 2")]), 16);
    let parsed = psp::from_pbp(&mut SliceSource::new(&pbp)).unwrap();
    assert_eq!(parsed.title.as_deref(), Some("Locoroco 2"));
}

#[test]
fn a_psp_iso_gives_the_title_of_psp_game_param_sfo() {
    let iso = fx::psp_iso(&sfo_of(&[("DISC_ID", "UCUS98632"), ("TITLE", "Killzone: Liberation")]), "PARAM.SFO;1");
    let parsed = psp::from_iso(&mut SliceSource::new(&iso)).unwrap();
    assert_eq!(parsed.title.as_deref(), Some("Killzone: Liberation"));
    assert_eq!(parsed.disc_id.as_deref(), Some("UCUS98632"));
}

#[test]
fn an_iso_file_entry_without_a_version_suffix_is_found() {
    let iso = fx::psp_iso(&sfo_of(&[("TITLE", "Gripshift")]), "PARAM.SFO");
    assert_eq!(psp::from_iso(&mut SliceSource::new(&iso)).unwrap().title.as_deref(), Some("Gripshift"));
}

#[test]
fn a_newline_in_the_title_becomes_a_space() {
    let parsed = psp::parse(&sfo_of(&[("TITLE", "Final Fantasy\nCrisis Core")])).unwrap();
    assert_eq!(parsed.title.as_deref(), Some("Final Fantasy Crisis Core"));
}

#[test]
fn a_blank_or_unprintable_title_is_no_title() {
    assert!(psp::parse(&sfo_of(&[("TITLE", "Daxter")])).and_then(|s| s.title).is_some()); // control
    assert_eq!(psp::parse(&sfo_of(&[("TITLE", "   ")])).and_then(|s| s.title), None);
    assert_eq!(psp::parse(&sfo_of(&[("TITLE", "\u{1}\u{2}")])).and_then(|s| s.title), None);
}

#[test]
fn garbage_and_truncated_input_is_no_title_and_never_panics() {
    assert!(psp::parse(&[]).is_none());
    assert!(psp::parse(&[7u8; 64]).is_none());
    assert!(psp::from_pbp(&mut SliceSource::new(&[0u8; 64])).is_none());
    assert!(psp::from_iso(&mut SliceSource::new(&vec![0u8; 100_000])).is_none());
    let whole = sfo_of(&[("TITLE", "Daxter")]);
    assert_eq!(psp::parse(&whole).and_then(|s| s.title).as_deref(), Some("Daxter")); // control
    for cut in 0..whole.len() {
        let title = psp::parse(&whole[..cut]).and_then(|s| s.title).unwrap_or_default();
        assert!("Daxter".starts_with(&title), "{cut}: {title}");
    }
    let pbp = fx::pbp(&whole, 16);
    for cut in (0..pbp.len()).step_by(7) {
        let _ = psp::from_pbp(&mut SliceSource::new(&pbp[..cut]));
    }
}

#[test]
fn offsets_pointing_outside_the_file_are_ignored() {
    assert!(psp::parse(&sfo_of(&[("TITLE", "Daxter")])).is_some()); // control
    let mut bad = sfo_of(&[("TITLE", "Daxter")]);
    bad[8..12].copy_from_slice(&i32::MAX.to_le_bytes());
    bad[12..16].copy_from_slice(&(-1i32).to_le_bytes());
    assert!(psp::parse(&bad).is_none());
    let mut bad_pbp = fx::pbp(&sfo_of(&[("TITLE", "Daxter")]), 16);
    bad_pbp[8..12].copy_from_slice(&(-5i32).to_le_bytes());
    assert!(psp::from_pbp(&mut SliceSource::new(&bad_pbp)).is_none());
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
fn a_reader_that_fails_is_no_title() {
    assert!(psp::from_pbp(&mut SliceSource::new(&fx::pbp(&sfo_of(&[("TITLE", "x")]), 16))).is_some()); // control
    assert!(psp::from_pbp(&mut Failing).is_none());
    assert!(psp::from_iso(&mut Failing).is_none());
}

struct Logged<'a> {
    inner: SliceSource<'a>,
    lengths: Vec<usize>,
}

impl ByteSource for Logged<'_> {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read_at(&mut self, o: u64, b: &mut [u8]) -> Result<(), ReadError> {
        self.lengths.push(b.len());
        self.inner.read_at(o, b)
    }
}

#[test]
fn an_oversized_sfo_is_read_only_up_to_the_cap() {
    let mut big = fx::pbp(&sfo_of(&[("TITLE", "Daxter")]), 1);
    big[12..16].copy_from_slice(&(i32::MAX / 2).to_le_bytes());
    let mut logged = Logged { inner: SliceSource::new(&big), lengths: Vec::new() };
    let _ = psp::from_pbp(&mut logged);
    assert!(logged.lengths.iter().max().copied().unwrap() <= psp::MAX_READ);
}

// ---- DiscIdTest (the original frontend id reader)

fn ok(r: DiscIdResult) -> (String, DiscRegion, DiscSystem) {
    match r {
        DiscIdResult::Ok { id, region, system } => (id, region, system),
        other => panic!("expected an id, got {other:?}"),
    }
}

fn refused(r: DiscIdResult) -> DiscIdRefusal {
    match r {
        DiscIdResult::Refused(reason) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn gc2048(id: &str) -> Vec<u8> {
    fx::disc_header_2048(id, false)
}

fn wii2048(id: &str) -> Vec<u8> {
    fx::disc_header_2048(id, true)
}

#[test]
fn gc_header_gives_6_char_id() {
    let (id, region, system) = ok(ids::disc_id(&gc2048("TSTE01")));
    assert_eq!((id.as_str(), region, system), ("TSTE01", DiscRegion::NtscU, DiscSystem::GameCube));
}

#[test]
fn wii_header_gives_6_char_id() {
    let (id, region, system) = ok(ids::disc_id(&wii2048("TSTP01")));
    assert_eq!((id.as_str(), region, system), ("TSTP01", DiscRegion::Pal, DiscSystem::Wii));
}

#[test]
fn regions_follow_the_fourth_id_character() {
    assert_eq!(ok(ids::disc_id(&gc2048("TSTJ01"))).1, DiscRegion::NtscJ);
    assert_eq!(ok(ids::disc_id(&gc2048("TSTK01"))).1, DiscRegion::NtscK);
    assert_eq!(ok(ids::disc_id(&gc2048("TSTA01"))).1, DiscRegion::Unknown);
}

#[test]
fn zero_byte_file_refused_too_short() {
    assert_eq!(refused(ids::disc_id(&[])), DiscIdRefusal::TooShort);
    assert_eq!(refused(ids::disc_id(&[0u8; 0x1F])), DiscIdRefusal::TooShort);
}

#[test]
fn random_bytes_refused_bad_magic() {
    // A fixed pseudo-random block (an LCG), clear of every container magic.
    let mut state = 7u32;
    let mut bytes: Vec<u8> = (0..2048)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect();
    bytes[0] = 0x11;
    assert_eq!(refused(ids::disc_id(&bytes)), DiscIdRefusal::BadMagic);
}

#[test]
fn id_with_non_ascii_refused() {
    assert!(matches!(ids::disc_id(&gc2048("TSTE01")), DiscIdResult::Ok { .. })); // control
    let mut h = gc2048("TSTE01");
    h[2] = 0xE9;
    assert_eq!(refused(ids::disc_id(&h)), DiscIdRefusal::Unreadable);
    let mut z = wii2048("TSTP01");
    z[0] = 0;
    assert_eq!(refused(ids::disc_id(&z)), DiscIdRefusal::Unreadable);
}

#[test]
fn rvz_wia_read_from_container_header_and_truncated_or_non_disc_ones_are_refused() {
    let (id, _, system) = ok(ids::disc_id(&fx::compressed_2048("RVZ", &gc2048("TSTE01"))));
    assert_eq!((id.as_str(), system), ("TSTE01", DiscSystem::GameCube));
    let (id, _, system) = ok(ids::disc_id(&fx::compressed_2048("WIA", &wii2048("TSTP01"))));
    assert_eq!((id.as_str(), system), ("TSTP01", DiscSystem::Wii));
    assert_eq!(refused(ids::disc_id(&fx::compressed_2048("RVZ", &gc2048("TSTE01"))[..0x60])), DiscIdRefusal::TooShort);
    assert_eq!(refused(ids::disc_id(&fx::compressed_2048("RVZ", &[0u8; 0x80]))), DiscIdRefusal::BadMagic);
}

#[test]
fn other_compressed_containers_are_unsupported() {
    let mut wbfs = vec![0u8; 2048];
    wbfs[..4].copy_from_slice(b"WBFS");
    let mut ciso = vec![0u8; 2048];
    ciso[..4].copy_from_slice(b"CISO");
    let mut gcz = vec![0u8; 2048];
    gcz[..4].copy_from_slice(&0xB10B_C001u32.to_le_bytes());
    for image in [wbfs, ciso, gcz] {
        assert_eq!(refused(ids::disc_id(&image)), DiscIdRefusal::UnsupportedContainer);
    }
}

#[test]
fn the_source_form_reads_the_header_never_a_name_and_a_failing_source_is_unreadable() {
    // The original implementation's file-based tests: a bad header is refused whatever the file is called; an empty file is too short;
    // a missing file or a folder (a source that cannot be read) is unreadable.
    assert_eq!(refused(ids::disc_id_of(&mut SliceSource::new(&vec![0x22u8; 4096]))), DiscIdRefusal::BadMagic);
    assert_eq!(refused(ids::disc_id_of(&mut SliceSource::new(&[]))), DiscIdRefusal::TooShort);
    assert_eq!(refused(ids::disc_id_of(&mut Failing)), DiscIdRefusal::Unreadable);
}

#[test]
fn a_source_with_a_good_header_reads_the_header_not_the_name() {
    let file = [gc2048("TSTE01"), vec![0u8; 8192]].concat();
    assert_eq!(ok(ids::disc_id_of(&mut SliceSource::new(&file))).0, "TSTE01");
}

#[test]
fn xbox_xbe_title_id_from_certificate() {
    let (id, region, system) = ok(ids::disc_id(&fx::xbe(0x4D53_0004, 1, 0x10000, 0x178)));
    assert_eq!((id.as_str(), region, system), ("4D530004", DiscRegion::NtscU, DiscSystem::Xbox));
    assert_eq!(ok(ids::disc_id(&fx::xbe(0x4D53_0004, 2, 0x10000, 0x178))).1, DiscRegion::NtscJ);
    assert_eq!(ok(ids::disc_id(&fx::xbe(0x4D53_0004, 4, 0x10000, 0x178))).1, DiscRegion::Pal);
    // The entry by file name gives the same id for a `.xbe`.
    let got = read("default.xbe", &fx::xbe(0x4D53_0004, 1, 0x10000, 0x178));
    assert_eq!(got.and_then(|i| i.xbox_id).as_deref(), Some("4D530004"));
}

#[test]
fn xbe_with_certificate_outside_the_header_is_refused() {
    assert_eq!(refused(ids::disc_id(&fx::xbe(1, 1, 0x10000, 0x4000))), DiscIdRefusal::TooShort);
    let mut h = fx::xbe(1, 1, 0x10000, 0x178);
    h[0x118..0x11C].copy_from_slice(&0x100u32.to_le_bytes());
    assert_eq!(refused(ids::disc_id(&h)), DiscIdRefusal::Unreadable);
    assert_eq!(refused(ids::disc_id(b"XBEH")), DiscIdRefusal::TooShort);
}
