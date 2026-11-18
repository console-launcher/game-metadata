// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Frames of a banner from the CPU renderer, and a [`Job`] that turns them into a GIF or an animated WebP one frame at a time.
//!
//! The picture is the on-screen size (400 x 240), optionally enlarged 2x or 3x by repeating pixels. Frame `k` is drawn by index at
//! a fixed rate, so the output is the same on every machine.

use crate::Error;
use crate::gif::{self, GifWriter};
use crate::timeline::Timeline;
use crate::webp::WebpWriter;
use banner3ds_scene::BannerScene;
use banner3ds_soft::{Options, Renderer};

/// The banner's width on screen, in pixels.
pub const WIDTH: u32 = 400;
/// The banner's height on screen, in pixels.
pub const HEIGHT: u32 = 240;
/// Pictures per second of an animated WebP and of an MP4: the banner's own rate.
pub const FPS: u32 = 60;
/// The most a picture is enlarged.
pub const MAX_SCALE: u32 = 3;

/// The pictures of one banner.
#[derive(Debug)]
pub struct Frames<'a> {
    renderer: Renderer<'a>,
    timeline: Timeline,
    scale: u32,
}

impl<'a> Frames<'a> {
    /// The pictures of `scene`, `scale` (1 to [`MAX_SCALE`]) times the on-screen size.
    pub fn new(scene: &'a BannerScene, scale: u32) -> Result<Frames<'a>, Error> {
        if !(1..=MAX_SCALE).contains(&scale) {
            return Err(Error::BadSize);
        }
        Ok(Frames { renderer: Renderer::new(scene), timeline: Timeline::of(scene), scale })
    }

    /// The picture width in pixels.
    pub fn width(&self) -> u32 {
        WIDTH * self.scale
    }

    /// The picture height in pixels.
    pub fn height(&self) -> u32 {
        HEIGHT * self.scale
    }

    /// Whether anything moves.
    pub fn animated(&self) -> bool {
        self.timeline.animated()
    }

    /// How many pictures an export at `fps` holds (one for a banner that does not move).
    pub fn count(&self, fps: u32) -> usize {
        self.timeline.count(fps)
    }

    /// Picture `index` of an export at `fps`: `width * height` RGBA bytes over `background` (0xAARRGGBB; 0 is transparent).
    pub fn render(&self, index: usize, fps: u32, background: u32) -> Result<Vec<u8>, Error> {
        let mut rgba = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
        let options = Options { background, ..Options::default() };
        self.renderer
            .render_with(self.timeline.frame_at(index, fps), WIDTH, HEIGHT, &options, &mut rgba)
            .map_err(|_| Error::BadSize)?;
        Ok(crate::upscale_nearest(&rgba, WIDTH as usize, HEIGHT as usize, self.scale as usize))
    }
}

/// What a [`Job`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A GIF at [`gif::FPS`].
    Gif,
    /// A lossless animated WebP at [`FPS`].
    Webp,
}

#[derive(Debug)]
enum Sink {
    Gif(GifWriter),
    Webp(WebpWriter),
}

/// How long WebP picture `index` shows at 60 a second, in whole milliseconds: the rounded running total, so the sum never drifts
/// (17, 17, 16, 17, 17, 16 ...).
pub fn webp_millis(index: usize) -> u32 {
    let at = |n: usize| (n as u64 * 1000 + 30) / 60;
    (at(index + 1) - at(index)) as u32
}

/// An export in progress: [`step`](Job::step) draws and writes one picture.
#[derive(Debug)]
pub struct Job<'a> {
    frames: Frames<'a>,
    format: Format,
    next: usize,
    count: usize,
    sink: Sink,
}

impl<'a> Job<'a> {
    /// Starts an export of `scene` (`scale` as in [`Frames::new`]).
    pub fn new(scene: &'a BannerScene, format: Format, scale: u32) -> Result<Job<'a>, Error> {
        let frames = Frames::new(scene, scale)?;
        let (fps, sink) = match format {
            Format::Gif => (gif::FPS, Sink::Gif(GifWriter::new(frames.width(), frames.height())?)),
            Format::Webp => (FPS, Sink::Webp(WebpWriter::new(frames.width(), frames.height())?)),
        };
        let count = frames.count(fps);
        Ok(Job { frames, format, next: 0, count, sink })
    }

    /// How many pictures the export holds.
    pub fn total(&self) -> usize {
        self.count
    }

    /// How many are written.
    pub fn done(&self) -> usize {
        self.next
    }

    /// Draws and writes the next picture; `false` once all are written.
    pub fn step(&mut self) -> Result<bool, Error> {
        if self.next >= self.count {
            return Ok(false);
        }
        let still = self.count == 1;
        match &mut self.sink {
            Sink::Gif(w) => {
                let rgba = self.frames.render(self.next, gif::FPS, 0)?;
                w.push(&rgba, if still { 100 } else { 100 / gif::FPS })?;
            }
            Sink::Webp(w) => {
                let rgba = self.frames.render(self.next, FPS, 0)?;
                w.push(&rgba, if still { 1000 } else { webp_millis(self.next) })?;
            }
        }
        self.next += 1;
        Ok(self.next < self.count)
    }

    /// The finished file (the pictures not yet drawn are drawn first).
    pub fn finish(mut self) -> Result<Vec<u8>, Error> {
        while self.step()? {}
        match self.sink {
            Sink::Gif(w) => w.finish(),
            Sink::Webp(w) => w.finish(),
        }
    }

    /// The format.
    pub fn format(&self) -> Format {
        self.format
    }
}

/// The whole export in one call; `progress(done, total)` is called after each picture and stops the export by returning `false`
/// (the result is then `Err(Error::Cancelled)`).
pub fn encode(
    scene: &BannerScene,
    format: Format,
    scale: u32,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<Vec<u8>, Error> {
    let mut job = Job::new(scene, format, scale)?;
    loop {
        let more = job.step()?;
        if !progress(job.done(), job.total()) {
            return Err(Error::Cancelled);
        }
        if !more {
            return job.finish();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use banner3ds_scene::source::from_banner;

    fn feature(name: &str) -> BannerScene {
        let bytes =
            std::fs::read(format!("{}/../../fixtures/scene/features/{name}.cbmd", env!("CARGO_MANIFEST_DIR"))).unwrap();
        from_banner(&bytes).unwrap()
    }

    #[test]
    fn webp_durations_add_up_to_the_exact_time() {
        let total: u32 = (0..60).map(webp_millis).sum();
        assert_eq!(total, 1000, "60 pictures at 60 a second last one second");
        assert_eq!((0..6).map(webp_millis).collect::<Vec<_>>(), [17, 16, 17, 17, 16, 17]);
        assert_eq!((0..3600).map(webp_millis).sum::<u32>(), 60_000);
    }

    #[test]
    fn a_gif_has_one_picture_per_fiftieth_of_a_second() {
        let scene = feature("bone-anim");
        let frames = Frames::new(&scene, 1).unwrap();
        let count = frames.count(gif::FPS);
        assert!(count > 50, "{count}");
        let bytes = encode(&scene, Format::Gif, 1, |_, _| true).unwrap();
        let mut d = ::gif::DecodeOptions::new().read_info(&bytes[..]).unwrap();
        assert_eq!((d.width(), d.height()), (400, 240));
        let (mut n, mut centis) = (0usize, 0usize);
        while let Some(f) = d.read_next_frame().unwrap() {
            n += 1;
            centis += usize::from(f.delay);
        }
        assert!(n <= count);
        assert_eq!(centis, count * 2, "the delays add up to the animation's length, whatever frames were merged");
    }

    #[test]
    fn a_webp_decodes_to_the_renderer_s_pixels() {
        let scene = feature("bone-anim");
        let bytes = encode(&scene, Format::Webp, 1, |_, _| true).unwrap();
        let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(d.dimensions(), (400, 240));
        let frames = Frames::new(&scene, 1).unwrap();
        let mut buf = vec![0u8; d.output_buffer_size().unwrap()];
        d.read_frame(&mut buf).unwrap();
        let clean = |mut v: Vec<u8>| {
            v.chunks_exact_mut(4).for_each(|p| {
                if p[3] == 0 {
                    p.fill(0)
                }
            });
            v
        };
        assert_eq!(buf, clean(frames.render(0, FPS, 0).unwrap()));
        let total: u64 = (0..d.num_frames()).map(|_| u64::from(d.read_frame(&mut buf).unwrap_or(0))).sum();
        assert!(total <= (frames.count(FPS) as u64 * 1000).div_ceil(60) + 1);
    }

    #[test]
    fn the_same_banner_gives_the_same_bytes() {
        let scene = feature("bone-anim");
        for format in [Format::Gif, Format::Webp] {
            let a = encode(&scene, format, 1, |_, _| true).unwrap();
            let b = encode(&scene, format, 1, |_, _| true).unwrap();
            assert!(a == b, "{format:?}");
        }
    }

    #[test]
    fn larger_pictures_repeat_pixels() {
        let scene = feature("bone-anim");
        let one = Frames::new(&scene, 1).unwrap().render(7, 60, 0).unwrap();
        let two = Frames::new(&scene, 2).unwrap();
        assert_eq!((two.width(), two.height()), (800, 480));
        let big = two.render(7, 60, 0).unwrap();
        assert_eq!(&big[..4], &one[..4]);
        assert_eq!(&big[4..8], &one[..4]);
        assert_eq!(&big[800 * 4..800 * 4 + 4], &one[..4]);
        assert!(Frames::new(&scene, 0).is_err() && Frames::new(&scene, 4).is_err());
    }

    #[test]
    fn a_background_fills_the_see_through_pixels() {
        let scene = feature("bone-anim");
        let frames = Frames::new(&scene, 1).unwrap();
        let on_black = frames.render(0, 60, 0xFF00_0000).unwrap();
        assert!(on_black.chunks_exact(4).all(|p| p[3] == 255));
        assert!(
            frames.render(0, 60, 0).unwrap().chunks_exact(4).any(|p| p[3] == 0),
            "the banner leaves the corners empty"
        );
    }

    #[test]
    fn progress_can_stop_the_export() {
        let scene = feature("bone-anim");
        let mut seen = 0;
        let r = encode(&scene, Format::Gif, 1, |done, total| {
            seen = done;
            assert!(done <= total);
            done < 3
        });
        assert_eq!((r, seen), (Err(Error::Cancelled), 3));
    }
}
