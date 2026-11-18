// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The ids a game file carries in its header (serials, game ids, title ids, a CHD's SHA-1, a zip's ROM CRC, a PSP game's
//! `PARAM.SFO` fields, an Xbox title id) through opaque [`GmIds`] handles, a streaming RetroAchievements hasher
//! ([`GmRaHasher`]) and the refusing disc-id reader ([`gm_disc_id_bytes`]). The caller does all I/O, as everywhere else:
//! files beside the game file (a cue sheet's track, a Wii U `.rpx`'s `meta` folder) come through a [`GmOpenFn`].

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use gamemeta_core::ids::{self, DiscIdRefusal, DiscIdResult, DiscRegion, DiscSystem, Ids, Opener, RaHasher};
use gamemeta_core::{ByteSource, ReadError, SliceSource};

use crate::{CallbackSource, GmFileResult, GmReadFn, GmStatus, Tracked, fd, guard};

/// Which id of a [`GmIds`] ([`gm_ids_get`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmIdField {
    /// PlayStation 1/2 serial (`SLUS00001`) or PSP disc id (`ULUS00001`).
    Serial = 0,
    /// GameCube / Wii 6-character game id.
    GameId = 1,
    /// 3DS, Wii WAD, Wii U or Switch title id: 16 upper-case hex digits.
    TitleId = 2,
    /// CHD v5 raw SHA-1, lower-case hex.
    ChdSha1 = 3,
    /// CRC-32 of the one ROM in a zip, eight lower-case hex digits.
    ZipCrc = 4,
    /// PSP `PARAM.SFO` `TITLE`.
    PspTitle = 5,
    /// PSP `PARAM.SFO` `DISC_ID`, as written.
    PspDiscId = 6,
    /// Xbox `default.xbe` title id, eight upper-case hex digits.
    XboxId = 7,
    /// RetroAchievements hash (never set by a read; see [`GmRaHasher`]).
    RaMd5 = 8,
    /// The one id a launcher keys a game by: serial, else game id, else title id.
    Primary = 9,
}

/// The result of an ids read: opaque, freed with [`gm_ids_free`].
#[derive(Debug)]
pub struct GmIds {
    ids: Ids,
    strings: [Option<CString>; 10],
}

impl GmIds {
    /// A handle for `ids`.
    pub fn new(ids: Ids) -> Self {
        let c = |s: Option<&str>| s.and_then(|s| CString::new(s).ok());
        let strings = [
            c(ids.serial.as_deref()),
            c(ids.game_id.as_deref()),
            c(ids.title_id.as_deref()),
            c(ids.chd_sha1.as_deref()),
            c(ids.zip_crc.as_deref()),
            c(ids.psp_title.as_deref()),
            c(ids.psp_disc_id.as_deref()),
            c(ids.xbox_id.as_deref()),
            c(ids.ra_md5.as_deref()),
            c(ids.primary_id()),
        ];
        GmIds { ids, strings }
    }

    /// The ids, for Rust callers of this crate (the JNI binding).
    pub fn ids(&self) -> &Ids {
        &self.ids
    }
}

/// A file opened by a [`GmOpenFn`]: its size, how to read it, and (optionally) how to close it. All fields are filled by the callback.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GmOpened {
    /// The file's size in bytes.
    pub size: u64,
    /// Reads from the file, as for `gm_read_callback`.
    pub read: Option<GmReadFn>,
    /// Passed to `read` and `close`.
    pub user_data: *mut c_void,
    /// Called once when the library is done with the file; may be null.
    pub close: Option<unsafe extern "C" fn(user_data: *mut c_void)>,
}

/// Opens a file beside the game file being read, by its path relative to the game file's folder (`game (Track 1).bin`,
/// `../meta/meta.xml`; `/` separators, UTF-8, NUL-terminated): fill `*out` and return [`GmFileResult::Ok`], or return
/// [`GmFileResult::NotFound`] / [`GmFileResult::Error`]. Called on the caller's thread, only during the read call; at
/// most one file is open at a time. Must not unwind or longjmp.
pub type GmOpenFn = unsafe extern "C" fn(user_data: *mut c_void, path: *const c_char, out: *mut GmOpened) -> i32;

struct OpenedSource {
    inner: CallbackSource,
    close: Option<unsafe extern "C" fn(*mut c_void)>,
}

impl ByteSource for OpenedSource {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        self.inner.read_at(offset, buf)
    }
}

impl Drop for OpenedSource {
    fn drop(&mut self) {
        if let Some(close) = self.close {
            // SAFETY: the callback contract: `close` takes the `user_data` it was opened with, once.
            unsafe { close(self.inner.user_data) };
        }
    }
}

struct BoundOpener {
    open: Option<GmOpenFn>,
    user_data: *mut c_void,
}

impl Opener for BoundOpener {
    fn open(&mut self, path: &str) -> Option<Box<dyn ByteSource + '_>> {
        let open = self.open?;
        let path = CString::new(path).ok()?;
        let mut out = GmOpened { size: 0, read: None, user_data: std::ptr::null_mut(), close: None };
        // SAFETY: the callback contract; `out` is writable and `path` NUL-terminated.
        let result = unsafe { open(self.user_data, path.as_ptr(), &mut out) };
        if result != GmFileResult::Ok as i32 {
            return None;
        }
        let Some(read) = out.read else {
            if let Some(close) = out.close {
                // SAFETY: the callback contract.
                unsafe { close(out.user_data) };
            }
            return None;
        };
        Some(Box::new(OpenedSource {
            inner: CallbackSource { size: out.size, read, user_data: out.user_data },
            close: out.close,
        }))
    }
}

/// The ids of `source`, or why there are none: the core of every ids read entry point, also used by the JNI binding.
pub fn read_ids<S: ByteSource>(source: S, file_name: &str, opener: &mut dyn Opener) -> Result<GmIds, GmStatus> {
    let mut tracked = Tracked::new(source);
    match ids::read_with(&mut tracked, file_name, opener) {
        Some(ids) => Ok(GmIds::new(ids)),
        None if tracked.failed() => Err(GmStatus::ErrIo),
        None => Err(GmStatus::NotFound),
    }
}

unsafe fn ids_entry(
    file_name: *const c_char,
    open: Option<GmOpenFn>,
    open_user: *mut c_void,
    out: *mut *mut GmIds,
    read: impl FnOnce(&str, &mut dyn Opener) -> Result<GmIds, GmStatus>,
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
    guard(GmStatus::ErrPanic, || match read(name, &mut BoundOpener { open, user_data: open_user }) {
        Ok(ids) => {
            // SAFETY: checked non-null above.
            unsafe { *out = Box::into_raw(Box::new(ids)) };
            GmStatus::Ok
        }
        Err(status) => status,
    })
}

/// Reads the ids of the `len` bytes at `data`, a file called `file_name` (its extension picks the readers: see the
/// `gamemeta_core::ids` table). `open` (may be null) opens files beside it: the track of a `.cue`, `../meta/meta.xml` of a
/// Wii U `.rpx`. On [`GmStatus::Ok`] `*out` is a handle to free with [`gm_ids_free`]; [`GmStatus::NotFound`] when the file
/// carries no id.
///
/// Safety: `data` points to `len` readable bytes (may be null when `len` is 0); `file_name` is a NUL-terminated string;
/// `open` honours the [`GmOpenFn`] contract with `open_user`; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ids_read_bytes(
    data: *const u8,
    len: usize,
    file_name: *const c_char,
    open: Option<GmOpenFn>,
    open_user: *mut c_void,
    out: *mut *mut GmIds,
) -> GmStatus {
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
    unsafe {
        ids_entry(file_name, open, open_user, out, |name, opener| read_ids(SliceSource::new(bytes), name, opener))
    }
}

/// Like [`gm_ids_read_bytes`] for a file of `size` bytes read through `read` (see `gm_read_callback`).
///
/// Safety: as [`gm_ids_read_bytes`]; `read` honours the [`GmReadFn`] contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ids_read_callback(
    size: u64,
    read: Option<GmReadFn>,
    user_data: *mut c_void,
    file_name: *const c_char,
    open: Option<GmOpenFn>,
    open_user: *mut c_void,
    out: *mut *mut GmIds,
) -> GmStatus {
    let Some(read) = read else {
        if !out.is_null() {
            // SAFETY: non-null and writable by contract.
            unsafe { *out = std::ptr::null_mut() };
        }
        return GmStatus::ErrArgument;
    };
    // SAFETY: forwarded caller contract.
    unsafe {
        ids_entry(file_name, open, open_user, out, |name, opener| {
            read_ids(CallbackSource { size, read, user_data }, name, opener)
        })
    }
}

/// Like [`gm_ids_read_bytes`] for an open, readable, seekable file descriptor (positioned reads; not moved, not closed).
/// [`GmStatus::ErrUnsupported`] where there are no POSIX descriptors.
///
/// Safety: as [`gm_ids_read_bytes`]; `fd` stays open for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ids_read_fd(
    fd: c_int,
    file_name: *const c_char,
    open: Option<GmOpenFn>,
    open_user: *mut c_void,
    out: *mut *mut GmIds,
) -> GmStatus {
    // SAFETY: forwarded caller contract.
    unsafe { ids_entry(file_name, open, open_user, out, |name, opener| read_ids(fd::open(fd)?, name, opener)) }
}

/// One id of the handle as a NUL-terminated string owned by the handle, or null when the file did not carry it (or the
/// handle is null or `field` unknown).
///
/// Safety: `ids` is a live handle or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ids_get(ids: *const GmIds, field: u32) -> *const c_char {
    // SAFETY: accessor contract.
    let Some(ids) = (unsafe { ids.as_ref() }) else { return std::ptr::null() };
    ids.strings.get(field as usize).and_then(|s| s.as_ref()).map_or(std::ptr::null(), |s| s.as_ptr())
}

/// The size of the one ROM in a zip (with [`GmIdField::ZipCrc`]), or -1.
///
/// Safety: `ids` is a live handle or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ids_zip_size(ids: *const GmIds) -> i64 {
    // SAFETY: accessor contract.
    unsafe { ids.as_ref() }.and_then(|i| i.ids.zip_size).and_then(|s| i64::try_from(s).ok()).unwrap_or(-1)
}

/// Frees an ids handle. Null is ignored.
///
/// Safety: `ids` came from an ids read entry point and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ids_free(ids: *mut GmIds) {
    if !ids.is_null() {
        // SAFETY: the caller hands back a pointer from Box::into_raw, once.
        guard((), || drop(unsafe { Box::from_raw(ids) }));
    }
}

/// A streaming RetroAchievements hasher: opaque, freed with [`gm_ra_hasher_free`].
#[derive(Debug)]
pub struct GmRaHasher {
    hasher: Option<RaHasher>,
    result: Option<CString>,
}

/// A hasher for a file called `file_name` of `size` bytes (-1 when unknown), or null when its RA hash is not computed here
/// (every platform but the whole-file ones, NES and SNES; NES and SNES need the size) or an argument is bad.
///
/// Safety: `file_name` is a NUL-terminated string or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ra_hasher_new(file_name: *const c_char, size: i64) -> *mut GmRaHasher {
    if file_name.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: the caller promises a NUL-terminated string.
    let Ok(name) = unsafe { CStr::from_ptr(file_name) }.to_str() else { return std::ptr::null_mut() };
    guard(std::ptr::null_mut(), || match RaHasher::for_file(name, u64::try_from(size).ok()) {
        Some(hasher) => Box::into_raw(Box::new(GmRaHasher { hasher: Some(hasher), result: None })),
        None => std::ptr::null_mut(),
    })
}

/// Feeds the next `len` bytes of the file, in order. Ignored after [`gm_ra_hasher_finish`].
///
/// Safety: `hasher` is a live handle; `data` points to `len` readable bytes (may be null when `len` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ra_hasher_update(hasher: *mut GmRaHasher, data: *const u8, len: usize) {
    // SAFETY: handle contract.
    let Some(h) = (unsafe { hasher.as_mut() }) else { return };
    if len == 0 || data.is_null() {
        return;
    }
    // SAFETY: `len` readable bytes at a non-null `data`.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    guard((), || {
        if let Some(inner) = h.hasher.as_mut() {
            inner.update(bytes);
        }
    });
}

/// The lower-case hex MD5 (NUL-terminated, owned by the handle), or null when the file turned out not to be hashable this
/// way (a NES file without the iNES magic or cut short). The same string each time it is called.
///
/// Safety: `hasher` is a live handle or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ra_hasher_finish(hasher: *mut GmRaHasher) -> *const c_char {
    // SAFETY: handle contract.
    let Some(h) = (unsafe { hasher.as_mut() }) else { return std::ptr::null() };
    guard((), || {
        if let Some(inner) = h.hasher.take() {
            h.result = inner.finish().and_then(|s| CString::new(s).ok());
        }
    });
    h.result.as_ref().map_or(std::ptr::null(), |s| s.as_ptr())
}

/// Frees a hasher. Null is ignored.
///
/// Safety: `hasher` came from [`gm_ra_hasher_new`] and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_ra_hasher_free(hasher: *mut GmRaHasher) {
    if !hasher.is_null() {
        // SAFETY: the caller hands back a pointer from Box::into_raw, once.
        guard((), || drop(unsafe { Box::from_raw(hasher) }));
    }
}

/// The outcome of a disc-id read ([`GmDiscId::status`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmDiscIdStatus {
    /// `id`, `region` and `system` are set.
    Ok = 0,
    /// Fewer bytes than the header needs.
    TooShort = 1,
    /// No disc or executable magic.
    BadMagic = 2,
    /// WBFS, CISO or GCZ: the id is not in the first bytes.
    UnsupportedContainer = 3,
    /// The source failed, or the id is not ASCII letters and digits, or an address is out of range.
    Unreadable = 4,
}

/// The sales region of a disc id.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmDiscRegion {
    /// NTSC-U.
    NtscU = 0,
    /// NTSC-J.
    NtscJ = 1,
    /// PAL.
    Pal = 2,
    /// NTSC-K.
    NtscK = 3,
    /// Not told by the id.
    Unknown = 4,
}

/// The console of a disc id.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmDiscSystem {
    /// GameCube.
    GameCube = 0,
    /// Wii.
    Wii = 1,
    /// Xbox.
    Xbox = 2,
}

/// A disc id or the reason there is none, filled by the `gm_disc_id_*` calls.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GmDiscId {
    /// [`GmDiscIdStatus::Ok`] when the other fields are set.
    pub status: GmDiscIdStatus,
    /// The region the id names.
    pub region: GmDiscRegion,
    /// The console.
    pub system: GmDiscSystem,
    /// The id, NUL-terminated: 6 characters for GameCube / Wii, 8 hex digits for an Xbox.
    pub id: [c_char; 9],
}

fn fill(out: &mut GmDiscId, result: DiscIdResult) {
    *out = GmDiscId {
        status: GmDiscIdStatus::Unreadable,
        region: GmDiscRegion::Unknown,
        system: GmDiscSystem::GameCube,
        id: [0; 9],
    };
    match result {
        DiscIdResult::Ok { id, region, system } => {
            out.status = GmDiscIdStatus::Ok;
            out.region = match region {
                DiscRegion::NtscU => GmDiscRegion::NtscU,
                DiscRegion::NtscJ => GmDiscRegion::NtscJ,
                DiscRegion::Pal => GmDiscRegion::Pal,
                DiscRegion::NtscK => GmDiscRegion::NtscK,
                DiscRegion::Unknown => GmDiscRegion::Unknown,
            };
            out.system = match system {
                DiscSystem::GameCube => GmDiscSystem::GameCube,
                DiscSystem::Wii => GmDiscSystem::Wii,
                DiscSystem::Xbox => GmDiscSystem::Xbox,
            };
            for (slot, b) in out.id.iter_mut().zip(id.bytes().take(8)) {
                *slot = b as c_char;
            }
        }
        DiscIdResult::Refused(reason) => {
            out.status = match reason {
                DiscIdRefusal::TooShort => GmDiscIdStatus::TooShort,
                DiscIdRefusal::BadMagic => GmDiscIdStatus::BadMagic,
                DiscIdRefusal::UnsupportedContainer => GmDiscIdStatus::UnsupportedContainer,
                DiscIdRefusal::Unreadable => GmDiscIdStatus::Unreadable,
            };
        }
    }
}

/// Reads a GameCube, Wii or Xbox disc id from the first bytes of an image (2048 are enough), never from a file name.
/// Always [`GmStatus::Ok`] when `out` is filled; whether an id was found is `out->status`.
///
/// Safety: `data` points to `len` readable bytes (may be null when `len` is 0); `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_disc_id_bytes(data: *const u8, len: usize, out: *mut GmDiscId) -> GmStatus {
    if out.is_null() || (data.is_null() && len != 0) {
        return GmStatus::ErrArgument;
    }
    // SAFETY: the caller promises `len` readable bytes at a non-null `data`.
    let bytes = if len == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(data, len) } };
    guard(GmStatus::ErrPanic, || {
        // SAFETY: `out` is non-null and writable.
        fill(unsafe { &mut *out }, ids::disc_id(bytes));
        GmStatus::Ok
    })
}

/// Like [`gm_disc_id_bytes`] for a file of `size` bytes read through `read`: the first 2048 bytes are read. A failing read
/// is `out->status == GM_DISC_ID_STATUS_UNREADABLE`.
///
/// Safety: `read` honours the [`GmReadFn`] contract; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_disc_id_callback(
    size: u64,
    read: Option<GmReadFn>,
    user_data: *mut c_void,
    out: *mut GmDiscId,
) -> GmStatus {
    let (Some(read), false) = (read, out.is_null()) else { return GmStatus::ErrArgument };
    guard(GmStatus::ErrPanic, || {
        // SAFETY: `out` is non-null and writable.
        fill(unsafe { &mut *out }, ids::disc_id_of(&mut CallbackSource { size, read, user_data }));
        GmStatus::Ok
    })
}

/// Like [`gm_disc_id_callback`] for an open, readable, seekable file descriptor (positioned reads; not moved, not closed).
/// [`GmStatus::ErrUnsupported`] where there are no POSIX descriptors; a descriptor that cannot be read is `ErrIo`.
///
/// Safety: `fd` stays open for the call; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gm_disc_id_fd(fd: c_int, out: *mut GmDiscId) -> GmStatus {
    if out.is_null() {
        return GmStatus::ErrArgument;
    }
    guard(GmStatus::ErrPanic, || match fd::open(fd) {
        Ok(mut file) => {
            // SAFETY: `out` is non-null and writable.
            fill(unsafe { &mut *out }, ids::disc_id_of(&mut file));
            GmStatus::Ok
        }
        Err(status) => status,
    })
}
