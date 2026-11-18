// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reads from a caller's file descriptor with positioned reads.

use gamemeta_core::{GameInfo, ImageKind, RgbaImage};

use crate::GmStatus;

/// Reads the open descriptor `fd` as `file_name`. The descriptor is borrowed: not moved, not closed.
#[cfg(unix)]
pub fn read_fd(fd: std::os::fd::RawFd, file_name: &str) -> Result<GameInfo, GmStatus> {
    crate::read_tracked(open(fd)?, file_name)
}

/// The banner scene of the open descriptor `fd` (borrowed, as for [`read_fd`]).
#[cfg(unix)]
pub fn scene_fd(fd: std::os::fd::RawFd) -> Result<crate::scene::GmScene, GmStatus> {
    crate::scene::read_scene(open(fd)?)
}

/// The Wii channel banner of the open descriptor `fd` (borrowed, as for [`read_fd`]).
#[cfg(unix)]
pub fn banner_fd(fd: std::os::fd::RawFd) -> Result<crate::wii_banner::GmWiiBanner, GmStatus> {
    crate::wii_banner::read_banner(open(fd)?)
}

/// One picture of the base title of the `.wua` in the open descriptor `fd` (borrowed, as for [`read_fd`]).
#[cfg(unix)]
pub fn wua_image_fd(fd: std::os::fd::RawFd, kind: ImageKind) -> Result<RgbaImage, GmStatus> {
    crate::wua_image(open(fd)?, kind)
}

/// The file behind the descriptor `fd` (borrowed: duplicated, not moved or closed), as a [`ByteSource`](gamemeta_core::ByteSource).
#[cfg(unix)]
pub fn open(fd: std::os::fd::RawFd) -> Result<FileSource, GmStatus> {
    use std::os::fd::BorrowedFd;
    if fd < 0 {
        return Err(GmStatus::ErrArgument);
    }
    // SAFETY: the caller keeps `fd` open for the duration of the call.
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    let file = std::fs::File::from(borrowed.try_clone_to_owned().map_err(|_| GmStatus::ErrIo)?);
    let size = file.metadata().map_err(|_| GmStatus::ErrIo)?.len();
    Ok(FileSource { file, size })
}

/// File descriptors are POSIX; elsewhere this reports [`GmStatus::ErrUnsupported`].
#[cfg(not(unix))]
pub fn read_fd(_fd: i32, _file_name: &str) -> Result<GameInfo, GmStatus> {
    Err(GmStatus::ErrUnsupported)
}

/// File descriptors are POSIX; elsewhere this reports [`GmStatus::ErrUnsupported`].
#[cfg(not(unix))]
pub fn wua_image_fd(_fd: i32, _kind: ImageKind) -> Result<RgbaImage, GmStatus> {
    Err(GmStatus::ErrUnsupported)
}

/// The file behind a descriptor; where there are no POSIX descriptors, [`GmStatus::ErrUnsupported`].
#[cfg(not(unix))]
pub fn open(_fd: i32) -> Result<FileSource, GmStatus> {
    Err(GmStatus::ErrUnsupported)
}

/// File descriptors are POSIX; elsewhere this reports [`GmStatus::ErrUnsupported`].
#[cfg(not(unix))]
pub fn scene_fd(_fd: i32) -> Result<crate::scene::GmScene, GmStatus> {
    Err(GmStatus::ErrUnsupported)
}

/// File descriptors are POSIX; elsewhere this reports [`GmStatus::ErrUnsupported`].
#[cfg(not(unix))]
pub fn banner_fd(_fd: i32) -> Result<crate::wii_banner::GmWiiBanner, GmStatus> {
    Err(GmStatus::ErrUnsupported)
}

/// A [`gamemeta_core::ByteSource`] over a file, with positioned reads.
#[cfg(unix)]
#[derive(Debug)]
pub struct FileSource {
    file: std::fs::File,
    size: u64,
}

#[cfg(unix)]
impl gamemeta_core::ByteSource for FileSource {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), gamemeta_core::ReadError> {
        use std::os::unix::fs::FileExt;
        self.file.read_exact_at(buf, offset).map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => gamemeta_core::ReadError::OutOfRange,
            _ => gamemeta_core::ReadError::Io,
        })
    }
}

/// Never built where there are no descriptors: [`open`] always fails there.
#[cfg(not(unix))]
#[derive(Debug)]
pub struct FileSource;

#[cfg(not(unix))]
impl gamemeta_core::ByteSource for FileSource {
    fn size(&self) -> u64 {
        0
    }
    fn read_at(&mut self, _: u64, _: &mut [u8]) -> Result<(), gamemeta_core::ReadError> {
        Err(gamemeta_core::ReadError::Io)
    }
}
