// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Banner exports for the page: a GIF or WebP job that draws and encodes one picture per `step()`, the pictures themselves (for
//! the page's video encoder) and the MP4 muxer.

use crate::Scene;
use banner3ds_export::mp4::{Audio, Mp4Writer};
use banner3ds_export::render::{Format, Frames, Job};
use banner3ds_scene::BannerScene;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

fn js(e: banner3ds_export::Error) -> JsError {
    JsError::new(&e.to_string())
}

/// A GIF or animated WebP being made: `step()` until it returns false, then `finish()`.
#[wasm_bindgen]
#[derive(Debug)]
pub struct ExportJob {
    // Declared first: it borrows `_scene`'s allocation, which the `Rc` keeps alive for as long as this struct lives.
    job: Job<'static>,
    _scene: Rc<BannerScene>,
}

#[wasm_bindgen]
impl ExportJob {
    /// Starts a `"gif"` or `"webp"` export at `scale` (1 to 3) times the on-screen size.
    #[wasm_bindgen(constructor)]
    pub fn new(scene: &Scene, format: &str, scale: u32) -> Result<ExportJob, JsError> {
        let format = match format {
            "gif" => Format::Gif,
            "webp" => Format::Webp,
            _ => return Err(JsError::new("unknown format")),
        };
        let rc = Rc::clone(&scene.scene);
        // SAFETY: the job borrows the `BannerScene` inside `rc`, whose heap allocation does not move and is kept alive by
        // `_scene` in the same struct, dropped after the job.
        let job = unsafe { std::mem::transmute::<Job<'_>, Job<'static>>(Job::new(&rc, format, scale).map_err(js)?) };
        Ok(ExportJob { job, _scene: rc })
    }

    /// How many pictures the export holds.
    pub fn total(&self) -> usize {
        self.job.total()
    }

    /// How many are done.
    pub fn done(&self) -> usize {
        self.job.done()
    }

    /// Draws and writes the next picture; false when there are no more.
    pub fn step(&mut self) -> Result<bool, JsError> {
        self.job.step().map_err(js)
    }

    /// The finished file.
    pub fn finish(self) -> Result<Vec<u8>, JsError> {
        self.job.finish().map_err(js)
    }
}

/// The banner's pictures for a video: `render(index, fps, true)` is picture `index` over opaque black.
#[wasm_bindgen]
#[derive(Debug)]
pub struct ExportFrames {
    frames: Frames<'static>,
    _scene: Rc<BannerScene>,
}

#[wasm_bindgen]
impl ExportFrames {
    /// The pictures at `scale` (1 to 3) times the on-screen size.
    #[wasm_bindgen(constructor)]
    pub fn new(scene: &Scene, scale: u32) -> Result<ExportFrames, JsError> {
        let rc = Rc::clone(&scene.scene);
        // SAFETY: as in `ExportJob::new`.
        let frames =
            unsafe { std::mem::transmute::<Frames<'_>, Frames<'static>>(Frames::new(&rc, scale).map_err(js)?) };
        Ok(ExportFrames { frames, _scene: rc })
    }

    /// The picture width.
    pub fn width(&self) -> u32 {
        self.frames.width()
    }

    /// The picture height.
    pub fn height(&self) -> u32 {
        self.frames.height()
    }

    /// Whether anything moves (a still banner is one picture).
    pub fn animated(&self) -> bool {
        self.frames.animated()
    }

    /// How many pictures at `fps`.
    pub fn count(&self, fps: u32) -> usize {
        self.frames.count(fps)
    }

    /// Picture `index` at `fps`, RGBA, over opaque black when `opaque` and else over nothing.
    pub fn render(&self, index: usize, fps: u32, opaque: bool) -> Result<Vec<u8>, JsError> {
        self.frames.render(index, fps, if opaque { 0xFF00_0000 } else { 0 }).map_err(js)
    }
}

/// An MP4 being muxed from the page's encoded video (and audio) chunks.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Mp4 {
    writer: Mp4Writer,
}

#[wasm_bindgen]
impl Mp4 {
    /// `avcc` is the video encoder's `description`; sample durations are in 1/`timescale` s.
    #[wasm_bindgen(constructor)]
    pub fn new(width: u16, height: u16, timescale: u32, avcc: &[u8]) -> Result<Mp4, JsError> {
        Ok(Mp4 { writer: Mp4Writer::new(width, height, timescale, avcc).map_err(js)? })
    }

    /// Adds an AAC track; `config` is the audio encoder's `description`.
    pub fn set_aac(&mut self, config: &[u8], rate: u32, channels: u16) -> Result<(), JsError> {
        self.writer.set_audio(Audio::Aac { config: config.to_vec() }, rate, channels).map_err(js)
    }

    /// Adds an Opus track (48 kHz).
    pub fn set_opus(&mut self, pre_skip: u16, input_rate: u32, channels: u16) -> Result<(), JsError> {
        self.writer.set_audio(Audio::Opus { pre_skip, input_rate }, 48000, channels).map_err(js)
    }

    /// Adds a video sample.
    pub fn push_video(&mut self, data: &[u8], duration: u32, key: bool) -> Result<(), JsError> {
        self.writer.push_video(data, duration, key).map_err(js)
    }

    /// Adds an audio sample of `duration` samples of the audio rate.
    pub fn push_audio(&mut self, data: &[u8], duration: u32) -> Result<(), JsError> {
        self.writer.push_audio(data, duration).map_err(js)
    }

    /// The finished file.
    pub fn finish(self) -> Result<Vec<u8>, JsError> {
        self.writer.finish().map_err(js)
    }
}
