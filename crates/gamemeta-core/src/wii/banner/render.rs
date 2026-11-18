// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Draws a channel banner's layout at an animation time on the CPU: the pane tree top-down, each picture pane as a transformed
//! quad (translate, Z rotation, scale, origin, size), vertex colors interpolated across it, the first texture map sampled
//! bilinearly through its SRT and wrap, further texture maps layered over by their alpha, everything multiplied by the material
//! color and the pane alpha (inherited from the parent when the pane is influenced), composited with premultiplied "over" onto
//! transparent. Not drawn: text, window frames, X/Y rotation and Z depth, TEV stages beyond that modulate-and-layer
//! approximation, blend modes.
//!
//! The arithmetic is the original implementation's, operation for operation (single precision where it is single, double where it is
//! double), so a frame matches it bit for bit.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::anim::{Animation, Target};
use super::layout::{Kind, Layout, Pane};
use crate::RgbaImage;

struct PaneState {
    tx: f32,
    ty: f32,
    rot_z: f32,
    sx: f32,
    sy: f32,
    w: f32,
    h: f32,
    alpha: f32,
    visible: bool,
    colors: [[f32; 4]; 4],
}

impl PaneState {
    fn new(pane: &Pane) -> Self {
        let channel = |v: u32, shift: u32| ((v >> shift) & 0xFF) as f32 / 255.0;
        let mut colors = [[1.0f32; 4]; 4];
        if let Some(vc) = pane.vertex_colors {
            for (c, v) in colors.iter_mut().zip(vc) {
                *c = [channel(v, 24), channel(v, 16), channel(v, 8), channel(v, 0)];
            }
        }
        PaneState {
            tx: pane.translate[0],
            ty: pane.translate[1],
            rot_z: pane.rotate[2],
            sx: pane.scale[0],
            sy: pane.scale[1],
            w: pane.size[0],
            h: pane.size[1],
            alpha: f32::from(pane.alpha) / 255.0,
            visible: pane.visible,
            colors,
        }
    }
}

struct MaterialState {
    color: [f32; 4],
    black: [f32; 4],
    white: [f32; 4],
    srt: Vec<[f32; 5]>,
    texture: Vec<usize>,
}

/// A texture the layout draws with: its decoded image, by file name.
pub(super) type Textures = BTreeMap<alloc::string::String, RgbaImage>;

fn round_to_int(x: f32) -> i32 {
    // Java's Math.round: floor(x + 1/2), exact in double; NaN is 0.
    libm::floor(f64::from(x) + 0.5) as i32
}

fn to_radians(degrees: f64) -> f64 {
    degrees * 0.017_453_292_519_943_295
}

/// The size in pixels of the layout at `scale` pixels per layout unit.
pub(super) fn pixel_size(layout: &Layout, scale: f32) -> (i32, i32) {
    (round_to_int(layout.width * scale).max(1), round_to_int(layout.height * scale).max(1))
}

/// Draws into `out` (`width * height * 4` bytes, RGBA not premultiplied).
pub(super) fn render(
    layout: &Layout,
    textures: &Textures,
    animations: &[(&Animation, f32)],
    scale: f32,
    width: i32,
    height: i32,
    out: &mut [u8],
) {
    let (w, h) = (width as usize, height as usize);
    let mut surface = alloc::vec![0f32; w * h * 4]; // premultiplied r, g, b, a
    let mut states: Vec<PaneState> = layout.panes.iter().map(PaneState::new).collect();
    let mut materials: Vec<MaterialState> = layout
        .materials
        .iter()
        .map(|m| MaterialState {
            color: [0, 1, 2, 3].map(|i| f32::from(m.color[i]) / 255.0),
            black: m.black.map(|v| f32::from(v) / 255.0),
            white: m.white.map(|v| f32::from(v) / 255.0),
            srt: m.srt.clone(),
            texture: m.tex_maps.iter().map(|t| usize::from(t.texture)).collect(),
        })
        .collect();
    let mut pane_by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, p) in layout.panes.iter().enumerate() {
        pane_by_name.entry(p.name.as_str()).or_default().push(i);
    }
    let mut material_by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, m) in layout.materials.iter().enumerate() {
        material_by_name.entry(m.name.as_str()).or_default().push(i);
    }
    for (animation, frame) in animations {
        let f = animation.clamp(*frame);
        for target in &animation.targets {
            if target.material {
                for &i in material_by_name.get(target.name.as_str()).into_iter().flatten() {
                    apply_material(layout, animation, target, f, &mut materials[i]);
                }
            } else {
                for &i in pane_by_name.get(target.name.as_str()).into_iter().flatten() {
                    apply_pane(target, f, &mut states[i]);
                }
            }
        }
    }
    let screen = [
        f64::from(scale),
        0.0,
        f64::from(layout.width * scale) / 2.0,
        0.0,
        -f64::from(scale),
        f64::from(layout.height * scale) / 2.0,
    ];
    let ctx = Ctx { layout, textures, states: &states, materials: &materials, width, height };
    draw(&ctx, 0, &screen, 1.0, &mut surface);
    for i in 0..w * h {
        let a = surface[i * 4 + 3];
        for c in 0..3 {
            let v = if a > 0.0 { surface[i * 4 + c] / a } else { 0.0 };
            out[i * 4 + c] = round_to_int(v * 255.0).clamp(0, 255) as u8;
        }
        out[i * 4 + 3] = round_to_int(a * 255.0).clamp(0, 255) as u8;
    }
}

fn apply_pane(target: &Target, frame: f32, s: &mut PaneState) {
    for track in &target.tracks {
        let v = track.at(frame);
        match &track.tag {
            b"RLPA" => match track.kind {
                0 => s.tx = v,
                1 => s.ty = v,
                5 => s.rot_z = v,
                6 => s.sx = v,
                7 => s.sy = v,
                8 => s.w = v,
                9 => s.h = v,
                _ => {}
            },
            b"RLVI" => s.visible = v >= 0.5,
            b"RLVC" => {
                if track.kind == 16 {
                    s.alpha = v / 255.0;
                } else if track.kind < 16 {
                    s.colors[usize::from(track.kind / 4)][usize::from(track.kind % 4)] = v / 255.0;
                }
            }
            _ => {}
        }
    }
}

fn apply_material(layout: &Layout, animation: &Animation, target: &Target, frame: f32, m: &mut MaterialState) {
    for track in &target.tracks {
        let v = track.at(frame);
        let index = usize::from(track.index);
        match &track.tag {
            b"RLTS" => {
                if let Some(srt) = m.srt.get_mut(index) {
                    if track.kind <= 4 {
                        srt[usize::from(track.kind)] = v;
                    }
                }
            }
            b"RLMC" => {
                if track.kind <= 3 {
                    m.color[usize::from(track.kind)] = v / 255.0;
                }
            }
            b"RLTP" if index < m.texture.len() => {
                let name = (v as i32 >= 0).then(|| animation.textures.get(v as i32 as usize)).flatten();
                if let Some(found) = name.and_then(|n| layout.textures.iter().position(|t| t == n)) {
                    m.texture[index] = found;
                }
            }
            _ => {}
        }
    }
}

struct Ctx<'a> {
    layout: &'a Layout,
    textures: &'a Textures,
    states: &'a [PaneState],
    materials: &'a [MaterialState],
    width: i32,
    height: i32,
}

/// A 2x3 affine matrix as (a, b, c, d, e, f): x' = a*x + b*y + c, y' = d*x + e*y + f.
type Affine = [f64; 6];

fn multiply(p: &Affine, l: &Affine) -> Affine {
    [
        p[0] * l[0] + p[1] * l[3],
        p[0] * l[1] + p[1] * l[4],
        p[0] * l[2] + p[1] * l[5] + p[2],
        p[3] * l[0] + p[4] * l[3],
        p[3] * l[1] + p[4] * l[4],
        p[3] * l[2] + p[4] * l[5] + p[5],
    ]
}

fn draw(ctx: &Ctx, pane_index: usize, parent: &Affine, parent_alpha: f32, surface: &mut [f32]) {
    let pane = &ctx.layout.panes[pane_index];
    let s = &ctx.states[pane_index];
    if !s.visible {
        return;
    }
    let alpha = s.alpha * if pane.influenced_alpha { parent_alpha } else { 1.0 };
    let r = to_radians(f64::from(s.rot_z));
    let (cr, sr) = (libm::cos(r), libm::sin(r));
    let local = [
        cr * f64::from(s.sx),
        -sr * f64::from(s.sy),
        f64::from(s.tx),
        sr * f64::from(s.sx),
        cr * f64::from(s.sy),
        f64::from(s.ty),
    ];
    let m = multiply(parent, &local);
    if pane.kind == Kind::Pic {
        quad(ctx, pane, s, &m, alpha, surface);
    }
    for &child in &pane.children {
        draw(ctx, child, &m, alpha, surface);
    }
}

fn bilerp(tl: f32, tr: f32, bl: f32, br: f32, u: f32, v: f32) -> f32 {
    (tl * (1.0 - u) + tr * u) * (1.0 - v) + (bl * (1.0 - u) + br * u) * v
}

fn wrap(i: i32, size: i32, mode: u8) -> i32 {
    match mode {
        1 => i.rem_euclid(size),
        2 => {
            let p = i.rem_euclid(size * 2);
            if p < size { p } else { size * 2 - 1 - p }
        }
        _ => i.clamp(0, size - 1),
    }
}

/// Bilinear sample of `image` at (s, t) in 0..1 as r, g, b, a in 0..1 (color not premultiplied).
fn texel(image: &RgbaImage, s: f32, t: f32, wrap_s: u8, wrap_t: u8) -> [f32; 4] {
    let (iw, ih) = (image.width as i32, image.height as i32);
    let x = s * iw as f32 - 0.5;
    let y = t * ih as f32 - 0.5;
    let x0 = libm::floorf(x) as i32;
    let y0 = libm::floorf(y) as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let mut out = [0f32; 4];
    for j in 0..2 {
        for i in 0..2 {
            let w = (if i == 0 { 1.0 - fx } else { fx }) * (if j == 0 { 1.0 - fy } else { fy });
            let p = (wrap(y0.wrapping_add(j), ih, wrap_t) * iw + wrap(x0.wrapping_add(i), iw, wrap_s)) as usize * 4;
            let a = f32::from(image.pixels[p + 3]) / 255.0;
            for (c, channel) in out[..3].iter_mut().enumerate() {
                *channel += w * a * f32::from(image.pixels[p + c]) / 255.0;
            }
            out[3] += w * a;
        }
    }
    if out[3] > 0.0 {
        let alpha = out[3];
        for channel in &mut out[..3] {
            *channel /= alpha;
        }
    }
    out
}

fn quad(ctx: &Ctx, pane: &Pane, s: &PaneState, m: &Affine, alpha: f32, surface: &mut [f32]) {
    if s.w == 0.0 || s.h == 0.0 {
        return;
    }
    let x0 = -(pane.origin_x as f32 * 0.5) * s.w;
    let y_top = pane.origin_y as f32 * 0.5 * s.h;
    let det = m[0] * m[4] - m[1] * m[3];
    if libm::fabs(det) < 1e-12 {
        return;
    }
    let corners = [(x0, y_top), (x0 + s.w, y_top), (x0, y_top - s.h), (x0 + s.w, y_top - s.h)].map(|(x, y)| {
        let (x, y) = (f64::from(x), f64::from(y));
        (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
    });
    let min_of = |f: fn(&(f64, f64)) -> f64| corners.iter().map(f).fold(f64::INFINITY, f64::min);
    let max_of = |f: fn(&(f64, f64)) -> f64| corners.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
    let min_x = (libm::floor(min_of(|c| c.0)) as i32).max(0);
    let max_x = (libm::ceil(max_of(|c| c.0)) as i32).min(ctx.width - 1);
    let min_y = (libm::floor(min_of(|c| c.1)) as i32).max(0);
    let max_y = (libm::ceil(max_of(|c| c.1)) as i32).min(ctx.height - 1);
    let material = usize::try_from(pane.material).ok().and_then(|i| ctx.materials.get(i));
    let coords = pane.tex_coords.first().copied().unwrap_or([0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    let maps = usize::try_from(pane.material)
        .ok()
        .and_then(|i| ctx.layout.materials.get(i))
        .map_or(&[][..], |m| &m.tex_maps[..]);
    let images: Vec<Option<&RgbaImage>> = (0..maps.len())
        .map(|i| {
            material.and_then(|mat| ctx.layout.textures.get(mat.texture[i])).and_then(|name| ctx.textures.get(name))
        })
        .collect();
    let (w, h) = (f64::from(s.w), f64::from(s.h));
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            // pixel center to local layout coordinates via the inverse of the 2x3 matrix
            let ex = f64::from(px) + 0.5 - m[2];
            let ey = f64::from(py) + 0.5 - m[5];
            let lx = (m[4] * ex - m[1] * ey) / det;
            let ly = (-m[3] * ex + m[0] * ey) / det;
            let u = (lx - f64::from(x0)) / w;
            let v = (f64::from(y_top) - ly) / h;
            if u < 0.0 || u > 1.0 || v < 0.0 || v > 1.0 {
                continue;
            }
            let (uf, vf) = (u as f32, v as f32);
            let mix = |k: usize| bilerp(s.colors[0][k], s.colors[1][k], s.colors[2][k], s.colors[3][k], uf, vf);
            let (mut cr, mut cg, mut cb, mut ca) = (mix(0), mix(1), mix(2), mix(3));
            if let Some(mat) = material {
                cr *= mat.color[0];
                cg *= mat.color[1];
                cb *= mat.color[2];
                ca *= mat.color[3];
            }
            if !images.is_empty() {
                let ts = bilerp(coords[0], coords[2], coords[4], coords[6], uf, vf);
                let tt = bilerp(coords[1], coords[3], coords[5], coords[7], uf, vf);
                let (mut tr, mut tg, mut tb, mut ta) = (0f32, 0f32, 0f32, 0f32);
                for (i, image) in images.iter().enumerate() {
                    let Some(image) = image else { continue };
                    let (mut s2, mut t2) = (ts, tt);
                    if let Some(srt) = material.and_then(|mat| mat.srt.get(i)) {
                        let rad = to_radians(f64::from(srt[2]));
                        let (c, sn) = (libm::cos(rad) as f32, libm::sin(rad) as f32);
                        let a = (ts - 0.5) * srt[3];
                        let b = (tt - 0.5) * srt[4];
                        s2 = a * c - b * sn + 0.5 + srt[0];
                        t2 = a * sn + b * c + 0.5 + srt[1];
                    }
                    let mut sample = texel(image, s2, t2, maps[i].wrap_s, maps[i].wrap_t);
                    if let Some(mat) = material {
                        for (k, v) in sample.iter_mut().enumerate() {
                            *v = mat.black[k] + (mat.white[k] - mat.black[k]) * *v;
                        }
                    }
                    if i == 0 {
                        [tr, tg, tb, ta] = sample;
                    } else {
                        tr += (sample[0] - tr) * sample[3];
                        tg += (sample[1] - tg) * sample[3];
                        tb += (sample[2] - tb) * sample[3];
                        ta += sample[3] * (1.0 - ta);
                    }
                }
                cr *= tr;
                cg *= tg;
                cb *= tb;
                ca *= ta;
            }
            ca *= alpha;
            if ca <= 0.0 {
                continue;
            }
            let o = (py as usize * ctx.width as usize + px as usize) * 4;
            let keep = 1.0 - ca;
            surface[o] = cr * ca + surface[o] * keep;
            surface[o + 1] = cg * ca + surface[o + 1] * keep;
            surface[o + 2] = cb * ca + surface[o + 2] * keep;
            surface[o + 3] = ca + surface[o + 3] * keep;
        }
    }
}
