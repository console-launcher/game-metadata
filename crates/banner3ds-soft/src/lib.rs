// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A CPU renderer for 3DS HOME banners: one frame of a [`BannerScene`] into a caller-supplied RGBA buffer, at any size. It is
//! the pixel reference `banner3ds-gles` is tested against, and a renderer in its own right where there is no GL.
//!
//! Features: HOME's fixed framing and turntable, the scene's clips (bones, texture coordinates, colours, visibility), skinning
//! and bone billboards, the PICA combiners, texture wrap and filter modes (clamp to border included), sphere environment
//! maps, fragment lighting (tables, spot and distance attenuation, Fresnel), bump mapping, vertex and hemisphere lighting,
//! alpha test, culling, depth, the PICA colour blend with coverage alpha, NW4C's draw order and HOME's window.
//!
//! ```
//! let scene = banner3ds_scene::BannerScene::new(Vec::new(), Default::default());
//! let mut rgba = vec![0u8; 8 * 4 * 4];
//! banner3ds_soft::render(&scene, 0.0, 8, 4, &mut rgba).unwrap();
//! assert!(rgba.iter().all(|&b| b == 0)); // nothing drawn over the transparent background
//! ```
#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod raster;
pub mod shading;
pub mod texenv;

use banner3ds_scene::BannerScene;
use banner3ds_scene::draw::{Camera, Player, Pose, camera};

/// Most pixels one render accepts (64 Mpx), so a size can never overflow the float buffers.
pub const MAX_PIXELS: u64 = 1 << 26;

/// Why a frame could not be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    /// A zero width or height, or more than [`MAX_PIXELS`].
    BadSize,
    /// The output (or backdrop) is not `width * height * 4` bytes.
    BadBuffer,
}

/// How to render a frame; the default is HOME's view over a transparent background.
#[derive(Debug, Clone, Default)]
pub struct Options<'a> {
    /// The background as 0xAARRGGBB (0: transparent black).
    pub background: u32,
    /// A picture under the banner instead of the background (RGBA, the output's size).
    pub backdrop: Option<&'a [u8]>,
    /// The camera; `None` for the banner's framing (HOME's fixed view).
    pub camera: Option<Camera>,
    /// The clips' frame when it differs from the turntable's (lining a capture up); `None` for the same instant.
    pub clip_frame: Option<f32>,
}

/// A scene ready to render many frames (the rest pose and the loop length worked out once).
#[derive(Debug, Clone)]
pub struct Renderer<'a> {
    scene: &'a BannerScene,
    player: Player<'a>,
}

impl<'a> Renderer<'a> {
    /// A renderer for `scene`.
    pub fn new(scene: &'a BannerScene) -> Renderer<'a> {
        Renderer { scene, player: Player::new(scene) }
    }

    /// The loop length in frames (60 to the second); 0 when the banner does not animate.
    pub fn frames(&self) -> f32 {
        self.player.frames
    }

    /// Whether anything moves.
    pub fn animated(&self) -> bool {
        self.player.animated
    }

    /// The player that poses the scene.
    pub fn player(&self) -> &Player<'a> {
        &self.player
    }

    /// The banner at `frame` (60 to the second, looped by the clips) into `out` (`width * height` RGBA, top row first),
    /// through HOME's view over a transparent background.
    pub fn render(&self, frame: f32, width: u32, height: u32, out: &mut [u8]) -> Result<(), RenderError> {
        self.render_with(frame, width, height, &Options::default(), out)
    }

    /// [`render`](Self::render) with `options`.
    pub fn render_with(
        &self,
        frame: f32,
        width: u32,
        height: u32,
        options: &Options<'_>,
        out: &mut [u8],
    ) -> Result<(), RenderError> {
        let pose = self.player.pose(frame, options.clip_frame.unwrap_or(frame));
        self.render_pose(&pose, width, height, options, out)
    }

    /// One frame of `pose` (from [`Player::pose`]) into `out`.
    pub fn render_pose(
        &self,
        pose: &Pose,
        width: u32,
        height: u32,
        options: &Options<'_>,
        out: &mut [u8],
    ) -> Result<(), RenderError> {
        let pixels = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || pixels > MAX_PIXELS {
            return Err(RenderError::BadSize);
        }
        if out.len() as u64 != pixels * 4 || options.backdrop.is_some_and(|b| b.len() as u64 != pixels * 4) {
            return Err(RenderError::BadBuffer);
        }
        let framing;
        let cam = match &options.camera {
            Some(c) => c,
            None => {
                framing = camera::framing(self.scene, width as f32 / height as f32);
                &framing
            }
        };
        raster::render(self.scene, &self.player, pose, cam, width as i32, height as i32, options, out);
        Ok(())
    }
}

/// One frame of `scene` at `frame` (60 to the second) into `out` (`width * height` RGBA, top row first): HOME's view over a
/// transparent background. For many frames of one scene, keep a [`Renderer`].
pub fn render(scene: &BannerScene, frame: f32, width: u32, height: u32, out: &mut [u8]) -> Result<(), RenderError> {
    Renderer::new(scene).render(frame, width, height, out)
}
