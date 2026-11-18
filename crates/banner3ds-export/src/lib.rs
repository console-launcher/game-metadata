// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Animated exports of a 3DS HOME banner: which frames to draw ([`timeline`]), then encoders that take those frames one at a time:
//! GIF ([`gif`]), animated WebP ([`webp`]) and an MP4 muxer for H.264 and AAC or Opus that a browser's WebCodecs produced ([`mp4`]).
//!
//! Everything is pure Rust. The frames come from the CPU renderer, so the same banner gives the same files on every machine.
//! [`render`] ties the pieces together for a caller that has no other source of frames (the command line).
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod audio;
pub mod gif;
pub mod mp4;
pub mod render;
pub mod timeline;
pub mod webp;

mod pixels;

pub use pixels::upscale_nearest;

/// Why an encoder refused its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A zero size, or one the format cannot hold.
    BadSize,
    /// A frame whose byte length is not `width * height * 4`.
    BadFrame,
    /// The caller's progress callback asked to stop.
    Cancelled,
    /// The encoder itself failed (writing to memory cannot, so this is a bug or an out-of-range input).
    Encode,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Error::BadSize => "the picture size is not one this format holds",
            Error::BadFrame => "a frame is not width x height x 4 bytes",
            Error::Cancelled => "the export was cancelled",
            Error::Encode => "the encoder failed",
        })
    }
}

impl std::error::Error for Error {}
