// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Parity with the original Kotlin reader's tests (`ThreeDsFileTest`, `ThreeDsLargeFileTest`, `ThreeDsTitleIdTest`
//! and the 3DS cases of `DsGameMetadataTest`): same synthetic inputs, same expected outputs. One Rust test per
//! reference test, named after it.

#![allow(clippy::needless_range_loop)] // mirrors the original test's index loop

use gamemeta_core::threeds::{self, Reason};
use gamemeta_core::{ByteSource, Language, RatingBoard, ReadError, Region, SliceSource, read};
use gamemeta_fixtures::{cbmd, exefs, ncch, ncsd, plain_ncch, reference_smdh, three_ds, with_program_id};

fn banner() -> Vec<u8> {
    (0..300).map(|i| (i * 7) as u8).collect()
}
fn icon() -> Vec<u8> {
    (0..64).map(|i| i as u8).collect()
}
fn reason(bytes: &[u8]) -> Reason {
    threeds::read(&mut SliceSource::new(bytes)).expect_err("expected no banner")
}

// ---- ThreeDsFileTest ----

#[test]
fn reads_the_banner_and_icon_from_a_decrypted_cartridge_image() {
    let file = ncsd(&plain_ncch(&exefs(&[(".code", &[0; 100]), ("banner", &banner()), ("icon", &icon())])));
    let found = threeds::read(&mut SliceSource::new(&file)).unwrap();
    assert_eq!(found.banner, banner());
    assert_eq!(found.icon, Some(icon()));
}

#[test]
fn reads_a_bare_ncch() {
    let found = threeds::read(&mut SliceSource::new(&plain_ncch(&exefs(&[("banner", &banner())])))).unwrap();
    assert_eq!(found.banner, banner());
}

#[test]
fn a_dump_that_is_plain_but_does_not_set_the_no_crypto_flag_still_reads() {
    let file = ncsd(&ncch(&exefs(&[("banner", &banner())]), 0));
    assert!(threeds::read(&mut SliceSource::new(&file)).is_ok());
}

#[test]
fn an_encrypted_exefs_is_reported_as_encrypted_not_parsed() {
    let noise: Vec<u8> = (0..0x400).map(|i: i32| (i * 31 + 7) as u8).collect();
    assert_eq!(reason(&ncsd(&ncch(&noise, 0))), Reason::Encrypted);
}

#[test]
fn a_cia_whose_content_is_not_plain_ncch_is_encrypted() {
    let mut cia = vec![0u8; 0x2020 + 0x400];
    cia[0..4].copy_from_slice(&0x2020u32.to_le_bytes());
    for i in 0x2040..0x2420 {
        cia[i] = (i * 13) as u8;
    }
    assert_eq!(reason(&cia), Reason::Encrypted);
}

#[test]
fn anything_else_is_not_a_3ds_file_and_short_or_lying_files_never_throw() {
    assert_eq!(reason(&[]), Reason::NotA3dsFile);
    assert_eq!(reason(&[1u8; 5000]), Reason::NotA3dsFile);
    let file = ncsd(&plain_ncch(&exefs(&[("banner", &banner())])));
    reason(&file[..0x4000 + 0x200 + 0x100]);
    let mut lying = file.clone();
    lying[0x4000 + 0x200 + 12 + 1] = 0x7f;
    reason(&lying);
}

#[test]
fn an_oversized_banner_is_refused() {
    let file = ncsd(&plain_ncch(&exefs(&[("banner", &vec![0u8; 5 << 20])])));
    assert_eq!(reason(&file), Reason::Damaged);
}

// ---- ThreeDsLargeFileTest: files over 2 GiB, offsets stay 64-bit end to end ----

const GIB: u64 = 1 << 30;

/// The NCSD header at 0 and the NCCH at `base`, with `size` reported and nothing else stored.
struct SparseSource {
    file: Vec<u8>,
    head: Vec<u8>,
    base: u64,
    size: u64,
}

impl SparseSource {
    fn new(file: Vec<u8>, base: u64, size: u64) -> Self {
        let mut head = file[..0x200].to_vec();
        head[0x120..0x124].copy_from_slice(&((base / 0x200) as u32).to_le_bytes());
        SparseSource { file, head, base, size }
    }
}

impl ByteSource for SparseSource {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        if offset + buf.len() as u64 > self.size {
            return Err(ReadError::OutOfRange);
        }
        for (i, b) in buf.iter_mut().enumerate() {
            let at = offset + i as u64;
            *b = if at < 0x200 {
                self.head[at as usize]
            } else if at >= self.base && ((at - self.base + 0x4000) as usize) < self.file.len() {
                self.file[(at - self.base + 0x4000) as usize]
            } else {
                0
            };
        }
        Ok(())
    }
}

fn image() -> Vec<u8> {
    ncsd(&plain_ncch(&exefs(&[("banner", &banner()), ("icon", &icon())])))
}

#[test]
fn a_file_longer_than_2_gib_is_read_not_skipped_as_not_a_3ds_file() {
    let found = threeds::read(&mut SparseSource::new(image(), 0x4000, 3 * GIB)).unwrap();
    assert_eq!(found.banner, banner());
}

#[test]
fn a_partition_past_2_gib_is_read() {
    let found = threeds::read(&mut SparseSource::new(image(), 2 * GIB + 0x10000, 3 * GIB)).unwrap();
    assert_eq!(found.banner, banner());
    assert_eq!(found.icon, Some(icon()));
}

// ---- ThreeDsTitleIdTest ----

fn plain() -> Vec<u8> {
    plain_ncch(&exefs(&[("banner", &[0; 16])]))
}

#[test]
fn a_bare_ncch_gives_its_program_id_as_16_hex_digits() {
    let file = with_program_id(plain(), 0x0004_0000_00AB_CDEF);
    assert_eq!(threeds::title_id(&mut SliceSource::new(&file)).as_deref(), Some("0004000000ABCDEF"));
}

#[test]
fn a_cartridge_image_gives_the_id_of_partition_0() {
    let file = ncsd(&with_program_id(plain(), 0x0004_0000_0012_34AB));
    assert_eq!(threeds::title_id(&mut SliceSource::new(&file)).as_deref(), Some("00040000001234AB"));
}

#[test]
fn a_file_that_is_not_a_3ds_file_or_is_cut_short_has_no_id() {
    assert_eq!(threeds::title_id(&mut SliceSource::new(&[])), None);
    assert_eq!(threeds::title_id(&mut SliceSource::new(&[7; 0x400])), None);
    assert_eq!(threeds::title_id(&mut SliceSource::new(&with_program_id(plain(), 5)[..0x110])), None);
}

// ---- DsGameMetadataTest, 3DS cases ----

const TITLE_ID: u64 = 0x0004_0000_0003_0800;

fn default_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![("banner", cbmd(&[0; 16], &[])), ("icon", reference_smdh(0x7FFF_FFFF, &[]))]
}

fn three_ds_with(files: &[(&'static str, Vec<u8>)], product_code: &str) -> Vec<u8> {
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, b)| (*n, b.as_slice())).collect();
    three_ds(&refs, TITLE_ID, product_code)
}

fn info(bytes: &[u8], name: &str) -> Option<gamemeta_core::GameInfo> {
    read(&mut SliceSource::new(bytes), name)
}

#[test]
fn a_3ds_file_gives_its_titles_icon_title_id_and_the_banner_location_without_reading_the_banner() {
    let i = info(&three_ds_with(&default_files(), "CTR-P-AMKE"), "game.3ds").unwrap();
    assert_eq!(i.platform, gamemeta_core::Platform::ThreeDs);
    let en = i.title_in(Language::English).unwrap();
    assert_eq!(en.short_name, "Short");
    assert_eq!(en.name, "Long Name");
    assert_eq!(en.publisher.as_deref(), Some("Pub"));
    assert_eq!(i.icon.as_ref().map(|im| im.width), Some(48));
    assert_eq!(i.ids.title_id.as_deref(), Some("0004000000030800"));
    assert_eq!(i.banner.unwrap().size, cbmd(&[0; 16], &[]).len() as u64);
}

#[test]
fn a_3ds_file_gives_its_product_code() {
    let i = info(&three_ds_with(&default_files(), "CTR-P-AMKE"), "game.cci").unwrap();
    assert_eq!(i.ids.product_code.as_deref(), Some("CTR-P-AMKE"));
}

#[test]
fn an_smdh_region_lockout_gives_its_regions_and_the_all_regions_value_is_region_free() {
    let usa = info(&three_ds_with(&[("icon", reference_smdh(0x02, &[]))], "CTR-P-AMKE"), "a.3ds").unwrap();
    assert_eq!(usa.regions, [Region::NorthAmerica]);
    assert!(!usa.region_free);
    let free = info(&three_ds_with(&default_files(), "CTR-P-AMKE"), "a.3ds").unwrap();
    assert!(free.region_free);
}

#[test]
fn smdh_age_ratings_come_through_for_active_boards_only() {
    // ESRB active, age 10; CERO pending (no age yet); PEGI active with no age restriction.
    let smdh = reference_smdh(0x7FFF_FFFF, &[(1, 0x80 | 10), (0, 0x80 | 0x40), (4, 0x80 | 0x20)]);
    let i = info(&three_ds_with(&[("icon", smdh)], "CTR-P-AMKE"), "a.3ds").unwrap();
    assert_eq!(i.ratings, [(RatingBoard::Esrb, 10), (RatingBoard::Pegi, 0)]);
}

#[test]
fn a_3ds_file_with_no_banner_still_gives_its_icon_and_ids_and_no_diorama() {
    let i = info(&three_ds_with(&[("icon", reference_smdh(0x7FFF_FFFF, &[]))], "CTR-P-AMKE"), "a.3ds").unwrap();
    assert_eq!(i.banner, None);
    assert_eq!(i.title(), Some("Long Name"));
    assert_eq!(i.ids.title_id.as_deref(), Some("0004000000030800"));
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
fn an_unknown_empty_damaged_or_unreadable_file_is_null_and_never_throws() {
    assert_eq!(info(&[], "game.3ds"), None);
    let counting: Vec<u8> = (0..0x4000).map(|i| i as u8).collect();
    assert_eq!(info(&counting, "game.3ds"), None);
    assert_eq!(info(&three_ds_with(&default_files(), "CTR-P-AMKE"), "notes.txt"), None);
    assert_eq!(info(&three_ds_with(&default_files(), "CTR-P-AMKE")[..0x4100], "cut.3ds"), None);
    assert_eq!(read(&mut Failing, "game.3ds"), None);
}
