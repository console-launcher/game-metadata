// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Readers for game file metadata: titles, publishers, icons, ids, regions, ratings.
//!
//! The caller owns all I/O. A reader takes a [`ByteSource`] (random-access reads the caller performs: a file, a
//! memory buffer, an Android content-URI descriptor, a network range request) and returns plain structs, with images
//! decoded to [`RgbaImage`] (8-bit RGBA, top-down). Nothing here opens files, spawns threads or touches a platform API,
//! and the crate is `no_std` + `alloc`.
//!
//! Readers never panic on malformed input and never read more than a few megabytes of a file: a damaged, truncated,
//! encrypted or unknown file is `None` (or a [`threeds::Reason`] from the lower-level 3DS calls).
//!
//! ```
//! use gamemeta_core::{read, SliceSource};
//! let bytes: &[u8] = &[];
//! assert!(read(&mut SliceSource::new(bytes), "game.3ds").is_none());
//! ```
#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod carts;
mod deflate;
mod formats;
pub mod gc;
pub mod ids;
mod image;
mod info;
mod le;
pub mod nds;
pub mod png;
mod sha256;
mod source;
pub(crate) mod text;
pub mod threeds;
pub mod wii;
pub mod wiiu;
mod zip;
mod zstd;

pub use formats::{Support, supported_formats};
pub use image::RgbaImage;
pub use info::{Extent, GameIds, GameInfo, IconStep, ImageKind, Language, Platform, RatingBoard, Region, Title};
pub use source::{ByteSource, ReadError, SliceSource};

/// The formats a file name's extension selects. Case-insensitive; `None` for an extension no reader handles.
pub fn format_for(file_name: &str) -> Option<Format> {
    let ext = file_name.rsplit_once('.').map(|(_, e)| e)?;
    let lower = |want: &str| ext.eq_ignore_ascii_case(want);
    if threeds::EXTENSIONS.into_iter().any(lower) {
        Some(Format::ThreeDs)
    } else if nds::is_rom(file_name) {
        Some(Format::Ds)
    } else if nds::is_zip(file_name) {
        Some(Format::DsZip)
    } else if wii::is_wad(file_name) {
        Some(Format::Wad)
    } else if wiiu::is_wua(file_name) {
        Some(Format::WiiUArchive)
    } else if wiiu::is_rpx(file_name) {
        Some(Format::WiiUTitle)
    } else if gc::is_candidate(file_name) || wii::is_candidate(file_name) {
        Some(Format::Disc)
    } else {
        carts::Family::for_extension(ext).map(Format::Cart)
    }
}

/// A file format family a reader handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// Nintendo 3DS: NCSD (`.3ds`, `.cci`), NCCH (`.cxi`, `.app`) and CIA (`.cia`).
    ThreeDs,
    /// Nintendo DS and DSi ROMs: `.nds`, `.dsi`, `.srl`, `.ids`.
    Ds,
    /// A zip holding a DS ROM (the first ROM in it is read), or else a cartridge ROM of the families in [`carts`].
    DsZip,
    /// A GameCube or Wii disc image: `.iso`, `.gcm`, `.ciso`, `.gcz`, `.rvz`, `.wia`, `.tgc`, `.wbfs`. The container
    /// and the disc type come from the file's magic, not its extension.
    Disc,
    /// A Wii WAD (an installable channel, WiiWare or Virtual Console title): `.wad`.
    Wad,
    /// A Wii U `.wua` (a ZArchive of titles); the base title is read.
    WiiUArchive,
    /// A Wii U `.rpx`: the title is the folder around it, which only the caller can open, so [`read`] gives nothing;
    /// use [`wiiu::read_title`] with [`wiiu::OpenerFiles`].
    WiiUTitle,
    /// A cartridge ROM: Game Boy and Color, Game Boy Advance, Nintendo 64, Super Nintendo, NES, Genesis / Mega Drive and
    /// 32X, Master System and Game Gear. The family follows from the extension; the header decides whether the file
    /// reads (an ambiguous `.bin` is read as Genesis and gives nothing without a Genesis header).
    Cart(carts::Family),
}

/// Reads what the file's headers say about the game, choosing the reader by `file_name`'s extension
/// ([`format_for`]). Reads headers and the icon only, never a whole file. `None` for a file of an unknown kind, one
/// that yields nothing, or one whose source failed to read.
pub fn read<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<GameInfo> {
    match format_for(file_name)? {
        Format::ThreeDs => threeds::read_info(source),
        Format::Ds => nds::read_info(source),
        Format::DsZip => nds::read_zip_info(source).or_else(|| carts::read_zip_info(source)),
        Format::Disc => gc::read_info(source).or_else(|| wii::read_info(source)),
        Format::Wad => wii::read_wad_info(source),
        Format::WiiUArchive => wiiu::read_wua_info(source),
        Format::WiiUTitle => None,
        Format::Cart(family) => carts::read_info(source, family, file_name),
    }
}
