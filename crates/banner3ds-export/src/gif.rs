// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! GIF: one frame at a time, palettes made per frame, repeating forever.

use crate::Error;
use std::collections::HashMap;

/// Frames per second of a GIF export: 2/100 s a frame. A GIF counts delays in 1/100 s, and browsers show anything under 2/100 s
/// as 1/10 s, so 2/100 s is the shortest honest delay (60 frames a second cannot be written; 50 can, exactly).
pub const FPS: u32 = 50;

/// The shortest delay a GIF frame gets, in 1/100 s.
pub const MIN_DELAY: u16 = 2;

/// Pixels with at least this much alpha are opaque; the rest are the one transparent colour (a GIF has no partial alpha).
const OPAQUE_FROM: u8 = 128;

/// NeuQuant sampling: 1 is best and slowest, 30 fastest. 5 keeps a 400x240 frame under a few milliseconds.
const QUALITY: i32 = 5;

/// The frame waiting for the next one, which tells whether it is a repeat.
#[derive(Debug)]
struct Pending {
    rgba: Vec<u8>,
    centis: u32,
}

/// Writes a GIF: every frame has its own palette (an exact one when it has few enough colours, else NeuQuant), a pixel with
/// alpha is transparent (the index after the last colour), and the file repeats forever.
pub struct GifWriter {
    encoder: ::gif::Encoder<Vec<u8>>,
    width: u16,
    height: u16,
    pending: Option<Pending>,
    frames: usize,
}

impl std::fmt::Debug for GifWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GifWriter").field("size", &(self.width, self.height)).field("frames", &self.frames()).finish()
    }
}

impl GifWriter {
    /// A writer of `width` x `height` pictures (each 1 to 65535).
    pub fn new(width: u32, height: u32) -> Result<GifWriter, Error> {
        let (Ok(w), Ok(h)) = (u16::try_from(width), u16::try_from(height)) else { return Err(Error::BadSize) };
        if w == 0 || h == 0 {
            return Err(Error::BadSize);
        }
        let mut encoder = ::gif::Encoder::new(Vec::new(), w, h, &[]).map_err(|_| Error::Encode)?;
        encoder.set_repeat(::gif::Repeat::Infinite).map_err(|_| Error::Encode)?;
        Ok(GifWriter { encoder, width: w, height: h, pending: None, frames: 0 })
    }

    /// Adds a frame (`width * height * 4` RGBA bytes) shown for `centis` hundredths of a second (at least [`MIN_DELAY`]).
    pub fn push(&mut self, rgba: &[u8], centis: u32) -> Result<(), Error> {
        if rgba.len() != usize::from(self.width) * usize::from(self.height) * 4 {
            return Err(Error::BadFrame);
        }
        let centis = centis.max(u32::from(MIN_DELAY));
        if let Some(p) = &mut self.pending {
            if same_picture(&p.rgba, rgba) {
                p.centis += centis;
                return Ok(());
            }
        }
        self.flush()?;
        self.pending = Some(Pending { rgba: rgba.to_vec(), centis });
        Ok(())
    }

    /// Frames written so far, counting the one still waiting (a frame equal to the one before it only lengthens that one).
    pub fn frames(&self) -> usize {
        self.frames + usize::from(self.pending.is_some())
    }

    fn flush(&mut self) -> Result<(), Error> {
        let Some(Pending { rgba, centis }) = self.pending.take() else { return Ok(()) };
        let (indexed, palette, transparent) = quantise(&rgba);
        // A frame that has holes must clear the one before it, or that one shows through.
        let dispose =
            if transparent.is_some() { ::gif::DisposalMethod::Background } else { ::gif::DisposalMethod::Keep };
        let mut left = centis;
        // One frame holds up to 65535/100 s: a longer still is split (its repeat costs a few bytes).
        while left > 0 {
            let part = left.min(u32::from(u16::MAX)) as u16;
            left -= u32::from(part);
            let frame = ::gif::Frame {
                delay: part,
                dispose,
                transparent,
                width: self.width,
                height: self.height,
                palette: Some(palette.clone()),
                buffer: std::borrow::Cow::Borrowed(&indexed),
                ..::gif::Frame::default()
            };
            self.encoder.write_frame(&frame).map_err(|_| Error::Encode)?;
            self.frames += 1;
        }
        Ok(())
    }

    /// The finished file.
    pub fn finish(mut self) -> Result<Vec<u8>, Error> {
        self.flush()?;
        if self.frames == 0 {
            return Err(Error::BadFrame);
        }
        self.encoder.into_inner().map_err(|_| Error::Encode)
    }
}

/// Whether two frames look the same: equal pixels, where a see-through pixel is the same whatever colour it hides.
fn same_picture(a: &[u8], b: &[u8]) -> bool {
    a.chunks_exact(4).zip(b.chunks_exact(4)).all(|(p, q)| {
        let (pa, qa) = (p[3] >= OPAQUE_FROM, q[3] >= OPAQUE_FROM);
        pa == qa && (!pa || p[..3] == q[..3])
    })
}

/// Indexes `rgba`: the pixels, a palette (RGB triples, padded to a power of two) and the transparent index when any pixel is.
fn quantise(rgba: &[u8]) -> (Vec<u8>, Vec<u8>, Option<u8>) {
    let see_through = rgba.chunks_exact(4).any(|p| p[3] < OPAQUE_FROM);
    let limit = if see_through { 255 } else { 256 };
    let opaque = |p: &[u8]| p[3] >= OPAQUE_FROM;
    // Few colours: an exact palette.
    let mut seen: HashMap<[u8; 3], u8> = HashMap::new();
    let mut colours: Vec<[u8; 3]> = Vec::new();
    let mut exact = true;
    for p in rgba.chunks_exact(4).filter(|p| opaque(p)) {
        let key = [p[0], p[1], p[2]];
        if let std::collections::hash_map::Entry::Vacant(slot) = seen.entry(key) {
            if colours.len() == limit {
                exact = false;
                break;
            }
            slot.insert(colours.len() as u8);
            colours.push(key);
        }
    }
    let (indexed, hole): (Vec<u8>, u8) = if exact {
        let hole = colours.len() as u8; // the first index no colour has (unused when nothing is see-through)
        (rgba.chunks_exact(4).map(|p| if opaque(p) { seen[&[p[0], p[1], p[2]]] } else { hole }).collect(), hole)
    } else {
        let training: Vec<u8> =
            rgba.chunks_exact(4).filter(|p| opaque(p)).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        let nq = color_quant::NeuQuant::new(QUALITY, limit, &training);
        colours = nq.color_map_rgb().chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        let hole = colours.len() as u8;
        let mut last: Option<([u8; 3], u8)> = None;
        let indexed = rgba
            .chunks_exact(4)
            .map(|p| {
                if !opaque(p) {
                    return hole;
                }
                let key = [p[0], p[1], p[2]];
                match last {
                    Some((k, i)) if k == key => i,
                    _ => {
                        let i = nq.index_of(&[p[0], p[1], p[2], 255]) as u8;
                        last = Some((key, i));
                        i
                    }
                }
            })
            .collect();
        (indexed, hole)
    };
    let transparent = see_through.then_some(hole);
    let mut palette: Vec<u8> = colours.iter().flatten().copied().collect();
    let entries = (colours.len() + usize::from(see_through)).next_power_of_two().max(2);
    palette.resize(entries * 3, 0);
    (indexed, palette, transparent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::gif::{ColorOutput, DecodeOptions};

    const W: u32 = 8;
    const H: u32 = 6;

    fn solid(r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
        [r, g, b, a].repeat((W * H) as usize)
    }

    struct Decoded {
        delays: Vec<u16>,
        frames: Vec<Vec<u8>>,
        raw: Vec<u8>,
    }

    fn decode(bytes: &[u8]) -> Decoded {
        let mut options = DecodeOptions::new();
        options.set_color_output(ColorOutput::RGBA);
        let mut d = options.read_info(bytes).unwrap();
        let (mut delays, mut frames) = (vec![], vec![]);
        while let Some(f) = d.read_next_frame().unwrap() {
            delays.push(f.delay);
            frames.push(f.buffer.to_vec());
        }
        Decoded { delays, frames, raw: bytes.to_vec() }
    }

    fn written(frames: &[(Vec<u8>, u32)]) -> Decoded {
        let mut w = GifWriter::new(W, H).unwrap();
        for (rgba, centis) in frames {
            w.push(rgba, *centis).unwrap();
        }
        decode(&w.finish().unwrap())
    }

    #[test]
    fn frames_keep_their_order_and_delays() {
        let d = written(&[(solid(255, 0, 0, 255), 2), (solid(0, 255, 0, 255), 4), (solid(0, 0, 255, 255), 2)]);
        assert_eq!(d.delays, [2, 4, 2]);
        assert_eq!(&d.frames[1][..4], [0, 255, 0, 255]);
        assert_eq!(&d.frames[2][..4], [0, 0, 255, 255]);
    }

    #[test]
    fn no_frame_is_shorter_than_two_hundredths() {
        let d = written(&[(solid(255, 0, 0, 255), 0), (solid(0, 255, 0, 255), 1)]);
        assert_eq!(d.delays, [2, 2]);
    }

    #[test]
    fn equal_neighbours_become_one_longer_frame() {
        let a = solid(9, 9, 9, 255);
        let mut w = GifWriter::new(W, H).unwrap();
        for _ in 0..5 {
            w.push(&a, 2).unwrap();
        }
        w.push(&solid(1, 2, 3, 255), 2).unwrap();
        assert_eq!(w.frames(), 2);
        assert_eq!(decode(&w.finish().unwrap()).delays, [10, 2]);
    }

    #[test]
    fn the_file_repeats_forever() {
        let d = written(&[(solid(1, 1, 1, 255), 2), (solid(2, 2, 2, 255), 2)]);
        let at = d.raw.windows(11).position(|w| w == b"NETSCAPE2.0").expect("a NETSCAPE2.0 extension");
        assert_eq!(&d.raw[at + 11..at + 16], [3, 1, 0, 0, 0], "sub-block of 3: id 1, a loop count of 0 (forever), end");
        assert_eq!(&d.raw[..6], b"GIF89a");
        assert_eq!(*d.raw.last().unwrap(), 0x3B);
    }

    #[test]
    fn a_picture_with_few_colours_is_exact() {
        let mut img = solid(10, 20, 30, 255);
        for (i, px) in img.chunks_exact_mut(4).enumerate() {
            if i % 3 == 0 {
                px.copy_from_slice(&[200, 100, 50, 255]);
            }
        }
        assert_eq!(written(&[(img.clone(), 2)]).frames[0], img);
    }

    #[test]
    fn see_through_pixels_stay_see_through_and_the_rest_stay_opaque() {
        let mut img = solid(10, 20, 30, 255);
        for px in img.chunks_exact_mut(4).take(10) {
            px.copy_from_slice(&[0, 0, 0, 0]);
        }
        img[40..44].copy_from_slice(&[0, 0, 0, 255]); // an opaque black pixel is not mistaken for the hole
        let got = &written(&[(img.clone(), 2)]).frames[0];
        assert_eq!(got.chunks_exact(4).filter(|p| p[3] == 0).count(), 10);
        assert_eq!(&got[40..44], [0, 0, 0, 255]);
        assert_eq!(&got[48..52], [10, 20, 30, 255]);
    }

    #[test]
    fn many_colours_are_reduced_close_to_the_original() {
        let mut img = Vec::new();
        for i in 0..(W * H) {
            img.extend_from_slice(&[(i * 5) as u8, (i * 3) as u8, (255 - i * 4) as u8, 255]);
        }
        let mut wide = GifWriter::new(64, 64).unwrap();
        let mut big = Vec::new();
        for y in 0..64u32 {
            for x in 0..64u32 {
                big.extend_from_slice(&[(x * 4) as u8, (y * 4) as u8, ((x + y) * 2) as u8, 255]);
            }
        }
        wide.push(&big, 2).unwrap();
        let out = decode(&wide.finish().unwrap());
        let err: u64 = big.iter().zip(&out.frames[0]).map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
        assert!(err / big.len() as u64 <= 6, "mean error {}", err / big.len() as u64);
        assert_eq!(img.len(), (W * H * 4) as usize);
    }

    #[test]
    fn bad_input_is_refused() {
        assert_eq!(GifWriter::new(0, 5).unwrap_err(), Error::BadSize);
        assert_eq!(GifWriter::new(70_000, 5).unwrap_err(), Error::BadSize);
        let mut w = GifWriter::new(W, H).unwrap();
        assert_eq!(w.push(&[0; 7], 2).unwrap_err(), Error::BadFrame);
        assert_eq!(GifWriter::new(W, H).unwrap().finish().unwrap_err(), Error::BadFrame, "no frames is not a GIF");
    }
}
