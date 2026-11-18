// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A banner at one instant: the clips sampled ([`Player::pose`]), the turntable HOME gives every banner model ([`spin`]), and the
//! bone math under both.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::math::{self, Mat34};
use crate::{
    BannerAnimation, BannerBone, BannerLight, BannerMaterial, BannerModel, BannerScene, TexEnvStage, TextureSlot,
    TrackKind, VertexLight,
};

/// One texture slot's animated coordinate transform.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotUv {
    /// Scale.
    pub scale: [f32; 2],
    /// Rotation, radians.
    pub rotation: f32,
    /// Translation.
    pub translation: [f32; 2],
}

/// A material's lighting colours (RGBA 0..1); a material animation can change the diffuse per frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialColors {
    /// Emission.
    pub emission: [f32; 4],
    /// Ambient.
    pub ambient: [f32; 4],
    /// Diffuse.
    pub diffuse: [f32; 4],
    /// Specular 0.
    pub specular0: [f32; 4],
    /// Specular 1.
    pub specular1: [f32; 4],
}

/// A banner at one instant. Only what the clips animate is here; anything absent keeps the scene's own value. Skinning
/// matrices are model space, already multiplied by the bone's inverse rest transform; `bones` are the posed bone world
/// transforms themselves.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pose {
    /// Per model name: each bone's posed world times its inverse rest world.
    pub skinning: BTreeMap<String, Vec<Mat34>>,
    /// Per material name: the three slots' animated transforms.
    pub slot_uv: BTreeMap<String, [Option<SlotUv>; 3]>,
    /// Per material name: the animated diffuse.
    pub diffuse: BTreeMap<String, [f32; 4]>,
    /// Per material name: the six animated constant colours.
    pub constants: BTreeMap<String, [Option<[f32; 4]>; 6]>,
    /// Mesh node name to visible; absent means the mesh's own flag.
    pub visibility: BTreeMap<String, bool>,
    /// Per model name: each posed bone's world transform (model space).
    pub bones: BTreeMap<String, Vec<Mat34>>,
    /// The turntable's yaw in radians at this instant, 0 for a banner that does not turn.
    pub turn: f32,
}

impl Pose {
    /// The bone transforms `model`'s attached meshes take: the posed ones, else `rest` (the model's rest worlds).
    pub fn bones_of<'a>(&'a self, model: &BannerModel, rest: &'a [Mat34]) -> &'a [Mat34] {
        self.bones.get(&model.name).map(|v| v.as_slice()).unwrap_or(rest)
    }

    /// `material`'s combiner stages with this pose's animated constant colours applied.
    pub fn stages_of(&self, material: &BannerMaterial) -> Vec<TexEnvStage> {
        let Some(constants) = self.constants.get(&material.name) else { return material.stages.clone() };
        material
            .stages
            .iter()
            .map(|st| {
                let c = usize::try_from(st.constant_index).ok().and_then(|i| constants.get(i).copied().flatten());
                match c {
                    Some(c) => TexEnvStage { constant: c, ..st.clone() },
                    None => st.clone(),
                }
            })
            .collect()
    }

    /// `material`'s lighting colours with this pose's animated diffuse applied.
    pub fn colors_of(&self, material: &BannerMaterial) -> MaterialColors {
        MaterialColors {
            emission: material.emission,
            ambient: material.ambient,
            diffuse: self.diffuse.get(&material.name).copied().unwrap_or(material.diffuse),
            specular0: material.specular0,
            specular1: material.specular1,
        }
    }

    /// Texture slot `index` of `material` with this pose's animated transform applied.
    pub fn slot_of(&self, material: &BannerMaterial, index: usize) -> Option<TextureSlot> {
        let base = material.slots.get(index)?.as_ref()?;
        match self.slot_uv.get(&material.name).and_then(|s| s.get(index)).and_then(|s| s.as_ref()) {
            Some(uv) => Some(TextureSlot {
                scale: uv.scale,
                rotation: uv.rotation,
                translation: uv.translation,
                ..base.clone()
            }),
            None => Some(base.clone()),
        }
    }

    fn turned(&self, v: &[f32; 3]) -> [f32; 3] {
        let (c, s) = (math::cos(self.turn), math::sin(self.turn));
        [c * v[0] + s * v[2], v[1], -s * v[0] + c * v[2]]
    }

    /// `lights` carried round by the turntable with the model: point and spot lights turn by [`turn`](Self::turn) about Y,
    /// directional lights keep their direction to the view.
    pub fn lights_of(&self, lights: Vec<&BannerLight>) -> Vec<BannerLight> {
        if self.turn == 0.0 || lights.is_empty() {
            return lights.into_iter().cloned().collect();
        }
        lights
            .into_iter()
            .map(|l| BannerLight {
                position: self.turned(&l.position),
                direction: if l.kind == 0 { l.direction } else { self.turned(&l.direction) },
                ..l.clone()
            })
            .collect()
    }

    /// The vertex lights carried round by the turntable like [`lights_of`](Self::lights_of)'s.
    pub fn vertex_lights_of(&self, lights: Vec<&VertexLight>) -> Vec<VertexLight> {
        if self.turn == 0.0 || lights.is_empty() {
            return lights.into_iter().cloned().collect();
        }
        lights
            .into_iter()
            .map(|l| {
                if l.kind == 0 {
                    l.clone()
                } else {
                    VertexLight {
                        position: self.turned(&l.position),
                        direction: self.turned(&l.direction),
                        ..l.clone()
                    }
                }
            })
            .collect()
    }

    /// The turn as a column-major 3x3 (the GL shader's `uTurn`, which turns the light geometry baked into it).
    pub fn turn_matrix(&self) -> [f32; 9] {
        let (c, s) = (math::cos(self.turn), math::sin(self.turn));
        [c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c]
    }
}

/// The turntable HOME gives every banner model: a full turn about Y per 600 clip frames, the right side toward the viewer.
pub mod spin {
    use super::*;

    /// Degrees per clip frame.
    pub const DEGREES_PER_FRAME: f32 = 0.6;
    /// Frames per turn.
    pub const LOOP_FRAMES: f32 = 600.0;

    /// The model's yaw in radians at `frame`.
    pub fn yaw(frame: f32) -> f32 {
        let f = frame % LOOP_FRAMES;
        -(math::to_radians((DEGREES_PER_FRAME * if f < 0.0 { f + LOOP_FRAMES } else { f }) as f64) as f32)
    }

    /// Whether turning shows anything: a bone with a mesh that is not a billboard, a billboard off the Y axis, or a clip that
    /// moves a bone off it.
    pub fn shows(scene: &BannerScene, rest: &[Vec<Mat34>]) -> bool {
        let moves = scene.animations.skeletal.iter().any(|clip| {
            clip.tracks.iter().any(|t| {
                t.kind == TrackKind::BoneTranslation && t.component != 1 && t.keys.values.iter().any(|v| v.abs() > 0.01)
            })
        });
        moves
            || scene.models.iter().enumerate().any(|(mi, model)| {
                !model.bones.is_empty()
                    && model.meshes.iter().any(|mesh| {
                        mesh.bone_palette.iter().any(|&b| {
                            let Some(bone) = usize::try_from(b).ok().and_then(|b| model.bones.get(b)) else {
                                return false;
                            };
                            let billboard = (1..=6).contains(&bone.billboard);
                            let world = rest.get(mi).and_then(|r| r.get(b as usize));
                            !billboard || world.is_some_and(|w| w[3].abs() + w[11].abs() > 0.01)
                        })
                    })
            })
    }

    /// `local` turned by the model's yaw at `frame` (the turn on the outside).
    pub fn turned(local: &Mat34, frame: f32) -> Mat34 {
        let a = yaw(frame);
        let (c, s) = (math::cos(a), math::sin(a));
        math::multiply34(&[c, 0.0, s, 0.0, 0.0, 1.0, 0.0, 0.0, -s, 0.0, c, 0.0], local)
    }
}

/// Each bone's world transform from the bones' own rest SRT down the parent chain.
pub fn rest_worlds(bones: &[BannerBone]) -> Vec<Mat34> {
    let n = bones.len();
    let mut out: Vec<Option<Mat34>> = alloc::vec![None; n];
    fn of(i: usize, depth: usize, bones: &[BannerBone], out: &mut [Option<Mat34>]) -> Mat34 {
        if let Some(m) = out[i] {
            return m;
        }
        let b = &bones[i];
        let local = math::local(&b.scale, &b.rotation, &b.translation);
        let n = bones.len();
        let result = match parent_of(b.parent, i, n) {
            Some(p) if depth < n => child(&of(p, depth + 1, bones, out), &bones[p].scale, &local, b.flags),
            _ => local,
        };
        out[i] = Some(result);
        result
    }
    (0..n).map(|i| of(i, 0, bones, &mut out)).collect()
}

/// A bone's parent index when it has one in range (not itself).
fn parent_of(parent: i32, i: usize, n: usize) -> Option<usize> {
    usize::try_from(parent).ok().filter(|&p| p < n && p != i)
}

/// A child bone's world transform from its parent's and its own local one; with segment scale compensation the parent's own
/// scale carries the child's offset but not its rotation and scale.
pub fn child(parent_world: &Mat34, parent_scale: &[f32; 3], local: &Mat34, child_flags: i32) -> Mat34 {
    if child_flags & BannerBone::SEGMENT_SCALE_COMPENSATE == 0 {
        return math::multiply34(parent_world, local);
    }
    let mut l = *local;
    for r in 0..3 {
        let s = parent_scale[r];
        if s != 0.0 {
            for c in 0..3 {
                l[r * 4 + c] /= s;
            }
        }
    }
    math::multiply34(parent_world, &l)
}

/// Evaluates a scene's animations at a frame: every clip plays together, looped over its own length.
#[derive(Debug, Clone)]
pub struct Player<'a> {
    scene: &'a BannerScene,
    rest: Vec<Vec<Mat34>>,
    /// Whether the banner's turntable shows.
    pub spins: bool,
    /// The loop length in clip frames (60 to the second); 0 when nothing animates.
    pub frames: f32,
    /// Whether anything moves.
    pub animated: bool,
}

const MAX_LOOP: i64 = 3600;

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

impl<'a> Player<'a> {
    /// A player for `scene`.
    pub fn new(scene: &'a BannerScene) -> Player<'a> {
        let rest: Vec<Vec<Mat34>> = scene.models.iter().map(|m| rest_worlds(&m.bones)).collect();
        let spins = spin::shows(scene, &rest);
        let clips =
            || scene.animations.skeletal.iter().chain(&scene.animations.material).chain(&scene.animations.visibility);
        let longest = clips().map(|c| c.frames).reduce(f32::max).unwrap_or(0.0);
        let frames = if !spins {
            longest
        } else {
            let mut lcm = spin::LOOP_FRAMES as i64;
            for n in clips().map(|c| round_long(c.frames)).filter(|&n| n > 0) {
                lcm = lcm / gcd(lcm, n) * n;
                if lcm > MAX_LOOP * 1_000_000 {
                    break;
                }
            }
            if lcm <= MAX_LOOP { lcm as f32 } else { longest.max(spin::LOOP_FRAMES) }
        };
        let animated = (spins || clips().any(|c| !c.tracks.is_empty())) && frames > 0.0;
        Player { scene, rest, spins, frames, animated }
    }

    /// The rest worlds of model `index` (empty past the end).
    pub fn rest(&self, index: usize) -> &[Mat34] {
        self.rest.get(index).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// The pose at `frame` on the turntable, the clips at `clip_frame` (the same instant on HOME unless a caller lines a
    /// capture up whose clip runs ahead of its turntable).
    pub fn pose(&self, frame: f32, clip_frame: f32) -> Pose {
        if !self.animated {
            return Pose::default();
        }
        let scene = self.scene;
        let mut pose = Pose::default();
        let bone_tracks: Vec<(&crate::AnimTrack, &BannerAnimation)> =
            scene.animations.skeletal.iter().flat_map(|clip| clip.tracks.iter().map(move |t| (t, clip))).collect();
        if !bone_tracks.is_empty() || self.spins {
            for (mi, model) in scene.models.iter().enumerate() {
                if model.bones.is_empty() {
                    continue;
                }
                let world = self.skeleton_pose(model, &bone_tracks, frame, clip_frame);
                let rest = &self.rest[mi];
                let skin = world.iter().zip(rest).map(|(w, r)| math::multiply34(w, &math::invert34(r))).collect();
                pose.skinning.insert(model.name.clone(), skin);
                pose.bones.insert(model.name.clone(), world);
            }
        }
        for clip in &scene.animations.material {
            let t = local_frame(clip, clip_frame);
            for track in &clip.tracks {
                let v = track.keys.sample(t);
                match track.kind {
                    TrackKind::MaterialTranslate | TrackKind::MaterialScale | TrackKind::MaterialRotate => {
                        let material = self.material_named(&track.target);
                        let slots = pose.slot_uv.entry(track.target.clone()).or_insert([None, None, None]);
                        let Some(slot) = usize::try_from(track.slot).ok().filter(|&s| s < 3) else { continue };
                        let base = material.and_then(|m| m.slots.get(slot)).and_then(|s| s.as_ref());
                        let cur = slots[slot].get_or_insert_with(|| SlotUv {
                            scale: base.map(|b| b.scale).unwrap_or([1.0, 1.0]),
                            rotation: base.map(|b| b.rotation).unwrap_or(0.0),
                            translation: base.map(|b| b.translation).unwrap_or([0.0, 0.0]),
                        });
                        let c = usize::try_from(track.component).ok().filter(|&c| c < 2);
                        match (track.kind, c) {
                            (TrackKind::MaterialScale, Some(c)) => cur.scale[c] = v,
                            (TrackKind::MaterialTranslate, Some(c)) => cur.translation[c] = v,
                            (TrackKind::MaterialRotate, _) => cur.rotation = v,
                            _ => {}
                        }
                    }
                    TrackKind::MaterialDiffuse => {
                        if let Some(c) = usize::try_from(track.component).ok().filter(|&c| c < 4) {
                            let base = self.material_named(&track.target).map(|m| m.diffuse).unwrap_or([1.0; 4]);
                            pose.diffuse.entry(track.target.clone()).or_insert(base)[c] = v;
                        }
                    }
                    TrackKind::MaterialConstant => {
                        let slot = usize::try_from(track.slot).ok().filter(|&s| s < 6);
                        let c = usize::try_from(track.component).ok().filter(|&c| c < 4);
                        if let (Some(slot), Some(c)) = (slot, c) {
                            let start = self
                                .material_named(&track.target)
                                .and_then(|m| m.stages.iter().find(|st| st.constant_index == track.slot))
                                .map(|st| st.constant)
                                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                            let all = pose.constants.entry(track.target.clone()).or_insert([None; 6]);
                            all[slot].get_or_insert(start)[c] = v;
                        }
                    }
                    _ => {}
                }
            }
        }
        for clip in &scene.animations.visibility {
            let t = local_frame(clip, clip_frame);
            for track in clip.tracks.iter().filter(|t| t.kind == TrackKind::MeshVisibility) {
                pose.visibility.insert(track.target.clone(), track.keys.sample(t) >= 0.5);
            }
        }
        pose.turn = if self.spins { spin::yaw(frame) } else { 0.0 };
        pose
    }

    /// The material a material track names: the first mesh's with that name.
    fn material_named(&self, name: &str) -> Option<&'a BannerMaterial> {
        self.scene.models.iter().find_map(|m| m.meshes.iter().find(|x| x.material.name == name).map(|x| &*x.material))
    }

    fn skeleton_pose(
        &self,
        model: &BannerModel,
        tracks: &[(&crate::AnimTrack, &BannerAnimation)],
        frame: f32,
        clip_frame: f32,
    ) -> Vec<Mat34> {
        let n = model.bones.len();
        let mut scale: Vec<[f32; 3]> = model.bones.iter().map(|b| b.scale).collect();
        let mut rotation: Vec<[f32; 3]> = model.bones.iter().map(|b| b.rotation).collect();
        let mut translation: Vec<[f32; 3]> = model.bones.iter().map(|b| b.translation).collect();
        let mut quat: Vec<Option<[f32; 4]>> = alloc::vec![None; n];
        // Kotlin's associate: the last bone of a name wins.
        let mut by_name: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, b) in model.bones.iter().enumerate() {
            by_name.insert(&b.name, i);
        }
        for (track, clip) in tracks {
            let Some(&bone) = by_name.get(track.target.as_str()) else { continue };
            let v = track.keys.sample(local_frame(clip, clip_frame));
            let c = track.component;
            let c3 = usize::try_from(c).ok().filter(|&c| c < 3);
            match track.kind {
                TrackKind::BoneScale => {
                    if let Some(c) = c3 {
                        scale[bone][c] = v
                    }
                }
                TrackKind::BoneRotation => {
                    if let Some(c) = c3 {
                        rotation[bone][c] = v
                    }
                }
                TrackKind::BoneTranslation => {
                    if let Some(c) = c3 {
                        translation[bone][c] = v
                    }
                }
                TrackKind::BoneRotationQuat => {
                    if let Some(c) = usize::try_from(c).ok().filter(|&c| c < 4) {
                        quat[bone].get_or_insert([0.0, 0.0, 0.0, 1.0])[c] = v;
                    }
                }
                _ => {}
            }
        }
        let mut world: Vec<Option<Mat34>> = alloc::vec![None; n];
        struct Ctx<'c> {
            model: &'c BannerModel,
            scale: &'c [[f32; 3]],
            rotation: &'c [[f32; 3]],
            translation: &'c [[f32; 3]],
            quat: &'c [Option<[f32; 4]>],
            spins: bool,
            frame: f32,
        }
        fn world_of(i: usize, depth: usize, cx: &Ctx, world: &mut [Option<Mat34>]) -> Mat34 {
            if let Some(m) = world[i] {
                return m;
            }
            let n = cx.model.bones.len();
            let local = match cx.quat[i] {
                Some(q) => math::local_quat(&cx.scale[i], &q, &cx.translation[i]),
                None => math::local(&cx.scale[i], &cx.rotation[i], &cx.translation[i]),
            };
            // A parent cycle has no root: the original implementation recurses until its stack runs out, here the chain stops at n deep.
            let result = match parent_of(cx.model.bones[i].parent, i, n) {
                Some(p) if depth < n => {
                    child(&world_of(p, depth + 1, cx, world), &cx.scale[p], &local, cx.model.bones[i].flags)
                }
                Some(_) => local,
                None if cx.spins => spin::turned(&local, cx.frame),
                None => local,
            };
            world[i] = Some(result);
            result
        }
        let cx = Ctx {
            model,
            scale: &scale,
            rotation: &rotation,
            translation: &translation,
            quat: &quat,
            spins: self.spins,
            frame,
        };
        (0..n).map(|i| world_of(i, 0, &cx, &mut world)).collect()
    }
}

fn round_long(x: f32) -> i64 {
    math::floor(x + 0.5) as i64
}

/// A clip's own frame at `frame`: looped over its length, or held at its end.
pub fn local_frame(clip: &BannerAnimation, frame: f32) -> f32 {
    let len = clip.frames;
    if len <= 0.0 {
        return 0.0;
    }
    if !clip.looping {
        return frame.min(len);
    }
    let m = frame % len;
    if m < 0.0 { m + len } else { m }
}
