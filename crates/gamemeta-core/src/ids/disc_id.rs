// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A GameCube, Wii or Xbox disc id from the first 2048 bytes of an image, never from a file name. A header that is short,
//! unrecognized or unreadable is refused with a reason instead of guessed at.

use alloc::string::String;

use crate::ByteSource;

/// Bytes of an image [`disc_id_of`] looks at: enough for the GC/Wii header, an RVZ/WIA container header and an XBE certificate.
pub const HEADER_BYTES: usize = 2048;

const GC_MAGIC: i64 = 0xC233_9F3D;
const WII_MAGIC: i64 = 0x5D1C_9EA3;
const GCZ_MAGIC: i64 = 0xB10B_C001;
const PLAIN_MIN: usize = 0x20;
/// RVZ and WIA keep the first 0x80 bytes of the disc, uncompressed, after header 1 (0x48) and four header-2 words.
const CONTAINER_DISC_HEADER_AT: usize = 0x58;
const CONTAINER_DISC_HEADER_SIZE: usize = 0x80;
const XBE_BASE_AT: usize = 0x104;
const XBE_CERT_AT: usize = 0x118;
const XBE_CERT_TITLE_ID: i64 = 0x08;
const XBE_CERT_REGION: i64 = 0xA0;

/// The sales region an id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiscRegion {
    /// NTSC-U (North America).
    NtscU,
    /// NTSC-J (Japan).
    NtscJ,
    /// PAL.
    Pal,
    /// NTSC-K (Korea).
    NtscK,
    /// Not told by the id.
    Unknown,
}

/// The console an id belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiscSystem {
    /// GameCube.
    GameCube,
    /// Wii.
    Wii,
    /// Xbox (an `default.xbe` header).
    Xbox,
}

/// Why no id was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiscIdRefusal {
    /// Fewer bytes than the header needs.
    TooShort,
    /// No disc or executable magic.
    BadMagic,
    /// WBFS, CISO or GCZ: the id is not in the first bytes.
    UnsupportedContainer,
    /// The source failed, or the id is not made of ASCII letters and digits, or an address is out of range.
    Unreadable,
}

/// A header's id or the reason there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscIdResult {
    /// `id` is 6 characters for GameCube and Wii, the 8-digit upper-case hex title id for an Xbox `default.xbe`.
    Ok {
        /// The id.
        id: String,
        /// The region the id names.
        region: DiscRegion,
        /// The console.
        system: DiscSystem,
    },
    /// Refused, with the reason.
    Refused(DiscIdRefusal),
}

fn u32be(h: &[u8], at: usize) -> i64 {
    h.get(at..at + 4).map_or(-1, |s| i64::from(u32::from_be_bytes([s[0], s[1], s[2], s[3]])))
}

fn u32le(h: &[u8], at: usize) -> i64 {
    h.get(at..at + 4).map_or(-1, |s| i64::from(u32::from_le_bytes([s[0], s[1], s[2], s[3]])))
}

/// The id in `header` (the first bytes of an image; 2048 are enough).
pub fn disc_id(header: &[u8]) -> DiscIdResult {
    if header.starts_with(b"RVZ\x01") || header.starts_with(b"WIA\x01") {
        from_container(header)
    } else if header.starts_with(b"WBFS") || header.starts_with(b"CISO") || u32le(header, 0) == GCZ_MAGIC {
        refused(DiscIdRefusal::UnsupportedContainer)
    } else if header.starts_with(b"XBEH") {
        from_xbe(header)
    } else {
        from_plain(header)
    }
}

/// Reads the first [`HEADER_BYTES`] of `source` and gives [`disc_id`] of them; a source that fails is
/// [`DiscIdRefusal::Unreadable`].
pub fn disc_id_of<S: ByteSource + ?Sized>(source: &mut S) -> DiscIdResult {
    let n = source.size().min(HEADER_BYTES as u64) as usize;
    let mut buf = alloc::vec![0u8; n];
    match source.read_at(0, &mut buf) {
        Ok(()) => disc_id(&buf),
        Err(_) => refused(DiscIdRefusal::Unreadable),
    }
}

fn from_container(h: &[u8]) -> DiscIdResult {
    let end = CONTAINER_DISC_HEADER_AT + CONTAINER_DISC_HEADER_SIZE;
    match h.get(CONTAINER_DISC_HEADER_AT..end) {
        Some(disc) => from_plain(disc),
        None => refused(DiscIdRefusal::TooShort),
    }
}

fn from_plain(h: &[u8]) -> DiscIdResult {
    if h.len() < PLAIN_MIN {
        return refused(DiscIdRefusal::TooShort);
    }
    let system = if u32be(h, 0x1C) == GC_MAGIC {
        DiscSystem::GameCube
    } else if u32be(h, 0x18) == WII_MAGIC {
        DiscSystem::Wii
    } else {
        return refused(DiscIdRefusal::BadMagic);
    };
    let id = &h[..6];
    if !id.iter().all(u8::is_ascii_alphanumeric) {
        return refused(DiscIdRefusal::Unreadable);
    }
    let text: String = id.iter().map(|&c| char::from(c)).collect();
    DiscIdResult::Ok { region: region_of(id[3]), id: text, system }
}

fn from_xbe(h: &[u8]) -> DiscIdResult {
    if h.len() < XBE_CERT_AT + 4 {
        return refused(DiscIdRefusal::TooShort);
    }
    let cert = u32le(h, XBE_CERT_AT) - u32le(h, XBE_BASE_AT);
    if cert < 0 {
        return refused(DiscIdRefusal::Unreadable);
    }
    if cert + XBE_CERT_REGION + 4 > h.len() as i64 {
        return refused(DiscIdRefusal::TooShort);
    }
    let title = u32le(h, (cert + XBE_CERT_TITLE_ID) as usize);
    let region = match u32le(h, (cert + XBE_CERT_REGION) as usize) & 7 {
        1 => DiscRegion::NtscU,
        2 => DiscRegion::NtscJ,
        4 => DiscRegion::Pal,
        _ => DiscRegion::Unknown,
    };
    DiscIdResult::Ok { id: alloc::format!("{title:08X}"), region, system: DiscSystem::Xbox }
}

fn region_of(c: u8) -> DiscRegion {
    match c {
        b'E' | b'N' => DiscRegion::NtscU,
        b'J' => DiscRegion::NtscJ,
        b'K' | b'Q' | b'T' => DiscRegion::NtscK,
        b'P' | b'D' | b'F' | b'I' | b'S' | b'H' | b'U' | b'X' | b'Y' | b'Z' => DiscRegion::Pal,
        _ => DiscRegion::Unknown,
    }
}

fn refused(reason: DiscIdRefusal) -> DiscIdResult {
    DiscIdResult::Refused(reason)
}
