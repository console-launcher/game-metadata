// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Wii channel banner entry points of the C ABI, as a C caller would use them.

use std::ffi::{CStr, c_void};
use std::ptr::null_mut;

use gamemeta::*;
use gamemeta_core::wii::banner::WiiBanner;
use gamemeta_fixtures::wii::{PartFile, WiiRvzOptions, opening, partition_data, rvz};
use gamemeta_fixtures::wii_banner_cases::cases;

fn case(name: &str) -> Vec<u8> {
    cases().into_iter().find(|c| c.name == name).unwrap().archive
}

fn open(bytes: &[u8]) -> (GmStatus, *mut GmWiiBanner) {
    let mut out = null_mut();
    let status = unsafe { gm_wii_banner_read_bytes(bytes.as_ptr(), bytes.len(), &mut out) };
    (status, out)
}

#[test]
fn a_banner_archive_counts_sizes_and_draws() {
    let bytes = case("start-loop");
    let (status, banner) = open(&bytes);
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(gm_wii_banner_count(banner, GmWiiBannerCount::StartFrames as u32), 60);
        assert_eq!(gm_wii_banner_count(banner, GmWiiBannerCount::LoopFrames as u32), 60);
        assert_eq!(gm_wii_banner_count(banner, GmWiiBannerCount::MissingTextures as u32), 0);
        assert_eq!(gm_wii_banner_count(banner, 99), 0);
        let (mut w, mut h) = (7, 7);
        assert_eq!(gm_wii_banner_size(banner, 1.5, &mut w, &mut h), GmStatus::Ok);
        assert_eq!((w, h), (300, 150));
        let mut pixels = vec![0u8; 300 * 150 * 4];
        assert_eq!(gm_wii_banner_render(banner, 75.0, 1.5, pixels.as_mut_ptr(), pixels.len()), GmStatus::Ok);
        let expected = WiiBanner::open(&bytes).unwrap().render(75.0, 1.5).unwrap();
        assert!(pixels == expected.pixels, "the C frame is the Rust frame");
        assert!(pixels.chunks(4).any(|p| p[3] != 0), "something is drawn");
        // Bad arguments write nothing and say so.
        let mut small = vec![9u8; 10];
        assert_eq!(gm_wii_banner_render(banner, 0.0, 1.5, small.as_mut_ptr(), small.len()), GmStatus::ErrArgument);
        assert_eq!(small, vec![9u8; 10]);
        assert_eq!(gm_wii_banner_render(banner, 0.0, 0.0, pixels.as_mut_ptr(), pixels.len()), GmStatus::ErrArgument);
        assert_eq!(
            gm_wii_banner_render(banner, 0.0, f32::NAN, pixels.as_mut_ptr(), pixels.len()),
            GmStatus::ErrArgument
        );
        assert_eq!(gm_wii_banner_render(banner, 0.0, 1.5, null_mut(), 0), GmStatus::ErrArgument);
        assert_eq!(gm_wii_banner_size(banner, 100000.0, &mut w, &mut h), GmStatus::ErrArgument);
        assert_eq!((w, h), (0, 0));
        gm_wii_banner_free(banner);
    }
}

#[test]
fn skipped_panes_and_missing_textures_are_reported() {
    unsafe {
        let (_, banner) = open(&case("skipped"));
        assert_eq!(gm_wii_banner_count(banner, GmWiiBannerCount::SkippedWindows as u32), 1);
        assert_eq!(gm_wii_banner_count(banner, GmWiiBannerCount::SkippedText as u32), 1);
        assert_eq!(gm_wii_banner_count(banner, GmWiiBannerCount::SkippedBounds as u32), 1);
        gm_wii_banner_free(banner);
        let (_, banner) = open(&case("missing-texture"));
        assert_eq!(gm_wii_banner_count(banner, GmWiiBannerCount::MissingTextures as u32), 1);
        assert_eq!(CStr::from_ptr(gm_wii_banner_missing_texture(banner, 0)).to_str().unwrap(), "gone.tpl");
        assert!(gm_wii_banner_missing_texture(banner, 1).is_null());
        gm_wii_banner_free(banner);
    }
}

#[test]
fn a_banner_opens_from_an_opening_bnr_and_from_an_rvz() {
    let bin = case("pane");
    let bnr = opening(&[("en", "Test")], None, Some(&bin), |a| a);
    let (status, banner) = open(&bnr);
    assert_eq!(status, GmStatus::Ok);
    unsafe { gm_wii_banner_free(banner) };
    let part = partition_data("RZDE01", "Header Title", &[PartFile("opening.bnr", &bnr)], None, 0x7C00);
    let disc = rvz(&part, &WiiRvzOptions::default()).bytes;
    let (status, banner) = open(&disc);
    assert_eq!(status, GmStatus::Ok);
    unsafe { gm_wii_banner_free(banner) };
    // The same through a read callback, which an RVZ is read through in place.
    unsafe extern "C" fn read(user: *mut c_void, offset: u64, buf: *mut u8, len: usize) -> i32 {
        let data = unsafe { &*(user as *const Vec<u8>) };
        let end = offset as usize + len;
        if end > data.len() {
            return GmReadResult::OutOfRange as i32;
        }
        unsafe { std::ptr::copy_nonoverlapping(data[offset as usize..end].as_ptr(), buf, len) };
        GmReadResult::Ok as i32
    }
    let mut out = null_mut();
    let status = unsafe {
        gm_wii_banner_read_callback(disc.len() as u64, Some(read), &disc as *const Vec<u8> as *mut c_void, &mut out)
    };
    assert_eq!(status, GmStatus::Ok);
    unsafe { gm_wii_banner_free(out) };
    // No banner in the opening.bnr, no banner at all, and a failing source.
    let no_banner = opening(&[("en", "Test")], None, None, |a| a);
    assert_eq!(open(&no_banner).0, GmStatus::NotFound);
    assert_eq!(open(&[1, 2, 3]).0, GmStatus::NotFound);
    assert_eq!(open(&[]).0, GmStatus::NotFound);
    unsafe extern "C" fn failing(_: *mut c_void, _: u64, _: *mut u8, _: usize) -> i32 {
        GmReadResult::Error as i32
    }
    let status = unsafe { gm_wii_banner_read_callback(1000, Some(failing), null_mut(), &mut out) };
    assert_eq!(status, GmStatus::ErrIo);
    assert!(out.is_null());
}

#[cfg(unix)]
#[test]
fn a_banner_opens_from_a_file_descriptor() {
    use std::os::fd::AsRawFd;
    let path = std::env::temp_dir().join(format!("gm-wii-banner-{}.bin", std::process::id()));
    std::fs::write(&path, case("pane")).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let mut out = null_mut();
    let status = unsafe { gm_wii_banner_read_fd(file.as_raw_fd(), &mut out) };
    std::fs::remove_file(&path).unwrap();
    assert_eq!(status, GmStatus::Ok);
    unsafe { gm_wii_banner_free(out) };
    assert_eq!(unsafe { gm_wii_banner_read_fd(-1, &mut out) }, GmStatus::ErrArgument);
}

#[test]
fn null_arguments_are_refused_and_null_handles_are_neutral() {
    unsafe {
        assert_eq!(gm_wii_banner_read_bytes(std::ptr::null(), 5, null_mut()), GmStatus::ErrArgument);
        let mut out = null_mut();
        assert_eq!(gm_wii_banner_read_bytes(std::ptr::null(), 5, &mut out), GmStatus::ErrArgument);
        assert_eq!(gm_wii_banner_read_callback(10, None, null_mut(), &mut out), GmStatus::ErrArgument);
        assert_eq!(gm_wii_banner_count(std::ptr::null(), 0), 0);
        assert!(gm_wii_banner_missing_texture(std::ptr::null(), 0).is_null());
        let (mut w, mut h) = (0, 0);
        assert_eq!(gm_wii_banner_size(std::ptr::null(), 1.0, &mut w, &mut h), GmStatus::ErrArgument);
        assert_eq!(gm_wii_banner_render(std::ptr::null(), 0.0, 1.0, null_mut(), 0), GmStatus::ErrArgument);
        gm_wii_banner_free(null_mut());
    }
}
