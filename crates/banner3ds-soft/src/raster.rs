// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The rasteriser: framing, draw order, per-vertex skinning and lighting, per-pixel texturing, lighting, combiners, alpha
//! test, depth and blend, then HOME's window. Same order of operations as the original CPU renderer, so pixels match it.

use alloc::vec;
use alloc::vec::Vec;

use banner3ds_scene::coords::{SPHERE, UV};
use banner3ds_scene::draw::math::{self, Mat34, clamp, floor};
use banner3ds_scene::draw::{Camera, Player, Pose, billboard, camera, light, order, skin};
use banner3ds_scene::{BannerMesh, BannerModel, BannerScene, TextureSlot};
use gamemeta_core::RgbaImage;

use crate::Options;
use crate::shading::{self, Blend};
use crate::texenv;

const BUMP_NORMAL_MAP: i32 = 1;
const BORDER: i32 = 1;

#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    scene: &BannerScene,
    player: &Player,
    pose: &Pose,
    camera: &Camera,
    width: i32,
    height: i32,
    options: &Options<'_>,
    out: &mut [u8],
) {
    let n = (width * height) as usize;
    let mut color = vec![0f32; n * 4];
    if let Some(backdrop) = options.backdrop {
        for (c, &b) in color.iter_mut().zip(backdrop) {
            *c = f32::from(b) / 255.0;
        }
    } else {
        let bg = options.background;
        let px = [
            ((bg >> 16) & 255) as f32 / 255.0,
            ((bg >> 8) & 255) as f32 / 255.0,
            (bg & 255) as f32 / 255.0,
            (bg >> 24) as f32 / 255.0,
        ];
        for c in color.chunks_mut(4) {
            c.copy_from_slice(&px);
        }
    }
    let mut depth = vec![f32::MAX; n];
    let port = camera::viewport(camera, width, height);
    let view_proj = math::multiply(&camera.projection(port[2] as f32 / port[3] as f32), &camera.view());
    let window = camera::home_window(camera, &port);
    let under = window.map(|_| color.clone());
    let target = Target { color: &mut color, depth: &mut depth, w: width, port };
    let mut target = target;
    for d in order::of(scene, player, camera, pose) {
        let model = &scene.models[d.model];
        let mesh = &model.meshes[d.mesh];
        if skin::consistent(mesh) {
            draw_mesh(scene, player.rest(d.model), model, mesh, &view_proj, camera, &mut target, pose);
        }
    }
    if let (Some(e), Some(under)) = (window, under) {
        for y in 0..height {
            for x in 0..width {
                let c = camera::window_coverage(&e, x as f32 + 0.5, y as f32 + 0.5);
                if c >= 1.0 {
                    continue;
                }
                let o = ((y * width + x) * 4) as usize;
                for k in 0..4 {
                    color[o + k] = under[o + k] + (color[o + k] - under[o + k]) * c;
                }
            }
        }
    }
    for (o, c) in out.iter_mut().zip(&color) {
        *o = (clamp(*c, 0.0, 1.0) * 255.0 + 0.5) as u8;
    }
}

struct Target<'t> {
    color: &'t mut [f32],
    depth: &'t mut [f32],
    w: i32,
    port: [i32; 4],
}

fn uv_set(v: &banner3ds_scene::VertexData, set: i32) -> Option<&Vec<f32>> {
    match set {
        1 => v.uv1.as_ref(),
        2 => v.uv2.as_ref(),
        _ => v.uv0.as_ref(),
    }
    .or(v.uv0.as_ref())
}

fn slot_uv(slot: &TextureSlot, uvs: &[f32], i: usize) -> [f32; 2] {
    shading::transform(uvs[i * 2], uvs[i * 2 + 1], &slot.scale, slot.rotation, &slot.translation, slot.matrix_mode)
}

#[allow(clippy::too_many_arguments)]
fn draw_mesh(
    scene: &BannerScene,
    rest: &[Mat34],
    model: &BannerModel,
    mesh: &BannerMesh,
    vp: &[f32; 16],
    camera: &Camera,
    t: &mut Target<'_>,
    pose: &Pose,
) {
    let v = &*mesh.vertices;
    let mat = &*mesh.material;
    let count = v.count;
    let eye = camera.eye;
    let posed = pose.bones_of(model, rest);
    let billboarded = billboard::apply(model, posed, camera);
    let bones: &[Mat34] = billboarded.as_deref().unwrap_or(posed);
    let skin_owned = billboarded.as_ref().map(|b| skin::skin(rest, b));
    let skinning: Option<&[Mat34]> = match &skin_owned {
        Some(s) => Some(s),
        None => pose.skinning.get(&model.name).map(|s| s.as_slice()),
    };
    let lit = mat.fragment_lighting;
    let slot_at = |i: usize| mat.slots.get(i).and_then(|s| s.as_ref());
    let sphere = v.normals.is_some() && mat.slots.iter().flatten().any(|s| s.mapping == SPHERE);
    let vertex_lit = mat.vertex_lighting || mat.hemisphere_lighting;
    let need_pos = lit || vertex_lit;
    let need_normal = lit || sphere || vertex_lit;
    let view_matrix = camera.view();
    let mut clip = vec![0f32; count * 4];
    let mut world_pos = if need_pos { vec![0f32; count * 3] } else { Vec::new() };
    let mut world_normal = if need_normal { vec![0f32; count * 3] } else { Vec::new() };
    let mut view_normal = if sphere { vec![0f32; count * 3] } else { Vec::new() };
    for i in 0..count {
        let p = skin::position(v, mesh, skinning, Some(bones), i);
        let p = math::transform_point(&model.world, p[0], p[1], p[2]);
        for r in 0..4 {
            clip[i * 4 + r] = vp[r] * p[0] + vp[4 + r] * p[1] + vp[8 + r] * p[2] + vp[12 + r];
        }
        if need_pos {
            world_pos[i * 3..i * 3 + 3].copy_from_slice(&p);
        }
        if need_normal {
            let nrm = skin::normal(v, mesh, skinning, Some(bones), i);
            let nrm = skin::to_world_normal(&model.world, &nrm);
            world_normal[i * 3..i * 3 + 3].copy_from_slice(&nrm);
            if sphere {
                view_normal[i * 3..i * 3 + 3].copy_from_slice(&shading::to_view(&view_matrix, &nrm));
            }
        }
    }
    // Colour source 0 per vertex: the vertex colour, lit by the vertex shader's lights where the material asks.
    let vertex_primary: Option<Vec<f32>> = vertex_lit.then(|| {
        let vertex_lights = pose.vertex_lights_of(scene.vertex_lights_for(mat));
        let hemisphere = scene.hemisphere_for(mat);
        let mut out = vec![0f32; count * 4];
        for i in 0..count {
            let n: [f32; 3] = world_normal[i * 3..i * 3 + 3].try_into().unwrap_or([0.0; 3]);
            let wp: [f32; 3] = world_pos[i * 3..i * 3 + 3].try_into().unwrap_or([0.0; 3]);
            let vc = v.colors.as_ref().map(|c| &c[i * 4..i * 4 + 4]);
            out[i * 4..i * 4 + 4].copy_from_slice(&light::vertex_primary(mat, &vertex_lights, hemisphere, &n, &wp, vc));
        }
        out
    });
    let lights = pose.lights_of(scene.lights_for(mat));
    let terms = lit.then(|| light::terms(&pose.colors_of(mat), &lights, &scene.scene_ambient));
    let slots: [Option<TextureSlot>; 3] = core::array::from_fn(|s| pose.slot_of(mat, s));
    let textures: [Option<&RgbaImage>; 3] =
        core::array::from_fn(|s| slot_at(s).and_then(|sl| scene.textures.get(&sl.texture)));
    let stages = pose.stages_of(mat);
    // Bump mode 1: the triangle's tangent frame in the bump slot's UVs.
    let bump_index = usize::try_from(mat.bump_slot).ok().filter(|_| mat.bump_mode == BUMP_NORMAL_MAP && lit);
    let bump_slot = bump_index.and_then(|b| slots.get(b).and_then(|s| s.as_ref()));
    let bump_uvs = bump_slot.filter(|s| s.mapping == UV).and_then(|s| uv_set(v, s.uv_set));
    let blend = Blend::of(mat.blend_function);
    let colors = vertex_primary.as_ref().or(v.colors.as_ref());
    let port = t.port;
    let mut tri = 0;
    while tri + 2 < mesh.indices.len() {
        let idx = [mesh.indices[tri] as usize, mesh.indices[tri + 1] as usize, mesh.indices[tri + 2] as usize];
        tri += 3;
        let [ia, ib, ic] = idx;
        if idx.iter().any(|&i| clip[i * 4 + 3] <= 1e-4) {
            continue;
        }
        let sx: [f32; 3] = core::array::from_fn(|k| {
            port[0] as f32 + (clip[idx[k] * 4] / clip[idx[k] * 4 + 3] * 0.5 + 0.5) * port[2] as f32
        });
        let sy: [f32; 3] = core::array::from_fn(|k| {
            port[1] as f32 + (1.0 - (clip[idx[k] * 4 + 1] / clip[idx[k] * 4 + 3] * 0.5 + 0.5)) * port[3] as f32
        });
        let sz: [f32; 3] = core::array::from_fn(|k| clip[idx[k] * 4 + 2] / clip[idx[k] * 4 + 3]);
        let iw: [f32; 3] = core::array::from_fn(|k| 1.0 / clip[idx[k] * 4 + 3]);
        let area = (sx[1] - sx[0]) * (sy[2] - sy[0]) - (sx[2] - sx[0]) * (sy[1] - sy[0]);
        if area.abs() < 1e-6 {
            continue;
        }
        // Screen y runs down, so a counter-clockwise triangle has negative area here.
        let front_facing = area < 0.0;
        match mat.face_culling {
            0 if front_facing => continue,
            1 if !front_facing => continue,
            2 => continue,
            _ => {}
        }
        let tangents = match (bump_slot, bump_uvs) {
            (Some(s), Some(uvs)) if need_pos => {
                let p = |i: usize| -> [f32; 3] { [world_pos[i * 3], world_pos[i * 3 + 1], world_pos[i * 3 + 2]] };
                let (pa, pb, pc) = (p(ia), p(ib), p(ic));
                shading::tangents([&pa, &pb, &pc], [slot_uv(s, uvs, ia), slot_uv(s, uvs, ib), slot_uv(s, uvs, ic)])
            }
            _ => None,
        };
        let min_x = port[0].max(floor(sx[0].min(sx[1].min(sx[2]))) as i32);
        let max_x = (port[0] + port[2] - 1).min(math::ceil(sx[0].max(sx[1].max(sx[2]))) as i32);
        let min_y = port[1].max(floor(sy[0].min(sy[1].min(sy[2]))) as i32);
        let max_y = (port[1] + port[3] - 1).min(math::ceil(sy[0].max(sy[1].max(sy[2]))) as i32);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((sx[1] - px) * (sy[2] - py) - (sx[2] - px) * (sy[1] - py)) / area;
                let w1 = ((sx[2] - px) * (sy[0] - py) - (sx[0] - px) * (sy[2] - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * sz[0] + w1 * sz[1] + w2 * sz[2];
                let di = (y * t.w + x) as usize;
                if mat.depth_test && z > t.depth[di] {
                    continue;
                }
                // Perspective-correct weights.
                let (pw0, pw1, pw2) = (w0 * iw[0], w1 * iw[1], w2 * iw[2]);
                let norm = 1.0 / (pw0 + pw1 + pw2);
                let (b0, b1, b2) = (pw0 * norm, pw1 * norm, pw2 * norm);
                let lerp = |a: &[f32], width: usize, k: usize| {
                    a[ia * width + k] * b0 + a[ib * width + k] * b1 + a[ic * width + k] * b2
                };
                let primary: [f32; 4] = core::array::from_fn(|k| colors.map_or(1.0, |c| lerp(c, 4, k)));
                let mut sampled: [Option<[f32; 4]>; 3] = [None; 3];
                for s in 0..3 {
                    let (Some(slot), Some(tex)) = (&slots[s], textures[s]) else { continue };
                    let (u, vv) = if slot.mapping == SPHERE && sphere {
                        let mut vn: [f32; 3] = core::array::from_fn(|k| lerp(&view_normal, 3, k));
                        math::normalize(&mut vn);
                        (vn[0] * 0.5 + 0.5, vn[1] * 0.5 + 0.5)
                    } else {
                        let Some(uvs) = uv_set(v, slot.uv_set) else { continue };
                        (lerp(uvs, 2, 0), lerp(uvs, 2, 1))
                    };
                    let g = shading::transform(u, vv, &slot.scale, slot.rotation, &slot.translation, slot.matrix_mode);
                    sampled[s] = Some(sample(tex, g[0], g[1], slot));
                }
                let (light_primary, light_secondary) = match &terms {
                    Some(terms) => {
                        let frag_pos: [f32; 3] = core::array::from_fn(|k| lerp(&world_pos, 3, k));
                        let mut frag_normal: [f32; 3] = core::array::from_fn(|k| lerp(&world_normal, 3, k));
                        let mut view: [f32; 3] = core::array::from_fn(|k| eye[k] - frag_pos[k]);
                        math::normalize(&mut frag_normal);
                        math::normalize(&mut view);
                        if v.normals.is_none() {
                            frag_normal = view;
                        } else if let (Some((tg, bt)), Some(b)) = (&tangents, bump_index) {
                            if let Some(texel) = &sampled[b] {
                                frag_normal = shading::perturb(&frag_normal, tg, bt, texel, mat.bump_renormalize);
                            }
                        }
                        shading::shade(mat.lighting.as_ref(), terms, &lights, &frag_normal, &view, &frag_pos)
                    }
                    None => (texenv::BLACK, texenv::BLACK),
                };
                let rgba =
                    texenv::evaluate(&stages, &primary, &sampled, &mat.buffer_color, &light_primary, &light_secondary);
                if mat.alpha_test_enabled && !alpha_pass(mat.alpha_test_function, rgba[3], mat.alpha_test_reference) {
                    continue;
                }
                let o = di * 4;
                if mat.blend_mode == 0 {
                    t.color[o..o + 3].copy_from_slice(&rgba[..3]);
                    t.color[o + 3] = 1.0;
                } else {
                    let dst = [t.color[o], t.color[o + 1], t.color[o + 2], t.color[o + 3]];
                    t.color[o..o + 4].copy_from_slice(&blend.blend(&rgba, &dst));
                }
                if mat.depth_write && mat.blend_mode == 0 {
                    t.depth[di] = z;
                }
            }
        }
    }
}

fn alpha_pass(function: i32, a: f32, reference: f32) -> bool {
    match function {
        0 => false,
        1 => true,
        2 => a == reference,
        3 => a != reference,
        4 => a < reference,
        5 => a <= reference,
        6 => a > reference,
        _ => a >= reference,
    }
}

/// Samples `tex` at (u, v), v = 0 at the bottom of the picture, with the slot's wrap and filter.
fn sample(tex: &RgbaImage, u: f32, v: f32, slot: &TextureSlot) -> [f32; 4] {
    let (w, h) = (tex.width as i32, tex.height as i32);
    let fx = wrap(u, slot.wrap_u) * w as f32 - 0.5;
    let fy = (1.0 - wrap(v, slot.wrap_v)) * h as f32 - 0.5;
    if !slot.linear {
        return texel(tex, floor(fx + 0.5) as i32, floor(fy + 0.5) as i32, slot);
    }
    let (x0, y0) = (floor(fx) as i32, floor(fy) as i32);
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let a = texel(tex, x0, y0, slot);
    let b = texel(tex, x0.saturating_add(1), y0, slot);
    let c = texel(tex, x0, y0.saturating_add(1), slot);
    let d = texel(tex, x0.saturating_add(1), y0.saturating_add(1), slot);
    core::array::from_fn(|k| (a[k] * (1.0 - tx) + b[k] * tx) * (1.0 - ty) + (c[k] * (1.0 - tx) + d[k] * tx) * ty)
}

/// PICA wrap modes: 0 clamp to edge, 1 clamp to border, 2 repeat, 3 mirror.
fn wrap(c: f32, mode: i32) -> f32 {
    match mode {
        2 => c - floor(c),
        3 => {
            let m = c - 2.0 * floor(c / 2.0);
            if m > 1.0 { 2.0 - m } else { m }
        }
        BORDER => c,
        _ => clamp(c, 0.0, 1.0),
    }
}

/// The texel at (x, y), or the slot's border colour off the texture along a clamp-to-border axis; other axes clamp to the edge.
fn texel(tex: &RgbaImage, x: i32, y: i32, slot: &TextureSlot) -> [f32; 4] {
    let (w, h) = (tex.width as i32, tex.height as i32);
    if (slot.wrap_u == BORDER && !(0..w).contains(&x)) || (slot.wrap_v == BORDER && !(0..h).contains(&y)) {
        return slot.border;
    }
    if w <= 0 || h <= 0 {
        return [0.0; 4];
    }
    let (cx, cy) = (x.clamp(0, w - 1), y.clamp(0, h - 1));
    let o = ((cy * w + cx) * 4) as usize;
    match tex.pixels.get(o..o + 4) {
        Some(p) => core::array::from_fn(|k| f32::from(p[k]) / 255.0),
        None => [0.0; 4],
    }
}
