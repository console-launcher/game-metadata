// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The order both renderers draw a scene's visible meshes in, as NW4C queues them: by the material's translucency layer
//! (0 opaque, 1 translucent, 2 subtractive, 3 additive), then the mesh's render priority, and within a non-opaque layer from
//! the farthest mesh to the nearest (depth along the line of sight, measured where the mesh is drawn: billboards turned).

use alloc::vec::Vec;

use super::camera::Camera;
use super::pose::{Player, Pose};
use super::{billboard, math, skin};
use crate::{BannerMesh, BannerScene};

/// One draw: the model and mesh indices into the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draw {
    /// Index into [`BannerScene::models`].
    pub model: usize,
    /// Index into that model's meshes.
    pub mesh: usize,
}

fn layer(mesh: &BannerMesh) -> i32 {
    mesh.material.translucency.clamp(0, 3)
}

/// The visible meshes of `scene` at `pose`, in draw order.
pub fn of(scene: &BannerScene, player: &Player, camera: &Camera, pose: &Pose) -> Vec<Draw> {
    let eye = camera.eye;
    let mut forward: [f32; 3] = core::array::from_fn(|i| camera.target[i] - eye[i]);
    let l0 = math::length(forward[0], forward[1], forward[2]);
    let l = if l0 > 1e-6 { l0 } else { 1.0 };
    for f in &mut forward {
        *f /= l;
    }
    let mut draws: Vec<(Draw, i32, i32, f32)> = Vec::new();
    for (mi, m) in scene.models.iter().enumerate() {
        if !m.visible {
            continue;
        }
        let mut placed: Option<(Vec<math::Mat34>, Option<Vec<math::Mat34>>)> = None;
        for (ni, mesh) in m.meshes.iter().enumerate() {
            if !pose.visibility.get(&mesh.node_name).copied().unwrap_or(mesh.visible) {
                continue;
            }
            let mut distance = 0.0;
            if layer(mesh) > 0 && skin::consistent(mesh) {
                let (bones, skinning) = placed.get_or_insert_with(|| {
                    let posed = pose.bones_of(m, player.rest(mi));
                    match billboard::apply(m, posed, camera) {
                        Some(b) => {
                            let s = skin::skin(player.rest(mi), &b);
                            (b, Some(s))
                        }
                        None => (posed.to_vec(), pose.skinning.get(&m.name).cloned()),
                    }
                });
                let p = centre_of(m, mesh, bones, skinning.as_deref());
                distance = (p[0] - eye[0]) * forward[0] + (p[1] - eye[1]) * forward[1] + (p[2] - eye[2]) * forward[2];
            }
            draws.push((Draw { model: mi, mesh: ni }, layer(mesh), mesh.render_priority, distance));
        }
    }
    // Stable: ties keep file order.
    draws.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)).then(b.3.total_cmp(&a.3)));
    draws.into_iter().map(|d| d.0).collect()
}

/// The mean of the mesh's used vertices, placed by `bones` and `skin`, in world space.
fn centre_of(
    model: &crate::BannerModel,
    mesh: &BannerMesh,
    bones: &[math::Mat34],
    skinning: Option<&[math::Mat34]>,
) -> [f32; 3] {
    let v = &*mesh.vertices;
    let mut seen = alloc::vec![false; v.count];
    let (mut x, mut y, mut z, mut n) = (0.0f32, 0.0f32, 0.0f32, 0);
    for &i in &mesh.indices {
        let i = i as usize;
        if i < v.count && !seen[i] {
            seen[i] = true;
            let q = skin::position(v, mesh, skinning, Some(bones), i);
            x += q[0];
            y += q[1];
            z += q[2];
            n += 1;
        }
    }
    if n > 0 {
        x /= n as f32;
        y /= n as f32;
        z /= n as f32;
    }
    math::transform_point(&model.world, x, y, z)
}
