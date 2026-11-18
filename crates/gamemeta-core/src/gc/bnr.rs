// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `opening.bnr`: BNR1 (one text) or BNR2 (six: English, German, French, Spanish, Italian, Dutch), after a 96x32 RGB5A3
//! image at 0x20.

use alloc::string::String;
use alloc::vec::Vec;

use crate::image::rgb5a3;
use crate::{Language, RgbaImage, Title};

/// The most of `opening.bnr` read (BNR2: header, image and six 0x140 byte text blocks).
pub(crate) const MAX_SIZE: u64 = 0x1FA0;
const IMAGE_AT: usize = 0x20;
const INFO_AT: usize = 0x1820;
const ENTRY: usize = 0x140;
const WIDTH: usize = 96;
const HEIGHT: usize = 32;
const V2_LANGUAGES: [Language; 6] =
    [Language::English, Language::German, Language::French, Language::Spanish, Language::Italian, Language::Dutch];

/// How a disc's text is encoded: Shift_JIS for a Japan disc, windows-1252 for the rest.
#[derive(Clone, Copy)]
pub(crate) enum Charset {
    Windows1252,
    ShiftJis,
}

impl Charset {
    pub(crate) fn for_region(region: Option<u8>) -> Charset {
        if region == Some(b'J') { Charset::ShiftJis } else { Charset::Windows1252 }
    }

    fn decode(self, bytes: &[u8]) -> String {
        match self {
            Charset::ShiftJis => encoding_rs::SHIFT_JIS.decode_without_bom_handling(bytes).0.into_owned(),
            Charset::Windows1252 => bytes.iter().map(|&b| windows_1252(b)).collect(),
        }
    }
}

/// windows-1252 as Java decodes it: the five bytes it leaves undefined are U+FFFD.
fn windows_1252(b: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0xFFFD,
        0x017D, 0xFFFD, 0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A,
        0x0153, 0xFFFD, 0x017E, 0x0178,
    ];
    match b {
        0x80..=0x9F => char::from_u32(u32::from(HIGH[usize::from(b - 0x80)])).unwrap_or(char::REPLACEMENT_CHARACTER),
        _ => char::from(b),
    }
}

/// What a banner holds.
pub(crate) struct Parsed {
    pub titles: Vec<(Language, Title)>,
    pub image: Option<RgbaImage>,
}

pub(crate) fn parse(bytes: &[u8], charset: Charset, japan: bool) -> Option<Parsed> {
    let languages: &[Language] = match bytes.get(..4)? {
        b"BNR1" => {
            if japan {
                &[Language::Japanese]
            } else {
                &[Language::English]
            }
        }
        b"BNR2" => &V2_LANGUAGES,
        _ => return None,
    };
    let image = (bytes.len() >= IMAGE_AT + WIDTH * HEIGHT * 2).then(|| decode_image(bytes));
    let entries = (bytes.len().saturating_sub(INFO_AT) / ENTRY).min(languages.len());
    let mut titles = Vec::new();
    for (i, &language) in languages.iter().enumerate().take(entries) {
        let at = INFO_AT + i * ENTRY;
        let short_title = text(bytes, at, 0x20, charset);
        let short_maker = text(bytes, at + 0x20, 0x20, charset);
        let long_title = text(bytes, at + 0x40, 0x40, charset);
        let long_maker = text(bytes, at + 0x80, 0x40, charset);
        let name = if crate::text::is_blank(&long_title) { short_title.clone() } else { long_title };
        if crate::text::is_blank(&name) {
            continue;
        }
        let publisher = [long_maker, short_maker].into_iter().find(|m| !crate::text::is_blank(m));
        let short_name = if crate::text::is_blank(&short_title) { name.clone() } else { short_title };
        titles.push((language, Title { name, short_name, publisher }));
    }
    titles.sort_by_key(|(l, _)| *l);
    Some(Parsed { titles, image })
}

/// The NUL-terminated text in `length` bytes at `at`, lines joined by single spaces.
pub(crate) fn text(b: &[u8], at: usize, length: usize, charset: Charset) -> String {
    let window = b.get(at..).unwrap_or(&[]);
    let window = &window[..length.min(window.len())];
    let end = window.iter().position(|&c| c == 0).unwrap_or(window.len());
    crate::text::joined_lines(&charset.decode(&window[..end]))
}

/// RGB5A3 in 4x4 tiles: a set top bit is opaque RGB555, else 3 bit alpha and RGB444.
fn decode_image(bytes: &[u8]) -> RgbaImage {
    let mut pixels = alloc::vec![0u8; WIDTH * HEIGHT * 4];
    let mut p = IMAGE_AT;
    for ty in 0..HEIGHT / 4 {
        for tx in 0..WIDTH / 4 {
            for py in 0..4 {
                for px in 0..4 {
                    let v = (u32::from(bytes[p]) << 8) | u32::from(bytes[p + 1]);
                    p += 2;
                    let o = ((ty * 4 + py) * WIDTH + tx * 4 + px) * 4;
                    pixels[o..o + 4].copy_from_slice(&rgb5a3(v));
                }
            }
        }
    }
    RgbaImage { width: WIDTH as u32, height: HEIGHT as u32, pixels }
}
