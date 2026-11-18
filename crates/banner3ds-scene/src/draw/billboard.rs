// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bone billboards: a billboard bone's world rotation is replaced by one that faces the camera, its place and scale kept.
//! World and screen modes face along the view direction, the viewpoint modes face the eye from the bone, the Y-axial modes
//! turn about the bone's own Y only. A billboard root carries its turn to the camera to the plain bones below it.

use alloc::vec::Vec;

use super::camera::Camera;
use super::math::{self, Mat34};
use crate::BannerModel;

/// `bones` (model-space worlds) with every billboard bone of `model` turned to `camera`; `None` when the model has none.
pub fn apply(model: &BannerModel, bones: &[Mat34], camera: &Camera) -> Option<Vec<Mat34>> {
    if !model.bones.iter().any(|b| (1..=6).contains(&b.billboard)) {
        return None;
    }
    let inv = math::invert34(&model.world);
    let eye = point(&inv, &camera.eye);
    let view = math::normalized(&direction(
        &inv,
        &[camera.target[0] - camera.eye[0], camera.target[1] - camera.eye[1], camera.target[2] - camera.eye[2]],
    ));
    let up = math::normalized(&direction(&inv, &camera.up));
    let l = math::length(model.world[0], model.world[4], model.world[8]);
    let unscale = 1.0 / if l > 0.0 { l } else { 1.0 };
    let n = bones.len();
    let mut out = bones.to_vec();
    let mut moved: Vec<Option<Mat34>> = alloc::vec![None; n];
    let mut done = alloc::vec![false; n];
    struct Ctx<'c> {
        model: &'c BannerModel,
        bones: &'c [Mat34],
        eye: [f32; 3],
        view: [f32; 3],
        up: [f32; 3],
        unscale: f32,
    }
    fn correction(
        i: usize,
        cx: &Ctx,
        out: &mut [Mat34],
        moved: &mut [Option<Mat34>],
        done: &mut [bool],
    ) -> Option<Mat34> {
        if done[i] {
            return moved[i];
        }
        done[i] = true;
        let bone = cx.model.bones.get(i)?;
        let n = cx.bones.len();
        let parent_index = usize::try_from(bone.parent).ok().filter(|&p| p < n && p != i);
        let parent = parent_index.and_then(|p| correction(p, cx, out, moved, done));
        let m = match &parent {
            Some(p) => math::multiply34(p, &cx.bones[i]),
            None => cx.bones[i],
        };
        let faced = if (1..=6).contains(&bone.billboard) {
            face(bone.billboard, &cx.bones[i], &cx.bones[i], &cx.eye, &cx.view, &cx.up, cx.unscale)
        } else {
            None
        };
        out[i] = faced.unwrap_or(m);
        let root = usize::try_from(bone.parent).ok().is_none_or(|p| p >= n || p == i);
        moved[i] = match faced {
            Some(f) if root => Some(math::multiply34(&f, &math::invert34(&cx.bones[i]))),
            _ => parent,
        };
        moved[i]
    }
    let cx = Ctx { model, bones, eye, view, up, unscale };
    for i in 0..n {
        correction(i, &cx, &mut out, &mut moved, &mut done);
    }
    Some(out)
}

fn is_zero(v: &[f32; 3]) -> bool {
    v.iter().all(|&x| x == 0.0)
}

/// `m` with its rotation replaced by one facing the camera in billboard `mode`; `None` when undefined.
fn face(
    mode: i32,
    m: &Mat34,
    own: &Mat34,
    eye: &[f32; 3],
    view: &[f32; 3],
    up: &[f32; 3],
    unscale: f32,
) -> Option<Mat34> {
    let pos = [m[3], m[7], m[11]];
    let mut z = match mode {
        2 | 4 | 6 => math::normalized(&[eye[0] - pos[0], eye[1] - pos[1], eye[2] - pos[2]]),
        _ => [-view[0], -view[1], -view[2]],
    };
    let (x, y);
    if mode >= 5 {
        y = math::normalized(&[own[1], own[5], own[9]]);
        if is_zero(&y) {
            return None;
        }
        let d = z[0] * y[0] + z[1] * y[1] + z[2] * y[2];
        z = math::normalized(&[z[0] - d * y[0], z[1] - d * y[1], z[2] - d * y[2]]);
        if is_zero(&z) {
            return None;
        }
        x = math::cross(&y, &z);
    } else if mode <= 2 {
        let oy = [own[1], own[5], own[9]];
        let d = z[0] * oy[0] + z[1] * oy[1] + z[2] * oy[2];
        let p = math::normalized(&[oy[0] - d * z[0], oy[1] - d * z[1], oy[2] - d * z[2]]);
        y = if is_zero(&p) { math::normalized(&math::cross(&z, &math::normalized(&math::cross(up, &z)))) } else { p };
        if is_zero(&y) {
            return None;
        }
        x = math::cross(&y, &z);
    } else {
        x = math::normalized(&math::cross(up, &z));
        if is_zero(&x) {
            return None;
        }
        y = math::cross(&z, &x);
    }
    let sx = math::length(m[0], m[4], m[8]) * unscale;
    let sy = math::length(m[1], m[5], m[9]) * unscale;
    let sz = math::length(m[2], m[6], m[10]) * unscale;
    Some([
        x[0] * sx,
        y[0] * sy,
        z[0] * sz,
        m[3],
        x[1] * sx,
        y[1] * sy,
        z[1] * sz,
        m[7],
        x[2] * sx,
        y[2] * sy,
        z[2] * sz,
        m[11],
    ])
}

fn point(m: &Mat34, p: &[f32; 3]) -> [f32; 3] {
    core::array::from_fn(|r| m[r * 4] * p[0] + m[r * 4 + 1] * p[1] + m[r * 4 + 2] * p[2] + m[r * 4 + 3])
}

fn direction(m: &Mat34, d: &[f32; 3]) -> [f32; 3] {
    core::array::from_fn(|r| m[r * 4] * d[0] + m[r * 4 + 1] * d[1] + m[r * 4 + 2] * d[2])
}
