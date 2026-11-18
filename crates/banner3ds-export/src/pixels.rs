// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pixel helpers shared by the encoders.

/// `rgba` (`width * height * 4` bytes) enlarged `factor` times by repeating pixels (nearest neighbour, so edges stay sharp).
pub fn upscale_nearest(rgba: &[u8], width: usize, height: usize, factor: usize) -> Vec<u8> {
    assert_eq!(rgba.len(), width * height * 4);
    if factor <= 1 {
        return rgba.to_vec();
    }
    let mut out = Vec::with_capacity(rgba.len() * factor * factor);
    for row in rgba.chunks_exact(width * 4) {
        let start = out.len();
        for px in row.chunks_exact(4) {
            for _ in 0..factor {
                out.extend_from_slice(px);
            }
        }
        let line = out[start..].to_vec();
        for _ in 1..factor {
            out.extend_from_slice(&line);
        }
    }
    out
}

/// Pixels with no alpha carry no colour: sets their RGB to 0 (so identical looks compare and compress the same).
pub(crate) fn clear_hidden_colour(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        if px[3] == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upscaling_repeats_pixels() {
        let img = [1, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255, 4, 4, 4, 255]; // 2 x 2
        let big = upscale_nearest(&img, 2, 2, 2);
        assert_eq!(big.len(), 4 * 4 * 4);
        let row = |y: usize| -> Vec<u8> { big[y * 16..y * 16 + 16].iter().step_by(4).copied().collect() };
        assert_eq!(row(0), [1, 1, 2, 2]);
        assert_eq!(row(1), [1, 1, 2, 2]);
        assert_eq!(row(2), [3, 3, 4, 4]);
        assert_eq!(row(3), [3, 3, 4, 4]);
        assert_eq!(upscale_nearest(&img, 2, 2, 1), img);
    }
}
