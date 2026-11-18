// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Scoring a renderer against reference keyframes captured from the real HOME Menu (the capture tooling under `scripts/`): each keyframe is
//! the top screen, 400 x 240, with everything outside the banner window painted pure magenta. Our frame (over black) gets the
//! same mask, both go to luma box-averaged to 100 x 60, and rows 6..54 are scored as `100 * MAD / 255 + 50 * (1 - SSIM)` (8 x 8
//! windows, stride 4): the reference implementation's `FrameScore`. 0 is identical; typical banners run 2-4 on a good renderer.
//! The clip offset that lines the capture up (HOME's start is detected from a fade) is searched, shared by all keyframes.

use std::path::Path;

use gamemeta_core::{RgbaImage, png};

/// The top screen's size.
pub const WIDTH: u32 = 400;
/// The top screen's height.
pub const HEIGHT: u32 = 240;
/// A banner at or under this mean score matches HOME.
pub const MATCH: f32 = 3.0;
const ROW_FROM: usize = 6;
const ROW_TO: usize = 54;
const DOWN: usize = 4;

/// One reference keyframe: its clip frame (refreshes after the banner appeared) and the masked top screen.
#[derive(Debug, Clone)]
pub struct Keyframe {
    /// Clip frame.
    pub clip: u32,
    /// The masked top screen.
    pub image: RgbaImage,
}

/// The keyframes (`c<clip>.png`) of a capture folder, by clip.
pub fn keyframes(dir: &Path) -> Vec<Keyframe> {
    let mut out: Vec<Keyframe> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| {
            let path = e.ok()?.path();
            let name = path.file_name()?.to_str()?;
            let clip = name.strip_prefix('c')?.strip_suffix(".png")?.parse().ok()?;
            let image = png::decode(&std::fs::read(&path).ok()?)?;
            assert_eq!((image.width, image.height), (WIDTH, HEIGHT), "{name}");
            Some(Keyframe { clip, image })
        })
        .collect();
    out.sort_by_key(|k| k.clip);
    out
}

fn magenta(p: &[u8]) -> bool {
    p[0] == 255 && p[1] == 0 && p[2] == 255
}

fn luma(rgba: &[u8], mask: &RgbaImage) -> Vec<f32> {
    let (w, h) = (WIDTH as usize / DOWN, HEIGHT as usize / DOWN);
    let mut out = vec![0f32; w * h];
    for (i, (p, m)) in rgba.chunks(4).zip(mask.pixels.chunks(4)).enumerate() {
        let (x, y) = (i % WIDTH as usize, i / WIDTH as usize);
        let (r, g, b) =
            if magenta(m) { (255.0, 0.0, 255.0) } else { (f32::from(p[0]), f32::from(p[1]), f32::from(p[2])) };
        out[(y / DOWN) * w + x / DOWN] += 0.299 * r + 0.587 * g + 0.114 * b;
    }
    for v in &mut out {
        *v /= (DOWN * DOWN) as f32;
    }
    out
}

fn ssim(a: &[f32], b: &[f32], w: usize) -> f64 {
    const WIN: usize = 8;
    const STRIDE: usize = 4;
    let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
    let (mut sum, mut n) = (0.0, 0);
    let mut y0 = ROW_FROM;
    while y0 + WIN <= ROW_TO {
        let mut x0 = 0;
        while x0 + WIN <= w {
            let at = |v: &[f32], x: usize, y: usize| f64::from(v[y * w + x]);
            let k = (WIN * WIN) as f64;
            let (mut ma, mut mb) = (0.0, 0.0);
            for y in y0..y0 + WIN {
                for x in x0..x0 + WIN {
                    ma += at(a, x, y);
                    mb += at(b, x, y);
                }
            }
            ma /= k;
            mb /= k;
            let (mut va, mut vb, mut cov) = (0.0, 0.0, 0.0);
            for y in y0..y0 + WIN {
                for x in x0..x0 + WIN {
                    let (da, db) = (at(a, x, y) - ma, at(b, x, y) - mb);
                    va += da * da;
                    vb += db * db;
                    cov += da * db;
                }
            }
            va /= k - 1.0;
            vb /= k - 1.0;
            cov /= k - 1.0;
            sum += ((2.0 * ma * mb + c1) * (2.0 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            n += 1;
            x0 += STRIDE;
        }
        y0 += STRIDE;
    }
    if n == 0 { 1.0 } else { sum / n as f64 }
}

/// The score of `ours` (400 x 240 RGBA over black) against `reference` (lower is closer, 0 identical).
pub fn score(ours: &[u8], reference: &RgbaImage) -> f32 {
    let w = WIDTH as usize / DOWN;
    let (a, b) = (luma(ours, reference), luma(&reference.pixels, reference));
    let mut mad = 0.0f64;
    for y in ROW_FROM..ROW_TO {
        for x in 0..w {
            mad += f64::from((a[y * w + x] - b[y * w + x]).abs());
        }
    }
    mad /= ((ROW_TO - ROW_FROM) * w) as f64;
    (100.0 * mad / 255.0 + 50.0 * (1.0 - ssim(&a, &b, w))) as f32
}

/// Mean and largest per-channel difference over the pixels the scorer keeps (not magenta), 0..255.
pub fn rgb_diff(ours: &[u8], reference: &RgbaImage) -> (f64, u8) {
    let (mut sum, mut n, mut max) = (0u64, 0u64, 0u8);
    for (p, r) in ours.chunks(4).zip(reference.pixels.chunks(4)) {
        if magenta(r) {
            continue;
        }
        for c in 0..3 {
            let d = p[c].abs_diff(r[c]);
            sum += u64::from(d);
            max = max.max(d);
        }
        n += 3;
    }
    (if n == 0 { 0.0 } else { sum as f64 / n as f64 }, max)
}

/// How a renderer lines up with a capture: the clip offset, the mean score, and per keyframe (clip, score, mean, max).
#[derive(Debug, Clone)]
pub struct Fit {
    /// Clip frames added to each keyframe's clip.
    pub offset: i32,
    /// The mean score over the keyframes.
    pub score: f32,
    /// Per keyframe: clip, score, mean and largest channel difference.
    pub frames: Vec<(u32, f32, f64, u8)>,
}

/// The best offset in `-window..=window` for `render(frame)` (400 x 240 RGBA over black) against `keys`: a grid at `step`,
/// then halving steps around the best (the original implementation's offset search).
pub fn fit(keys: &[Keyframe], window: i32, step: i32, mut render: impl FnMut(f32) -> Vec<u8>) -> Fit {
    assert!(!keys.is_empty(), "no keyframes");
    let mut seen: std::collections::BTreeMap<i32, f32> = Default::default();
    let mut cost = |o: i32| -> f32 {
        *seen.entry(o).or_insert_with(|| {
            keys.iter().map(|k| score(&render(k.clip as f32 + o as f32), &k.image)).sum::<f32>() / keys.len() as f32
        })
    };
    let mut grid: Vec<i32> = (-window..=window).step_by(step as usize).collect();
    if grid.last() != Some(&window) {
        grid.push(window);
    }
    let mut best = grid.iter().copied().min_by(|a, b| cost(*a).total_cmp(&cost(*b))).unwrap();
    let mut s = step;
    while s > 1 {
        let span = s;
        s = (s / 2).max(1);
        let (lo, hi) = ((best - span + 1).max(-window), (best + span - 1).min(window));
        let mut cand: Vec<i32> = (lo..=hi).step_by(s as usize).collect();
        cand.push(best);
        best = cand.into_iter().min_by(|a, b| cost(*a).total_cmp(&cost(*b))).unwrap();
    }
    let total = cost(best);
    let frames = keys
        .iter()
        .map(|k| {
            let ours = render(k.clip as f32 + best as f32);
            let (mean, max) = rgb_diff(&ours, &k.image);
            (k.clip, score(&ours, &k.image), mean, max)
        })
        .collect();
    Fit { offset: best, score: total, frames }
}

/// A keyframe known to score over [`MATCH`]: its clip, its ceiling (the measured score plus 1.5, rounded up to 0.5), and why.
pub type Known = (u32, f32, &'static str);

impl Fit {
    /// Ok when every keyframe scores at most [`MATCH`], or, for one listed in `known`, at most its ceiling while still over
    /// MATCH (a listed keyframe that now matches must come off the list, so the list only shrinks).
    pub fn check(&self, known: &[Known]) -> Result<(), String> {
        let mut problems = Vec::new();
        for &(clip, score, ..) in &self.frames {
            match known.iter().find(|k| k.0 == clip) {
                None if score > MATCH => problems.push(format!(
                    "c{clip:04}: a new mismatch, {score:.2} > {MATCH} (fix it, or list it with the reason)"
                )),
                Some((_, ceiling, why)) if score > *ceiling => {
                    problems.push(format!("c{clip:04}: {score:.2} > its ceiling {ceiling} ({why})"))
                }
                Some(_) if score <= MATCH => {
                    problems.push(format!("c{clip:04}: {score:.2} now matches: take it off the known mismatches"))
                }
                _ => {}
            }
        }
        if problems.is_empty() { Ok(()) } else { Err(problems.join("; ")) }
    }
}

impl std::fmt::Display for Fit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "offset {:+} frames, mean score {:.2}", self.offset, self.score)?;
        for (clip, s, mean, max) in &self.frames {
            writeln!(f, "  c{clip:04}: score {s:5.2}  rgb mean {mean:5.2} max {max:3}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(scores: &[(u32, f32)]) -> Fit {
        Fit { offset: 0, score: 0.0, frames: scores.iter().map(|&(c, s)| (c, s, 0.0, 0)).collect() }
    }

    #[test]
    fn every_keyframe_under_the_match_bar_passes() {
        assert_eq!(fit(&[(240, 0.6), (300, MATCH)]).check(&[]), Ok(()));
    }

    #[test]
    fn an_unlisted_keyframe_over_the_bar_fails_even_when_the_mean_matches() {
        let err = fit(&[(240, 0.5), (300, 0.5), (430, 3.1)]).check(&[]).unwrap_err();
        assert!(err.contains("c0430") && err.contains("new mismatch"), "{err}");
    }

    #[test]
    fn a_listed_keyframe_under_its_ceiling_passes() {
        assert_eq!(fit(&[(240, 0.6), (430, 3.1)]).check(&[(430, 5.0, "spark")]), Ok(()));
    }

    #[test]
    fn a_listed_keyframe_over_its_ceiling_fails_with_its_reason() {
        let err = fit(&[(430, 5.5)]).check(&[(430, 5.0, "spark")]).unwrap_err();
        assert!(err.contains("ceiling") && err.contains("spark"), "{err}");
    }

    #[test]
    fn a_listed_keyframe_that_now_matches_must_come_off_the_list() {
        let err = fit(&[(430, 1.0)]).check(&[(430, 5.0, "spark")]).unwrap_err();
        assert!(err.contains("now matches"), "{err}");
    }
}
