// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The lighting both renderers share: NW4C's vertex-shader lighting into colour source 0, the PICA fragment-lighting colour
//! registers a light environment holds for a material, and which tables each layer configuration reads. Facts: 3dbrew's
//! GPUREG_LIGHTING_* and GPUREG_LIGHTi_* pages.

use alloc::vec::Vec;

use super::math;
use super::pose::MaterialColors;
use crate::{BannerLight, BannerMaterial, HemisphereLight, VertexLight};

/// The spot table, for [`available`] (the material samplers are [`crate::lighting::RR`]..[`crate::lighting::FR`]).
pub const SP: usize = 6;

const fn bits(s: &[usize]) -> u32 {
    let mut acc = 0;
    let mut i = 0;
    while i < s.len() {
        acc |= 1 << s[i];
        i += 1;
    }
    acc
}

use crate::lighting::{D0, D1, FR, RB, RG, RR};
// GPUREG_LIGHTING_CONFIG0 layer configurations: the tables each one reads, a bit per sampler (RR..FR, SP at bit 6).
const LAYERS: [u32; 8] = [
    bits(&[D0, RR, SP]),
    bits(&[FR, RR, SP]),
    bits(&[D0, D1, RR]),
    bits(&[D0, D1, FR]),
    bits(&[D0, D1, RR, RG, RB, SP]),
    bits(&[D0, RR, RG, RB, FR, SP]),
    bits(&[D0, D1, RR, FR, SP]),
    bits(&[D0, D1, RR, RG, RB, FR, SP]),
];

/// Whether layer configuration `layer` (0..7) reads table `sampler` (RR..FR, [`SP`]).
pub fn available(layer: i32, sampler: usize) -> bool {
    (LAYERS[layer.clamp(0, 7) as usize] >> sampler) & 1 != 0
}

/// The register values a light environment holds for one material: material colour times light colour, per light (RGB).
#[derive(Debug, Clone, PartialEq)]
pub struct Terms {
    /// GPUREG_LIGHTING_AMBIENT: the emission plus the material's ambient times the scene ambient.
    pub global_ambient: [f32; 3],
    /// Per light: ambient.
    pub ambient: Vec<[f32; 3]>,
    /// Per light: diffuse.
    pub diffuse: Vec<[f32; 3]>,
    /// Per light: specular 0.
    pub specular0: Vec<[f32; 3]>,
    /// Per light: specular 1.
    pub specular1: Vec<[f32; 3]>,
}

/// The colour registers for `colors` under `lights`.
pub fn terms(colors: &MaterialColors, lights: &[BannerLight], scene_ambient: &[f32; 3]) -> Terms {
    let mul = |a: &[f32; 4], b: &[f32; 4]| -> [f32; 3] { core::array::from_fn(|k| a[k] * b[k]) };
    Terms {
        global_ambient: core::array::from_fn(|k| colors.emission[k] + colors.ambient[k] * scene_ambient[k]),
        ambient: lights.iter().map(|l| mul(&colors.ambient, &l.ambient)).collect(),
        diffuse: lights.iter().map(|l| mul(&colors.diffuse, &l.diffuse)).collect(),
        specular0: lights.iter().map(|l| mul(&colors.specular0, &l.specular0)).collect(),
        specular1: lights.iter().map(|l| mul(&colors.specular1, &l.specular1)).collect(),
    }
}

/// Colour source 0 of a vertex-lit or hemisphere-lit material: the emission, plus per vertex light its ambient times the
/// material's ambient and its diffuse times the material's diffuse times max(N.L, 0) (with distance attenuation), plus the
/// hemisphere's ground-to-sky blend; clamped. The vertex colour's RGB is not read (alpha is). A flag with no light of its kind
/// lights nothing: such a material reads its vertex colour.
pub fn vertex_primary(
    material: &BannerMaterial,
    lights: &[VertexLight],
    hemisphere: Option<&HemisphereLight>,
    normal: &[f32; 3],
    position: &[f32; 3],
    vertex_color: Option<&[f32]>,
) -> [f32; 4] {
    let by_vertex = material.vertex_lighting && !lights.is_empty();
    let hemisphere = hemisphere.filter(|_| material.hemisphere_lighting);
    if !by_vertex && hemisphere.is_none() {
        return core::array::from_fn(|k| vertex_color.map_or(1.0, |c| c[k]));
    }
    let mut sum: [f32; 3] = core::array::from_fn(|k| material.emission[k]);
    if by_vertex {
        for light in lights {
            let (lx, ly, lz);
            let mut atten = 1.0;
            if light.kind == 0 {
                (lx, ly, lz) = (-light.direction[0], -light.direction[1], -light.direction[2]);
            } else {
                (lx, ly, lz) =
                    (light.position[0] - position[0], light.position[1] - position[1], light.position[2] - position[2]);
                let d = math::sqrt(lx * lx + ly * ly + lz * lz);
                let a = light.attenuation;
                let denominator = a[0] + a[1] * d + a[2] * d * d;
                if denominator > 0.0 {
                    atten = 1.0 / denominator;
                }
            }
            let l = math::sqrt(lx * lx + ly * ly + lz * lz);
            let nl = if l > 0.0 { ((normal[0] * lx + normal[1] * ly + normal[2] * lz) / l).max(0.0) } else { 0.0 };
            for (k, s) in sum.iter_mut().enumerate() {
                *s += (light.ambient[k] * material.ambient[k] + light.diffuse[k] * material.diffuse[k] * nl) * atten;
            }
        }
    }
    if let Some(h) = hemisphere {
        let d = h.direction;
        let t = math::clamp((normal[0] * d[0] + normal[1] * d[1] + normal[2] * d[2]) * 0.5 + 0.5, 0.0, 1.0);
        for (k, s) in sum.iter_mut().enumerate() {
            *s += h.ground[k] + (h.sky[k] - h.ground[k]) * t;
        }
    }
    [
        math::clamp(sum[0], 0.0, 1.0),
        math::clamp(sum[1], 0.0, 1.0),
        math::clamp(sum[2], 0.0, 1.0),
        vertex_color.map_or(1.0, |c| c[3]),
    ]
}

/// PICA blend: true when the colour keeps the destination whole (destination factor one, an add or subtract), which adds
/// colour and no coverage (a glow), so the alpha stays as it was.
pub fn keeps_destination(dst_factor: i32, equation: i32) -> bool {
    dst_factor == 1 && equation <= 2
}
