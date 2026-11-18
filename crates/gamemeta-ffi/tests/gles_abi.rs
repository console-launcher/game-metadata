// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The GLES entry points of the C ABI on a real context (EGL surfaceless, llvmpipe; see banner3ds-gles' headless test):
//! create, draw and destroy draw what the library draws. `harness = false`: one context on the main thread.

use std::ffi::{CStr, c_char, c_void};
use std::ptr::null_mut;

use banner3ds_gles::{Gl, Renderer, Viewport};
use gamemeta::*;
use gamemeta_fixtures::headless::{self, Headless};

unsafe extern "C" fn load(user: *mut c_void, name: *const c_char) -> *const c_void {
    // SAFETY: `user` is the test's Headless and `name` a NUL-terminated GL name.
    unsafe { (*(user as *const Headless)).proc_address(CStr::from_ptr(name)) }
}

fn check(ctx: &Headless) -> Result<(), String> {
    let bytes =
        std::fs::read(format!("{}/../../fixtures/scene/features/bone-anim.cbmd", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let mut scene = null_mut();
    let mut gles = null_mut();
    unsafe {
        if gm_scene_read_bytes(bytes.as_ptr(), bytes.len(), &mut scene) != GmStatus::Ok {
            return Err("scene".into());
        }
        let user = ctx as *const Headless as *mut c_void;
        let status = gm_gles_create(scene, Some(load), user, &mut gles);
        if status != GmStatus::Ok || gles.is_null() {
            return Err(format!("gm_gles_create: {status:?}"));
        }
        // The scene handle may go before the renderer: it holds its own copy.
        gm_scene_free(scene);
        let status = gm_gles_draw(gles, 30.0, 0, 0, 400, 240);
        ctx.finish();
        let ours = ctx.read(0, 0, 400, 240);
        gm_gles_destroy(gles);
        if status != GmStatus::Ok {
            return Err(format!("gm_gles_draw: {status:?}"));
        }
        let scene = banner3ds_scene::Cbmd::parse(&bytes).and_then(|c| c.scene()).unwrap();
        let gl = Gl::load(|n| ctx.proc_address(n)).map_err(|m| format!("{m:?}"))?;
        let mut r = Renderer::new(&scene, gl).map_err(|e| format!("{e:?}"))?;
        r.draw(30.0, Viewport::of(400, 240));
        ctx.finish();
        let direct = ctx.read(0, 0, 400, 240);
        r.destroy();
        if ours != direct {
            return Err("the C ABI drew a different frame".into());
        }
        if ours.iter().all(|&b| b == 0) {
            return Err("nothing drawn".into());
        }
    }
    Ok(())
}

fn main() {
    if !cfg!(target_os = "linux") {
        println!("gles_abi: skipped (the EGL surfaceless check runs on Linux)");
        return;
    }
    // SAFETY: single-threaded here, before EGL loads.
    unsafe { headless::prefer_mesa_software() };
    let ctx = Headless::new(512, 512).unwrap_or_else(|e| {
        eprintln!("gles_abi: no context: {e}");
        std::process::exit(1)
    });
    match check(&ctx) {
        Ok(()) => {
            println!("test gles_create_draw_destroy_through_the_c_abi ... ok\ntest result: ok. 1 passed; 0 failed")
        }
        Err(e) => {
            println!(
                "test gles_create_draw_destroy_through_the_c_abi ... FAILED\n{e}\ntest result: FAILED. 0 passed; 1 failed"
            );
            drop(ctx);
            std::process::exit(101);
        }
    }
}
