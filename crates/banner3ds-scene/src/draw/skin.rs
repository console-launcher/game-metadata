// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Skinning on the CPU, shared by both renderers (banner meshes are small). A sub-mesh with no per-vertex bone indices and a
//! one-bone palette hangs off that bone; smooth-skinned sub-meshes take the pose's skinning matrices, rigid ones the bones' worlds.

use alloc::vec::Vec;

use super::math::{self, Mat34};
use crate::{BannerMesh, Skinning, VertexData};

/// The bone transform `mesh` hangs off, or `None` when it is skinned per vertex or names no bone.
pub fn attachment<'a>(mesh: &BannerMesh, bones: Option<&'a [Mat34]>) -> Option<&'a Mat34> {
    if mesh.vertices.bone_indices.is_some() || mesh.bone_palette.is_empty() {
        return None;
    }
    let b = usize::try_from(mesh.bone_palette[0]).ok()?;
    bones?.get(b)
}

/// The skinning matrices for a model: `pose_skin` when no billboard moved a bone (`billboarded` is `None`), else each
/// billboarded world times its inverse rest.
pub fn skin(rest: &[Mat34], billboarded: &[Mat34]) -> Vec<Mat34> {
    billboarded
        .iter()
        .enumerate()
        .map(|(i, b)| match rest.get(i) {
            Some(r) => math::multiply34(b, &math::invert34(r)),
            None => *b,
        })
        .collect()
}

fn matrices<'a>(mesh: &BannerMesh, skin: Option<&'a [Mat34]>, bones: Option<&'a [Mat34]>) -> Option<&'a [Mat34]> {
    if mesh.skinning == Skinning::Rigid && bones.is_some() { bones } else { skin }
}

/// The vertex position in model space: through its bone ([`attachment`]), the pose's skinning matrices, or as stored.
pub fn position(
    v: &VertexData,
    mesh: &BannerMesh,
    skin: Option<&[Mat34]>,
    bones: Option<&[Mat34]>,
    i: usize,
) -> [f32; 3] {
    let (x, y, z) = (v.positions[i * 3], v.positions[i * 3 + 1], v.positions[i * 3 + 2]);
    if let Some(m) = attachment(mesh, bones) {
        return math::transform_point(m, x, y, z);
    }
    let palette = matrices(mesh, skin, bones);
    let (Some(palette), Some(indices)) = (palette, &v.bone_indices) else { return [x, y, z] };
    if mesh.skinning == Skinning::None {
        return [x, y, z];
    }
    let (mut ox, mut oy, mut oz, mut total) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for k in 0..4 {
        let w = weight(v, i, k);
        if w <= 0.0 {
            continue;
        }
        let Some(m) = bone_matrix(mesh, palette, indices, i, k) else { continue };
        ox += w * (m[0] * x + m[1] * y + m[2] * z + m[3]);
        oy += w * (m[4] * x + m[5] * y + m[6] * z + m[7]);
        oz += w * (m[8] * x + m[9] * y + m[10] * z + m[11]);
        total += w;
    }
    if total <= 0.0 { [x, y, z] } else { [ox, oy, oz] }
}

fn weight(v: &VertexData, i: usize, k: usize) -> f32 {
    match &v.bone_weights {
        None => {
            if k == 0 {
                1.0
            } else {
                0.0
            }
        }
        Some(w) => w.get(i * 4 + k).copied().unwrap_or(0.0),
    }
}

fn bone_matrix<'a>(mesh: &BannerMesh, palette: &'a [Mat34], indices: &[i32], i: usize, k: usize) -> Option<&'a Mat34> {
    let slot = usize::try_from(*indices.get(i * 4 + k)?).ok()?;
    let bone = usize::try_from(*mesh.bone_palette.get(slot)?).ok()?;
    palette.get(bone)
}

/// The vertex normal after the pose's skinning (the rotation part of each bone matrix), not normalised; +Z where the shape
/// has none.
pub fn normal(
    v: &VertexData,
    mesh: &BannerMesh,
    skin: Option<&[Mat34]>,
    bones: Option<&[Mat34]>,
    i: usize,
) -> [f32; 3] {
    let Some(n) = &v.normals else { return [0.0, 0.0, 1.0] };
    let (x, y, z) = (n[i * 3], n[i * 3 + 1], n[i * 3 + 2]);
    if let Some(m) = attachment(mesh, bones) {
        return [m[0] * x + m[1] * y + m[2] * z, m[4] * x + m[5] * y + m[6] * z, m[8] * x + m[9] * y + m[10] * z];
    }
    let palette = matrices(mesh, skin, bones);
    let (Some(palette), Some(indices)) = (palette, &v.bone_indices) else { return [x, y, z] };
    if mesh.skinning == Skinning::None {
        return [x, y, z];
    }
    let (mut ox, mut oy, mut oz, mut total) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for k in 0..4 {
        let w = weight(v, i, k);
        if w <= 0.0 {
            continue;
        }
        let Some(m) = bone_matrix(mesh, palette, indices, i, k) else { continue };
        ox += w * (m[0] * x + m[1] * y + m[2] * z);
        oy += w * (m[4] * x + m[5] * y + m[6] * z);
        oz += w * (m[8] * x + m[9] * y + m[10] * z);
        total += w;
    }
    if total <= 0.0 { [x, y, z] } else { [ox, oy, oz] }
}

/// `n` rotated by the 3x4 `world`'s linear part and normalised (+Z for a zero result).
pub fn to_world_normal(world: &Mat34, n: &[f32; 3]) -> [f32; 3] {
    let x = world[0] * n[0] + world[1] * n[1] + world[2] * n[2];
    let y = world[4] * n[0] + world[5] * n[1] + world[6] * n[2];
    let z = world[8] * n[0] + world[9] * n[1] + world[10] * n[2];
    let l = math::sqrt(x * x + y * y + z * z);
    if l > 0.0 { [x / l, y / l, z / l] } else { [0.0, 0.0, 1.0] }
}

/// Whether `mesh`'s arrays agree with its vertex count and its indices stay in range (true for every parsed scene; a scene a
/// caller builds by hand may not, and the renderers skip such a mesh instead of reading past an array).
pub fn consistent(mesh: &BannerMesh) -> bool {
    let v = &*mesh.vertices;
    let n = v.count;
    let fits = |a: &Option<Vec<f32>>, w: usize| a.as_ref().is_none_or(|a| a.len() >= n * w);
    v.positions.len() >= n * 3
        && fits(&v.normals, 3)
        && fits(&v.uv0, 2)
        && fits(&v.uv1, 2)
        && fits(&v.uv2, 2)
        && fits(&v.colors, 4)
        && fits(&v.bone_weights, 4)
        && v.bone_indices.as_ref().is_none_or(|a| a.len() >= n * 4)
        && mesh.indices.iter().all(|&i| (i as usize) < n)
}
