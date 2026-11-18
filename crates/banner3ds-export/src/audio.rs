// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sound for a video export: the banner's jingle resampled to a rate the browser's audio encoder takes, and cut to the video's length.

/// `pcm` (interleaved, `channels` of them) at `from` Hz converted to `to` Hz by linear interpolation; empty for a zero rate or
/// channel count.
pub fn resample(pcm: &[i16], channels: usize, from: u32, to: u32) -> Vec<i16> {
    let frames = pcm.len().checked_div(channels).unwrap_or(0);
    if channels == 0 || frames == 0 || from == 0 || to == 0 {
        return Vec::new();
    }
    if from == to {
        return pcm[..frames * channels].to_vec();
    }
    let out_frames = (frames as u64 * u64::from(to)).div_ceil(u64::from(from)) as usize;
    let mut out = Vec::with_capacity(out_frames * channels);
    for i in 0..out_frames {
        // Position in the source, in 1/`to` of a source sample.
        let pos = i as u64 * u64::from(from);
        let (a, frac) = ((pos / u64::from(to)) as usize, (pos % u64::from(to)) as i64);
        let b = (a + 1).min(frames - 1);
        let a = a.min(frames - 1);
        for c in 0..channels {
            let (x, y) = (i64::from(pcm[a * channels + c]), i64::from(pcm[b * channels + c]));
            out.push((x + (y - x) * frac / i64::from(to)) as i16);
        }
    }
    out
}

/// The first `seconds` of `pcm` (interleaved at `rate` Hz); all of it when it is shorter.
pub fn trim(pcm: &[i16], channels: usize, rate: u32, seconds: f64) -> &[i16] {
    let frames = (seconds * f64::from(rate)).max(0.0) as usize;
    &pcm[..pcm.len().min(frames.saturating_mul(channels))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_rate_is_unchanged() {
        assert_eq!(resample(&[1, 2, 3, 4], 2, 32000, 32000), [1, 2, 3, 4]);
        assert_eq!(resample(&[1, 2, 3], 2, 8000, 8000), [1, 2], "a half frame is dropped");
    }

    #[test]
    fn doubling_the_rate_interpolates_between_samples() {
        assert_eq!(resample(&[0, 100, 200], 1, 8000, 16000), [0, 50, 100, 150, 200, 200]);
    }

    #[test]
    fn the_length_follows_the_rates_and_channels_stay_apart() {
        let stereo: Vec<i16> = (0..3200).flat_map(|i| [i as i16, -(i as i16)]).collect();
        let out = resample(&stereo, 2, 32000, 48000);
        assert_eq!(out.len() / 2, 4800);
        assert!(out.chunks_exact(2).all(|f| f[0] == -f[1]), "left and right never mix");
        assert_eq!(resample(&stereo, 2, 32000, 16000).len() / 2, 1600);
    }

    #[test]
    fn nothing_in_nothing_out() {
        assert!(resample(&[], 1, 8000, 16000).is_empty());
        assert!(resample(&[1, 2], 0, 8000, 16000).is_empty());
        assert!(resample(&[1, 2], 1, 0, 16000).is_empty());
        assert!(resample(&[1, 2], 1, 8000, 0).is_empty());
    }

    #[test]
    fn trimming_cuts_to_the_length_and_keeps_whole_frames() {
        let pcm = [0i16; 2000];
        assert_eq!(trim(&pcm, 2, 1000, 0.5).len(), 1000);
        assert_eq!(trim(&pcm, 2, 1000, 5.0).len(), 2000);
        assert_eq!(trim(&pcm, 2, 1000, -1.0).len(), 0);
    }
}
