// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Banner scenes: the 3DS HOME-menu banner of a file (a bare CBMD or CGFX, or a decrypted `.3ds`, `.cia`, `.cxi`) parsed
//! into a renderer-neutral scene, through opaque [`GmScene`] handles. The caller does all I/O, as everywhere else.

use std::ffi::{CString, c_char, c_int, c_void};
use std::sync::OnceLock;

use banner3ds_scene::BannerScene;
use banner3ds_scene::dump::scene_json;
use banner3ds_scene::source::{self, SceneError};
use gamemeta_core::{ByteSource, SliceSource};

use crate::{CallbackSource, GmReadFn, GmStatus, Tracked, fd, guard};

/// A parsed banner: opaque, freed with [`gm_scene_free`].
#[derive(Debug)]
pub struct GmScene {
    scene: BannerScene,
    texture_names: Vec<Option<CString>>,
    unsupported: Vec<Option<CString>>,
    json: OnceLock<Option<CString>>,
}

impl GmScene {
    /// A handle for `scene`.
    pub fn new(scene: BannerScene) -> Self {
        let c = |s: &String| CString::new(s.as_str()).ok();
        GmScene {
            texture_names: scene.textures.keys().map(c).collect(),
            unsupported: scene.unsupported.iter().map(c).collect(),
            json: OnceLock::new(),
            scene,
        }
    }

    /// How many of `what` (a [`GmSceneCount`] value) the scene has; 0 for an unknown `what`.
    pub fn count(&self, what: u32) -> u32 {
        let s = &self.scene;
        let n = match what {
            w if w == GmSceneCount::Models as u32 => s.models.len(),
            w if w == GmSceneCount::Meshes as u32 => s.models.iter().map(|m| m.meshes.len()).sum(),
            w if w == GmSceneCount::Textures as u32 => s.textures.len(),
            w if w == GmSceneCount::Lights as u32 => s.lights.len(),
            w if w == GmSceneCount::Cameras as u32 => s.cameras.len(),
            w if w == GmSceneCount::SkeletalAnimations as u32 => s.animations.skeletal.len(),
            w if w == GmSceneCount::MaterialAnimations as u32 => s.animations.material.len(),
            w if w == GmSceneCount::VisibilityAnimations as u32 => s.animations.visibility.len(),
            w if w == GmSceneCount::Unsupported as u32 => s.unsupported.len(),
            _ => 0,
        };
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// The scene as JSON, as [`gm_scene_json`] returns it.
    pub fn json(&self) -> Option<&CString> {
        self.json.get_or_init(|| guard(None, || CString::new(scene_json(&self.scene)).ok())).as_ref()
    }

    /// The parsed scene, for Rust callers of this crate (the JNI binding).
    pub fn scene(&self) -> &BannerScene {
        &self.scene
    }
}

/// What [`gm_scene_count`] counts.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmSceneCount {
    /// Models.
    Models = 0,
    /// Meshes, over all models.
    Meshes = 1,
    /// Textures, by name ([`gm_scene_texture_name`]).
    Textures = 2,
    /// Fragment lights.
    Lights = 3,
    /// Cameras the file defines.
    Cameras = 4,
    /// Bone animations.
    SkeletalAnimations = 5,
    /// Material animations (texture coordinates, colours).
    MaterialAnimations = 6,
    /// Mesh visibility animations.
    VisibilityAnimations = 7,
    /// Features the file uses that the scene does not model ([`gm_scene_unsupported`]).
    Unsupported = 8,
}

/// The scene of `source`, or why there is none: the core of every scene read entry point, also used by the JNI binding.
pub fn read_scene<S: ByteSource>(source: S) -> Result<GmScene, GmStatus> {
    let mut tracked = Tracked::new(source);
    match source::read(&mut tracked) {
        Ok(scene) => Ok(GmScene::new(scene)),
        Err(_) if tracked.failed() => Err(GmStatus::ErrIo),
        // Encrypted and damaged files give nothing to draw either; the scene library never decrypts.
        Err(SceneError::NotFound | SceneError::Encrypted | SceneError::Damaged) => Err(GmStatus::NotFound),
    }
}

/// Stores the handle of a successful read in `*out` (null otherwise), catching panics.
unsafe fn store_scene(out: *mut *mut GmScene, read: impl FnOnce() -> Result<GmScene, GmStatus>) -> GmStatus {
    if out.is_null() {
        return GmStatus::ErrArgument;
    }
    // SAFETY: `out` is non-null and the caller promises it is writable.
    unsafe { *out = std::ptr::null_mut() };
    guard(GmStatus::ErrPanic, || match read() {
        Ok(scene) => {
            // SAFETY: checked non-null above.
            unsafe { *out = Box::into_raw(Box::new(scene)) };
            GmStatus::Ok
        }
        Err(status) => status,
    })
}

/// Reads the banner scene of the `len` bytes at `data`: a CBMD or CGFX file, or a decrypted 3DS file (NCSD, CIA, NCCH)
/// whose ExeFS holds the banner. On [`GmStatus::Ok`] `*out` is a handle to free with [`gm_scene_free`]; an encrypted
/// file, a file without a banner and a banner that does not parse are [`GmStatus::NotFound`].
///
/// Safety: `data` points to `len` readable bytes (may be null when `len` is 0); `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_scene_read_bytes(data: *const u8, len: usize, out: *mut *mut GmScene) -> GmStatus {
    if data.is_null() && len != 0 {
        if !out.is_null() {
            // SAFETY: non-null and writable by contract.
            unsafe { *out = std::ptr::null_mut() };
        }
        return GmStatus::ErrArgument;
    }
    // SAFETY: the caller promises `len` readable bytes at a non-null `data`; an empty slice needs no pointer.
    let bytes = if len == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(data, len) } };
    // SAFETY: forwarded caller contract.
    unsafe { store_scene(out, || read_scene(SliceSource::new(bytes))) }
}

/// Like [`gm_scene_read_bytes`] for a file of `size` bytes read through `read` (see `gm_read_callback`).
///
/// Safety: `read` is a valid function honouring the [`GmReadFn`] contract; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_scene_read_callback(
    size: u64,
    read: Option<unsafe extern "C" fn(*mut c_void, u64, *mut u8, usize) -> i32>,
    user_data: *mut c_void,
    out: *mut *mut GmScene,
) -> GmStatus {
    let Some(read) = read else {
        if !out.is_null() {
            // SAFETY: non-null and writable by contract.
            unsafe { *out = std::ptr::null_mut() };
        }
        return GmStatus::ErrArgument;
    };
    let read: GmReadFn = read;
    // SAFETY: forwarded caller contract.
    unsafe { store_scene(out, || read_scene(CallbackSource { size, read, user_data })) }
}

/// Like [`gm_scene_read_bytes`] for an open, readable, seekable file descriptor read with positioned reads (`pread`): its
/// file offset is not moved and it is not closed. [`GmStatus::ErrUnsupported`] where there are no POSIX descriptors.
///
/// Safety: `fd` stays open for the call; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_scene_read_fd(fd: c_int, out: *mut *mut GmScene) -> GmStatus {
    // SAFETY: forwarded caller contract.
    unsafe { store_scene(out, || fd::scene_fd(fd)) }
}

/// Frees a scene handle. Null is ignored.
///
/// Safety: `scene` came from a scene read entry point and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_scene_free(scene: *mut GmScene) {
    if !scene.is_null() {
        // SAFETY: the caller hands back a pointer from Box::into_raw, once.
        guard((), || drop(unsafe { Box::from_raw(scene) }));
    }
}

/// How many of `what` ([`GmSceneCount`]) the scene has; 0 for a null handle or an unknown `what`.
///
/// Safety: `scene` is a live handle or null (every accessor below has the same contract).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_scene_count(scene: *const GmScene, what: u32) -> u32 {
    // SAFETY: accessor contract.
    unsafe { scene.as_ref() }.map_or(0, |s| s.count(what))
}

/// The whole scene as JSON (UTF-8, NUL-terminated; sorted keys; textures as size and SHA-256 of their RGBA bytes): the
/// text the parity tests compare against the original implementation. Owned by the handle, valid until it is freed.
/// Null for a null handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_scene_json(scene: *const GmScene) -> *const c_char {
    // SAFETY: accessor contract.
    let Some(s) = (unsafe { scene.as_ref() }) else { return std::ptr::null() };
    s.json().map_or(std::ptr::null(), |j| j.as_ptr())
}

/// The name of the `index`th texture (sorted by name), or null when out of range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_scene_texture_name(scene: *const GmScene, index: u32) -> *const c_char {
    // SAFETY: accessor contract.
    let Some(s) = (unsafe { scene.as_ref() }) else { return std::ptr::null() };
    s.texture_names.get(index as usize).and_then(|n| n.as_ref()).map_or(std::ptr::null(), |n| n.as_ptr())
}

/// The `index`th texture's RGBA8 pixels (top-down rows, `width * height * 4` bytes) and its size, or null (sizes 0) when
/// out of range. `width` and `height` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_scene_texture_pixels(
    scene: *const GmScene,
    index: u32,
    width: *mut u32,
    height: *mut u32,
) -> *const u8 {
    // SAFETY: accessor contract.
    let image = unsafe { scene.as_ref() }.and_then(|s| s.scene.textures.values().nth(index as usize));
    let (w, h, ptr) = image.map_or((0, 0, std::ptr::null()), |im| (im.width, im.height, im.pixels.as_ptr()));
    // SAFETY: the caller passes writable pointers or null.
    unsafe {
        if let Some(p) = width.as_mut() {
            *p = w;
        }
        if let Some(p) = height.as_mut() {
            *p = h;
        }
    }
    ptr
}

/// The `index`th unsupported-feature flag (sorted), e.g. `dot3-combiner`, or null when out of range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_scene_unsupported(scene: *const GmScene, index: u32) -> *const c_char {
    // SAFETY: accessor contract.
    let Some(s) = (unsafe { scene.as_ref() }) else { return std::ptr::null() };
    s.unsupported.get(index as usize).and_then(|n| n.as_ref()).map_or(std::ptr::null(), |n| n.as_ptr())
}
