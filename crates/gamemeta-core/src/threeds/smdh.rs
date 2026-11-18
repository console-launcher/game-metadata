// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The SMDH ("icon" file in the ExeFS): titles in 16 language slots, the 48x48 and 24x24 icons, region lockout and
//! age ratings.

use alloc::string::String;
use alloc::vec::Vec;

use super::pica::morton_xy;
use crate::RgbaImage;
use crate::le::u32_at;
use crate::text::{is_blank, trim, utf16le_until_nul};

/// The region-lockout value that allows every region.
pub const REGION_FREE: u32 = 0x7FFF_FFFF;
/// The size of an SMDH file.
pub const SIZE: usize = 0x36C0;
const MAGIC: u32 = 0x4844_4D53; // "SMDH"
const TITLES_AT: usize = 0x08;
const TITLE_SIZE: usize = 0x200;
const RATINGS_AT: usize = 0x2008;
const REGION_AT: usize = 0x2018;
const SMALL_ICON_AT: usize = 0x2040;
const LARGE_ICON_AT: usize = 0x24C0;

/// One language slot's raw strings (trimmed; possibly multi-line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmdhTitle {
    /// Short title (0x80 bytes of UTF-16).
    pub short_title: String,
    /// Long title (0x100 bytes of UTF-16).
    pub long_title: String,
    /// Publisher (0x80 bytes of UTF-16).
    pub publisher: String,
}

/// A parsed SMDH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Smdh {
    /// Slot (0 Japanese, 1 English, ...) to title, for the slots whose short or long title is not blank, in slot order.
    pub titles: Vec<(usize, SmdhTitle)>,
    /// The 48x48 icon.
    pub large_icon: RgbaImage,
    /// The 24x24 icon.
    pub small_icon: RgbaImage,
    /// Bit 0 Japan, 1 North America, 2 Europe, 3 Australia, 4 China, 5 Korea, 6 Taiwan; [`REGION_FREE`] for all.
    pub region_lockout: u32,
    /// One byte per board slot: bit 7 active, bit 6 pending, bit 5 no age restriction, bits 0-4 the age.
    pub ratings: [u8; 16],
}

impl Smdh {
    /// Parses `data`; `None` when it is shorter than an SMDH or lacks the magic.
    pub fn parse(data: &[u8]) -> Option<Smdh> {
        if data.len() < SIZE || u32_at(data, 0)? != MAGIC {
            return None;
        }
        let mut titles = Vec::new();
        for slot in 0..16 {
            let at = TITLES_AT + slot * TITLE_SIZE;
            let field =
                |off: usize, len: usize| String::from(trim(&utf16le_until_nul(&data[at + off..at + off + len])));
            let short_title = field(0, 0x80);
            let long_title = field(0x80, 0x100);
            if !is_blank(&short_title) || !is_blank(&long_title) {
                titles.push((slot, SmdhTitle { short_title, long_title, publisher: field(0x180, 0x80) }));
            }
        }
        let mut ratings = [0u8; 16];
        ratings.copy_from_slice(&data[RATINGS_AT..RATINGS_AT + 16]);
        Some(Smdh {
            titles,
            large_icon: rgb565_tiled(&data[LARGE_ICON_AT..], 48),
            small_icon: rgb565_tiled(&data[SMALL_ICON_AT..], 24),
            region_lockout: u32_at(data, REGION_AT)?,
            ratings,
        })
    }
}

/// A `side` x `side` RGB565 image in 8x8 Morton-ordered tiles; `data` holds at least `side * side * 2` bytes.
fn rgb565_tiled(data: &[u8], side: usize) -> RgbaImage {
    let mut out = alloc::vec![0u8; side * side * 4];
    let mut i = 0;
    for ty in (0..side).step_by(8) {
        for tx in (0..side).step_by(8) {
            for px in 0..64 {
                let (x, y) = morton_xy(px);
                let v = u16::from_le_bytes([data[i], data[i + 1]]);
                i += 2;
                let o = ((ty + y) * side + tx + x) * 4;
                let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
                out[o] = ((r << 3) | (r >> 2)) as u8;
                out[o + 1] = ((g << 2) | (g >> 4)) as u8;
                out[o + 2] = ((b << 3) | (b >> 2)) as u8;
                out[o + 3] = 0xFF;
            }
        }
    }
    RgbaImage { width: side as u32, height: side as u32, pixels: out }
}
