// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A Wii disc's or channel's `opening.bnr` (or `00000000.app`): the IMET header (`IMET` at 0x40, the titles of ten
//! languages as UTF-16BE, 0x54 bytes each, from 0x5C), then at 0x600 a U8 archive (plain, or LZ77 compressed, or IMD5
//! wrapped) holding `meta/icon.bin`, a U8 archive of its own (same wrappers) of layered scene textures. The picture
//! taken is the title logo: the largest wide TPL (64 or more across, wider than tall) that has transparent pixels.
//! `meta/banner.bin` is not read: its scene needs layout and TEV composition.
//!
//! Nothing here is encrypted. An ASH0 compressed archive or any damaged part leaves its picture out; the titles stay.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use super::u8_archive::U8Archive;
use super::{lz77, tpl};
use crate::{Language, RgbaImage, Title};

const ARCHIVE_AT: usize = 0x600;
const NAMES_AT: usize = 0x1C; // from the start of "IMET"
const NAME: usize = 0x54;
const LANGUAGES: [Language; 10] = [
    Language::Japanese,
    Language::English,
    Language::German,
    Language::French,
    Language::Spanish,
    Language::Italian,
    Language::Dutch,
    Language::ChineseSimplified,
    Language::ChineseTraditional,
    Language::Korean,
];
const MAX_BIN: usize = 8 << 20;
const MIN_LOGO_WIDTH: u32 = 64;

pub(crate) struct Parsed {
    pub titles: Vec<(Language, Title)>,
    pub logo: Option<RgbaImage>,
}

pub(crate) fn parse(bytes: &[u8]) -> Option<Parsed> {
    let imet = [0x40usize, 0x80]
        .into_iter()
        .find(|&at| at + NAMES_AT + LANGUAGES.len() * NAME <= bytes.len() && &bytes[at..at + 4] == b"IMET")?;
    let mut titles = Vec::new();
    for (i, &language) in LANGUAGES.iter().enumerate() {
        let at = imet + NAMES_AT + i * NAME;
        let mut end = at;
        while end + 1 < at + NAME && (bytes[end] != 0 || bytes[end + 1] != 0) {
            end += 2;
        }
        let units = bytes[at..end].chunks_exact(2).map(|p| u16::from_be_bytes([p[0], p[1]]));
        let raw: String = char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect();
        let name = crate::text::joined_lines(&raw);
        if !crate::text::is_blank(&name) {
            titles.push((language, Title { name: name.clone(), short_name: name, publisher: None }));
        }
    }
    titles.sort_by_key(|(l, _)| *l);
    let archive_at = imet - 0x40 + ARCHIVE_AT;
    let logo = (archive_at < bytes.len())
        .then(|| unwrap_archive(Cow::Borrowed(bytes), archive_at))
        .flatten()
        .and_then(|a| logo(&a));
    Some(Parsed { titles, logo })
}

/// The bytes of `meta/banner.bin` (still wrapped) of an `opening.bnr`; `None` when there is none.
pub(crate) fn banner_bin(bytes: &[u8]) -> Option<Vec<u8>> {
    meta_bin(bytes, "meta/banner.bin")
}

/// The bytes of `meta/icon.bin` (still wrapped) of an `opening.bnr`; `None` when there is none.
pub(crate) fn icon_bin(bytes: &[u8]) -> Option<Vec<u8>> {
    meta_bin(bytes, "meta/icon.bin")
}

fn meta_bin(bytes: &[u8], path: &str) -> Option<Vec<u8>> {
    let imet =
        [0x40usize, 0x80].into_iter().find(|&at| at + NAMES_AT <= bytes.len() && &bytes[at..at + 4] == b"IMET")?;
    let archive = unwrap_archive(Cow::Borrowed(bytes), imet - 0x40 + ARCHIVE_AT)?;
    archive.file(path).filter(|b| b.len() <= MAX_BIN).map(<[u8]>::to_vec)
}

/// The U8 archive at `offset`, looking through IMD5 and LZ77 wrappers (a few levels); `None` for an unknown wrapper such as ASH0.
pub(crate) fn unwrap_archive(data: Cow<'_, [u8]>, offset: usize) -> Option<U8Archive<'_>> {
    let mut buffer = data;
    let mut at = offset;
    for _ in 0..4 {
        let magic = buffer.get(at..at.checked_add(4)?)?;
        match magic {
            b"IMD5" => at += 0x20,
            b"LZ77" => {
                buffer = Cow::Owned(lz77::decompress(&buffer, at + 4, lz77::MAX_SIZE)?);
                at = 0;
            }
            _ => return U8Archive::parse(buffer, at),
        }
    }
    None
}

fn logo(archive: &U8Archive<'_>) -> Option<RgbaImage> {
    let bin = archive.file("meta/icon.bin").filter(|b| b.len() <= MAX_BIN)?;
    let inner = unwrap_archive(Cow::Borrowed(bin), 0)?;
    let mut candidates: Vec<(u64, &[u8])> = inner
        .entries
        .iter()
        .filter(|e| e.path.len() >= 4 && e.path.as_bytes()[e.path.len() - 4..].eq_ignore_ascii_case(b".tpl"))
        .filter_map(|e| inner.file(&e.path))
        .filter_map(|bytes| {
            tpl::size(bytes)
                .filter(|&(w, h)| w >= MIN_LOGO_WIDTH && w > h)
                .map(|(w, h)| (u64::from(w) * u64::from(h), bytes))
        })
        .collect();
    candidates.sort_by_key(|c| core::cmp::Reverse(c.0));
    candidates.into_iter().find_map(|(_, bytes)| tpl::decode(bytes).filter(has_transparency))
}

fn has_transparency(image: &RgbaImage) -> bool {
    image.pixels.iter().skip(3).step_by(4).any(|&a| a != 0xFF)
}
