// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A channel banner's animation (`RLAN`): per pane or material, tracks of keyframes (step or hermite). `frames` is the length; a
//! loop animation repeats. Tags: `RLPA` pane transform, `RLTS` texture SRT, `RLVI` visibility, `RLVC` vertex colors and alpha,
//! `RLMC` material color, `RLTP` texture pattern. Unverified against real banners (see `docs/design.md`); the offsets that are
//! guesses are marked "GUESS".

use alloc::string::String;
use alloc::vec::Vec;

use super::be::Be;

/// Caps that keep a damaged file cheap to read (the original implementation has none): keys over the whole animation, and tracks.
const MAX_KEYS: usize = 1 << 20;
const MAX_TRACKS: usize = 1 << 16;

#[derive(Debug, Clone, Copy)]
pub(super) struct Key {
    pub frame: f32,
    pub value: f32,
    pub slope: f32,
}

/// One animated value: `tag` ("RLPA" etc.), `index` (texture map for RLTS and RLTP), `kind` the attribute of the tag.
#[derive(Debug)]
pub(super) struct Track {
    pub tag: [u8; 4],
    pub index: u8,
    pub kind: u8,
    pub hermite: bool,
    pub keys: Vec<Key>,
}

impl Track {
    pub fn at(&self, frame: f32) -> f32 {
        let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) else { return 0.0 };
        if frame <= first.frame {
            return first.value;
        }
        if frame >= last.frame {
            return last.value;
        }
        // `frame` is below the last key's frame, so the last key is never the one found: `i + 1` exists.
        let Some(i) = self.keys.iter().rposition(|k| k.frame <= frame) else { return first.value };
        let (a, b) = (self.keys[i], self.keys[(i + 1).min(self.keys.len() - 1)]);
        if !self.hermite {
            return a.value;
        }
        let d = b.frame - a.frame;
        let t = (frame - a.frame) / d;
        let t2 = t * t;
        let t3 = t2 * t;
        (2.0 * t3 - 3.0 * t2 + 1.0) * a.value
            + (t3 - 2.0 * t2 + t) * a.slope * d
            + (-2.0 * t3 + 3.0 * t2) * b.value
            + (t3 - t2) * b.slope * d
    }
}

/// A pane's or (with `material`) a material's tracks, by name.
#[derive(Debug)]
pub(super) struct Target {
    pub name: String,
    pub material: bool,
    pub tracks: Vec<Track>,
}

#[derive(Debug)]
pub(super) struct Animation {
    pub frames: u32,
    pub looping: bool,
    pub textures: Vec<String>,
    pub targets: Vec<Target>,
}

impl Animation {
    /// The animation frame for time `frame` (0-based): wrapped when it loops, held at the end otherwise.
    pub fn clamp(&self, frame: f32) -> f32 {
        let frames = self.frames as f32;
        if self.frames == 0 {
            0.0
        } else if self.looping {
            ((frame % frames) + frames) % frames
        } else if frame < 0.0 {
            0.0
        } else if frame > frames {
            frames
        } else {
            frame
        }
    }

    pub fn parse(bytes: &[u8]) -> Option<Animation> {
        let be = Be(bytes);
        let pai = be.sections(b"RLAN")?.into_iter().find(|s| &s.magic == b"pai1")?;
        let at = pai.start;
        let frames = u32::from(be.u16(at + 8)?);
        let looping = be.u8(at + 10)? & 1 != 0;
        let texture_count = usize::from(be.u16(at + 12)?);
        let entry_count = usize::from(be.u16(at + 14)?);
        // GUESS: the entry table's offset is from the section start, the texture and entry offsets from the table.
        let entries_at = be.offset(at, at + 16)?;
        let mut textures = Vec::new();
        if texture_count != 0 {
            let table = at + 20;
            for i in 0..texture_count {
                textures.push(be.name(be.offset(table, table + i * 4)?, 64)?);
            }
        }
        let (mut keys_total, mut tracks_total) = (0usize, 0usize);
        let mut targets = Vec::new();
        for e in 0..entry_count {
            let entry = be.offset(at, entries_at.checked_add(e * 4)?)?;
            let mut tracks = Vec::new();
            for t in 0..usize::from(be.u8(entry + 20)?) {
                let tag = be.offset(entry, entry + 24 + t * 4)?;
                let kind = be.magic(tag)?;
                for k in 0..usize::from(be.u8(tag + 4)?) {
                    let track = be.offset(tag, tag + 8 + k * 4)?;
                    let hermite = be.u8(track + 2)? == 2;
                    let key_count = usize::from(be.u16(track + 4)?);
                    let keys_at = be.offset(track, track + 8)?;
                    tracks_total += 1;
                    keys_total += key_count;
                    if tracks_total > MAX_TRACKS || keys_total > MAX_KEYS {
                        return None;
                    }
                    let mut keys = Vec::new();
                    for i in 0..key_count {
                        keys.push(if hermite {
                            let o = keys_at + i * 12;
                            Key { frame: be.f32(o)?, value: be.f32(o + 4)?, slope: be.f32(o + 8)? }
                        } else {
                            let o = keys_at + i * 8;
                            Key { frame: be.f32(o)?, value: f32::from(be.u16(o + 4)?), slope: 0.0 }
                        });
                    }
                    tracks.push(Track { tag: kind, index: be.u8(track)?, kind: be.u8(track + 1)?, hermite, keys });
                }
            }
            targets.push(Target { name: be.name(entry, 20)?, material: be.u8(entry + 21)? != 0, tracks });
        }
        Some(Animation { frames, looping, textures, targets })
    }
}
