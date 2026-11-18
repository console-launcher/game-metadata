// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Nintendo 3DS: NCSD (`.3ds`, `.cci`), CIA, or a bare NCCH (`.cxi`, `.app`) down to the ExeFS `icon` (SMDH) and
//! `banner` files.
//!
//! Only decrypted content is readable: an encrypted ExeFS is [`Reason::Encrypted`] and no key is ever used.

pub mod pica;
pub mod smdh;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::le::{u16_at, u32_at};
use crate::source::fetch;
use crate::text::{is_blank, joined_lines, trim};
use crate::{ByteSource, Extent, GameIds, GameInfo, Language, Platform, RatingBoard, ReadError, Region, Title};
use smdh::Smdh;

/// Why a 3DS file gives no banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    /// No NCSD, NCCH or CIA header.
    NotA3dsFile,
    /// The content is encrypted.
    Encrypted,
    /// Truncated or inconsistent headers, or the source failed.
    Damaged,
    /// A readable ExeFS without a `banner` entry.
    NoBannerInExeFs,
}

/// The parts of a 3DS file as its headers place them ([`locate`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    /// From a plain NCCH header: the title id, 16 upper-case hex digits.
    pub title_id: Option<String>,
    /// From a plain NCCH header: the product code.
    pub product_code: Option<String>,
    /// The ExeFS `banner` entry.
    pub banner: Option<Extent>,
    /// The ExeFS `icon` entry (the SMDH).
    pub icon: Option<Extent>,
    /// Why the file cannot give a banner; `None` when it can, or when the ExeFS simply has none.
    pub problem: Option<Reason>,
}

/// The banner and icon file bytes of a 3DS file ([`read`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The ExeFS `banner` (a CBMD).
    pub banner: Vec<u8>,
    /// The ExeFS `icon` (an SMDH), when present and readable.
    pub icon: Option<Vec<u8>>,
}

/// The extensions of a 3DS file this reads.
pub(crate) const EXTENSIONS: [&str; 5] = ["3ds", "cci", "cxi", "cia", "app"];
const MEDIA_UNIT: u64 = 0x200;
const MAX_BANNER: u64 = 4 << 20;
const MAX_ICON: u64 = 64 << 10;
const MAGIC_NCSD: u32 = 0x4453_434E; // "NCSD"
const MAGIC_NCCH: u32 = 0x4843_434E; // "NCCH"

/// A header walk that cannot go on: inconsistent offsets, or the source failed. Both read as [`Reason::Damaged`].
#[derive(Debug)]
struct Broken;

impl From<ReadError> for Broken {
    fn from(_: ReadError) -> Self {
        Broken
    }
}

/// Where the NCCH is.
enum Ncch {
    At(u64),
    /// A container whose content is not a plain NCCH.
    Encrypted,
}

/// Where the parts of a 3DS file are, from its headers alone (a few KB; the banner is not read). An unreadable file is
/// a layout with a [`Layout::problem`] (and the ids when the NCCH header is plain).
pub fn locate<S: ByteSource + ?Sized>(source: &mut S) -> Layout {
    let walked = match locate_ncch(source) {
        Ok(None) => Ok(problem(Reason::NotA3dsFile)),
        Ok(Some(Ncch::Encrypted)) => Ok(problem(Reason::Encrypted)),
        Ok(Some(Ncch::At(at))) => locate_exefs(source, at),
        Err(e) => Err(e),
    };
    walked.unwrap_or_else(|Broken| problem(Reason::Damaged))
}

fn problem(reason: Reason) -> Layout {
    Layout { problem: Some(reason), ..Layout::default() }
}

/// The banner and icon bytes, or why there are none.
pub fn read<S: ByteSource + ?Sized>(source: &mut S) -> Result<Found, Reason> {
    let layout = locate(source);
    if let Some(reason) = layout.problem {
        return Err(reason);
    }
    let banner = layout.banner.ok_or(Reason::NoBannerInExeFs)?;
    let banner = fetch(source, banner.offset, banner.size).map_err(|_| Reason::Damaged)?;
    let icon = match layout.icon.map(|i| fetch(source, i.offset, i.size)) {
        None | Some(Err(ReadError::OutOfRange)) => None,
        Some(Err(ReadError::Io)) => return Err(Reason::Damaged),
        Some(Ok(bytes)) => Some(bytes),
    };
    Ok(Found { banner, icon })
}

/// The title id from the NCCH header; `None` for a file that is not a 3DS file, is unreadable, or has no plain NCCH.
/// Reads a few hundred bytes.
pub fn title_id<S: ByteSource + ?Sized>(source: &mut S) -> Option<String> {
    let Ok(Some(Ncch::At(at))) = locate_ncch(source) else { return None };
    let id = fetch(source, at + 0x118, 8).ok()?;
    Some(hex_id(&id))
}

/// 8 little-endian bytes as 16 upper-case hex digits, most significant first.
fn hex_id(le: &[u8]) -> String {
    let mut s = String::with_capacity(16);
    for b in le.iter().rev() {
        let _ = write!(s, "{b:02X}");
    }
    s
}

/// `Ok(None)` when the file is no 3DS file.
fn locate_ncch<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<Ncch>, Broken> {
    let size = source.size();
    let head = match fetch(source, 0, size.min(0x200)) {
        Ok(h) => h,
        Err(ReadError::OutOfRange) => return Ok(None),
        Err(ReadError::Io) => return Err(Broken),
    };
    let magic = u32_at(&head, 0x100);
    if magic == Some(MAGIC_NCSD) {
        let partition = u64::from(u32_at(&head, 0x120).ok_or(Broken)?) * MEDIA_UNIT;
        if partition == 0 || partition >= size {
            return Err(Broken);
        }
        return Ok(Some(if is_ncch(source, partition)? { Ncch::At(partition) } else { Ncch::Encrypted }));
    }
    if magic == Some(MAGIC_NCCH) {
        return Ok(Some(Ncch::At(0)));
    }
    // CIA: header size 0x2020, type 0.
    if head.len() >= 0x20 && u32_at(&head, 0) == Some(0x2020) && u16_at(&head, 4) == Some(0) {
        let align = |v: u64| (v + 63) & !63;
        let section = |at: usize| u32_at(&head, at).map(u64::from).ok_or(Broken);
        let content =
            align(0x2020) + align(section(8)?) + align(section(0xC)?) + align(section(0x10)?) + align(section(0x14)?);
        if content + 0x200 > size {
            return Err(Broken);
        }
        return Ok(Some(if is_ncch(source, content)? { Ncch::At(content) } else { Ncch::Encrypted }));
    }
    Ok(None)
}

fn is_ncch<S: ByteSource + ?Sized>(source: &mut S, at: u64) -> Result<bool, Broken> {
    match fetch(source, at + 0x100, 4) {
        Ok(m) => Ok(u32_at(&m, 0) == Some(MAGIC_NCCH)),
        Err(ReadError::OutOfRange) => Ok(false),
        Err(ReadError::Io) => Err(Broken),
    }
}

fn locate_exefs<S: ByteSource + ?Sized>(source: &mut S, ncch: u64) -> Result<Layout, Broken> {
    let header = fetch(source, ncch, 0x200)?;
    let title_id = Some(hex_id(&header[0x118..0x120]));
    let product_code = product_code(&header[0x150..0x160]);
    let ids = |reason: Option<Reason>| Layout {
        title_id: title_id.clone(),
        product_code: product_code.clone(),
        problem: reason,
        ..Layout::default()
    };
    let exefs_offset = u64::from(u32_at(&header, 0x1A0).ok_or(Broken)?) * MEDIA_UNIT;
    let exefs_size = u64::from(u32_at(&header, 0x1A4).ok_or(Broken)?) * MEDIA_UNIT;
    if exefs_offset == 0 || exefs_size < 0x200 {
        return Ok(ids(None));
    }
    let base = ncch + exefs_offset;
    if base + 0x200 > source.size() {
        return Ok(ids(Some(Reason::Damaged)));
    }
    let e = fetch(source, base, 0x200)?;
    // The NoCrypto flag says plain; some dumps are plain without setting it, so the header itself is the judge: an
    // encrypted ExeFS header is noise and fails the name and range checks below.
    let (mut icon, mut banner) = (None, None);
    for entry in e[..160].chunks_exact(16) {
        let name_len = entry[..8].iter().position(|&b| b == 0).unwrap_or(8);
        let name = &entry[..name_len];
        if name.is_empty() {
            continue;
        }
        let printable = name.iter().all(|b| (0x20..=0x7E).contains(b));
        let valid = printable && (name[0] == b'.' || name.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_'));
        let off = u64::from(u32_at(entry, 8).ok_or(Broken)?);
        let len = u64::from(u32_at(entry, 12).ok_or(Broken)?);
        if !valid || off + len > exefs_size {
            return Ok(ids(Some(Reason::Encrypted)));
        }
        // A repeated name: the last entry wins.
        match name {
            b"icon" => icon = Some((off, len)),
            b"banner" => banner = Some((off, len)),
            _ => {}
        }
    }
    let icon = icon
        .filter(|&(_, len)| (1..=MAX_ICON).contains(&len))
        .map(|(off, len)| Extent { offset: base + 0x200 + off, size: len });
    let Some((off, len)) = banner else { return Ok(Layout { icon, ..ids(None) }) };
    if len == 0 || len > MAX_BANNER {
        return Ok(ids(Some(Reason::Damaged)));
    }
    Ok(Layout { banner: Some(Extent { offset: base + 0x200 + off, size: len }), icon, ..ids(None) })
}

/// Printable ASCII with trailing NULs dropped, not blank; else `None`.
fn product_code(raw: &[u8]) -> Option<String> {
    let end = raw.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    let code = &raw[..end];
    let ok = code.iter().all(|b| (0x20..=0x7E).contains(b)) && code.iter().any(|&b| b != b' ');
    ok.then(|| code.iter().map(|&b| b as char).collect())
}

/// Titles, icons, ids, regions, ratings and the banner location of a 3DS file. Reads the headers and the SMDH only;
/// `None` when the file gives neither a title id nor an SMDH, or the source fails.
pub fn read_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo> {
    let layout = locate(source);
    let smdh = match layout.icon.map(|i| fetch(source, i.offset, i.size)) {
        None | Some(Err(ReadError::OutOfRange)) => None,
        Some(Err(ReadError::Io)) => return None,
        Some(Ok(bytes)) => Smdh::parse(&bytes),
    };
    if layout.title_id.is_none() && smdh.is_none() {
        return None;
    }
    let mut info = GameInfo {
        ids: GameIds { title_id: layout.title_id, product_code: layout.product_code, ..GameIds::default() },
        banner: layout.banner,
        ..GameInfo::new(Platform::ThreeDs)
    };
    if let Some(smdh) = smdh {
        for (slot, t) in &smdh.titles {
            let Some(language) = Language::from_slot(*slot) else { continue };
            let long_or_short = if is_blank(&t.long_title) { &t.short_title } else { &t.long_title };
            let publisher = trim(&t.publisher);
            info.titles.push((
                language,
                Title {
                    name: joined_lines(long_or_short),
                    short_name: joined_lines(&t.short_title),
                    publisher: (!publisher.is_empty()).then(|| publisher.into()),
                },
            ));
        }
        info.region_free = smdh.region_lockout == smdh::REGION_FREE;
        if !info.region_free {
            info.regions = Region::ALL.into_iter().filter(|r| smdh.region_lockout & (1 << *r as u32) != 0).collect();
        }
        info.ratings = RatingBoard::ALL
            .into_iter()
            .filter_map(|board| {
                let v = smdh.ratings[board as usize];
                match v {
                    _ if v & 0x80 == 0 || v & 0x40 != 0 => None,
                    _ if v & 0x20 != 0 => Some((board, 0)),
                    _ => Some((board, v & 0x1F)),
                }
            })
            .collect();
        info.icon = Some(smdh.large_icon);
        info.small_icon = Some(smdh.small_icon);
    }
    Some(info)
}
