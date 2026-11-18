// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The ids entry points of the C ABI, as a C caller would use them.

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr::{null, null_mut};

use gamemeta::*;
use gamemeta_fixtures::ids as fx;

fn text(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned())
}

fn read(bytes: &[u8], name: &str, open: Option<GmOpenFn>, user: *mut c_void) -> (GmStatus, *mut GmIds) {
    let name = CString::new(name).unwrap();
    let mut out = null_mut();
    let status = unsafe { gm_ids_read_bytes(bytes.as_ptr(), bytes.len(), name.as_ptr(), open, user, &mut out) };
    (status, out)
}

fn get(ids: *const GmIds, field: GmIdField) -> Option<String> {
    text(unsafe { gm_ids_get(ids, field as u32) })
}

#[test]
fn a_ps2_disc_gives_its_serial_through_the_handle() {
    let (status, ids) = read(&fx::sample_ps2_iso(), "game.iso", None, null_mut());
    assert_eq!(status, GmStatus::Ok);
    assert_eq!(get(ids, GmIdField::Serial).as_deref(), Some("SLUS00003"));
    assert_eq!(get(ids, GmIdField::Primary).as_deref(), Some("SLUS00003"));
    assert_eq!(get(ids, GmIdField::GameId), None);
    assert_eq!(unsafe { gm_ids_zip_size(ids) }, -1);
    assert!(unsafe { gm_ids_get(ids, 99) }.is_null());
    assert!(unsafe { gm_ids_get(null(), 0) }.is_null());
    unsafe { gm_ids_free(ids) };
}

#[test]
fn a_zip_gives_crc_and_size_and_a_psp_iso_its_sfo_fields() {
    let rom = fx::sample_rom(4096);
    let (_, ids) = read(&fx::zip(&[("game.sfc", &rom)]), "g.zip", None, null_mut());
    assert_eq!(get(ids, GmIdField::ZipCrc), Some(fx::crc_hex(&rom)));
    assert_eq!(unsafe { gm_ids_zip_size(ids) }, 4096);
    unsafe { gm_ids_free(ids) };
    let (_, ids) = read(&fx::sample_psp_iso(), "g.iso", None, null_mut());
    assert_eq!(get(ids, GmIdField::PspTitle).as_deref(), Some("Sample Portable"));
    assert_eq!(get(ids, GmIdField::PspDiscId).as_deref(), Some("ULUS00001"));
    unsafe { gm_ids_free(ids) };
}

struct Files {
    name: &'static str,
    bytes: Vec<u8>,
    opened: Vec<String>,
    closed: usize,
}

unsafe extern "C" fn read_open(user: *mut c_void, offset: u64, buf: *mut u8, len: usize) -> i32 {
    let files = unsafe { &*(user as *const Files) };
    let (start, end) = (offset as usize, offset as usize + len);
    match files.bytes.get(start..end) {
        Some(src) => {
            unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), buf, len) };
            GmReadResult::Ok as i32
        }
        None => GmReadResult::OutOfRange as i32,
    }
}

unsafe extern "C" fn close_open(user: *mut c_void) {
    unsafe { &mut *(user as *mut Files) }.closed += 1;
}

unsafe extern "C" fn open_file(user: *mut c_void, path: *const c_char, out: *mut GmOpened) -> i32 {
    let files = unsafe { &mut *(user as *mut Files) };
    let path = unsafe { CStr::from_ptr(path) }.to_str().unwrap().to_owned();
    files.opened.push(path.clone());
    if path != files.name {
        return GmFileResult::NotFound as i32;
    }
    unsafe {
        *out = GmOpened {
            size: files.bytes.len() as u64,
            read: Some(read_open),
            user_data: user,
            close: Some(close_open),
        };
    }
    GmFileResult::Ok as i32
}

#[test]
fn a_cue_sheet_opens_its_track_through_the_callback_and_closes_it() {
    let iso = fx::iso9660(&[("SYSTEM.CNF;1", &fx::system_cnf("BOOT = cdrom:\\SCES_000.04;1"))]);
    let mut files = Files { name: "game (Track 1).bin", bytes: fx::raw2352(&iso, 2), opened: Vec::new(), closed: 0 };
    let cue = b"FILE \"game (Track 1).bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n";
    let (status, ids) = read(cue, "game.cue", Some(open_file), &mut files as *mut Files as *mut c_void);
    assert_eq!(status, GmStatus::Ok);
    assert_eq!(get(ids, GmIdField::Serial).as_deref(), Some("SCES00004"));
    assert_eq!((files.opened.as_slice(), files.closed), (&["game (Track 1).bin".to_owned()][..], 1));
    unsafe { gm_ids_free(ids) };
    // No opener: nothing to read; a file the opener does not have: nothing either.
    assert_eq!(read(cue, "game.cue", None, null_mut()).0, GmStatus::NotFound);
    files.name = "other.bin";
    assert_eq!(read(cue, "game.cue", Some(open_file), &mut files as *mut Files as *mut c_void).0, GmStatus::NotFound);
}

#[test]
fn a_wii_u_rpx_asks_for_the_meta_folder_beside_its_code_folder() {
    let mut files =
        Files { name: "../meta/meta.xml", bytes: fx::wiiu_meta("000500001010abcd"), opened: Vec::new(), closed: 0 };
    let (status, ids) = read(&[0u8; 16], "game.rpx", Some(open_file), &mut files as *mut Files as *mut c_void);
    assert_eq!(status, GmStatus::Ok);
    assert_eq!(get(ids, GmIdField::TitleId).as_deref(), Some("000500001010ABCD"));
    assert_eq!(files.opened, ["../meta/meta.xml"]);
    unsafe { gm_ids_free(ids) };
}

#[test]
fn bad_arguments_and_unknown_files_are_neutral() {
    let name = CString::new("g.iso").unwrap();
    let mut out = null_mut();
    unsafe {
        assert_eq!(gm_ids_read_bytes(null(), 5, name.as_ptr(), None, null_mut(), &mut out), GmStatus::ErrArgument);
        assert_eq!(gm_ids_read_bytes(null(), 0, null(), None, null_mut(), &mut out), GmStatus::ErrArgument);
        assert_eq!(gm_ids_read_bytes(null(), 0, name.as_ptr(), None, null_mut(), null_mut()), GmStatus::ErrArgument);
        assert_eq!(gm_ids_read_bytes(null(), 0, name.as_ptr(), None, null_mut(), &mut out), GmStatus::NotFound);
        assert!(out.is_null());
        assert_eq!(
            gm_ids_read_callback(1, None, null_mut(), name.as_ptr(), None, null_mut(), &mut out),
            GmStatus::ErrArgument
        );
        gm_ids_free(null_mut());
    }
    assert_eq!(read(&[0u8; 100], "g.unknown", None, null_mut()).0, GmStatus::NotFound);
}

unsafe extern "C" fn always_fails(_: *mut c_void, _: u64, _: *mut u8, _: usize) -> i32 {
    GmReadResult::Error as i32
}

#[test]
fn a_failing_read_callback_is_an_io_error() {
    let name = CString::new("g.chd").unwrap();
    let mut out = null_mut();
    let status = unsafe {
        gm_ids_read_callback(1000, Some(always_fails), null_mut(), name.as_ptr(), None, null_mut(), &mut out)
    };
    assert_eq!(status, GmStatus::ErrIo);
}

#[cfg(unix)]
#[test]
fn a_descriptor_reads_with_positioned_reads() {
    use std::os::fd::AsRawFd;
    let path = std::env::temp_dir().join(format!("gamemeta-ids-fd-{}.chd", std::process::id()));
    std::fs::write(&path, fx::sample_chd()).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let name = CString::new("sample.chd").unwrap();
    let mut out = null_mut();
    assert_eq!(unsafe { gm_ids_read_fd(file.as_raw_fd(), name.as_ptr(), None, null_mut(), &mut out) }, GmStatus::Ok);
    assert_eq!(get(out, GmIdField::ChdSha1).as_deref(), Some("0123456789abcdef0123456789abcdef01234567"));
    unsafe { gm_ids_free(out) };
    assert_eq!(unsafe { gm_ids_read_fd(-1, name.as_ptr(), None, null_mut(), &mut out) }, GmStatus::ErrArgument);
    let mut disc = unsafe { std::mem::zeroed::<GmDiscId>() };
    assert_eq!(unsafe { gm_disc_id_fd(file.as_raw_fd(), &mut disc) }, GmStatus::Ok);
    assert_eq!(disc.status, GmDiscIdStatus::BadMagic);
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_ra_hasher_streams_and_matches_the_core() {
    let nes = fx::sample_nes();
    let name = CString::new("g.nes").unwrap();
    let want = gamemeta_core::ids::ra_hash(&mut gamemeta_core::SliceSource::new(&nes), "g.nes").unwrap();
    unsafe {
        let h = gm_ra_hasher_new(name.as_ptr(), nes.len() as i64);
        assert!(!h.is_null());
        for part in nes.chunks(1000) {
            gm_ra_hasher_update(h, part.as_ptr(), part.len());
        }
        gm_ra_hasher_update(h, null(), 0);
        assert_eq!(text(gm_ra_hasher_finish(h)).as_deref(), Some(want.as_str()));
        assert_eq!(text(gm_ra_hasher_finish(h)).as_deref(), Some(want.as_str()));
        gm_ra_hasher_free(h);
        // NES needs its size, other platforms are not hashed, a short NES has no hash.
        assert!(gm_ra_hasher_new(name.as_ptr(), -1).is_null());
        assert!(gm_ra_hasher_new(CString::new("g.iso").unwrap().as_ptr(), 10).is_null());
        assert!(gm_ra_hasher_new(null(), 10).is_null());
        let short = gm_ra_hasher_new(name.as_ptr(), 10);
        gm_ra_hasher_update(short, b"NES".as_ptr(), 3);
        assert!(gm_ra_hasher_finish(short).is_null());
        gm_ra_hasher_free(short);
        gm_ra_hasher_update(null_mut(), null(), 0);
        assert!(gm_ra_hasher_finish(null_mut()).is_null());
        gm_ra_hasher_free(null_mut());
    }
}

fn disc_id(header: &[u8]) -> GmDiscId {
    let mut out = unsafe { std::mem::zeroed::<GmDiscId>() };
    assert_eq!(unsafe { gm_disc_id_bytes(header.as_ptr(), header.len(), &mut out) }, GmStatus::Ok);
    out
}

fn id_text(d: &GmDiscId) -> String {
    unsafe { CStr::from_ptr(d.id.as_ptr()) }.to_str().unwrap().to_owned()
}

#[test]
fn the_disc_id_reads_gc_wii_and_xbox_headers_and_refuses_the_rest_with_a_reason() {
    let gc = disc_id(&fx::disc_header_2048("TSTE01", false));
    assert_eq!(
        (gc.status, gc.region, gc.system, id_text(&gc).as_str()),
        (GmDiscIdStatus::Ok, GmDiscRegion::NtscU, GmDiscSystem::GameCube, "TSTE01")
    );
    let wii = disc_id(&fx::disc_header_2048("TSTP01", true));
    assert_eq!((wii.region, wii.system), (GmDiscRegion::Pal, GmDiscSystem::Wii));
    let xbox = disc_id(&fx::xbe(0x4D53_0004, 2, 0x10000, 0x178));
    assert_eq!(
        (xbox.system, xbox.region, id_text(&xbox).as_str()),
        (GmDiscSystem::Xbox, GmDiscRegion::NtscJ, "4D530004")
    );
    assert_eq!(disc_id(&[]).status, GmDiscIdStatus::TooShort);
    assert_eq!(disc_id(&[0x11u8; 2048]).status, GmDiscIdStatus::BadMagic);
    assert_eq!(disc_id(b"WBFS\0\0\0\0").status, GmDiscIdStatus::UnsupportedContainer);
    let mut bad = fx::disc_header_2048("TSTE01", false);
    bad[2] = 0xE9;
    assert_eq!(disc_id(&bad).status, GmDiscIdStatus::Unreadable);
    let mut out = unsafe { std::mem::zeroed::<GmDiscId>() };
    assert_eq!(unsafe { gm_disc_id_bytes(null(), 3, &mut out) }, GmStatus::ErrArgument);
    assert_eq!(unsafe { gm_disc_id_bytes(null(), 0, null_mut()) }, GmStatus::ErrArgument);
}

unsafe extern "C" fn header_read(user: *mut c_void, offset: u64, buf: *mut u8, len: usize) -> i32 {
    let bytes = unsafe { &*(user as *const Vec<u8>) };
    match bytes.get(offset as usize..offset as usize + len) {
        Some(src) => {
            unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), buf, len) };
            0
        }
        None => 1,
    }
}

#[test]
fn the_disc_id_reads_the_first_2048_bytes_through_a_callback() {
    let file = [fx::disc_header_2048("TSTE01", false), vec![0u8; 8192]].concat();
    let mut out = unsafe { std::mem::zeroed::<GmDiscId>() };
    let status = unsafe {
        gm_disc_id_callback(file.len() as u64, Some(header_read), &file as *const Vec<u8> as *mut c_void, &mut out)
    };
    assert_eq!((status, out.status, id_text(&out).as_str()), (GmStatus::Ok, GmDiscIdStatus::Ok, "TSTE01"));
    let status = unsafe { gm_disc_id_callback(10, Some(always_fails), null_mut(), &mut out) };
    assert_eq!((status, out.status), (GmStatus::Ok, GmDiscIdStatus::Unreadable));
    assert_eq!(unsafe { gm_disc_id_callback(10, None, null_mut(), &mut out) }, GmStatus::ErrArgument);
}
