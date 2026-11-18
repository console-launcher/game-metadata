// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Per-fragment work on the CPU: PICA fragment lighting, bump mapping, texture coordinate generation and the colour blend.

use banner3ds_scene::draw::light::{self, Terms};
use banner3ds_scene::draw::math::{self, clamp, cross, dot};
use banner3ds_scene::lighting::{self as lut, D0, D1, FR, MAX_LIGHTS, RB, RG, RR};
use banner3ds_scene::{BannerLight, FragmentLightingSetup, LutSampler};

/// Lights one fragment: `normal` and `view` (towards the eye) unit vectors, `position` in world space. Returns the primary
/// and secondary RGBA, clamped (alpha 1 unless Fresnel writes it). Per light: diffuse = max(N.L, 0) times the light's diffuse
/// register plus its ambient register, times the distance and spot factors; specular 0 = D0 x specular 0 (x geometric factor
/// 0), specular 1 = D1 x reflection x specular 1 (x geometric factor 1), zeroed where N.L <= 0 when highlights are clamped. A
/// table the layer configuration doesn't offer or the material disables reads as 1. Only the last light's Fresnel term counts.
pub fn shade(
    setup: Option<&FragmentLightingSetup>,
    terms: &Terms,
    lights: &[BannerLight],
    normal: &[f32; 3],
    view: &[f32; 3],
    position: &[f32; 3],
) -> ([f32; 4], [f32; 4]) {
    let (mut pr, mut pg, mut pb) = (terms.global_ambient[0], terms.global_ambient[1], terms.global_ambient[2]);
    let (mut sr, mut sg, mut sb) = (0.0f32, 0.0f32, 0.0f32);
    let mut fresnel = 1.0f32;
    let layer = setup.map_or(0, |s| s.layer_config);
    let [nx, ny, nz] = *normal;
    let [vx, vy, vz] = *view;
    let n = lights.len().min(MAX_LIGHTS).min(terms.diffuse.len());
    let sampler = |i: usize| setup.and_then(|s| s.samplers.get(i)).and_then(|s| s.as_ref());
    for (i, light) in lights.iter().enumerate().take(n) {
        let (mut lx, mut ly, mut lz);
        let mut distance = 0.0;
        if light.kind == 0 {
            (lx, ly, lz) = (-light.direction[0], -light.direction[1], -light.direction[2]);
        } else {
            (lx, ly, lz) =
                (light.position[0] - position[0], light.position[1] - position[1], light.position[2] - position[2]);
            distance = math::sqrt(lx * lx + ly * ly + lz * lz);
        }
        let ll = math::sqrt(lx * lx + ly * ly + lz * lz);
        if ll > 0.0 {
            lx /= ll;
            ly /= ll;
            lz /= ll;
        }
        let nl = nx * lx + ny * ly + nz * lz;
        let diffuse_dot = if light.two_sided_diffuse { nl.abs() } else { nl.max(0.0) };
        let (hx0, hy0, hz0) = (lx + vx, ly + vy, lz + vz);
        let h2 = hx0 * hx0 + hy0 * hy0 + hz0 * hz0;
        let hl = math::sqrt(h2);
        let (hx, hy, hz) = if hl > 0.0 { (hx0 / hl, hy0 / hl, hz0 / hl) } else { (0.0, 0.0, 0.0) };
        let mut spot_dot = 0.0;
        if light.kind == 2 {
            let d = light.direction;
            let dl = math::sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
            if dl > 0.0 {
                spot_dot = -(lx * d[0] + ly * d[1] + lz * d[2]) / dl;
            }
        }
        let inputs =
            [nx * hx + ny * hy + nz * hz, vx * hx + vy * hy + vz * hz, nx * vx + ny * vy + nz * vz, nl, spot_dot, 0.0];
        let read = |s: Option<&LutSampler>| -> f32 {
            match s {
                None => 1.0,
                Some(s) => s.scale * lut::lookup(&s.table, inputs[s.input.clamp(0, 5) as usize]),
            }
        };
        let spot = if light.kind == 2 && light.spot.is_some() && light::available(layer, light::SP) {
            read(light.spot.as_ref())
        } else {
            1.0
        };
        let atten = light.distance.as_ref().map_or(1.0, |t| {
            lut::lookup(t, clamp(light.attenuation_bias + light.attenuation_scale * distance, 0.0, 1.0))
        });
        let factor = spot * atten;
        let (d, a) = (terms.diffuse[i], terms.ambient[i]);
        pr += (d[0] * diffuse_dot + a[0]) * factor;
        pg += (d[1] * diffuse_dot + a[1]) * factor;
        pb += (d[2] * diffuse_dot + a[2]) * factor;

        let table = |index: usize, enabled: bool| -> f32 {
            if setup.is_none() || !enabled || !light::available(layer, index) { 1.0 } else { read(sampler(index)) }
        };
        let d0 = table(D0, setup.is_some_and(|s| s.distribution0));
        let d1 = table(D1, setup.is_some_and(|s| s.distribution1));
        let reflection = setup.is_some_and(|s| s.reflection);
        let rr = table(RR, reflection);
        let rg = if reflection && sampler(RG).is_some() && light::available(layer, RG) { table(RG, true) } else { rr };
        let rb = if reflection && sampler(RB).is_some() && light::available(layer, RB) { table(RB, true) } else { rr };
        let geo = if h2 > 0.0 { (nl.max(0.0) / h2).min(1.0) } else { 0.0 };
        let g0 = if setup.is_some_and(|s| s.geometric0) { geo } else { 1.0 };
        let g1 = if setup.is_some_and(|s| s.geometric1) { geo } else { 1.0 };
        let clamp_h = if setup.is_some_and(|s| s.clamp_highlights) && nl <= 0.0 { 0.0 } else { 1.0 };
        let (s0, s1) = (terms.specular0[i], terms.specular1[i]);
        let k0 = d0 * g0;
        let k1 = d1 * g1;
        sr += (s0[0] * k0 + s1[0] * rr * k1) * clamp_h * factor;
        sg += (s0[1] * k0 + s1[1] * rg * k1) * clamp_h * factor;
        sb += (s0[2] * k0 + s1[2] * rb * k1) * clamp_h * factor;
        if setup.is_some_and(|s| s.fresnel_selector != 0) && light::available(layer, FR) {
            fresnel = read(sampler(FR));
        }
    }
    let mut primary = [clamp(pr, 0.0, 1.0), clamp(pg, 0.0, 1.0), clamp(pb, 0.0, 1.0), 1.0];
    let mut secondary = [clamp(sr, 0.0, 1.0), clamp(sg, 0.0, 1.0), clamp(sb, 0.0, 1.0), 1.0];
    let selector = setup.map_or(0, |s| s.fresnel_selector);
    if selector & 1 != 0 {
        primary[3] = clamp(fresnel, 0.0, 1.0);
    }
    if selector & 2 != 0 {
        secondary[3] = clamp(fresnel, 0.0, 1.0);
    }
    (primary, secondary)
}

/// A triangle's tangent and bitangent (directions of +u and +v) from its corner positions and UVs; `None` when its UVs are
/// degenerate.
pub fn tangents(p: [&[f32; 3]; 3], uv: [[f32; 2]; 3]) -> Option<([f32; 3], [f32; 3])> {
    let (du1, dv1) = (uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]);
    let (du2, dv2) = (uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]);
    let det = du1 * dv2 - du2 * dv1;
    if det.abs() < 1e-12 {
        return None;
    }
    let mut t = [0.0; 3];
    let mut b = [0.0; 3];
    for k in 0..3 {
        let (e1, e2) = (p[1][k] - p[0][k], p[2][k] - p[0][k]);
        t[k] = (e1 * dv2 - e2 * dv1) / det;
        b[k] = (e2 * du1 - e1 * du2) / det;
    }
    Some((t, b))
}

/// The unit `normal` tilted by the normal-map `texel` (RGBA 0..1) in the frame of `tangent` and `bitangent`.
pub fn perturb(
    normal: &[f32; 3],
    tangent: &[f32; 3],
    bitangent: &[f32; 3],
    texel: &[f32; 4],
    renormalize: bool,
) -> [f32; 3] {
    let (mx, my) = (texel[0] * 2.0 - 1.0, texel[1] * 2.0 - 1.0);
    let mz = if renormalize { math::sqrt((1.0 - mx * mx - my * my).max(0.0)) } else { texel[2] * 2.0 - 1.0 };
    let nt = dot(normal, tangent);
    let mut tp: [f32; 3] = core::array::from_fn(|k| tangent[k] - normal[k] * nt);
    let tl = math::sqrt(dot(&tp, &tp));
    if tl < 1e-6 {
        return *normal;
    }
    for t in &mut tp {
        *t /= tl;
    }
    let c = cross(normal, tangent);
    let sign = if dot(&c, bitangent) < 0.0 { -1.0 } else { 1.0 };
    let bn = cross(normal, &tp);
    let mut out: [f32; 3] = core::array::from_fn(|k| tp[k] * mx + bn[k] * sign * my + normal[k] * mz);
    let l = math::sqrt(dot(&out, &out));
    if l > 0.0 {
        for o in &mut out {
            *o /= l;
        }
    }
    out
}

/// Texture coordinate transform types (coordinator +0xC).
use banner3ds_scene::coords::{MAX, SOFTIMAGE};

/// A coordinator's scale, rotation and translation on (u, v): Maya takes the translation off, turns about the centre and
/// scales last; Max turns and scales about the centre; Softimage about the origin.
pub fn transform(u: f32, v: f32, scale: &[f32; 2], rotation: f32, translation: &[f32; 2], mode: i32) -> [f32; 2] {
    let (c, n) = (math::cos(rotation), math::sin(rotation));
    let (tu, tv) = (translation[0], translation[1]);
    match mode {
        MAX => {
            let (mu, mv) = (u - tu - 0.5, v + tv - 0.5);
            [scale[0] * (mu * c - mv * n) + 0.5, scale[1] * (mu * n + mv * c) + 0.5]
        }
        SOFTIMAGE => [scale[0] * (c * (u - tu) - n * (v + tv)), scale[1] * (n * (u + tu) + c * (v - tv))],
        _ => {
            let (du, dv) = (u - 0.5, v - 0.5);
            [scale[0] * (du * c - dv * n + 0.5 - tu), scale[1] * (du * n + dv * c + 0.5 - tv)]
        }
    }
}

/// `normal` (world space) turned into view space by the column-major `view`, normalised.
pub fn to_view(view: &[f32; 16], normal: &[f32; 3]) -> [f32; 3] {
    let [x, y, z] = *normal;
    let mut out = [
        view[0] * x + view[4] * y + view[8] * z,
        view[1] * x + view[5] * y + view[9] * z,
        view[2] * x + view[6] * y + view[10] * z,
    ];
    let l = math::sqrt(out[0] * out[0] + out[1] * out[1] + out[2] * out[2]);
    if l > 1e-8 {
        for o in &mut out {
            *o /= l;
        }
    }
    out
}

/// PICA colour blend for a BLEND_FUNC register: colour by its equation (0 add, 1 subtract, 2 reverse subtract, 3 min, 4 max;
/// min and max take no factors) and factors (source bits 16-19, destination 20-23); alpha as coverage (one, one minus source
/// alpha), unchanged under a glow (destination factor one).
#[derive(Debug, Clone, Copy)]
pub struct Blend {
    cs: i32,
    cd: i32,
    equation: i32,
    glow: bool,
}

impl Blend {
    /// Decodes a BLEND_FUNC register.
    pub fn of(register: i32) -> Blend {
        let (cs, cd, equation) = ((register >> 16) & 15, (register >> 20) & 15, register & 7);
        Blend { cs, cd, equation, glow: light::keeps_destination(cd, equation) }
    }

    fn factor(code: i32, k: usize, src: &[f32; 4], dst: &[f32; 4]) -> f32 {
        match code {
            0 => 0.0,
            1 => 1.0,
            2 => src[k],
            3 => 1.0 - src[k],
            4 => dst[k],
            5 => 1.0 - dst[k],
            6 => src[3],
            7 => 1.0 - src[3],
            8 => dst[3],
            9 => 1.0 - dst[3],
            14 => src[3].min(1.0 - dst[3]),
            _ => 1.0,
        }
    }

    /// `src` over `dst`.
    pub fn blend(&self, src: &[f32; 4], dst: &[f32; 4]) -> [f32; 4] {
        let mut out = [0.0; 4];
        for k in 0..3 {
            let s = src[k] * Self::factor(self.cs, k, src, dst);
            let d = dst[k] * Self::factor(self.cd, k, src, dst);
            out[k] = clamp(
                match self.equation {
                    1 => s - d,
                    2 => d - s,
                    3 => src[k].min(dst[k]),
                    4 => src[k].max(dst[k]),
                    _ => s + d,
                },
                0.0,
                1.0,
            );
        }
        let (sa, da) = if self.glow { (0.0, 1.0) } else { (1.0, 1.0 - src[3]) };
        out[3] = clamp(src[3] * sa + dst[3] * da, 0.0, 1.0);
        out
    }
}
