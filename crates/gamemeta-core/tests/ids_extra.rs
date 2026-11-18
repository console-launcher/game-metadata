// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Rust-only checks of the id readers beyond the original suites: raw-sector images, container variants, the shared SFO and
//! ISO code, and the limits that keep a hostile file small.

use gamemeta_core::ids::{self, Ids, RaHasher};
use gamemeta_core::{ByteSource, SliceSource};
use gamemeta_fixtures::ids as fx;

fn read(name: &str, bytes: &[u8]) -> Option<Ids> {
    ids::read(&mut SliceSource::new(bytes), name)
}

#[test]
fn a_raw_2352_byte_bin_of_either_mode_gives_the_serial_without_a_cue() {
    let iso = fx::iso9660(&[("SYSTEM.CNF;1", &fx::system_cnf("BOOT = cdrom:\\SCES_000.04;1"))]);
    for mode in [1, 2] {
        let got = read("game.bin", &fx::raw2352(&iso, mode));
        assert_eq!(got.and_then(|i| i.serial).as_deref(), Some("SCES00004"), "mode {mode}");
    }
    assert_eq!(read("game.img", &iso).and_then(|i| i.serial).as_deref(), Some("SCES00004"));
}

#[test]
fn a_psp_iso_gives_the_serial_and_the_sfo_fields_together() {
    let got = read("g.iso", &fx::sample_psp_iso()).unwrap();
    assert_eq!(got.serial.as_deref(), Some("ULUS00001"));
    assert_eq!(got.psp_disc_id.as_deref(), Some("ULUS00001"));
    assert_eq!(got.psp_title.as_deref(), Some("Sample Portable"));
    assert_eq!(got.primary_id(), Some("ULUS00001"));
}

#[test]
fn an_eboot_gives_title_disc_id_and_serial() {
    let pbp = fx::pbp(&fx::sfo(&[("DISC_ID", "ulus-10041"), ("TITLE", "Daxter")]), 8);
    let got = read("EBOOT.PBP", &pbp).unwrap();
    assert_eq!(got.psp_title.as_deref(), Some("Daxter"));
    assert_eq!(got.psp_disc_id.as_deref(), Some("ulus-10041"));
    assert_eq!(got.serial.as_deref(), Some("ULUS10041"));
}

#[test]
fn the_primary_id_prefers_the_serial_then_the_game_id_then_the_title_id() {
    let ids = Ids { game_id: Some("G".into()), title_id: Some("T".into()), ..Ids::default() };
    assert_eq!(ids.primary_id(), Some("G"));
    assert_eq!(Ids { serial: Some("S".into()), ..ids.clone() }.primary_id(), Some("S"));
    assert_eq!(Ids { title_id: Some("T".into()), ..Ids::default() }.primary_id(), Some("T"));
    assert!(Ids::default().is_empty());
}

#[test]
fn the_extension_case_does_not_matter_and_a_name_without_one_reads_nothing() {
    assert_eq!(read("G.ISO", &fx::game_cube("GZZE01")).and_then(|i| i.game_id).as_deref(), Some("GZZE01"));
    assert!(read("iso", &fx::game_cube("GZZE01")).is_none());
    assert!(read("noextension", &fx::game_cube("GZZE01")).is_none());
}

#[test]
fn a_truncated_ciso_still_gives_its_id_and_a_gcz_is_read_like_the_other_containers() {
    assert_eq!(read("g.ciso", &fx::ciso(&fx::wii("RZZP01"))).and_then(|i| i.game_id).as_deref(), Some("RZZP01"));
    let mut no_block = fx::ciso(&fx::game_cube("GZZE01"));
    no_block[8] = 0;
    assert!(read("g.ciso", &no_block).is_none());
}

#[test]
fn a_zip64_size_or_a_lying_directory_gives_none() {
    let mut zip = fx::zip(&[("a.sfc", &[1u8; 10])]);
    let cd = zip.windows(4).position(|w| w == [0x50, 0x4B, 0x01, 0x02]).unwrap();
    let mut with_zip64 = zip.clone();
    with_zip64[cd + 24..cd + 28].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(read("g.zip", &with_zip64).is_none());
    let eocd = zip.len() - 22;
    zip[eocd + 16..eocd + 20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(read("g.zip", &zip).is_none());
}

#[test]
fn an_nsp_with_an_enormous_table_or_count_gives_none_without_allocating_it() {
    let mut nsp = fx::pfs0(&["0100000000001000.cnmt.nca"]);
    nsp[4..8].copy_from_slice(&5000u32.to_le_bytes());
    assert!(read("g.nsp", &nsp).is_none());
    let mut big = fx::pfs0(&["0100000000001000.cnmt.nca"]);
    big[8..12].copy_from_slice(&(1u32 << 30).to_le_bytes());
    assert!(read("g.nsp", &big).is_none());
}

#[test]
fn a_directory_record_that_overruns_its_sector_ends_the_walk() {
    let mut iso = fx::iso9660(&[("SYSTEM.CNF;1", &fx::system_cnf("BOOT = cdrom:\\SLUS_000.01;1"))]);
    iso[18 * 2048] = 255; // the first record claims 255 bytes and a 255-byte name: it overruns the directory
    iso[18 * 2048 + 32] = 255;
    assert!(read("g.iso", &iso).is_none());
}

#[test]
fn an_nes_file_shorter_than_its_header_is_not_hashed_and_a_whole_platform_empty_file_is() {
    let mut nes = RaHasher::for_file("g.nes", Some(10)).unwrap();
    nes.update(b"NES");
    assert_eq!(nes.finish(), None);
    let empty = RaHasher::for_file("g.gb", Some(0)).unwrap();
    assert_eq!(empty.finish().as_deref(), Some("d41d8cd98f00b204e9800998ecf8427e"));
}

#[test]
fn ra_hash_streams_a_source_and_matches_the_chunked_hasher() {
    let rom = fx::sample_nes();
    let whole = ids::ra_hash(&mut SliceSource::new(&rom), "g.nes").unwrap();
    let mut chunked = RaHasher::for_file("g.nes", Some(rom.len() as u64)).unwrap();
    for part in rom.chunks(333) {
        chunked.update(part);
    }
    assert_eq!(chunked.finish().as_deref(), Some(whole.as_str()));
    assert!(ids::ra_hash(&mut SliceSource::new(&rom), "g.n64").is_none());
}

struct Counting<'a> {
    inner: SliceSource<'a>,
    total: u64,
}

impl ByteSource for Counting<'_> {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read_at(&mut self, o: u64, b: &mut [u8]) -> Result<(), gamemeta_core::ReadError> {
        self.total += b.len() as u64;
        self.inner.read_at(o, b)
    }
}

#[test]
fn every_reader_reads_a_bounded_amount_whatever_the_file_says() {
    let zeros = vec![0xFFu8; 1 << 20];
    for name in
        ["a.iso", "a.bin", "a.cue", "a.pbp", "a.chd", "a.zip", "a.nsp", "a.wad", "a.wua", "a.xbe", "a.3ds", "a.rvz"]
    {
        let mut counting = Counting { inner: SliceSource::new(&zeros), total: 0 };
        let _ = ids::read(&mut counting, name);
        assert!(counting.total <= 1 << 20, "{name} read {}", counting.total);
    }
}
