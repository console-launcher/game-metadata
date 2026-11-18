// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Animated WebP: lossless VP8L frames (the pure-Rust `image-webp` encoder) in a hand-written RIFF container (RFC 9649:
//! `VP8X`, `ANIM`, one `ANMF` per frame). Every frame covers the whole canvas, durations are whole milliseconds, and the file
//! repeats forever.

use crate::Error;
use crate::pixels::clear_hidden_colour;

/// The most a WebP canvas is wide or high.
pub const MAX_SIDE: u32 = 16_384;

/// The longest one frame can last, in milliseconds (a 24-bit field).
pub const MAX_DURATION: u32 = 0xFF_FFFF;

/// One encoded frame: its whole `VP8L` chunk (header and padding included) and how long it shows.
#[derive(Debug)]
struct Encoded {
    chunk: Vec<u8>,
    millis: u32,
}

/// Writes an animated WebP.
#[derive(Debug)]
pub struct WebpWriter {
    width: u32,
    height: u32,
    pending: Option<(Vec<u8>, u32)>,
    done: Vec<Encoded>,
    alpha: bool,
}

impl WebpWriter {
    /// A writer of `width` x `height` pictures (each 1 to [`MAX_SIDE`]).
    pub fn new(width: u32, height: u32) -> Result<WebpWriter, Error> {
        if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) {
            return Err(Error::BadSize);
        }
        Ok(WebpWriter { width, height, pending: None, done: Vec::new(), alpha: false })
    }

    /// Adds a frame (`width * height * 4` RGBA bytes) shown for `millis` milliseconds.
    pub fn push(&mut self, rgba: &[u8], millis: u32) -> Result<(), Error> {
        if rgba.len() != self.width as usize * self.height as usize * 4 {
            return Err(Error::BadFrame);
        }
        let mut rgba = rgba.to_vec();
        clear_hidden_colour(&mut rgba);
        if let Some((prev, ms)) = &mut self.pending {
            if *prev == rgba {
                *ms += millis;
                return Ok(());
            }
        }
        self.flush()?;
        self.pending = Some((rgba, millis));
        Ok(())
    }

    /// Frames in the file so far (a frame equal to the one before it only lengthens that one).
    pub fn frames(&self) -> usize {
        self.done.len() + usize::from(self.pending.is_some())
    }

    fn flush(&mut self) -> Result<(), Error> {
        let Some((rgba, millis)) = self.pending.take() else { return Ok(()) };
        self.alpha |= rgba.chunks_exact(4).any(|p| p[3] != 255);
        let mut file = Vec::new();
        image_webp::WebPEncoder::new(&mut file)
            .encode(&rgba, self.width, self.height, image_webp::ColorType::Rgba8)
            .map_err(|_| Error::Encode)?;
        // The encoder writes the simple format: RIFF header, then one VP8L chunk.
        if file.len() < 20 || &file[12..16] != b"VP8L" {
            return Err(Error::Encode);
        }
        let chunk = file.split_off(12);
        let mut left = millis;
        loop {
            let part = left.min(MAX_DURATION);
            self.done.push(Encoded { chunk: chunk.clone(), millis: part });
            left -= part;
            if left == 0 {
                return Ok(());
            }
        }
    }

    /// The finished file.
    pub fn finish(mut self) -> Result<Vec<u8>, Error> {
        self.flush()?;
        if self.done.is_empty() {
            return Err(Error::BadFrame);
        }
        let u24 = |v: u32| [v as u8, (v >> 8) as u8, (v >> 16) as u8];
        let mut body = Vec::new();
        body.extend_from_slice(b"WEBP");
        // VP8X: flags (animation, alpha), 3 reserved bytes, canvas size minus one.
        body.extend_from_slice(b"VP8X");
        body.extend_from_slice(&10u32.to_le_bytes());
        body.extend_from_slice(&[0x02 | if self.alpha { 0x10 } else { 0 }, 0, 0, 0]);
        body.extend_from_slice(&u24(self.width - 1));
        body.extend_from_slice(&u24(self.height - 1));
        // ANIM: background colour (BGRA, ignored by players that blend nothing), loop count 0 = forever.
        body.extend_from_slice(b"ANIM");
        body.extend_from_slice(&6u32.to_le_bytes());
        body.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        for f in &self.done {
            body.extend_from_slice(b"ANMF");
            body.extend_from_slice(&((16 + f.chunk.len()) as u32).to_le_bytes());
            body.extend_from_slice(&[0; 6]); // x, y
            body.extend_from_slice(&u24(self.width - 1));
            body.extend_from_slice(&u24(self.height - 1));
            body.extend_from_slice(&u24(f.millis));
            body.push(0b10); // do not blend, do not dispose: the frame replaces the whole canvas
            body.extend_from_slice(&f.chunk);
        }
        let mut file = Vec::with_capacity(body.len() + 8);
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&u32::try_from(body.len()).map_err(|_| Error::Encode)?.to_le_bytes());
        file.extend_from_slice(&body);
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image_webp::WebPDecoder;
    use std::io::Cursor;

    const W: u32 = 10;
    const H: u32 = 6;

    fn picture(seed: u8, alpha: u8) -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..(W * H) as u8 {
            v.extend_from_slice(&[
                i.wrapping_mul(seed),
                i.wrapping_add(seed),
                255 - i,
                if i % 7 == 0 { alpha } else { 255 },
            ]);
        }
        v
    }

    fn make(frames: &[(Vec<u8>, u32)]) -> Vec<u8> {
        let mut w = WebpWriter::new(W, H).unwrap();
        for (rgba, ms) in frames {
            w.push(rgba, *ms).unwrap();
        }
        w.finish().unwrap()
    }

    /// The chunks at the top level of the RIFF: (fourcc, payload).
    fn chunks(file: &[u8]) -> Vec<([u8; 4], &[u8])> {
        let mut at = 12;
        let mut out = vec![];
        while at < file.len() {
            let size = u32::from_le_bytes(file[at + 4..at + 8].try_into().unwrap()) as usize;
            out.push((file[at..at + 4].try_into().unwrap(), &file[at + 8..at + 8 + size]));
            at += 8 + size + (size & 1);
        }
        assert_eq!(at, file.len(), "chunks end exactly at the end of the file");
        out
    }

    fn u24(b: &[u8]) -> u32 {
        u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16
    }

    #[test]
    fn the_container_has_the_chunks_in_order() {
        let file = make(&[(picture(3, 0), 17), (picture(5, 0), 16), (picture(7, 0), 17)]);
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(&file[8..12], b"WEBP");
        assert_eq!(u32::from_le_bytes(file[4..8].try_into().unwrap()) as usize, file.len() - 8);
        let cs = chunks(&file);
        let names: Vec<&[u8]> = cs.iter().map(|c| &c.0[..]).collect();
        assert_eq!(names, [&b"VP8X"[..], b"ANIM", b"ANMF", b"ANMF", b"ANMF"]);
        let vp8x = cs[0].1;
        assert_eq!(vp8x.len(), 10);
        assert_eq!(vp8x[0] & 0x02, 0x02, "animation flag");
        assert_eq!(vp8x[0] & 0x10, 0x10, "alpha flag (a frame has see-through pixels)");
        assert_eq!((u24(&vp8x[4..]) + 1, u24(&vp8x[7..]) + 1), (W, H), "canvas size is stored minus one");
        let anim = cs[1].1;
        assert_eq!(anim.len(), 6);
        assert_eq!(u16::from_le_bytes([anim[4], anim[5]]), 0, "loop count 0 is forever");
    }

    #[test]
    fn each_frame_has_its_exact_duration_and_covers_the_canvas() {
        let file = make(&[(picture(3, 255), 17), (picture(5, 255), 16), (picture(7, 255), 1234)]);
        let frames: Vec<&[u8]> = chunks(&file).into_iter().filter(|c| &c.0 == b"ANMF").map(|c| c.1).collect();
        let durations: Vec<u32> = frames.iter().map(|f| u24(&f[12..])).collect();
        assert_eq!(durations, [17, 16, 1234]);
        for f in &frames {
            assert_eq!((u24(f), u24(&f[3..])), (0, 0), "no offset");
            assert_eq!((u24(&f[6..]) + 1, u24(&f[9..]) + 1), (W, H));
            assert_eq!(f[15] & 0b10, 0b10, "no blending: the frame replaces the canvas");
            assert_eq!(&f[16..20], b"VP8L");
            let size = u32::from_le_bytes(f[20..24].try_into().unwrap()) as usize;
            assert_eq!(f.len(), 16 + 8 + size + (size & 1), "one VP8L chunk, padded to even");
            assert_eq!(f[24], 0x2F, "the VP8L signature");
        }
    }

    #[test]
    fn opaque_frames_do_not_set_the_alpha_flag() {
        let file = make(&[(picture(3, 255), 20)]);
        assert_eq!(chunks(&file)[0].1[0] & 0x10, 0);
    }

    #[test]
    fn it_decodes_back_to_the_same_pixels() {
        let (a, b) = (picture(3, 0), picture(9, 100));
        let file = make(&[(a.clone(), 17), (b.clone(), 40)]);
        let mut d = WebPDecoder::new(Cursor::new(&file)).unwrap();
        assert!(d.is_animated());
        assert_eq!((d.dimensions(), d.num_frames(), d.has_alpha()), ((W, H), 2, true));
        let mut buf = vec![0u8; d.output_buffer_size().unwrap()];
        let clean = |rgba: &[u8]| -> Vec<u8> {
            rgba.chunks_exact(4).flat_map(|p| if p[3] == 0 { [0, 0, 0, 0] } else { [p[0], p[1], p[2], p[3]] }).collect()
        };
        assert_eq!(d.read_frame(&mut buf).unwrap(), 17);
        assert_eq!(buf, clean(&a));
        assert_eq!(d.read_frame(&mut buf).unwrap(), 40);
        assert_eq!(buf, clean(&b));
    }

    #[test]
    fn equal_neighbours_become_one_longer_frame() {
        let a = picture(3, 255);
        let mut w = WebpWriter::new(W, H).unwrap();
        for ms in [17, 16, 17] {
            w.push(&a, ms).unwrap();
        }
        w.push(&picture(4, 255), 16).unwrap();
        assert_eq!(w.frames(), 2);
        let file = w.finish().unwrap();
        let durations: Vec<u32> =
            chunks(&file).into_iter().filter(|c| &c.0 == b"ANMF").map(|c| u24(&c.1[12..])).collect();
        assert_eq!(durations, [50, 16]);
    }

    #[test]
    fn a_still_longer_than_the_field_is_split() {
        let mut w = WebpWriter::new(W, H).unwrap();
        w.push(&picture(3, 255), MAX_DURATION).unwrap();
        w.push(&picture(3, 255), 10).unwrap();
        let file = w.finish().unwrap();
        let durations: Vec<u32> =
            chunks(&file).into_iter().filter(|c| &c.0 == b"ANMF").map(|c| u24(&c.1[12..])).collect();
        assert_eq!(durations, [MAX_DURATION, 10]);
    }

    #[test]
    fn bad_input_is_refused() {
        assert_eq!(WebpWriter::new(0, 5).unwrap_err(), Error::BadSize);
        assert_eq!(WebpWriter::new(MAX_SIDE + 1, 5).unwrap_err(), Error::BadSize);
        let mut w = WebpWriter::new(W, H).unwrap();
        assert_eq!(w.push(&[0; 9], 10).unwrap_err(), Error::BadFrame);
        assert_eq!(WebpWriter::new(W, H).unwrap().finish().unwrap_err(), Error::BadFrame);
    }
}
