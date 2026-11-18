// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! PSP games: the title and disc id in a `PARAM.SFO`, read from an `EBOOT.PBP` or from a PSP ISO. Every offset is
//! bounds-checked, every read is capped at [`MAX_READ`], and nothing here panics: a file that is not what it claims is `None`.

use alloc::string::String;
use alloc::vec::Vec;

use super::iso9660::{self, Layout};
use super::{get_upto, latin1};
use crate::ByteSource;
use crate::le::{u16_at, u32_at};
use crate::text;

/// The most this reader asks for in one read.
pub const MAX_READ: usize = 16 * 1024;
const MAX_ENTRIES: u32 = 64;
const MAX_TITLE: usize = 128;

/// What a PSP game's `PARAM.SFO` says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PspSfo {
    /// `TITLE`, with line breaks and runs of spaces collapsed; `None` when nothing printable is left.
    pub title: Option<String>,
    /// `DISC_ID` (`ULUS10041`), cleaned the same way.
    pub disc_id: Option<String>,
}

/// The SFO of a file by its extension: `pbp` (an `EBOOT.PBP`) or `iso`; `None` for any other.
pub fn read<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<PspSfo> {
    let ext = file_name.rsplit_once('.').map_or("", |(_, e)| e);
    if ext.eq_ignore_ascii_case("pbp") {
        from_pbp(source)
    } else if ext.eq_ignore_ascii_case("iso") {
        from_iso(source)
    } else {
        None
    }
}

/// The SFO in a PBP: its header is `\0PBP`, a version, then little-endian offsets, the first of `PARAM.SFO`.
pub fn from_pbp<S: ByteSource + ?Sized>(source: &mut S) -> Option<PspSfo> {
    let header = get_upto(source, 0, 16).filter(|h| h.len() >= 16 && h[..4] == [0, b'P', b'B', b'P'])?;
    let (start, next) = (u64::from(u32_at(&header, 8)?), u64::from(u32_at(&header, 12)?));
    if start < 16 {
        return None;
    }
    let length = if next > start { (next - start).min(MAX_READ as u64) } else { MAX_READ as u64 };
    parse(&get_upto(source, start, length)?)
}

/// The SFO of a PSP ISO: `PSP_GAME/PARAM.SFO`, found by walking ISO 9660 from the primary volume descriptor.
pub fn from_iso<S: ByteSource + ?Sized>(source: &mut S) -> Option<PspSfo> {
    let layout = iso9660::layout_of(source);
    parse(&sfo_file(source, layout)?)
}

/// The bytes of `PSP_GAME/PARAM.SFO` in an ISO.
pub(crate) fn sfo_file<S: ByteSource + ?Sized>(source: &mut S, layout: Layout) -> Option<Vec<u8>> {
    iso9660::read_file(source, layout, "PSP_GAME/PARAM.SFO", MAX_READ as u64)
}

/// The title and disc id in the bytes of a `PARAM.SFO` (`\0PSF`, key table, data table, 16-byte index entries).
pub fn parse(bytes: &[u8]) -> Option<PspSfo> {
    if bytes.len() < 20 || bytes[..4] != [0, b'P', b'S', b'F'] {
        return None;
    }
    let key_start = u64::from(u32_at(bytes, 8)?);
    let data_start = u64::from(u32_at(bytes, 12)?);
    let count = u32_at(bytes, 16)?.min(MAX_ENTRIES) as usize;
    let (mut title, mut disc_id) = (None, None);
    for i in 0..count {
        let entry = 20 + i * 16;
        if entry + 16 > bytes.len() {
            break;
        }
        let key_at = key_start + u64::from(u16_at(bytes, entry)?);
        let Some(key) = c_string(bytes, key_at, u64::MAX) else { continue };
        if key != "TITLE" && key != "DISC_ID" {
            continue;
        }
        let length = u64::from(u32_at(bytes, entry + 4)?);
        let Some(value) = c_string(bytes, data_start + u64::from(u32_at(bytes, entry + 12)?), length) else {
            continue;
        };
        if key == "TITLE" {
            title = printable(&value);
        } else {
            disc_id = printable(&value);
        }
    }
    (title.is_some() || disc_id.is_some()).then_some(PspSfo { title, disc_id })
}

/// The NUL-terminated string at `offset`, at most `limit` bytes, or `None` when it lies outside `bytes`.
fn c_string(bytes: &[u8], offset: u64, limit: u64) -> Option<String> {
    let len = bytes.len() as u64;
    if offset >= len {
        return None;
    }
    let end = len.min(limit.min(len) + offset) as usize;
    let start = offset as usize;
    let stop = bytes[start..end].iter().position(|&b| b == 0).map_or(end, |p| start + p);
    Some(String::from_utf8_lossy(&bytes[start..stop]).into_owned())
}

/// `value` with line breaks and runs of spaces collapsed, or `None` when nothing printable is left of it.
fn printable(value: &str) -> Option<String> {
    let mut spaced = String::new();
    let mut previous_space = false;
    for c in value.chars().map(|c| if text::is_whitespace(c) { ' ' } else { c }) {
        if c == ' ' && previous_space {
            continue;
        }
        previous_space = c == ' ';
        spaced.push(c);
    }
    let mut cleaned = String::new();
    let mut units = 0usize;
    for c in text::trim(&spaced).chars() {
        units += c.len_utf16();
        if units > MAX_TITLE {
            break;
        }
        cleaned.push(c);
    }
    let control = |c: char| matches!(c, '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}');
    (!cleaned.is_empty() && !cleaned.chars().any(|c| control(c) || c == '\u{FFFD}')).then_some(cleaned)
}

/// The PSP disc id (`ULUS00001`) of an ISO: `DISC_ID` from `PARAM.SFO`, else the id in `UMD_DATA.BIN`, normalized.
pub(crate) fn disc_id<S: ByteSource + ?Sized>(source: &mut S, layout: Layout) -> Option<String> {
    if let Some(id) = sfo_file(source, layout).and_then(|sfo| parse(&sfo)).and_then(|s| s.disc_id) {
        return normalize(&id);
    }
    let umd = latin1(&iso9660::read_file(source, layout, "UMD_DATA.BIN", 256)?);
    normalize(umd.split('|').next().unwrap_or(""))
}

/// Upper case, letters and digits only, 8 to 12 of them.
pub(crate) fn normalize(id: &str) -> Option<String> {
    let cleaned: String = id.to_uppercase().chars().filter(|c| c.is_alphabetic() || c.is_ascii_digit()).collect();
    (8..=12).contains(&cleaned.chars().count()).then_some(cleaned)
}
