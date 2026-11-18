// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wii channel banners: the animated scene of an RVZ disc, an `opening.bnr` or a `meta/banner.bin`, drawn on the CPU into a
//! caller buffer at any scale, through opaque [`GmWiiBanner`] handles. The caller does all I/O, as everywhere else; nothing is
//! decrypted.

use std::ffi::{CString, c_char, c_int, c_void};

use gamemeta_core::wii::banner::WiiBanner;
use gamemeta_core::{ByteSource, SliceSource};

use crate::{CallbackSource, GmReadFn, GmStatus, Tracked, fd, guard};

/// A channel banner: opaque, freed with [`gm_wii_banner_free`].
#[derive(Debug)]
pub struct GmWiiBanner {
    banner: WiiBanner,
    missing: Vec<Option<CString>>,
}

impl GmWiiBanner {
    /// A handle for `banner`.
    pub fn new(banner: WiiBanner) -> Self {
        let missing = banner.missing_textures().iter().map(|t| CString::new(t.as_str()).ok()).collect();
        GmWiiBanner { banner, missing }
    }

    /// The banner, for Rust callers of this crate (the JNI binding).
    pub fn banner(&self) -> &WiiBanner {
        &self.banner
    }

    /// How many of `what` (a [`GmWiiBannerCount`] value); 0 for an unknown `what`.
    pub fn count(&self, what: u32) -> u32 {
        let skipped = |kind: &str| self.banner.skipped().iter().find(|(k, _)| *k == kind).map_or(0, |(_, n)| *n);
        match what {
            w if w == GmWiiBannerCount::StartFrames as u32 => self.banner.start_frames(),
            w if w == GmWiiBannerCount::LoopFrames as u32 => self.banner.loop_frames(),
            w if w == GmWiiBannerCount::SkippedWindows as u32 => skipped("wnd1"),
            w if w == GmWiiBannerCount::SkippedText as u32 => skipped("txt1"),
            w if w == GmWiiBannerCount::SkippedBounds as u32 => skipped("bnd1"),
            w if w == GmWiiBannerCount::MissingTextures as u32 => u32::try_from(self.missing.len()).unwrap_or(u32::MAX),
            _ => 0,
        }
    }
}

/// What [`gm_wii_banner_count`] counts.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmWiiBannerCount {
    /// Frames of the start animation (0 when none); the loop begins after it. 60 frames are a second.
    StartFrames = 0,
    /// Frames of the loop animation (0 when none).
    LoopFrames = 1,
    /// Window frame panes (`wnd1`): parsed, not drawn.
    SkippedWindows = 2,
    /// Text panes (`txt1`): parsed, not drawn.
    SkippedText = 3,
    /// Bounding box panes (`bnd1`): parsed, not drawn.
    SkippedBounds = 4,
    /// Textures the layout names that the archive lacks ([`gm_wii_banner_missing_texture`]).
    MissingTextures = 5,
}

/// The banner of `source`, or why there is none: the core of every banner read entry point, also used by the JNI binding.
pub fn read_banner<S: ByteSource>(source: S) -> Result<GmWiiBanner, GmStatus> {
    let mut tracked = Tracked::new(source);
    match gamemeta_core::wii::open_channel_banner(&mut tracked) {
        Ok(Some(banner)) => Ok(GmWiiBanner::new(banner)),
        Ok(None) | Err(_) if !tracked.failed() => Err(GmStatus::NotFound),
        _ => Err(GmStatus::ErrIo),
    }
}

/// Stores the handle of a successful read in `*out` (null otherwise), catching panics.
unsafe fn store_banner(out: *mut *mut GmWiiBanner, read: impl FnOnce() -> Result<GmWiiBanner, GmStatus>) -> GmStatus {
    if out.is_null() {
        return GmStatus::ErrArgument;
    }
    // SAFETY: `out` is non-null and the caller promises it is writable.
    unsafe { *out = std::ptr::null_mut() };
    guard(GmStatus::ErrPanic, || match read() {
        Ok(banner) => {
            // SAFETY: checked non-null above.
            unsafe { *out = Box::into_raw(Box::new(banner)) };
            GmStatus::Ok
        }
        Err(status) => status,
    })
}

/// Reads the channel banner of the `len` bytes at `data`: an RVZ disc image, an `opening.bnr` (or a channel's
/// `00000000.app`), or a `meta/banner.bin` archive (plain, LZ77 or IMD5 wrapped), told by content. On [`GmStatus::Ok`] `*out` is
/// a handle to free with [`gm_wii_banner_free`]; no banner, an encrypted disc and a banner that does not parse are
/// [`GmStatus::NotFound`].
///
/// Safety: `data` points to `len` readable bytes (may be null when `len` is 0); `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_wii_banner_read_bytes(data: *const u8, len: usize, out: *mut *mut GmWiiBanner) -> GmStatus {
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
    unsafe { store_banner(out, || read_banner(SliceSource::new(bytes))) }
}

/// Like [`gm_wii_banner_read_bytes`] for a file of `size` bytes read through `read` (see `gm_read_callback`): an RVZ is read
/// through it in place, never whole.
///
/// Safety: `read` is a valid function honouring the [`GmReadFn`] contract; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_wii_banner_read_callback(
    size: u64,
    read: Option<unsafe extern "C" fn(*mut c_void, u64, *mut u8, usize) -> i32>,
    user_data: *mut c_void,
    out: *mut *mut GmWiiBanner,
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
    unsafe { store_banner(out, || read_banner(CallbackSource { size, read, user_data })) }
}

/// Like [`gm_wii_banner_read_bytes`] for an open, readable, seekable file descriptor read with positioned reads (`pread`): its
/// file offset is not moved and it is not closed. [`GmStatus::ErrUnsupported`] where there are no POSIX descriptors.
///
/// Safety: `fd` stays open for the call; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_wii_banner_read_fd(fd: c_int, out: *mut *mut GmWiiBanner) -> GmStatus {
    // SAFETY: forwarded caller contract.
    unsafe { store_banner(out, || fd::banner_fd(fd)) }
}

/// How many of `what` ([`GmWiiBannerCount`]); 0 for a null handle or an unknown `what`.
///
/// Safety: `banner` is a live handle or null (every accessor below has the same contract).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_wii_banner_count(banner: *const GmWiiBanner, what: u32) -> u32 {
    // SAFETY: accessor contract.
    unsafe { banner.as_ref() }.map_or(0, |b| b.count(what))
}

/// The name of the `index`th texture the layout names but the archive lacks, or null when out of range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_wii_banner_missing_texture(banner: *const GmWiiBanner, index: u32) -> *const c_char {
    // SAFETY: accessor contract.
    let Some(b) = (unsafe { banner.as_ref() }) else { return std::ptr::null() };
    b.missing.get(index as usize).and_then(|n| n.as_ref()).map_or(std::ptr::null(), |n| n.as_ptr())
}

/// The size in pixels of a frame at `scale` pixels per layout unit (608 x 456 at 1 for a channel banner) in `*width` and
/// `*height`. [`GmStatus::ErrArgument`] for a null pointer, a scale that is not positive and finite, or one over 8192 pixels a
/// side or 64 Mpx; the sizes are then 0.
///
/// Safety: `banner` is a live handle or null; `width` and `height` are writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_wii_banner_size(
    banner: *const GmWiiBanner,
    scale: f32,
    width: *mut u32,
    height: *mut u32,
) -> GmStatus {
    if width.is_null() || height.is_null() {
        return GmStatus::ErrArgument;
    }
    // SAFETY: writable by contract.
    unsafe {
        *width = 0;
        *height = 0;
    }
    // SAFETY: accessor contract.
    let Some(b) = (unsafe { banner.as_ref() }) else { return GmStatus::ErrArgument };
    guard(GmStatus::ErrPanic, || match b.banner.pixel_size(scale) {
        Some((w, h)) => {
            // SAFETY: writable by contract.
            unsafe {
                *width = w;
                *height = h;
            }
            GmStatus::Ok
        }
        None => GmStatus::ErrArgument,
    })
}

/// Draws the banner `frame` frames (60 to the second) after it appears (the start animation, then the loop, which repeats)
/// into `out` at `scale` pixels per layout unit: `width * height` RGBA8 pixels (not premultiplied), top row first, over
/// transparent; the size is [`gm_wii_banner_size`]'s and `out_len` must be exactly `width * height * 4`. A frame at or after
/// `StartFrames` is the settled banner (the "poster" is `frame = StartFrames`). [`GmStatus::ErrArgument`] for a null pointer, a
/// bad scale or a wrong `out_len`, writing nothing.
///
/// Safety: `banner` is a live handle or null; `out` points to `out_len` writable bytes or is null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_wii_banner_render(
    banner: *const GmWiiBanner,
    frame: f32,
    scale: f32,
    out: *mut u8,
    out_len: usize,
) -> GmStatus {
    // SAFETY: accessor contract.
    let Some(b) = (unsafe { banner.as_ref() }) else { return GmStatus::ErrArgument };
    if out.is_null() {
        return GmStatus::ErrArgument;
    }
    guard(GmStatus::ErrPanic, || {
        let Some((w, h)) = b.banner.pixel_size(scale) else { return GmStatus::ErrArgument };
        if out_len != w as usize * h as usize * 4 {
            return GmStatus::ErrArgument;
        }
        // SAFETY: the caller promises `out_len` writable bytes at a non-null `out`.
        let buf = unsafe { std::slice::from_raw_parts_mut(out, out_len) };
        match b.banner.render_into(frame, scale, buf) {
            Some(_) => GmStatus::Ok,
            None => GmStatus::ErrArgument,
        }
    })
}

/// Frees a banner handle. Null is ignored.
///
/// Safety: `banner` came from a banner read entry point and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_wii_banner_free(banner: *mut GmWiiBanner) {
    if !banner.is_null() {
        // SAFETY: the caller hands back a pointer from Box::into_raw, once.
        guard((), || drop(unsafe { Box::from_raw(banner) }));
    }
}
