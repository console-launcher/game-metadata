// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The C ABI of game-metadata.
//!
//! Conventions (also in the generated `include/gamemeta.h`):
//! - Every entry point catches panics and returns [`GmStatus::ErrPanic`] (or a null/neutral value) instead of
//!   unwinding into the caller.
//! - The caller does all I/O: give bytes in memory ([`gm_read_bytes`]), a read callback ([`gm_read_callback`]), or a
//!   readable, seekable file descriptor ([`gm_read_fd`], POSIX `pread`; the descriptor is neither moved nor closed).
//! - A Wii U title that is a folder of files is read through a file callback ([`gm_read_title_files`]); a `.wua` is an
//!   ordinary file for [`gm_read_bytes`] and friends. The large splashes are decoded on demand ([`gm_title_image`]).
//! - A successful read returns an opaque [`GmInfo`] the caller frees with [`gm_info_free`]. Strings and pixels the
//!   accessors return are owned by that handle and valid until it is freed. Strings are UTF-8, NUL-terminated.
//! - A 3DS banner is read into a renderer-neutral scene by [`gm_scene_read_bytes`] and friends, with its own opaque
//!   [`GmScene`] handle freed by [`gm_scene_free`].
//! - A Wii channel banner (an RVZ, an `opening.bnr` or a `banner.bin`) is read by [`gm_wii_banner_read_bytes`] and friends into a
//!   [`GmWiiBanner`] freed by [`gm_wii_banner_free`], and drawn on the CPU by [`gm_wii_banner_render`].
//! - Check [`gm_abi_version`] against `GM_ABI_VERSION` from the header you compiled with.
#![allow(clippy::missing_safety_doc)] // the safety contract of every entry point is in its doc comment

pub mod fd;
pub mod ids;
pub mod render;
pub mod scene;
pub mod wii_banner;

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};

pub use ids::*;
pub use render::*;
pub use scene::*;
pub use wii_banner::*;

use gamemeta_core::wiiu::{self, TitleFiles};
use gamemeta_core::{
    ByteSource, GameInfo, ImageKind, Language, RatingBoard, ReadError, Region, RgbaImage, SliceSource,
};

/// The ABI version this library implements. Bumped on any incompatible change to the C API.
pub const GM_ABI_VERSION: u32 = 1;

/// The result of a read.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmStatus {
    /// `*out` holds a new handle.
    Ok = 0,
    /// The file is of an unknown kind or yields nothing; `*out` is null.
    NotFound = 1,
    /// A null pointer or an invalid argument; `*out` is null.
    ErrArgument = 2,
    /// The source failed (the callback returned an error, or the descriptor could not be read); `*out` is null.
    ErrIo = 3,
    /// Not available on this platform (e.g. file descriptors on Windows).
    ErrUnsupported = 4,
    /// An internal error was caught at the boundary. Please report it with the file that caused it.
    ErrPanic = 5,
}

/// What a read callback returns.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmReadResult {
    /// `buf` was filled completely.
    Ok = 0,
    /// The range lies outside the file.
    OutOfRange = 1,
    /// The read failed.
    Error = 2,
}

/// The read callback of [`gm_read_callback`]: fill `buf[0..len]` with the file's bytes at `offset`. Called on the caller's thread, only during the read call,
/// only for ranges inside the size given to [`gm_read_callback`]. Must not unwind or longjmp.
/// Returns a [`GmReadResult`] value; any other value counts as an error.
pub type GmReadFn = unsafe extern "C" fn(user_data: *mut c_void, offset: u64, buf: *mut u8, len: usize) -> i32;

/// The platform a file belongs to ([`gm_info_platform`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmPlatform {
    /// Nintendo DS.
    Ds = 1,
    /// Nintendo DSi.
    Dsi = 2,
    /// Nintendo 3DS.
    ThreeDs = 3,
    /// Nintendo GameCube.
    GameCube = 4,
    /// Nintendo Wii.
    Wii = 5,
    /// Nintendo Wii U.
    WiiU = 6,
    /// Game Boy.
    GameBoy = 7,
    /// Game Boy Color.
    GameBoyColor = 8,
    /// Game Boy Advance.
    GameBoyAdvance = 9,
    /// Nintendo 64.
    Nintendo64 = 10,
    /// Super Nintendo.
    Snes = 11,
    /// NES.
    Nes = 12,
    /// Sega Genesis / Mega Drive.
    MegaDrive = 13,
    /// Sega 32X.
    Sega32x = 14,
    /// Sega Master System.
    MasterSystem = 15,
    /// Sega Game Gear.
    GameGear = 16,
}

/// Which string of a title ([`gm_info_title`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmTitleField {
    /// What a list shows: the long title, else the short one, lines joined with a space.
    Name = 0,
    /// The short title.
    ShortName = 1,
    /// The publisher.
    Publisher = 2,
}

/// Which id ([`gm_info_id`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmId {
    /// 3DS, Wii U, Wii WAD: the title id, 16 upper-case hex digits.
    TitleId = 0,
    /// 3DS `CTR-P-XXXX`, Wii U `WUP-P-XXXX`.
    ProductCode = 1,
    /// DS 4-character game code; GameCube and Wii 6-character game id; Wii WAD's 4 ASCII characters.
    GameId = 2,
    /// DS 2-character maker code; GameCube and Wii: the game id's last two characters.
    MakerCode = 3,
}

/// Which boot asset ([`gm_info_boot_asset`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmBootAsset {
    /// Wii U `bootMovie.h264`.
    Movie = 0,
    /// Wii U `bootSound.btsnd`.
    Sound = 1,
}

/// Pictures a file holds, as bits of [`gm_info_images`] (`1 << value`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmImage {
    /// Wii U `iconTex.tga`.
    Icon = 0,
    /// GameCube `opening.bnr` banner (96x32).
    Banner = 1,
    /// Wii title logo.
    Logo = 2,
    /// Wii U TV boot splash.
    TvSplash = 3,
    /// Wii U GamePad boot splash.
    GamePadSplash = 4,
    /// Wii U boot logo.
    BootLogo = 5,
}

/// Which icon ([`gm_info_icon`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmIcon {
    /// The main icon (3DS 48x48).
    Large = 0,
    /// The small icon (3DS 24x24).
    Small = 1,
}

/// Languages are numbered 0..GM_LANGUAGE_COUNT: Japanese, English, French, German, Italian, Spanish, Chinese
/// (simplified), Korean, Dutch, Portuguese, Russian, Chinese (traditional).
pub const GM_LANGUAGE_COUNT: u32 = 12;

/// The result of a read: opaque, freed with [`gm_info_free`].
#[derive(Debug)]
pub struct GmInfo {
    info: GameInfo,
    /// Per language, name / short name / publisher.
    titles: Vec<[Option<CString>; 3]>,
    ids: [Option<CString>; 4],
    header_title: Option<CString>,
    cart_json: Option<CString>,
}

impl GmInfo {
    fn new(info: GameInfo) -> Self {
        let c = |s: &str| CString::new(s).ok();
        let titles = Language::ALL
            .iter()
            .map(|l| match info.title_in(*l) {
                Some(t) => [c(&t.name), c(&t.short_name), t.publisher.as_deref().and_then(c)],
                None => [None, None, None],
            })
            .collect();
        let ids = [
            info.ids.title_id.as_deref().and_then(c),
            info.ids.product_code.as_deref().and_then(c),
            info.ids.game_id.as_deref().and_then(c),
            info.ids.maker_code.as_deref().and_then(c),
        ];
        let header_title = info.header_title.as_deref().and_then(c);
        let cart_json = info.cart.as_ref().map(|c| c.to_json()).as_deref().and_then(c);
        GmInfo { info, titles, ids, header_title, cart_json }
    }

    /// The parsed result, for Rust callers of this crate (the JNI binding).
    pub fn info(&self) -> &GameInfo {
        &self.info
    }
}

/// A source that remembers whether any read failed, so a failed source is reported as [`GmStatus::ErrIo`] rather
/// than as "nothing found".
#[derive(Debug)]
pub struct Tracked<S> {
    inner: S,
    failed: bool,
}

impl<S> Tracked<S> {
    /// Wraps `inner`.
    pub fn new(inner: S) -> Self {
        Tracked { inner, failed: false }
    }
    /// Whether a read returned [`ReadError::Io`].
    pub fn failed(&self) -> bool {
        self.failed
    }
}

impl<S: ByteSource> ByteSource for Tracked<S> {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        let r = self.inner.read_at(offset, buf);
        self.failed |= r == Err(ReadError::Io);
        r
    }
}

/// Reads `source` as `file_name` and reports the outcome: the shared core of every read entry point.
pub fn read_tracked<S: ByteSource>(source: S, file_name: &str) -> Result<GameInfo, GmStatus> {
    let mut tracked = Tracked::new(source);
    match gamemeta_core::read(&mut tracked, file_name) {
        Some(info) => Ok(info),
        None if tracked.failed() => Err(GmStatus::ErrIo),
        None => Err(GmStatus::NotFound),
    }
}

fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// Stores the handle of a successful read in `*out` (null otherwise), catching panics: the tail of every read entry
/// point.
unsafe fn store_info(out: *mut *mut GmInfo, read: impl FnOnce() -> Result<GameInfo, GmStatus>) -> GmStatus {
    if out.is_null() {
        return GmStatus::ErrArgument;
    }
    // SAFETY: `out` is non-null and the caller promises it is writable.
    unsafe { *out = std::ptr::null_mut() };
    guard(GmStatus::ErrPanic, || match read() {
        Ok(info) => {
            let handle = Box::into_raw(Box::new(GmInfo::new(info)));
            // SAFETY: checked non-null above.
            unsafe { *out = handle };
            GmStatus::Ok
        }
        Err(status) => status,
    })
}

/// Runs a read entry point: checks `out` and `file_name`, catches panics, stores the handle.
unsafe fn read_entry(
    file_name: *const c_char,
    out: *mut *mut GmInfo,
    read: impl FnOnce(&str) -> Result<GameInfo, GmStatus>,
) -> GmStatus {
    if out.is_null() {
        return GmStatus::ErrArgument;
    }
    // SAFETY: `out` is non-null and the caller promises it is writable.
    unsafe { *out = std::ptr::null_mut() };
    if file_name.is_null() {
        return GmStatus::ErrArgument;
    }
    // SAFETY: the caller promises a NUL-terminated string.
    let Ok(name) = unsafe { CStr::from_ptr(file_name) }.to_str() else { return GmStatus::ErrArgument };
    // SAFETY: `out` is non-null and writable (checked above).
    unsafe { store_info(out, || read(name)) }
}

/// The ABI version of the loaded library; compare with `GM_ABI_VERSION`.
#[unsafe(no_mangle)]
pub extern "C" fn gm_abi_version() -> u32 {
    GM_ABI_VERSION
}

/// The library version, e.g. "0.5.1". Static; never freed.
#[unsafe(no_mangle)]
pub extern "C" fn gm_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

/// Reads `len` bytes at `data`. `file_name` (UTF-8, NUL-terminated) selects the reader by its extension; only its
/// extension matters. On [`GmStatus::Ok`] `*out` is a handle to free with [`gm_info_free`].
///
/// Safety: `data` points to `len` readable bytes (may be null when `len` is 0); `file_name` is a NUL-terminated
/// string; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_read_bytes(
    data: *const u8,
    len: usize,
    file_name: *const c_char,
    out: *mut *mut GmInfo,
) -> GmStatus {
    if data.is_null() && len != 0 {
        return GmStatus::ErrArgument;
    }
    // SAFETY: the caller promises `len` readable bytes at a non-null `data`; an empty slice needs no pointer.
    let bytes = if len == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(data, len) } };
    // SAFETY: forwarded caller contract.
    unsafe { read_entry(file_name, out, |name| read_tracked(SliceSource::new(bytes), name)) }
}

#[derive(Debug)]
struct CallbackSource {
    size: u64,
    read: GmReadFn,
    user_data: *mut c_void,
}

impl ByteSource for CallbackSource {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        // SAFETY: the callback contract: fill `buf.len()` bytes at `buf`.
        match unsafe { (self.read)(self.user_data, offset, buf.as_mut_ptr(), buf.len()) } {
            r if r == GmReadResult::Ok as i32 => Ok(()),
            r if r == GmReadResult::OutOfRange as i32 => Err(ReadError::OutOfRange),
            _ => Err(ReadError::Io),
        }
    }
}

/// Reads a file of `size` bytes through `read` (called with `user_data`). See [`gm_read_bytes`] for `file_name` and
/// `out`.
///
/// Safety: `read` is a valid function honouring the [`GmReadFn`] contract; `file_name` is a NUL-terminated string;
/// `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_read_callback(
    size: u64,
    read: Option<unsafe extern "C" fn(*mut c_void, u64, *mut u8, usize) -> i32>,
    user_data: *mut c_void,
    file_name: *const c_char,
    out: *mut *mut GmInfo,
) -> GmStatus {
    let Some(read) = read else { return GmStatus::ErrArgument };
    // SAFETY: forwarded caller contract.
    unsafe { read_entry(file_name, out, |name| read_tracked(CallbackSource { size, read, user_data }, name)) }
}

/// Reads an open, readable, seekable file descriptor with positioned reads (`pread`): its file offset is not moved
/// and it is not closed. On Android, the descriptor of a `ParcelFileDescriptor` from a content URI. Returns
/// [`GmStatus::ErrUnsupported`] where there are no POSIX descriptors.
///
/// Safety: `fd` stays open for the call; `file_name` is a NUL-terminated string; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_read_fd(fd: c_int, file_name: *const c_char, out: *mut *mut GmInfo) -> GmStatus {
    // SAFETY: forwarded caller contract.
    unsafe { read_entry(file_name, out, |name| fd::read_fd(fd, name)) }
}

/// What a [`GmFileFn`] returns.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmFileResult {
    /// The file exists; `*size` is its size, and `buf` holds it when it was given and `cap >= *size`.
    Ok = 0,
    /// There is no such file.
    NotFound = 1,
    /// The file could not be read.
    Error = 2,
}

/// The file callback of the Wii U entry points: the files of one title by path from its root (`meta/meta.xml`,
/// `meta/iconTex.tga`; `/` separators, UTF-8, NUL-terminated). Sets `*size` to the file's size; copies the file to
/// `buf` only when `buf` is non-null and `cap >= *size` (a size query passes null and 0). Called on the caller's
/// thread, only during the call it was given to. Must not unwind or longjmp. Returns a [`GmFileResult`] value; any
/// other value counts as an error.
pub type GmFileFn =
    unsafe extern "C" fn(user_data: *mut c_void, path: *const c_char, buf: *mut u8, cap: usize, size: *mut u64) -> i32;

/// A Wii U title's files through a [`GmFileFn`], remembering whether the callback failed.
#[derive(Debug)]
struct CFiles {
    files: GmFileFn,
    user_data: *mut c_void,
    failed: bool,
}

impl CFiles {
    fn call(&mut self, path: &str, buf: *mut u8, cap: usize) -> Option<u64> {
        let path = CString::new(path).ok()?;
        let mut size = 0u64;
        // SAFETY: the callback contract; `size` is writable, `buf` is null or `cap` writable bytes.
        match unsafe { (self.files)(self.user_data, path.as_ptr(), buf, cap, &mut size) } {
            r if r == GmFileResult::Ok as i32 => Some(size),
            r if r == GmFileResult::NotFound as i32 => None,
            _ => {
                self.failed = true;
                None
            }
        }
    }
}

impl TitleFiles for CFiles {
    fn size(&mut self, path: &str) -> Option<u64> {
        self.call(path, std::ptr::null_mut(), 0)
    }

    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        let size = self.size(path)?;
        if size > max as u64 {
            return None;
        }
        let mut buf = vec![0u8; usize::try_from(size).ok()?];
        (self.call(path, buf.as_mut_ptr(), buf.len())? == size).then_some(buf)
    }
}

/// `result`, or why there is none: a failed source is [`GmStatus::ErrIo`], else [`GmStatus::NotFound`].
fn outcome<T>(result: Option<T>, failed: bool) -> Result<T, GmStatus> {
    result.ok_or(if failed { GmStatus::ErrIo } else { GmStatus::NotFound })
}

/// Reads a Wii U title from its files: `files` serves `meta/meta.xml`, the TGA pictures and the boot media (the app
/// finds them from the `.rpx` it lists; see the `wiiu` module of `gamemeta-core`). On [`GmStatus::Ok`] `*out` is a
/// handle to free with [`gm_info_free`]; [`GmStatus::NotFound`] when there is no readable `meta/meta.xml`.
///
/// Safety: `files` is a valid function honouring the [`GmFileFn`] contract; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_read_title_files(
    files: Option<GmFileFn>,
    user_data: *mut c_void,
    out: *mut *mut GmInfo,
) -> GmStatus {
    let Some(files) = files else { return GmStatus::ErrArgument };
    // SAFETY: forwarded caller contract.
    unsafe {
        store_info(out, || {
            let mut files = CFiles { files, user_data, failed: false };
            let info = wiiu::read_title(&mut files);
            outcome(info, files.failed)
        })
    }
}

/// A decoded picture: opaque, freed with [`gm_pixels_free`].
#[derive(Debug)]
pub struct GmPixels {
    image: RgbaImage,
}

/// The picture kind a [`GmImage`] value names, for the kinds a Wii U title can decode on demand.
pub fn image_kind(kind: u32) -> Option<ImageKind> {
    match kind {
        k if k == GmImage::Icon as u32 => Some(ImageKind::Icon),
        k if k == GmImage::TvSplash as u32 => Some(ImageKind::TvSplash),
        k if k == GmImage::GamePadSplash as u32 => Some(ImageKind::GamePadSplash),
        k if k == GmImage::BootLogo as u32 => Some(ImageKind::BootLogo),
        _ => None,
    }
}

/// Stores a picture's handle in `*out`, as [`store_info`] does for a read.
unsafe fn store_pixels(
    out: *mut *mut GmPixels,
    kind: u32,
    decode: impl FnOnce(ImageKind) -> Result<RgbaImage, GmStatus>,
) -> GmStatus {
    if out.is_null() {
        return GmStatus::ErrArgument;
    }
    // SAFETY: `out` is non-null and the caller promises it is writable.
    unsafe { *out = std::ptr::null_mut() };
    let Some(kind) = image_kind(kind) else { return GmStatus::ErrArgument };
    guard(GmStatus::ErrPanic, || match decode(kind) {
        Ok(image) => {
            // SAFETY: checked non-null above.
            unsafe { *out = Box::into_raw(Box::new(GmPixels { image })) };
            GmStatus::Ok
        }
        Err(status) => status,
    })
}

/// Decodes one picture of a Wii U title ([`GmImage`] `kind`: Icon, TvSplash, GamePadSplash or BootLogo) from its
/// files, for the large splashes a read does not carry. [`GmStatus::NotFound`] when the file is missing or not a
/// readable TGA. On [`GmStatus::Ok`] `*out` is a handle to free with [`gm_pixels_free`].
///
/// Safety: as [`gm_read_title_files`]; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_title_image(
    files: Option<GmFileFn>,
    user_data: *mut c_void,
    kind: u32,
    out: *mut *mut GmPixels,
) -> GmStatus {
    let Some(files) = files else { return GmStatus::ErrArgument };
    // SAFETY: forwarded caller contract.
    unsafe {
        store_pixels(out, kind, |kind| {
            let mut files = CFiles { files, user_data, failed: false };
            let image = wiiu::image(&mut files, kind);
            outcome(image, files.failed)
        })
    }
}

/// Like [`gm_title_image`] for the base title of a `.wua` of `size` bytes read through `read` (see
/// [`gm_read_callback`]).
///
/// Safety: `read` is a valid function honouring the [`GmReadFn`] contract; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_wua_image(
    size: u64,
    read: Option<unsafe extern "C" fn(*mut c_void, u64, *mut u8, usize) -> i32>,
    user_data: *mut c_void,
    kind: u32,
    out: *mut *mut GmPixels,
) -> GmStatus {
    let Some(read) = read else { return GmStatus::ErrArgument };
    // SAFETY: forwarded caller contract.
    unsafe { store_pixels(out, kind, |kind| wua_image(CallbackSource { size, read, user_data }, kind)) }
}

/// One picture of the base title of the `.wua` in `source`: the core of [`gm_wua_image`], also used by the JNI binding.
pub fn wua_image<S: ByteSource>(source: S, kind: ImageKind) -> Result<RgbaImage, GmStatus> {
    let mut source = Tracked::new(source);
    let image = wiiu::open_wua(&mut source).and_then(|mut files| wiiu::image(&mut files, kind));
    outcome(image, source.failed())
}

/// A picture's RGBA8 pixels (top-down rows, `width * height * 4` bytes), owned by the handle; sets the size. Null
/// (sizes 0) for a null handle. `width` and `height` may be null.
///
/// Safety: `pixels` is a live handle or null; the size pointers are writable or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_pixels_data(pixels: *const GmPixels, width: *mut u32, height: *mut u32) -> *const u8 {
    // SAFETY: the caller promises a live handle or null.
    let image = unsafe { pixels.as_ref() }.map(|p| &p.image);
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

/// Frees a picture handle. Null is ignored.
///
/// Safety: `pixels` came from [`gm_title_image`] or [`gm_wua_image`] and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_pixels_free(pixels: *mut GmPixels) {
    if !pixels.is_null() {
        // SAFETY: the caller hands back a pointer from Box::into_raw, once.
        guard((), || drop(unsafe { Box::from_raw(pixels) }));
    }
}

/// Frees a handle from a read. Null is ignored.
///
/// Safety: `info` came from a read entry point and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_free(info: *mut GmInfo) {
    if !info.is_null() {
        // SAFETY: the caller hands back a pointer from Box::into_raw, once.
        guard((), || drop(unsafe { Box::from_raw(info) }));
    }
}

/// Borrows the handle, or `None` for null.
unsafe fn borrow<'a>(info: *const GmInfo) -> Option<&'a GmInfo> {
    // SAFETY: the caller promises a live handle or null.
    unsafe { info.as_ref() }
}

/// The platform ([`GmPlatform`] value), 0 for a null handle.
///
/// Safety: `info` is a live handle or null (every accessor below has the same contract).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_platform(info: *const GmInfo) -> u32 {
    // SAFETY: accessor contract.
    unsafe { borrow(info) }.map_or(0, |i| i.info.platform as u32)
}

/// A title string ([`GmTitleField`] `field`) in `language` (0..GM_LANGUAGE_COUNT), or null when the file has none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_title(info: *const GmInfo, language: u32, field: u32) -> *const c_char {
    // SAFETY: accessor contract.
    let Some(i) = (unsafe { borrow(info) }) else { return std::ptr::null() };
    let slot = i.titles.get(language as usize).and_then(|t| t.get(field as usize)).and_then(|s| s.as_ref());
    slot.map_or(std::ptr::null(), |s| s.as_ptr())
}

/// The language a list should show (English if present, else the lowest that has a title); -1 when there are no
/// titles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_preferred_language(info: *const GmInfo) -> i32 {
    // SAFETY: accessor contract.
    unsafe { borrow(info) }.and_then(|i| i.info.preferred_title()).map_or(-1, |(l, _)| l as i32)
}

/// An id string ([`GmId`] `id`), or null when the file has none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_id(info: *const GmInfo, id: u32) -> *const c_char {
    // SAFETY: accessor contract.
    let Some(i) = (unsafe { borrow(info) }) else { return std::ptr::null() };
    i.ids.get(id as usize).and_then(|s| s.as_ref()).map_or(std::ptr::null(), |s| s.as_ptr())
}

/// An icon's ([`GmIcon`] `which`) RGBA8 pixels (top-down rows, `width * height * 4` bytes) and its size, or null (sizes set to 0) when
/// the file has none. `width` and `height` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_icon(info: *const GmInfo, which: u32, width: *mut u32, height: *mut u32) -> *const u8 {
    // SAFETY: accessor contract.
    let image = unsafe { borrow(info) }.and_then(|i| match which {
        w if w == GmIcon::Large as u32 => i.info.icon.as_ref(),
        w if w == GmIcon::Small as u32 => i.info.small_icon.as_ref(),
        _ => None,
    });
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

/// The regions the file is locked to, as a bit mask (bit 0 Japan, 1 North America, 2 Europe, 3 Australia, 4 China,
/// 5 Korea, 6 Taiwan); 0 when it names none or is region free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_regions(info: *const GmInfo) -> u32 {
    // SAFETY: accessor contract.
    unsafe { borrow(info) }.map_or(0, |i| i.info.regions.iter().fold(0, |m, r: &Region| m | 1 << *r as u32))
}

/// Whether the file allows every region.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_region_free(info: *const GmInfo) -> bool {
    // SAFETY: accessor contract.
    unsafe { borrow(info) }.is_some_and(|i| i.info.region_free)
}

/// The minimum age a rating board gives (0 = no age restriction), or -1 when that board has no active rating.
/// Boards by slot: 0 CERO, 1 ESRB, 3 USK, 4 PEGI, 6 PEGI Portugal, 7 BBFC, 8 COB, 9 GRB, 10 CGSRR.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_rating(info: *const GmInfo, board: u32) -> i32 {
    // SAFETY: accessor contract.
    let Some(i) = (unsafe { borrow(info) }) else { return -1 };
    i.info.ratings.iter().find(|(b, _)| *b as u32 == board).map_or(-1, |(_, age): &(RatingBoard, u8)| i32::from(*age))
}

/// The number of steps of a DSi animated icon; 0 for a still icon or a file with none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_animation_count(info: *const GmInfo) -> u32 {
    // SAFETY: accessor contract.
    unsafe { borrow(info) }.map_or(0, |i| i.info.animated_icon.len() as u32)
}

/// Step `index` of the animated icon: its RGBA8 pixels (`width * height * 4` bytes), size, and how long it shows in
/// sixtieths of a second. Null (outputs 0) when `index` is out of range. Any output pointer may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_animation_frame(
    info: *const GmInfo,
    index: u32,
    width: *mut u32,
    height: *mut u32,
    duration_frames: *mut u32,
) -> *const u8 {
    // SAFETY: accessor contract.
    let step = unsafe { borrow(info) }.and_then(|i| i.info.animated_icon.get(index as usize));
    let (w, h, d, ptr) = step.map_or((0, 0, 0, std::ptr::null()), |s| {
        (s.frame.width, s.frame.height, s.duration_frames, s.frame.pixels.as_ptr())
    });
    // SAFETY: the caller passes writable pointers or null.
    unsafe {
        if let Some(p) = width.as_mut() {
            *p = w;
        }
        if let Some(p) = height.as_mut() {
            *p = h;
        }
        if let Some(p) = duration_frames.as_mut() {
            *p = d;
        }
    }
    ptr
}

/// The disc header's own title (GameCube and Wii), or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_header_title(info: *const GmInfo) -> *const c_char {
    // SAFETY: accessor contract.
    unsafe { borrow(info) }.and_then(|i| i.header_title.as_ref()).map_or(std::ptr::null(), |s| s.as_ptr())
}

/// The game's version (cartridge consoles: the header's version byte or revision), or -1 when the file has none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_version(info: *const GmInfo) -> i64 {
    // SAFETY: accessor contract.
    unsafe { borrow(info) }.and_then(|i| i.info.version).map_or(-1, i64::from)
}

/// The cartridge header as one JSON object (`{"family":"gb","title":"...",...}`, every decoded field, `null` for one the
/// header lacks), or null when the file is not a cartridge ROM.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_cart_json(info: *const GmInfo) -> *const c_char {
    // SAFETY: accessor contract.
    unsafe { borrow(info) }.and_then(|i| i.cart_json.as_ref()).map_or(std::ptr::null(), |s| s.as_ptr())
}

/// The pictures the file holds, as a bit mask over [`GmImage`] (`1 << value`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_images(info: *const GmInfo) -> u32 {
    // SAFETY: accessor contract.
    unsafe { borrow(info) }.map_or(0, |i| i.info.images.iter().fold(0, |m, k| m | 1 << *k as u32))
}

/// The size in bytes of a Wii U boot asset ([`GmBootAsset`] `which`), or -1 when the title has none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_boot_asset(info: *const GmInfo, which: u32) -> i64 {
    // SAFETY: accessor contract.
    let Some(i) = (unsafe { borrow(info) }) else { return -1 };
    let size = match which {
        w if w == GmBootAsset::Movie as u32 => i.info.boot_movie,
        w if w == GmBootAsset::Sound as u32 => i.info.boot_sound,
        _ => None,
    };
    size.map_or(-1, |s| i64::try_from(s).unwrap_or(i64::MAX))
}

/// Where the 3DS banner (or the DS banner block) sits in the file. False (outputs untouched) when there is none.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_info_banner(info: *const GmInfo, offset: *mut u64, size: *mut u64) -> bool {
    // SAFETY: accessor contract.
    let Some(b) = (unsafe { borrow(info) }).and_then(|i| i.info.banner) else { return false };
    // SAFETY: the caller passes writable pointers or null.
    unsafe {
        if let Some(p) = offset.as_mut() {
            *p = b.offset;
        }
        if let Some(p) = size.as_mut() {
            *p = b.size;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_panic_inside_the_guard_becomes_the_fallback() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let status = super::guard(super::GmStatus::ErrPanic, || -> super::GmStatus { panic!("inside") });
        std::panic::set_hook(previous);
        assert_eq!(status, super::GmStatus::ErrPanic);
    }
}
