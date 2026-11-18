// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The jingle of a 3DS banner: a BCWAV (CWAV) sound file, decoded to 16-bit PCM.
//!
//! PCM8, PCM16 and DSP-ADPCM read; IMA-ADPCM (which banners do not use) and big-endian files give `None`. Parsing never panics on
//! hostile input and never allocates more than [`MAX_SAMPLES`] samples.

use crate::reader::{R, Reader};
use alloc::vec;
use alloc::vec::Vec;

/// Most samples (all channels together) one sound may hold: 3 minutes of stereo at 48 kHz.
pub const MAX_SAMPLES: usize = 48_000 * 2 * 180;

/// A decoded sound: interleaved 16-bit samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    /// Samples per second, per channel.
    pub sample_rate: u32,
    /// Channels (1 or 2 are the usual).
    pub channels: u8,
    /// `frames * channels` samples, channel-interleaved.
    pub pcm: Vec<i16>,
}

impl Sound {
    /// Samples per channel.
    pub fn frames(&self) -> usize {
        self.pcm.len() / usize::from(self.channels.max(1))
    }
}

/// Decodes a BCWAV file; `None` when it is not one, uses another encoding, or is damaged.
pub fn decode(bytes: &[u8]) -> Option<Sound> {
    parse(bytes).ok().flatten()
}

fn parse(bytes: &[u8]) -> R<Option<Sound>> {
    let r = Reader(bytes);
    if r.slice(0, 4)? != b"CWAV" || r.u16(4)? != 0xFEFF {
        return Ok(None); // not a CWAV, or big-endian
    }
    let (mut info, mut data) = (None, None);
    for i in 0..i64::from(r.u16(0x10)?.min(8)) {
        let at = 0x14 + 12 * i;
        match r.u16(at)? {
            0x7000 => info = Some(i64::from(r.u32(at + 4)?)),
            0x7001 => data = Some(i64::from(r.u32(at + 4)?)),
            _ => {}
        }
    }
    let (Some(info), Some(data)) = (info, data) else { return Ok(None) };
    if r.slice(info, 4)? != b"INFO" || r.slice(data, 4)? != b"DATA" {
        return Ok(None);
    }
    let encoding = r.u8(info + 8)?;
    let sample_rate = r.u32(info + 0xC)?;
    let frames = r.u32(info + 0x14)? as usize; // the loop end of a file that does not loop is its length
    let table = info + 0x1C;
    let channels = r.u32(table)? as usize;
    if !(1..=8).contains(&channels) || sample_rate == 0 || frames == 0 || frames.saturating_mul(channels) > MAX_SAMPLES
    {
        return Ok(None);
    }
    let mut decoded: Vec<Vec<i16>> = Vec::with_capacity(channels);
    for c in 0..channels as i64 {
        let ci = table + i64::from(r.u32(table + 8 + 8 * c)?);
        let samples = data + 8 + i64::from(r.u32(ci + 4)?);
        let pcm = match encoding {
            0 => r.slice(samples, frames as i64)?.iter().map(|&b| i16::from(b as i8) * 256).collect(),
            1 => {
                (0..frames as i64).map(|i| r.u16(samples + 2 * i).map(|v| v as u16 as i16)).collect::<R<Vec<i16>>>()?
            }
            2 => {
                let adpcm = ci + i64::from(r.u32(ci + 12)?);
                let mut coefs = [0i32; 16];
                for (k, c) in coefs.iter_mut().enumerate() {
                    *c = i32::from(r.u16(adpcm + 2 * k as i64)? as u16 as i16);
                }
                dsp_adpcm(r.slice(samples, frames.div_ceil(14) as i64 * 8)?, &coefs, frames)
            }
            _ => return Ok(None), // IMA-ADPCM and anything newer
        };
        decoded.push(pcm);
    }
    let mut pcm = vec![0i16; frames * channels];
    for (c, channel) in decoded.iter().enumerate() {
        for (i, &s) in channel.iter().enumerate() {
            pcm[i * channels + c] = s;
        }
    }
    Ok(Some(Sound { sample_rate, channels: channels as u8, pcm }))
}

/// Nintendo DSP-ADPCM: 8-byte frames of 14 samples (a header byte: predictor index and scale, then 14 signed nibbles).
fn dsp_adpcm(data: &[u8], coefs: &[i32; 16], frames: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(frames);
    let (mut h1, mut h2) = (0i32, 0i32);
    for frame in data.chunks_exact(8) {
        let predictor = usize::from(frame[0] >> 4).min(7);
        let scale = u32::from(frame[0] & 15);
        let (c1, c2) = (coefs[predictor * 2], coefs[predictor * 2 + 1]);
        for n in 0..14 {
            if out.len() == frames {
                return out;
            }
            let byte = frame[1 + n / 2];
            let nibble = i32::from(if n % 2 == 0 { byte >> 4 } else { byte & 15 });
            let nibble = if nibble >= 8 { nibble - 16 } else { nibble };
            let sample = ((((nibble << scale) << 11) + 1024 + c1 * h1 + c2 * h2) >> 11).clamp(-32768, 32767);
            h2 = h1;
            h1 = sample;
            out.push(sample as i16);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamemeta_fixtures as fx;

    #[test]
    fn pcm16_stereo_is_interleaved() {
        let s = decode(&fx::bcwav_pcm16(32768, &[&[1, 2, 3], &[-1, -2, -3]])).unwrap();
        assert_eq!((s.sample_rate, s.channels, s.frames()), (32768, 2, 3));
        assert_eq!(s.pcm, [1, -1, 2, -2, 3, -3]);
    }

    #[test]
    fn pcm8_widens_to_16_bits() {
        let s = decode(&fx::bcwav_pcm8(16000, &[&[0, 1, -1, 127, -128]])).unwrap();
        assert_eq!((s.sample_rate, s.channels), (16000, 1));
        assert_eq!(s.pcm, [0, 256, -256, 127 * 256, -128 * 256]);
    }

    #[test]
    fn dsp_adpcm_with_zero_coefficients_scales_the_nibbles() {
        // Header: predictor 0, scale 2; nibbles 1, 2, 7, 8 (-8), F (-1), 0 .. then padding. sample = nibble << 2.
        let frame = [0x02, 0x12, 0x78, 0xF0, 0, 0, 0, 0];
        let s = decode(&fx::bcwav_dsp(32000, 6, &[([0; 16], &frame)])).unwrap();
        assert_eq!(s.pcm, [4, 8, 28, -32, -4, 0]);
    }

    #[test]
    fn dsp_adpcm_predicts_from_the_two_previous_samples() {
        // Coefficients for predictor 1: c1 = 2048 (1.0), c2 = 0: each sample adds the previous one.
        let mut coefs = [0i16; 16];
        coefs[2] = 2048;
        // Header: predictor 1, scale 0; nibbles 1 1 1 1 -> 1, 2, 3, 4.
        let frame = [0x10, 0x11, 0x11, 0, 0, 0, 0, 0];
        let s = decode(&fx::bcwav_dsp(32000, 4, &[(coefs, &frame)])).unwrap();
        assert_eq!(s.pcm, [1, 2, 3, 4]);
    }

    #[test]
    fn dsp_adpcm_clamps_to_16_bits() {
        let frame = [0x0E, 0x70, 0, 0, 0, 0, 0, 0]; // scale 14: 7 << 14 = 114688 -> 32767
        let s = decode(&fx::bcwav_dsp(32000, 1, &[([0; 16], &frame)])).unwrap();
        assert_eq!(s.pcm, [32767]);
    }

    #[test]
    fn a_banner_container_hands_over_its_jingle() {
        let wav = fx::bcwav_pcm16(8000, &[&[5, 6]]);
        let cbmd = fx::cbmd_with_jingle(&[0; 0x20], &[], &wav);
        assert_eq!(crate::Cbmd::parse(&cbmd).and_then(|c| c.jingle).as_deref(), Some(&wav[..]));
        assert!(crate::Cbmd::parse(&fx::cbmd(&[0; 0x20], &[])).unwrap().jingle.is_none());
    }

    #[test]
    fn junk_gives_none_and_never_panics() {
        let good = fx::bcwav_pcm16(8000, &[&[1, 2, 3, 4]]);
        assert!(decode(&good).is_some());
        assert!(decode(&[]).is_none());
        assert!(decode(b"CWAV").is_none());
        for cut in 0..good.len() {
            let _ = decode(&good[..cut]);
        }
        let mut big_endian = good.clone();
        big_endian[4..6].copy_from_slice(&[0xFE, 0xFF]);
        assert!(decode(&big_endian).is_none());
        let mut huge = good.clone();
        huge[0x40 + 0x14..0x40 + 0x18].copy_from_slice(&u32::MAX.to_le_bytes()); // a sample count far past the data
        assert!(decode(&huge).is_none());
        for i in 0..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 0xFF;
            let _ = decode(&bad);
        }
    }
}
