// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Which frames of a banner an export draws: the intro once, then one full loop; or the whole animation when nothing loops;
//! or a single frame when nothing moves.

use banner3ds_scene::BannerScene;
use banner3ds_scene::draw::Player;

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The longest export, in clip frames (60 to the second): a minute.
pub const MAX_FRAMES: f32 = 3600.0;

/// Seconds a still banner lasts in a video (a GIF or WebP of one is a single frame).
pub const STILL_SECONDS: f32 = 3.0;

/// The shape of a banner's animation, in clip frames (60 to the second).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timeline {
    /// The part that plays once (clips that do not loop; they hold their last pose afterwards).
    pub intro: f32,
    /// The part that repeats (0: nothing repeats).
    pub looped: f32,
}

impl Timeline {
    /// The timeline of `scene`.
    pub fn of(scene: &BannerScene) -> Timeline {
        let player = Player::new(scene);
        let clips = || {
            scene
                .animations
                .skeletal
                .iter()
                .chain(&scene.animations.material)
                .chain(&scene.animations.visibility)
                .filter(|c| c.frames > 0.0 && !c.tracks.is_empty())
        };
        let intro = clips().filter(|c| !c.looping).map(|c| c.frames).fold(0.0f32, f32::max);
        let looped = if !player.animated {
            0.0
        } else if player.spins {
            player.frames // the turntable's turn and every clip's length, already worked out for the page
        } else {
            let lengths: Vec<i64> = clips().filter(|c| c.looping).map(|c| (c.frames.round() as i64).max(1)).collect();
            let lcm = lengths.iter().try_fold(1i64, |acc, &n| {
                let l = acc / gcd(acc, n) * n;
                (l <= MAX_FRAMES as i64).then_some(l)
            });
            match lcm {
                _ if lengths.is_empty() => 0.0,
                Some(l) => l as f32,
                None => clips().filter(|c| c.looping).map(|c| c.frames).fold(0.0f32, f32::max),
            }
        };
        Timeline { intro, looped }
    }

    /// Whether anything moves.
    pub fn animated(&self) -> bool {
        self.total() > 0.0
    }

    /// The length exported: the intro and one loop, at most [`MAX_FRAMES`].
    pub fn total(&self) -> f32 {
        (self.intro + self.looped).min(MAX_FRAMES)
    }

    /// How many pictures an export at `fps` holds: one for a still banner.
    pub fn count(&self, fps: u32) -> usize {
        if self.animated() { ((self.total() * fps as f32 / 60.0).ceil() as usize).max(1) } else { 1 }
    }

    /// The clip frame (60 to the second) of picture `index` at `fps`.
    pub fn frame_at(&self, index: usize, fps: u32) -> f32 {
        index as f32 * 60.0 / fps as f32
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
    fn a_looping_clip_exports_one_loop() {
        let t = Timeline::of(&feature("bone-anim"));
        assert!(t.animated());
        assert_eq!(t.intro, 0.0);
        assert!(t.looped > 0.0, "{t:?}");
        assert_eq!(t.total(), t.looped);
    }

    #[test]
    fn pictures_are_counted_and_placed_at_the_rate() {
        let t = Timeline { intro: 30.0, looped: 90.0 };
        assert_eq!(t.total(), 120.0);
        assert_eq!(t.count(60), 120);
        assert_eq!(t.count(50), 100);
        assert_eq!(t.frame_at(0, 50), 0.0);
        assert!((t.frame_at(50, 50) - 60.0).abs() < 1e-4);
        assert_eq!(t.count(30), 60);
    }

    #[test]
    fn a_banner_that_does_not_move_is_one_picture() {
        let t = Timeline { intro: 0.0, looped: 0.0 };
        assert!(!t.animated());
        assert_eq!((t.count(60), t.count(50)), (1, 1));
    }

    #[test]
    fn nothing_is_longer_than_a_minute() {
        let t = Timeline { intro: 2000.0, looped: 3000.0 };
        assert_eq!(t.total(), MAX_FRAMES);
        assert_eq!(t.count(60), 3600);
    }

    #[test]
    fn every_feature_banner_has_a_sane_timeline() {
        let dir = format!("{}/../../fixtures/scene/features", env!("CARGO_MANIFEST_DIR"));
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "cbmd") {
                let scene = from_banner(&std::fs::read(&path).unwrap()).unwrap();
                let t = Timeline::of(&scene);
                assert!(t.intro >= 0.0 && t.looped >= 0.0 && t.total() <= MAX_FRAMES, "{path:?} {t:?}");
                assert_eq!(
                    t.animated(),
                    banner3ds_soft::Renderer::new(&scene).animated(),
                    "{path:?}: the renderer agrees on whether it moves"
                );
            }
        }
    }

    #[test]
    fn a_clip_that_plays_once_is_the_intro_and_a_looping_one_after_it_the_loop() {
        let mut scene = feature("bone-anim");
        let base = Timeline::of(&scene);
        let mut once = scene.animations.skeletal[0].clone();
        once.looping = false;
        once.frames = 45.0;
        scene.animations.skeletal.push(once);
        let t = Timeline::of(&scene);
        assert_eq!(t.intro, 45.0);
        assert_eq!(t.looped, base.looped, "the loop is the looping clip's, whatever plays once before it");
        assert_eq!(t.total(), 45.0 + base.looped);
        // Only a clip that plays once: the whole animation, nothing repeats.
        scene.animations.skeletal[0].looping = false;
        let only_intro = Timeline::of(&scene);
        assert_eq!(only_intro.intro, 120.0);
        assert!(only_intro.looped == 0.0 || Player::new(&scene).spins, "{only_intro:?}");
    }
}
