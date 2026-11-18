// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Nintendo Wii: disc images (ISO, WBFS, WIA, RVZ, CISO, GCZ) and WADs. No Nintendo key is ever used, held or derived,
//! and no encrypted data is read:
//!
//! - every disc format gives the disc header: game id, maker, internal title and region (the Wii magic 0x5D1C9EA3 tells
//!   it from a GameCube disc);
//! - an RVZ stores the game partition decrypted, so its file system is walked for `opening.bnr` (IMET titles per
//!   language and the title logo); a WIA's groups need bzip2 or LZMA, an ISO's, WBFS's, CISO's and GCZ's partitions are
//!   encrypted: those give the header only;
//! - a WAD gives its title id (and the 4 character game id inside it) from the plain header and title metadata.
//!
//! Format facts: wiibrew and the public RVZ format notes (see `NOTICE.md`).

pub mod banner;
mod lz77;
mod opening;
mod tpl;
mod u8_archive;
mod wad;

#[cfg(test)]
mod tests;

use alloc::vec::Vec;

use crate::gc::rvz::Rvz;
use crate::gc::{bnr, containers, disc};
use crate::source::fetch_opt;
use crate::{ByteSource, GameIds, GameInfo, ImageKind, Platform, ReadError, RgbaImage};

pub(crate) const EXTENSIONS: [&str; 7] = ["iso", "wbfs", "rvz", "wia", "ciso", "gcz", "wad"];
const HEADER: u64 = 0x60;
const MAX_OPENING: u64 = 2 * 1024 * 1024;
const WII_MAGIC: [u8; 4] = [0x5D, 0x1C, 0x9E, 0xA3];

/// Whether a file of this name could be a Wii disc or WAD this reader opens; no file access.
pub fn is_candidate(file_name: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(_, e)| EXTENSIONS.iter().any(|w| e.eq_ignore_ascii_case(w)))
}

/// Whether a file of this name is a WAD.
pub fn is_wad(file_name: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case("wad"))
}

/// A Wii disc image's ids, region and title, and (RVZ only) its channel titles and pictures. `None` when the file is
/// not a Wii disc this reader recognizes.
pub fn read_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo> {
    read_disc(source).ok().flatten()
}

/// A WAD's title id (and the game id inside it). `None` when the file is not a WAD.
pub fn read_wad_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo> {
    let info = wad::read(source).ok().flatten()?;
    let mut out = GameInfo::new(Platform::Wii);
    out.ids = GameIds { game_id: info.game_id, title_id: Some(info.title_id), ..GameIds::default() };
    Some(out)
}

/// What an `opening.bnr` holds: its titles and the title logo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpeningBnr {
    /// Titles by language, in language order.
    pub titles: Vec<(crate::Language, crate::Title)>,
    /// The title logo: the largest wide texture with transparency in `meta/icon.bin`.
    pub logo: Option<crate::RgbaImage>,
}

/// Parses an `opening.bnr` (or a channel's `00000000.app`) held in memory: the IMET titles and the logo. `None` when
/// the bytes carry no IMET header.
pub fn parse_opening_bnr(bytes: &[u8]) -> Option<OpeningBnr> {
    opening::parse(bytes).map(|p| OpeningBnr { titles: p.titles, logo: p.logo })
}

/// The disc info of the first disc in a WBFS: one head sector in, whose size the head's `hd_sec_sz_s` gives (512
/// bytes usually).
fn wbfs_disc_info_at<S: ByteSource + ?Sized>(source: &mut S) -> Result<u64, ReadError> {
    let shift = fetch_opt(source, 8, 1)?.map_or(9, |b| i32::from(b[0] as i8));
    Ok(if (9..=12).contains(&shift) { 1u64 << shift } else { 0x200 })
}

/// The first `len` bytes of the disc behind any container this reader understands: the copy WIA, RVZ and WBFS keep in their
/// own header, or the plain disc (ISO, CISO, GCZ). `Ok(None)` for a file that is none of those or is too short. Shared
/// with the id readers.
pub(crate) fn disc_header<S: ByteSource + ?Sized>(source: &mut S, len: u64) -> Result<Option<Vec<u8>>, ReadError> {
    let Some(magic) = fetch_opt(source, 0, 4)? else { return Ok(None) };
    match &magic[..] {
        // WIA and RVZ keep the first 0x80 disc bytes in their own header; WBFS a copy of the first 0x100 after its head sector.
        b"RVZ\x01" | b"WIA\x01" => fetch_opt(source, 0x58, len),
        b"WBFS" => {
            let at = wbfs_disc_info_at(source)?;
            fetch_opt(source, at, len)
        }
        // CISO: when block 0 is present it follows the 0x8000-byte header directly, so the disc header is read without
        // opening the block map (a truncated image still gives its id).
        b"CISO" => match fetch_opt(source, 8, 1)? {
            Some(map) if map[0] == 1 => fetch_opt(source, 0x8000, len),
            _ => Ok(None),
        },
        _ => match containers::open(&mut *source)? {
            Some(mut plain) => fetch_opt(&mut *plain, 0, len),
            None => Ok(None),
        },
    }
}

fn read_disc<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<GameInfo>, ReadError> {
    let is_rvz = fetch_opt(source, 0, 4)?.is_some_and(|m| &m[..] == b"RVZ\x01");
    let header = disc_header(source, HEADER)?;
    let Some(header) = header else { return Ok(None) };
    if header[0x18..0x1C] != WII_MAGIC {
        return Ok(None);
    }
    let game_id = disc::game_id(&header);
    let region = game_id.map(|id| id[3]);
    let charset = bnr::Charset::for_region(region);
    let parsed = if is_rvz { opening_of_rvz(source)? } else { None };
    let mut info = GameInfo::new(Platform::Wii);
    info.ids = GameIds {
        game_id: game_id.map(|id| id.iter().map(|&c| char::from(c)).collect()),
        maker_code: game_id.map(|id| id[4..].iter().map(|&c| char::from(c)).collect()),
        ..GameIds::default()
    };
    info.regions = disc::regions(region);
    let header_title = bnr::text(&header, 0x20, 0x40, charset);
    info.header_title = Some(header_title).filter(|t| !crate::text::is_blank(t));
    if let Some(parsed) = parsed {
        info.titles = parsed.titles;
        if parsed.logo.is_some() {
            info.images.push(ImageKind::Logo);
        }
        info.icon = parsed.logo;
    }
    Ok(Some(info))
}

/// `opening.bnr` of the root folder of the decrypted game partition of an RVZ, parsed; `None` when there is none.
fn opening_of_rvz<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<opening::Parsed>, ReadError> {
    Ok(opening_bytes_of_rvz(source)?.and_then(|bytes| opening::parse(&bytes)))
}

/// The bytes of `opening.bnr` in the root folder of the decrypted game partition of an RVZ.
fn opening_bytes_of_rvz<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<Vec<u8>>, ReadError> {
    let Some(mut rvz) = Rvz::open(source)? else { return Ok(None) };
    let Some(mut partition) = rvz.partition_data()? else { return Ok(None) };
    let Some(boot) = fetch_opt(&mut partition, 0, 0x440)? else { return Ok(None) };
    disc::read_root_file(&mut partition, &boot, "opening.bnr", MAX_OPENING, 2)
}

/// An RVZ's channel banner archive (`meta/banner.bin` of its `opening.bnr`, still wrapped); `Ok(None)` when there is none.
pub fn channel_banner_bin<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<Vec<u8>>, ReadError> {
    let Some(opening) = opening_bytes_of_rvz(source)? else { return Ok(None) };
    Ok(banner::banner_bin(&opening))
}

/// An RVZ's channel icon archive (`meta/icon.bin` of its `opening.bnr`, still wrapped): what the Wii Menu draws on a disc's grid
/// tile, in the same format as the banner archive. `Ok(None)` when there is none.
pub fn channel_icon_bin<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<Vec<u8>>, ReadError> {
    let Some(opening) = opening_bytes_of_rvz(source)? else { return Ok(None) };
    Ok(opening::icon_bin(&opening))
}

/// The largest `opening.bnr` or `banner.bin` file read whole.
const MAX_BANNER_FILE: u64 = 16 << 20;

/// The channel banner archive (`meta/banner.bin`, still wrapped) of what `source` holds: an RVZ disc (taken from its
/// `opening.bnr`), an `opening.bnr` (or a channel's `00000000.app`), or a `banner.bin` itself (returned as it is), told by
/// content. `Ok(None)` when there is none; only a failing source is an error. Whether the archive holds a readable banner is
/// [`banner::WiiBanner::open`]'s to say.
pub fn channel_banner_archive<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<Vec<u8>>, ReadError> {
    if fetch_opt(source, 0, 4)?.is_some_and(|m| &m[..] == b"RVZ\x01") {
        return channel_banner_bin(source);
    }
    let size = source.size();
    if size > MAX_BANNER_FILE {
        return Ok(None);
    }
    Ok(fetch_opt(source, 0, size)?.map(|bytes| banner::banner_bin(&bytes).unwrap_or(bytes)))
}

/// A channel banner from what `source` holds (see [`channel_banner_archive`]). `Ok(None)` when there is no banner or it does not
/// parse; only a failing source is an error.
pub fn open_channel_banner<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<banner::WiiBanner>, ReadError> {
    Ok(channel_banner_archive(source)?.and_then(|bin| banner::WiiBanner::open(&bin)))
}

/// An RVZ's channel banner drawn at rest (the start animation done) at one pixel per layout unit (608 x 456 for a channel
/// banner). `None` for any other file, a missing banner or one that does not parse. Not part of [`read`](crate::read): the
/// picture is drawn on demand.
pub fn channel_banner<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<RgbaImage> {
    if !file_name.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case("rvz")) {
        return None;
    }
    open_channel_banner(source).ok()??.poster(1.0)
}
