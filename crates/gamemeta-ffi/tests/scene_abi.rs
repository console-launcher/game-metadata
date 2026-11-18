// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The scene entry points of the C ABI, as a C caller would use them.

use std::ffi::{CStr, c_char, c_void};
use std::ptr::{null, null_mut};

use gamemeta::*;
use gamemeta_fixtures as fx;

fn feature(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../fixtures/scene/features/{name}.cbmd", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn text(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned())
}

fn read_bytes(bytes: &[u8]) -> (GmStatus, *mut GmScene) {
    let mut out = null_mut();
    let status = unsafe { gm_scene_read_bytes(bytes.as_ptr(), bytes.len(), &mut out) };
    (status, out)
}

#[test]
fn a_banner_reads_through_every_scene_accessor() {
    let (status, scene) = read_bytes(&feature("tex-etc1"));
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert!(gm_scene_count(scene, GmSceneCount::Models as u32) >= 1);
        assert!(gm_scene_count(scene, GmSceneCount::Meshes as u32) >= 1);
        let textures = gm_scene_count(scene, GmSceneCount::Textures as u32);
        assert!(textures >= 1);
        assert!(text(gm_scene_texture_name(scene, 0)).is_some_and(|n| !n.is_empty()));
        let (mut w, mut h) = (0, 0);
        let pixels = gm_scene_texture_pixels(scene, 0, &mut w, &mut h);
        assert!(!pixels.is_null() && w >= 8 && h >= 8);
        // Out of range indices and unknown counters are neutral.
        assert!(gm_scene_texture_name(scene, textures).is_null());
        assert!(gm_scene_texture_pixels(scene, textures, &mut w, &mut h).is_null());
        assert_eq!((w, h), (0, 0));
        assert_eq!(gm_scene_count(scene, 99), 0);
        let json = text(gm_scene_json(scene)).unwrap();
        assert!(json.starts_with('{') && json.contains("\"models\":") && json.contains("\"textures\":"));
        // The same text each time, owned by the handle.
        assert_eq!(gm_scene_json(scene), gm_scene_json(scene));
        gm_scene_free(scene);
    }
}

#[test]
fn unsupported_features_are_listed() {
    let bytes =
        std::fs::read(format!("{}/../../fixtures/scene/cases/combiner-dot3.cgfx", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let (status, scene) = read_bytes(&bytes);
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert_eq!(gm_scene_count(scene, GmSceneCount::Unsupported as u32), 1);
        assert_eq!(text(gm_scene_unsupported(scene, 0)).as_deref(), Some("dot3-combiner"));
        assert!(gm_scene_unsupported(scene, 1).is_null());
        gm_scene_free(scene);
    }
}

#[test]
fn a_3ds_file_gives_its_banner_scene() {
    let file = fx::ncsd(&fx::plain_ncch(&fx::exefs(&[("banner", &feature("alpha-test"))])));
    let (status, scene) = read_bytes(&file);
    assert_eq!(status, GmStatus::Ok);
    unsafe {
        assert!(gm_scene_count(scene, GmSceneCount::Models as u32) >= 1);
        gm_scene_free(scene);
    }
}

#[test]
fn nothing_found_failed_sources_and_bad_arguments() {
    for bytes in [&b""[..], b"hello", &fx::sample_3ds()[..0x200]] {
        let (status, scene) = read_bytes(bytes);
        assert_eq!(status, GmStatus::NotFound);
        assert!(scene.is_null());
    }
    let mut out = std::ptr::dangling_mut::<GmScene>();
    unsafe {
        assert_eq!(gm_scene_read_bytes(null(), 5, &mut out), GmStatus::ErrArgument);
        assert!(out.is_null());
        assert_eq!(gm_scene_read_bytes(null(), 0, null_mut()), GmStatus::ErrArgument);
        assert_eq!(gm_scene_read_callback(100, None, null_mut(), &mut out), GmStatus::ErrArgument);
        assert_eq!(gm_scene_read_callback(100, Some(failing), null_mut(), &mut out), GmStatus::ErrIo);
        assert!(out.is_null());
        // Null handles are safe everywhere.
        gm_scene_free(null_mut());
        assert_eq!(gm_scene_count(null(), 0), 0);
        assert!(gm_scene_json(null()).is_null());
        assert!(gm_scene_texture_name(null(), 0).is_null());
        assert!(gm_scene_texture_pixels(null(), 0, null_mut(), null_mut()).is_null());
        assert!(gm_scene_unsupported(null(), 0).is_null());
    }
}

unsafe extern "C" fn from_vec(user: *mut c_void, offset: u64, buf: *mut u8, len: usize) -> i32 {
    let bytes = unsafe { &*(user as *const Vec<u8>) };
    let Some(src) = bytes.get(offset as usize..offset as usize + len) else { return GmReadResult::OutOfRange as i32 };
    unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), buf, len) };
    GmReadResult::Ok as i32
}

unsafe extern "C" fn failing(_: *mut c_void, _: u64, _: *mut u8, _: usize) -> i32 {
    GmReadResult::Error as i32
}

#[test]
fn a_callback_source_reads() {
    let bytes = feature("alpha-test");
    let mut out = null_mut();
    let user = &bytes as *const Vec<u8> as *mut c_void;
    unsafe {
        assert_eq!(gm_scene_read_callback(bytes.len() as u64, Some(from_vec), user, &mut out), GmStatus::Ok);
        assert!(gm_scene_count(out, GmSceneCount::Models as u32) >= 1);
        gm_scene_free(out);
    }
}

#[cfg(unix)]
#[test]
fn a_file_descriptor_reads_without_moving_it() {
    use std::io::{Seek, SeekFrom};
    use std::os::fd::AsRawFd;
    let path = std::env::temp_dir().join(format!("gamemeta-scene-fd-{}.bnr", std::process::id()));
    std::fs::write(&path, feature("alpha-test")).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    file.seek(SeekFrom::Start(9)).unwrap();
    let mut out = null_mut();
    assert_eq!(unsafe { gm_scene_read_fd(file.as_raw_fd(), &mut out) }, GmStatus::Ok);
    unsafe { gm_scene_free(out) };
    assert_eq!(file.stream_position().unwrap(), 9);
    assert_eq!(unsafe { gm_scene_read_fd(-1, &mut out) }, GmStatus::ErrArgument);
    drop(file);
    std::fs::remove_file(&path).unwrap();
}
