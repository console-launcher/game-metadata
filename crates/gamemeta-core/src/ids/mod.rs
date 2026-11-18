// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The ids a game file carries in its header, the way a launcher keys a local game: PlayStation serials, GameCube/Wii
//! game ids, 3DS / Wii / Wii U / Switch title ids, a CHD's raw SHA-1, the CRC-32 and size of the one ROM in a zip, a PSP
//! game's `PARAM.SFO` fields, an Xbox title id and the RetroAchievements hash.
//!
//! Like every reader here these read a few KB through a [`ByteSource`] (no I/O of their own) and give `None` for
//! anything they do not recognize. A file that points at other files (a cue sheet at its track, a Wii U `.rpx` at its
//! `meta/` folder) reaches them through an [`Opener`] the caller supplies. [`read`] picks the readers by the file name's
//! extension; [`RaHasher`] is a streaming hasher because the RA hash needs the whole file.
//!
//! ```
//! use gamemeta_core::{ids, SliceSource};
//! assert!(ids::read(&mut SliceSource::new(&[]), "game.iso").is_none());
//! ```

mod disc_id;
mod gamecube;
pub(crate) mod iso9660;
mod playstation;
pub mod psp;
mod ra;
pub(crate) use ra::WHOLE as RA_WHOLE;
mod small;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

pub use disc_id::{DiscIdRefusal, DiscIdResult, DiscRegion, DiscSystem, HEADER_BYTES, disc_id, disc_id_of};
pub use ra::{RaHasher, ra_hash};

use crate::source::fetch;
use crate::wiiu::{self, TitleFiles};
use crate::{ByteSource, threeds, wii};

/// What a file's header says of its identity. Every field is optional; [`read`] gives `None` when all are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ids {
    /// PlayStation 1 / 2 serial from `SYSTEM.CNF` (`SLUS00001`), or a PSP disc id (`ULUS00001`): upper case, letters and digits.
    pub serial: Option<String>,
    /// GameCube / Wii 6-character game id (`GZZE01`).
    pub game_id: Option<String>,
    /// 3DS, Wii WAD, Wii U or Switch title id: 16 upper-case hex digits.
    pub title_id: Option<String>,
    /// The raw SHA-1 of the data inside a CHD v5 (header offset 0x40), lower-case hex.
    pub chd_sha1: Option<String>,
    /// CRC-32 of the one ROM inside a zip, from its central directory, eight lower-case hex digits.
    pub zip_crc: Option<String>,
    /// Size of that ROM.
    pub zip_size: Option<u64>,
    /// PSP: `TITLE` of the `PARAM.SFO` (from an `EBOOT.PBP` or an ISO).
    pub psp_title: Option<String>,
    /// PSP: `DISC_ID` of the `PARAM.SFO`, as written.
    pub psp_disc_id: Option<String>,
    /// Xbox: the title id of a `default.xbe`, eight upper-case hex digits.
    pub xbox_id: Option<String>,
    /// RetroAchievements hash (lower-case hex MD5): never set by [`read`]; the caller fills it from [`RaHasher`] / [`ra_hash`].
    pub ra_md5: Option<String>,
}

impl Ids {
    /// Whether no field is set.
    pub fn is_empty(&self) -> bool {
        *self == Ids::default()
    }

    /// The one id a launcher keys a game by: the serial, else the game id, else the title id. (The original reader's
    /// `titleId`: "serial, game id or title ID, normalized".)
    pub fn primary_id(&self) -> Option<&str> {
        self.serial.as_deref().or(self.game_id.as_deref()).or(self.title_id.as_deref())
    }
}

/// Opens a file by its path relative to the folder of the game file being read: a cue sheet's `game (Track 1).bin`, a Wii U
/// `.rpx`'s `../meta/meta.xml`. `None` for a file that is not there or cannot be opened. Any
/// `FnMut(&str) -> Option<Box<dyn ByteSource>>` is an opener.
pub trait Opener {
    /// Opens `path`. The source may borrow the opener (a binding that calls back into its host); it is dropped
    /// (closed) before the next `open`.
    fn open(&mut self, path: &str) -> Option<Box<dyn ByteSource + '_>>;
}

impl<F: FnMut(&str) -> Option<Box<dyn ByteSource>>> Opener for F {
    fn open(&mut self, path: &str) -> Option<Box<dyn ByteSource + '_>> {
        self(path)
    }
}

/// An opener that has no files.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoFiles;

impl Opener for NoFiles {
    fn open(&mut self, _: &str) -> Option<Box<dyn ByteSource + '_>> {
        None
    }
}

/// `len` bytes at `offset`, all or nothing (a range outside the source or a failed read is `None`).
pub(crate) fn get<S: ByteSource + ?Sized>(source: &mut S, offset: u64, len: u64) -> Option<Vec<u8>> {
    fetch(source, offset, len).ok()
}

/// Up to `len` bytes at `offset`: fewer at the end of the source, `None` when `offset` is at or past it or a read failed.
pub(crate) fn get_upto<S: ByteSource + ?Sized>(source: &mut S, offset: u64, len: u64) -> Option<Vec<u8>> {
    let size = source.size();
    if offset >= size {
        return None;
    }
    get(source, offset, len.min(size - offset))
}

/// ISO-8859-1 bytes as text.
pub(crate) fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// The extensions [`read_with`] has a reader for (the arms of its `match`; a test keeps the two equal).
pub const EXTENSIONS: [&str; 23] = [
    "zip", "chd", "iso", "gcm", "rvz", "wia", "wbfs", "ciso", "gcz", "bin", "img", "cue", "pbp", "wad", "3ds", "cci",
    "cxi", "app", "cia", "nsp", "wua", "rpx", "xbe",
];

/// The ids in `source`, a file called `file_name`; the readers are chosen by its extension. A file that points at other
/// files finds none of them (see [`read_with`]).
pub fn read<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<Ids> {
    read_with(source, file_name, &mut NoFiles)
}

/// [`read`] with an [`Opener`] for the files beside the game file: the track of a `.cue`, the `meta` folder of a Wii U `.rpx`.
///
/// | Extension | Gives |
/// |---|---|
/// | `.iso` | `game_id` (GameCube / Wii), `serial` (PlayStation `SYSTEM.CNF`, PSP `PARAM.SFO` / `UMD_DATA.BIN`), `psp_*` |
/// | `.gcm .rvz .wia .wbfs .ciso .gcz` | `game_id` |
/// | `.bin .img` | `serial` (a plain or raw 2352-byte data track) |
/// | `.cue` | `serial` of the first data track, opened through `opener` |
/// | `.pbp` | `psp_title`, `psp_disc_id`, `serial` |
/// | `.chd` | `chd_sha1` |
/// | `.zip` | `zip_crc`, `zip_size` |
/// | `.wad` `.3ds .cci .cxi .app .cia` `.nsp` `.wua` `.rpx` | `title_id` (the `.rpx` through `opener`) |
/// | `.xbe` | `xbox_id` |
pub fn read_with<S: ByteSource + ?Sized>(source: &mut S, file_name: &str, opener: &mut dyn Opener) -> Option<Ids> {
    let ext = file_name.rsplit_once('.').map_or("", |(_, e)| e).to_ascii_lowercase();
    let mut ids = Ids::default();
    match ext.as_str() {
        "zip" => {
            (ids.zip_crc, ids.zip_size) = small::zip_entry(source).map_or((None, None), |(c, s)| (Some(c), Some(s)))
        }
        "chd" => ids.chd_sha1 = small::chd_sha1(source),
        "iso" => {
            ids.game_id = gamecube::game_id(source);
            let layout = iso9660::layout_of(source);
            ids.serial = playstation::serial(source);
            read_psp(source, layout, &mut ids);
        }
        "gcm" | "rvz" | "wia" | "wbfs" | "ciso" | "gcz" => ids.game_id = gamecube::game_id(source),
        "bin" | "img" => ids.serial = playstation::serial(source),
        "cue" => ids.serial = playstation::serial_from_cue(source, opener),
        "pbp" => {
            if let Some(sfo) = psp::from_pbp(source) {
                ids.serial = sfo.disc_id.as_deref().and_then(psp::normalize);
                (ids.psp_title, ids.psp_disc_id) = (sfo.title, sfo.disc_id);
            }
        }
        "wad" => ids.title_id = wii::read_wad_info(source).and_then(|i| i.ids.title_id),
        "3ds" | "cci" | "cxi" | "app" | "cia" => ids.title_id = threeds::title_id(source),
        "nsp" => ids.title_id = small::nsp_title_id(source),
        "wua" => ids.title_id = wiiu::open_wua(source).and_then(|mut files| wiiu::title_id(&mut files)),
        "rpx" => ids.title_id = wiiu::title_id(&mut RpxFiles(opener)),
        "xbe" => {
            if let DiscIdResult::Ok { id, system: DiscSystem::Xbox, .. } = disc_id_of(source) {
                ids.xbox_id = Some(id);
            }
        }
        _ => {}
    }
    (!ids.is_empty()).then_some(ids)
}

/// A Wii U title seen from its `.rpx` through an [`Opener`]: the title path `meta/meta.xml` is opened as `../meta/meta.xml`
/// (what [`wiiu::OpenerFiles`] does for an opener that returns owned sources).
struct RpxFiles<'a>(&'a mut dyn Opener);

impl TitleFiles for RpxFiles<'_> {
    fn size(&mut self, path: &str) -> Option<u64> {
        Some(self.0.open(&alloc::format!("../{path}"))?.size())
    }

    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        let mut source = self.0.open(&alloc::format!("../{path}"))?;
        wiiu::read_all(&mut *source, max)
    }
}

fn read_psp<S: ByteSource + ?Sized>(source: &mut S, layout: iso9660::Layout, ids: &mut Ids) {
    if let Some(sfo) = psp::sfo_file(source, layout).and_then(|bytes| psp::parse(&bytes)) {
        (ids.psp_title, ids.psp_disc_id) = (sfo.title, sfo.disc_id);
    }
    if let Some(serial) = psp::disc_id(source, layout) {
        ids.serial.get_or_insert(serial);
    }
}
