// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The PICA texture-environment stages evaluated on the CPU; the GLES fragment shader writes the same out stage for stage.

use banner3ds_scene::draw::math::clamp;
use banner3ds_scene::{CombinerMode, TexEnvStage};

const WHITE: [f32; 4] = [1.0; 4];
/// What sources 1 and 2 read on a material without fragment lighting.
pub const BLACK: [f32; 4] = [0.0; 4];

/// `primary` is the interpolated vertex colour, `textures` the sampled colours of units 0..2 (`None` reads white), all RGBA
/// 0..1; `light_primary` and `light_secondary` the fragment-lit colours ([`BLACK`] without lighting). The final RGBA, clamped.
pub fn evaluate(
    stages: &[TexEnvStage],
    primary: &[f32; 4],
    textures: &[Option<[f32; 4]>; 3],
    buffer_color: &[f32; 4],
    light_primary: &[f32; 4],
    light_secondary: &[f32; 4],
) -> [f32; 4] {
    let mut previous = *primary;
    let mut buffer = *buffer_color;
    for (i, stage) in stages.iter().enumerate() {
        let source = |id: i32| -> [f32; 4] {
            match id {
                0 => *primary,
                1 => *light_primary,
                2 => *light_secondary,
                3..=5 => textures[(id - 3) as usize].unwrap_or(WHITE),
                13 => buffer,
                14 => stage.constant,
                15 => previous,
                _ => WHITE,
            }
        };
        let color_operand = |index: usize| -> [f32; 3] {
            let s = source(stage.color_sources[index]);
            core::array::from_fn(|k| match stage.color_operands[index] {
                0 => s[k],
                1 => 1.0 - s[k],
                2 => s[3],
                3 => 1.0 - s[3],
                4 => s[0],
                5 => 1.0 - s[0],
                8 => s[1],
                9 => 1.0 - s[1],
                12 => s[2],
                13 => 1.0 - s[2],
                _ => s[k],
            })
        };
        let alpha_operand = |index: usize| -> f32 {
            let s = source(stage.alpha_sources[index]);
            match stage.alpha_operands[index] {
                0 => s[3],
                1 => 1.0 - s[3],
                2 => s[0],
                3 => 1.0 - s[0],
                4 => s[1],
                5 => 1.0 - s[1],
                6 => s[2],
                7 => 1.0 - s[2],
                _ => s[3],
            }
        };
        let (a, b, c) = (color_operand(0), color_operand(1), color_operand(2));
        let rgb: [f32; 3] = match stage.color_mode {
            CombinerMode::Replace => a,
            CombinerMode::Modulate => core::array::from_fn(|k| a[k] * b[k]),
            CombinerMode::Add => core::array::from_fn(|k| a[k] + b[k]),
            CombinerMode::AddSigned => core::array::from_fn(|k| a[k] + b[k] - 0.5),
            CombinerMode::Interpolate => core::array::from_fn(|k| a[k] * c[k] + b[k] * (1.0 - c[k])),
            CombinerMode::Subtract => core::array::from_fn(|k| a[k] - b[k]),
            CombinerMode::DotProduct3Rgb | CombinerMode::DotProduct3Rgba => {
                let d = 4.0 * ((a[0] - 0.5) * (b[0] - 0.5) + (a[1] - 0.5) * (b[1] - 0.5) + (a[2] - 0.5) * (b[2] - 0.5));
                [d; 3]
            }
            CombinerMode::MultAdd => core::array::from_fn(|k| a[k] * b[k] + c[k]),
            CombinerMode::AddMult => core::array::from_fn(|k| (a[k] + b[k]) * c[k]),
        };
        let (aa, ab, ac) = (alpha_operand(0), alpha_operand(1), alpha_operand(2));
        let alpha = match stage.alpha_mode {
            CombinerMode::Replace => aa,
            CombinerMode::Modulate => aa * ab,
            CombinerMode::Add => aa + ab,
            CombinerMode::AddSigned => aa + ab - 0.5,
            CombinerMode::Interpolate => aa * ac + ab * (1.0 - ac),
            CombinerMode::Subtract => aa - ab,
            CombinerMode::DotProduct3Rgb | CombinerMode::DotProduct3Rgba => rgb[0],
            CombinerMode::MultAdd => aa * ab + ac,
            CombinerMode::AddMult => (aa + ab) * ac,
        };
        let cs = stage.color_scale as f32;
        let next = [
            clamp(rgb[0] * cs, 0.0, 1.0),
            clamp(rgb[1] * cs, 0.0, 1.0),
            clamp(rgb[2] * cs, 0.0, 1.0),
            clamp(alpha * stage.alpha_scale as f32, 0.0, 1.0),
        ];
        // The buffer register a stage writes is read by the stages after it, never by itself.
        if let Some(following) = stages.get(i + 1) {
            if following.update_color_buffer {
                buffer[..3].copy_from_slice(&next[..3]);
            }
            if following.update_alpha_buffer {
                buffer[3] = next[3];
            }
        }
        previous = next;
    }
    previous
}
