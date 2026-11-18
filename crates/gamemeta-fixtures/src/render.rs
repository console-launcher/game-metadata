// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Hand-built banner scenes for the renderer tests (the original implementation's `QuadFixture` and its lit quad): the smallest things both
//! renderers can draw.

use std::collections::BTreeMap;
use std::sync::Arc;

use banner3ds_scene::{
    BannerLight, BannerMaterial, BannerMesh, BannerModel, BannerScene, CombinerMode, FragmentLightingSetup,
    LightingLut, LutSampler, Skinning, TexEnvStage, TextureSlot, VertexData, lighting,
};
use gamemeta_core::RgbaImage;

/// A stage that replaces with source `src` in colour and alpha.
pub fn stage(src: i32, mode: CombinerMode) -> TexEnvStage {
    TexEnvStage {
        color_sources: [src, src, src],
        alpha_sources: [src, src, src],
        color_operands: [0; 3],
        alpha_operands: [0; 3],
        color_mode: mode,
        alpha_mode: mode,
        color_scale: 1,
        alpha_scale: 1,
        constant: [1.0; 4],
        update_color_buffer: false,
        update_alpha_buffer: false,
        constant_index: -1,
    }
}

/// An opaque, unlit, double-sided material with one texture slot reading `texture` (or none) and `stages`.
pub fn material(texture: Option<&str>, stages: Vec<TexEnvStage>) -> BannerMaterial {
    BannerMaterial {
        name: "m".into(),
        slots: vec![texture.map(|t| slot(t, 0)), None, None],
        stages,
        buffer_color: [0.0; 4],
        diffuse: [0.0; 4],
        ambient: [0.0; 4],
        emission: [0.0; 4],
        fragment_lighting: false,
        alpha_test_enabled: false,
        alpha_test_function: 1,
        alpha_test_reference: 0.0,
        blend_mode: 0,
        blend_function: 0,
        blend_color: [0.0; 4],
        face_culling: 3,
        depth_test: true,
        depth_write: true,
        translucency: 0,
        specular0: [0.0; 4],
        specular1: [0.0; 4],
        lighting: None,
        logic_op: 3,
        light_set: 0,
        bump_mode: 0,
        bump_slot: 0,
        bump_renormalize: false,
        vertex_lighting: false,
        hemisphere_lighting: false,
    }
}

/// A clamped, nearest texture slot reading UV set 0 of `texture`.
pub fn slot(texture: &str, wrap: i32) -> TextureSlot {
    TextureSlot {
        texture: texture.into(),
        wrap_u: wrap,
        wrap_v: wrap,
        linear: false,
        uv_set: 0,
        scale: [1.0, 1.0],
        rotation: 0.0,
        translation: [0.0, 0.0],
        mapping: 0,
        border: [0.0; 4],
        matrix_mode: 0,
    }
}

/// A square of half size `half` in the XY plane, v = 0 at the bottom edge, normals +Z when `normals`.
pub fn quad_vertices(half: f32, normals: bool) -> VertexData {
    VertexData {
        count: 4,
        positions: vec![-half, -half, 0.0, half, -half, 0.0, half, half, 0.0, -half, half, 0.0],
        normals: normals.then(|| [0.0, 0.0, 1.0].repeat(4)),
        uv0: Some(vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]),
        uv1: None,
        uv2: None,
        colors: None,
        bone_indices: None,
        bone_weights: None,
    }
}

/// One model of one mesh drawing `vertices` with `material`.
pub fn one_mesh(vertices: VertexData, material: BannerMaterial, textures: BTreeMap<String, RgbaImage>) -> BannerScene {
    let mesh = BannerMesh {
        name: "m/q".into(),
        vertices: Arc::new(vertices),
        indices: vec![0, 1, 2, 0, 2, 3],
        skinning: Skinning::None,
        bone_palette: Vec::new(),
        material: Arc::new(material),
        node_name: "q".into(),
        visible: true,
        render_priority: 0,
    };
    let model = BannerModel {
        name: "m".into(),
        world: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        meshes: vec![mesh],
        bones: Vec::new(),
        visible: true,
    };
    BannerScene::new(vec![model], textures)
}

/// The original implementation's `QuadFixture.scene`: `texture` on a square of half size `half` (HOME's fixed view needs about 30 to fill).
pub fn quad(texture: RgbaImage, half: f32) -> BannerScene {
    let textures = BTreeMap::from([("t".to_string(), texture)]);
    one_mesh(quad_vertices(half, false), material(Some("t"), vec![stage(3, CombinerMode::Replace)]), textures)
}

/// An 8 x 8 picture: the top half red, the bottom half blue (row 0 is the top), opaque.
pub fn red_over_blue() -> RgbaImage {
    let mut px = vec![0u8; 8 * 8 * 4];
    for y in 0..8 {
        for x in 0..8 {
            let o = (y * 8 + x) * 4;
            if y < 4 {
                px[o] = 255
            } else {
                px[o + 2] = 255
            }
            px[o + 3] = 255;
        }
    }
    RgbaImage { width: 8, height: 8, pixels: px }
}

/// A fragment-lit unit quad (normals +Z, diffuse 0.5) whose one stage replaces with the lit primary colour, under one
/// directional light of diffuse 1 pointing along `direction`.
pub fn lit_quad(direction: [f32; 3]) -> BannerScene {
    let mut m = material(None, vec![stage(1, CombinerMode::Replace)]);
    m.fragment_lighting = true;
    m.diffuse = [0.5, 0.5, 0.5, 1.0];
    m.lighting = Some(FragmentLightingSetup {
        clamp_highlights: false,
        distribution0: false,
        distribution1: false,
        geometric0: false,
        geometric1: false,
        reflection: false,
        layer_config: 0,
        fresnel_selector: 0,
        samplers: vec![None; 6],
    });
    let mut scene = one_mesh(quad_vertices(1.0, true), m, BTreeMap::new());
    scene.lights.push(light("sun", 0, [0.0; 3], direction));
    scene
}

/// A light of `kind` (0 directional, 1 point, 2 spot) of diffuse 1 and nothing else.
pub fn light(name: &str, kind: i32, position: [f32; 3], direction: [f32; 3]) -> BannerLight {
    BannerLight {
        name: name.into(),
        kind,
        position,
        direction,
        ambient: [0.0; 4],
        diffuse: [1.0; 4],
        specular0: [0.0; 4],
        specular1: [0.0; 4],
        spot: None,
        distance: None,
        attenuation_bias: 0.0,
        attenuation_scale: 0.0,
        two_sided_diffuse: false,
    }
}

/// An absolute lighting table of `f` over [0, 1].
pub fn absolute_table(f: impl Fn(f32) -> f32) -> Arc<LightingLut> {
    let values: Vec<f32> = (0..256).map(|i| f(i as f32 / 256.0)).collect();
    let deltas = (0..256).map(|i| if i < 255 { values[i + 1] - values[i] } else { 0.0 }).collect();
    Arc::new(LightingLut { values, deltas, absolute: true })
}

/// The original implementation's GL lighting scene: a lit quad (distribution 0 from a squared table) whose one stage adds the lit primary
/// and secondary colours, under a directional light and a spot light with a cone table: two tables in all.
pub fn lit_two_lights() -> BannerScene {
    let mut m = material(None, vec![stage(1, CombinerMode::Add)]);
    m.stages[0].color_sources = [1, 2, 2];
    m.stages[0].alpha_sources = [1, 2, 2];
    m.fragment_lighting = true;
    m.diffuse = [0.5, 0.5, 0.5, 1.0];
    m.specular0 = [1.0; 4];
    let mut samplers = vec![None; 6];
    samplers[lighting::D0] = Some(LutSampler { table: absolute_table(|x| x * x), input: 0, scale: 1.0 });
    m.lighting = Some(FragmentLightingSetup {
        clamp_highlights: false,
        distribution0: true,
        distribution1: false,
        geometric0: false,
        geometric1: false,
        reflection: false,
        layer_config: 0,
        fresnel_selector: 0,
        samplers,
    });
    let mut scene = one_mesh(quad_vertices(1.0, true), m, BTreeMap::new());
    scene.lights.push(light("sun", 0, [0.0; 3], [0.0, 0.0, -1.0]));
    let mut spot = light("spot", 2, [0.0, 0.0, 4.0], [0.0, 0.0, -1.0]);
    spot.spot = Some(LutSampler { table: absolute_table(|x| 1.0 - x), input: 4, scale: 1.0 });
    scene.lights.push(spot);
    scene
}
