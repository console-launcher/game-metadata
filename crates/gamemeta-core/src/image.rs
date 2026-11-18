// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use alloc::vec::Vec;

/// A decoded image: top-down rows, 4 bytes per pixel in R, G, B, A order, `pixels.len() == width * height * 4`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height` pixels, R, G, B, A.
    pub pixels: Vec<u8>,
}

/// `value` of `max` levels scaled to 0..=255, rounded.
pub(crate) fn scale(value: u32, max: u32) -> u8 {
    ((value * 255 + max / 2) / max) as u8
}

/// A GameCube/Wii RGB565 word as opaque RGBA.
pub(crate) fn rgb565(v: u32) -> [u8; 4] {
    [scale((v >> 11) & 31, 31), scale((v >> 5) & 63, 63), scale(v & 31, 31), 255]
}

/// A GameCube/Wii RGB5A3 word: a set top bit is opaque RGB555, else 3 bit alpha and RGB444.
pub(crate) fn rgb5a3(v: u32) -> [u8; 4] {
    if v & 0x8000 != 0 {
        [scale((v >> 10) & 31, 31), scale((v >> 5) & 31, 31), scale(v & 31, 31), 255]
    } else {
        [scale((v >> 8) & 15, 15), scale((v >> 4) & 15, 15), scale(v & 15, 15), scale((v >> 12) & 7, 7)]
    }
}
