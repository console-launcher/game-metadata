// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Rendering a banner scene: the CPU renderer into a caller buffer, and the GLES renderer in the caller's current context
//! through the caller's GL loader. Frames count 60 to the second, looped by the banner's clips.

use std::ffi::{CStr, c_char, c_void};
use std::mem::ManuallyDrop;

use banner3ds_gles::{Gl, Renderer, Viewport};
use banner3ds_scene::BannerScene;
use banner3ds_scene::draw::Player;

use crate::{GmScene, GmStatus, guard};

/// Loads a GL function: returns the address of the OpenGL ES 3.0 function `name` (e.g. `glDrawElements`; NUL-terminated)
/// for the caller's context, or null. Called only during [`gm_gles_create`].
pub type GmGlLoadFn = unsafe extern "C" fn(user_data: *mut c_void, name: *const c_char) -> *const c_void;

/// A scene's GLES resources in one context: opaque, made by [`gm_gles_create`], deleted by [`gm_gles_destroy`]. It holds its
/// own copy of the scene, so the scene handle may be freed first.
pub struct GmGles {
    // Declared first: the renderer borrows `scene` and must go before it.
    renderer: ManuallyDrop<Renderer<'static>>,
    scene: *mut BannerScene,
}

impl std::fmt::Debug for GmGles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GmGles")
    }
}

/// The banner's loop length in frames (60 to the second); 0 when it does not animate, or for a null handle.
///
/// Safety: `scene` is a live handle or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_render_frames(scene: *const GmScene) -> f32 {
    // SAFETY: caller contract.
    let Some(s) = (unsafe { scene.as_ref() }) else { return 0.0 };
    guard(0.0, || Player::new(s.scene()).frames)
}

/// Renders the banner at `frame` (60 to the second) with the CPU renderer into `out`: `width * height` RGBA8 pixels, top
/// row first, `out_len` bytes (must be `width * height * 4`), through HOME's view over transparent black.
/// [`GmStatus::ErrArgument`] for a null pointer, a zero or too large size (over 64 Mpx) or a wrong `out_len`, writing
/// nothing.
///
/// Safety: `scene` is a live handle or null; `out` points to `out_len` writable bytes or is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_render_soft(
    scene: *const GmScene,
    frame: f32,
    width: u32,
    height: u32,
    out: *mut u8,
    out_len: usize,
) -> GmStatus {
    // SAFETY: caller contract.
    let Some(s) = (unsafe { scene.as_ref() }) else { return GmStatus::ErrArgument };
    if out.is_null() {
        return GmStatus::ErrArgument;
    }
    // SAFETY: the caller promises `out_len` writable bytes at a non-null `out`.
    let buf = unsafe { std::slice::from_raw_parts_mut(out, out_len) };
    guard(GmStatus::ErrPanic, || match banner3ds_soft::render(s.scene(), frame, width, height, buf) {
        Ok(()) => GmStatus::Ok,
        Err(_) => GmStatus::ErrArgument,
    })
}

/// Creates the GLES resources (programs, buffers, textures) for `scene` in the OpenGL ES 3.0 (or WebGL 2) context current on
/// the calling thread, loading every GL function through `load` (a [`GmGlLoadFn`]). On [`GmStatus::Ok`] `*out` is a handle for
/// [`gm_gles_draw`] and [`gm_gles_destroy`]; [`GmStatus::ErrUnsupported`] when `load` misses a function,
/// [`GmStatus::NotFound`] when nothing in the scene could be set up (no drawable mesh, or no program compiled).
///
/// Safety: `scene` is a live handle or null; `load` returns real GL functions of the current context; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_gles_create(
    scene: *const GmScene,
    load: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *const c_void>,
    user_data: *mut c_void,
    out: *mut *mut GmGles,
) -> GmStatus {
    if out.is_null() {
        return GmStatus::ErrArgument;
    }
    // SAFETY: non-null and writable by contract.
    unsafe { *out = std::ptr::null_mut() };
    // SAFETY: caller contract.
    let (Some(s), Some(load)) = (unsafe { scene.as_ref() }, load) else { return GmStatus::ErrArgument };
    let load: GmGlLoadFn = load;
    guard(GmStatus::ErrPanic, || {
        // SAFETY: the caller promises `load` gives this context's GL functions.
        let gl = match unsafe { Gl::load(|name: &CStr| load(user_data, name.as_ptr())) } {
            Ok(gl) => gl,
            Err(_) => return GmStatus::ErrUnsupported,
        };
        let scene: *mut BannerScene = Box::into_raw(Box::new(s.scene().clone()));
        // SAFETY: `scene` lives until gm_gles_destroy frees it, after the renderer; the context is current by contract.
        match unsafe { Renderer::new(&*scene, gl) } {
            Ok(renderer) => {
                // SAFETY: checked non-null above.
                unsafe { *out = Box::into_raw(Box::new(GmGles { renderer: ManuallyDrop::new(renderer), scene })) };
                GmStatus::Ok
            }
            Err(_) => {
                // SAFETY: from Box::into_raw above; the failed renderer released everything it made.
                drop(unsafe { Box::from_raw(scene) });
                GmStatus::NotFound
            }
        }
    })
}

/// Draws the banner at `frame` (60 to the second) through HOME's view into the viewport (`x`, `y` of its bottom-left corner
/// in GL window coordinates, `width` x `height`) of the framebuffer bound in the renderer's context, clearing that viewport
/// to transparent black first. [`GmStatus::ErrArgument`] for a null handle or an empty viewport.
///
/// Safety: `gles` is a live handle or null, and its context is current on the calling thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_gles_draw(
    gles: *mut GmGles,
    frame: f32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> GmStatus {
    // SAFETY: caller contract.
    let Some(g) = (unsafe { gles.as_mut() }) else { return GmStatus::ErrArgument };
    if width <= 0 || height <= 0 {
        return GmStatus::ErrArgument;
    }
    guard(GmStatus::ErrPanic, || {
        // SAFETY: the renderer's context is current by contract.
        unsafe { g.renderer.draw(frame, Viewport { x, y, width, height }) };
        GmStatus::Ok
    })
}

/// Deletes the handle's GL resources and frees it. Null is ignored.
///
/// Safety: `gles` came from [`gm_gles_create`] and is not used again; its context is current on the calling thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_gles_destroy(gles: *mut GmGles) {
    if gles.is_null() {
        return;
    }
    guard((), || {
        // SAFETY: the caller hands back a pointer from Box::into_raw, once.
        let mut g = unsafe { Box::from_raw(gles) };
        // SAFETY: taken once; the context is current by contract.
        unsafe { ManuallyDrop::take(&mut g.renderer).destroy() };
        // SAFETY: the renderer borrowing it is gone; from Box::into_raw in gm_gles_create.
        drop(unsafe { Box::from_raw(g.scene) });
    });
}
