// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A Wii channel banner: the layout, animations and textures of `meta/banner.bin` of an `opening.bnr`, ready to draw on the CPU.
//! The archive holds the `.brlyt` in `arc/blyt` (one layout), the `.brlan` files in `arc/anim` (named animations; a "start" one
//! plays once, then a "loop" one repeats) and the `.tpl` files in `arc/timg` (textures, by file name). Nothing is decrypted.
//!
//! Status: written from the NW4R layout format and tested on authored banners only; see `docs/design.md`, "Wii channel
//! banners", for what is not drawn and which format details are guesses. The frame clock is 60 frames per second.
//!
//! ```
//! use gamemeta_core::wii::banner::WiiBanner;
//! assert!(WiiBanner::open(b"not a banner archive").is_none());
//! ```

mod anim;
mod be;
mod layout;
mod render;

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use anim::Animation;
use layout::Layout;

use super::{opening, tpl};
use crate::RgbaImage;

const MAX_TEXTURE_FILES: usize = 64;
/// Decoded texture pixels kept over all the textures of one banner (the rest are reported missing), and the largest frame drawn
/// (side, pixels): the original implementation has no limits.
const MAX_TEXTURE_PIXELS: u64 = 16 << 20;
const MAX_SIDE: i32 = 8192;
const MAX_PIXELS: u64 = 1 << 26;

/// A channel banner, ready to draw.
#[derive(Debug)]
pub struct WiiBanner {
    layout: Layout,
    start: Option<Animation>,
    looping: Option<Animation>,
    textures: render::Textures,
    missing: Vec<String>,
}

fn ends_with_ignore_case(path: &str, suffix: &str) -> bool {
    path.len() >= suffix.len() && path.as_bytes()[path.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
}

fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

impl WiiBanner {
    /// Opens `meta/banner.bin` (plain, LZ77 or IMD5 wrapped). `None` when the archive has no readable layout.
    pub fn open(banner_bin: &[u8]) -> Option<WiiBanner> {
        let archive = opening::unwrap_archive(Cow::Borrowed(banner_bin), 0)?;
        let by_suffix =
            |suffix: &'static str| archive.entries.iter().filter(move |e| ends_with_ignore_case(&e.path, suffix));
        let layout = by_suffix(".brlyt").next().and_then(|e| archive.file(&e.path)).and_then(Layout::parse)?;
        let animations: Vec<(String, Animation)> = by_suffix(".brlan")
            .filter_map(|e| {
                let parsed = archive.file(&e.path).and_then(Animation::parse)?;
                Some((file_name(&e.path).to_ascii_lowercase(), parsed))
            })
            .collect();
        let pick = |word: &str| animations.iter().position(|(name, _)| name.contains(word));
        let (start, mut looping) = (pick("start"), pick("loop"));
        // Many banners ship one plain `banner.brlan`, no start or loop in the name: it is the loop (the menu plays it from 0).
        if start.is_none() && looping.is_none() && animations.len() == 1 {
            looping = Some(0);
        }
        let mut animations: Vec<Option<Animation>> = animations.into_iter().map(|(_, a)| Some(a)).collect();
        let start = start.and_then(|i| animations[i].take());
        let looping = looping.and_then(|i| animations[i].take());
        let mut textures = BTreeMap::new();
        let mut pixels = 0u64;
        for e in by_suffix(".tpl").take(MAX_TEXTURE_FILES) {
            let Some(image) = archive.file(&e.path).and_then(tpl::decode_gx) else { continue };
            pixels += u64::from(image.width) * u64::from(image.height);
            if pixels <= MAX_TEXTURE_PIXELS {
                textures.insert(String::from(file_name(&e.path)), image);
            }
        }
        let missing = layout.textures.iter().filter(|t| !textures.contains_key(*t)).cloned().collect();
        Some(WiiBanner { layout, start, looping, textures, missing })
    }

    /// Frames of the start animation (0 when there is none): the loop begins after it.
    pub fn start_frames(&self) -> u32 {
        self.start.as_ref().map_or(0, |a| a.frames)
    }

    /// Frames of the loop animation (0 when there is none).
    pub fn loop_frames(&self) -> u32 {
        self.looping.as_ref().map_or(0, |a| a.frames)
    }

    /// The layout's size in layout units: the size of [`render`](Self::render) at scale 1.
    pub fn layout_size(&self) -> (f32, f32) {
        (self.layout.width, self.layout.height)
    }

    /// Pane kinds that were parsed but do not draw (`"wnd1"` windows, `"txt1"` text, `"bnd1"` bounding boxes) with how many
    /// there are, in order of first appearance: what to blame when a banner differs from the console.
    pub fn skipped(&self) -> &[(&'static str, u32)] {
        &self.layout.skipped
    }

    /// Textures the layout names that the archive did not supply (nothing is drawn for those maps).
    pub fn missing_textures(&self) -> &[String] {
        &self.missing
    }

    /// The size in pixels of a frame at `scale` pixels per layout unit (608 x 456 at 1 for a channel banner). `None` for a scale
    /// that is not a positive finite number, or one that makes the frame larger than 8192 pixels a side or 64 million pixels.
    pub fn pixel_size(&self, scale: f32) -> Option<(u32, u32)> {
        if !(scale.is_finite() && scale > 0.0) {
            return None;
        }
        let (w, h) = render::pixel_size(&self.layout, scale);
        (w <= MAX_SIDE && h <= MAX_SIDE && (w as u64) * (h as u64) <= MAX_PIXELS).then_some((w as u32, h as u32))
    }

    /// Draws the banner `frame` frames (60 per second) after it appears, `scale` pixels per layout unit, into `out` as RGBA
    /// (top-down, not premultiplied) and returns the size drawn. `None` when [`pixel_size`](Self::pixel_size) is, or `out` is
    /// shorter than `width * height * 4`; bytes past that are left alone.
    pub fn render_into(&self, frame: f32, scale: f32, out: &mut [u8]) -> Option<(u32, u32)> {
        let (w, h) = self.pixel_size(scale)?;
        let out = out.get_mut(..w as usize * h as usize * 4)?;
        let loop_frame =
            if frame - (self.start_frames() as f32) < 0.0 { 0.0 } else { frame - self.start_frames() as f32 };
        let mut animations: Vec<(&Animation, f32)> = Vec::new();
        animations.extend(self.start.as_ref().map(|a| (a, frame)));
        animations.extend(self.looping.as_ref().map(|a| (a, loop_frame)));
        render::render(&self.layout, &self.textures, &animations, scale, w as i32, h as i32, out);
        Some((w, h))
    }

    /// [`render_into`](Self::render_into) into a new image.
    pub fn render(&self, frame: f32, scale: f32) -> Option<RgbaImage> {
        let (width, height) = self.pixel_size(scale)?;
        let mut pixels = alloc::vec![0u8; width as usize * height as usize * 4];
        self.render_into(frame, scale, &mut pixels)?;
        Some(RgbaImage { width, height, pixels })
    }

    /// The banner settled: the start animation done, the loop at its first frame.
    pub fn poster(&self, scale: f32) -> Option<RgbaImage> {
        self.render(self.start_frames() as f32, scale)
    }
}

/// The bytes of `meta/banner.bin` (the channel banner's own archive, still wrapped) of an `opening.bnr`; `None` when there is none.
pub fn banner_bin(opening_bnr: &[u8]) -> Option<Vec<u8>> {
    opening::banner_bin(opening_bnr)
}
