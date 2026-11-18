// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The rendering entry points of the C ABI that need no GL context: the CPU renderer into a caller buffer, and how the
//! GLES entry points refuse bad arguments and a loader that finds nothing.

use std::ffi::{c_char, c_void};
use std::ptr::{null, null_mut};

use gamemeta::*;

fn scene(name: &str) -> *mut GmScene {
    let bytes =
        std::fs::read(format!("{}/../../fixtures/scene/features/{name}.cbmd", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let mut out = null_mut();
    assert_eq!(unsafe { gm_scene_read_bytes(bytes.as_ptr(), bytes.len(), &mut out) }, GmStatus::Ok);
    out
}

#[test]
fn the_cpu_renderer_draws_a_frame_into_the_callers_buffer_as_the_library_does() {
    let s = scene("bone-anim");
    let mut ours = vec![0u8; 400 * 240 * 4];
    unsafe {
        assert_eq!(gm_render_soft(s, 30.0, 400, 240, ours.as_mut_ptr(), ours.len()), GmStatus::Ok);
        let mut direct = vec![0u8; 400 * 240 * 4];
        banner3ds_soft::render((*s).scene(), 30.0, 400, 240, &mut direct).unwrap();
        assert_eq!(ours, direct);
        assert!(ours.iter().any(|&b| b != 0));
        gm_scene_free(s);
    }
}

#[test]
fn the_loop_length_is_the_players() {
    let (animated, still) = (scene("bone-anim"), scene("alpha-test"));
    unsafe {
        assert_eq!(gm_render_frames(animated), 120.0);
        assert_eq!(gm_render_frames(still), 0.0);
        assert_eq!(gm_render_frames(null()), 0.0);
        gm_scene_free(animated);
        gm_scene_free(still);
    }
}

#[test]
fn the_cpu_renderer_refuses_a_bad_size_or_buffer_and_writes_nothing() {
    let s = scene("alpha-test");
    let mut buf = vec![7u8; 16 * 16 * 4];
    unsafe {
        assert_eq!(gm_render_soft(null(), 0.0, 16, 16, buf.as_mut_ptr(), buf.len()), GmStatus::ErrArgument);
        assert_eq!(gm_render_soft(s, 0.0, 16, 16, null_mut(), buf.len()), GmStatus::ErrArgument);
        assert_eq!(gm_render_soft(s, 0.0, 16, 16, buf.as_mut_ptr(), buf.len() - 1), GmStatus::ErrArgument);
        assert_eq!(gm_render_soft(s, 0.0, 0, 16, buf.as_mut_ptr(), 0), GmStatus::ErrArgument);
        assert_eq!(gm_render_soft(s, 0.0, 1 << 16, 1 << 16, buf.as_mut_ptr(), buf.len()), GmStatus::ErrArgument);
        assert!(buf.iter().all(|&b| b == 7));
        gm_scene_free(s);
    }
}

unsafe extern "C" fn nothing(_user: *mut c_void, _name: *const c_char) -> *const c_void {
    null()
}

#[test]
fn gles_creation_refuses_null_arguments_and_a_loader_without_gl() {
    let s = scene("alpha-test");
    let mut out: *mut GmGles = std::ptr::dangling_mut::<GmGles>();
    unsafe {
        assert_eq!(gm_gles_create(null(), Some(nothing), null_mut(), &mut out), GmStatus::ErrArgument);
        assert!(out.is_null());
        out = std::ptr::dangling_mut::<GmGles>();
        assert_eq!(gm_gles_create(s, None, null_mut(), &mut out), GmStatus::ErrArgument);
        assert!(out.is_null());
        assert_eq!(gm_gles_create(s, Some(nothing), null_mut(), null_mut()), GmStatus::ErrArgument);
        out = std::ptr::dangling_mut::<GmGles>();
        assert_eq!(gm_gles_create(s, Some(nothing), null_mut(), &mut out), GmStatus::ErrUnsupported);
        assert!(out.is_null());
        assert_eq!(gm_gles_draw(null_mut(), 0.0, 0, 0, 16, 16), GmStatus::ErrArgument);
        gm_gles_destroy(null_mut());
        gm_scene_free(s);
    }
}
