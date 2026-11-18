// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A headless OpenGL ES 3 context for the GL renderer's tests and bench: EGL on Mesa's surfaceless platform (no X, no
//! Wayland, no GPU needed) with an RGBA8 + depth 24 pbuffer, current on the creating thread. [`prefer_mesa_software`] forces
//! the software rasteriser llvmpipe (deterministic pixels) unless the environment already chose.

use std::ffi::{CStr, CString, c_void};
use std::path::Path;

use khronos_egl as egl;

type ReadPixels = unsafe extern "system" fn(i32, i32, i32, i32, u32, u32, *mut c_void);
type ClearColor = unsafe extern "system" fn(f32, f32, f32, f32);
type Clear = unsafe extern "system" fn(u32);
type Finish = unsafe extern "system" fn();
type GetString = unsafe extern "system" fn(u32) -> *const u8;

const EGL_PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

/// `LIBGL_ALWAYS_SOFTWARE=1`, and glvnd pointed at Mesa's EGL vendor file, unless the environment sets them.
///
/// # Safety
/// Changes the process environment: call it before any other thread exists (a `harness = false` test's `main`, a bench's).
pub unsafe fn prefer_mesa_software() {
    // SAFETY: the caller promises no other thread reads the environment now.
    unsafe {
        if std::env::var_os("LIBGL_ALWAYS_SOFTWARE").is_none() {
            std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
        }
        let mesa = "/usr/share/glvnd/egl_vendor.d/50_mesa.json";
        if std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_none() && Path::new(mesa).exists() {
            std::env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", mesa);
        }
    }
}

/// A current GLES 3 context with a pbuffer. Use it from the thread that made it.
pub struct Headless {
    egl: egl::DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    context: egl::Context,
    surface: egl::Surface,
    read_pixels: ReadPixels,
    clear_color: ClearColor,
    clear: Clear,
    finish: Finish,
    /// `GL_RENDERER / GL_VERSION`.
    pub renderer: String,
}

impl std::fmt::Debug for Headless {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Headless").field("renderer", &self.renderer).finish()
    }
}

impl Headless {
    /// A context with a `width` x `height` pbuffer, made current; why not, when this host has no EGL or GLES 3.
    pub fn new(width: i32, height: i32) -> Result<Headless, String> {
        // SAFETY: libEGL is the system's EGL.
        let egl =
            unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }.map_err(|e| format!("no libEGL: {e}"))?;
        // SAFETY: the surfaceless platform takes no native display.
        let display = unsafe {
            egl.get_platform_display(EGL_PLATFORM_SURFACELESS_MESA, egl::DEFAULT_DISPLAY, &[egl::ATTRIB_NONE])
        }
        .map_err(|e| format!("no surfaceless display: {e}"))?;
        egl.initialize(display).map_err(|e| format!("eglInitialize: {e}"))?;
        egl.bind_api(egl::OPENGL_ES_API).map_err(|e| format!("eglBindAPI: {e}"))?;
        #[rustfmt::skip]
        let attributes = [
            egl::RENDERABLE_TYPE, egl::OPENGL_ES3_BIT, egl::SURFACE_TYPE, egl::PBUFFER_BIT, egl::RED_SIZE, 8,
            egl::GREEN_SIZE, 8, egl::BLUE_SIZE, 8, egl::ALPHA_SIZE, 8, egl::DEPTH_SIZE, 24, egl::NONE,
        ];
        let config = egl
            .choose_first_config(display, &attributes)
            .map_err(|e| format!("eglChooseConfig: {e}"))?
            .ok_or("no ES3 pbuffer config")?;
        let context = egl
            .create_context(display, config, None, &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE])
            .map_err(|e| format!("eglCreateContext: {e}"))?;
        let surface = egl
            .create_pbuffer_surface(display, config, &[egl::WIDTH, width, egl::HEIGHT, height, egl::NONE])
            .map_err(|e| format!("eglCreatePbufferSurface: {e}"))?;
        egl.make_current(display, Some(surface), Some(surface), Some(context))
            .map_err(|e| format!("eglMakeCurrent: {e}"))?;
        let get = |name: &str| egl.get_proc_address(name).map_or(std::ptr::null(), |f| f as *const c_void);
        let (rp, cc, cl, fi, gs) =
            (get("glReadPixels"), get("glClearColor"), get("glClear"), get("glFinish"), get("glGetString"));
        if [rp, cc, cl, fi, gs].iter().any(|p| p.is_null()) {
            return Err("missing a GL entry point".into());
        }
        // SAFETY: these are the GL functions of those names.
        let (read_pixels, clear_color, clear, finish, get_string) = unsafe {
            (
                std::mem::transmute::<*const c_void, ReadPixels>(rp),
                std::mem::transmute::<*const c_void, ClearColor>(cc),
                std::mem::transmute::<*const c_void, Clear>(cl),
                std::mem::transmute::<*const c_void, Finish>(fi),
                std::mem::transmute::<*const c_void, GetString>(gs),
            )
        };
        let text = |e: u32| {
            // SAFETY: glGetString returns a static NUL-terminated string, or null.
            let p = unsafe { get_string(e) };
            if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p.cast()) }.to_string_lossy().into_owned() }
        };
        let renderer = format!("{} / {}", text(0x1F01), text(0x1F02));
        Ok(Headless { egl, display, context, surface, read_pixels, clear_color, clear, finish, renderer })
    }

    /// The address of GL function `name` (null when missing): the loader a GL renderer takes.
    pub fn proc_address(&self, name: &CStr) -> *const c_void {
        name.to_str().ok().and_then(|n| self.egl.get_proc_address(n)).map_or(std::ptr::null(), |f| f as *const c_void)
    }

    /// [`proc_address`](Self::proc_address) for a Rust string.
    pub fn proc(&self, name: &str) -> *const c_void {
        CString::new(name).map_or(std::ptr::null(), |n| self.proc_address(&n))
    }

    /// Waits for the GPU (glFinish).
    pub fn finish(&self) {
        // SAFETY: the context is current.
        unsafe { (self.finish)() }
    }

    /// The `width` x `height` region at (`x`, `y`) (GL window coordinates) as RGBA, top row first.
    pub fn read(&self, x: i32, y: i32, width: i32, height: i32) -> Vec<u8> {
        let mut buf = vec![0u8; (width * height * 4) as usize];
        // SAFETY: the context is current and `buf` holds the region.
        unsafe { (self.read_pixels)(x, y, width, height, 0x1908, 0x1401, buf.as_mut_ptr().cast()) };
        let row = (width * 4) as usize;
        let mut out = vec![0u8; buf.len()];
        for r in 0..height as usize {
            out[r * row..(r + 1) * row]
                .copy_from_slice(&buf[(height as usize - 1 - r) * row..(height as usize - r) * row]);
        }
        out
    }

    /// Clears the whole surface (colour and depth) to `rgba`.
    pub fn fill(&self, rgba: [f32; 4]) {
        // SAFETY: the context is current.
        unsafe {
            (self.clear_color)(rgba[0], rgba[1], rgba[2], rgba[3]);
            (self.clear)(0x4000 | 0x100);
        }
    }
}

impl Drop for Headless {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_surface(self.display, self.surface);
        let _ = self.egl.destroy_context(self.display, self.context);
        let _ = self.egl.terminate(self.display);
        let _ = self.egl.release_thread();
    }
}
