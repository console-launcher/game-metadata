// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The original implementation's `GlslTest` and `GlLightingTest`: the shape of the generated shaders and the lighting-table atlas.

use banner3ds_gles::glsl::{self, LutAtlas};
use banner3ds_scene::{BannerMaterial, CombinerMode, TexEnvStage, lighting};
use gamemeta_fixtures::render;

fn stage(src: i32, mode: CombinerMode, op: i32, scale: i32) -> TexEnvStage {
    TexEnvStage {
        color_sources: [src, 14, 15],
        alpha_sources: [src, 14, 15],
        color_operands: [op, 0, 0],
        alpha_operands: [0; 3],
        color_mode: mode,
        alpha_mode: mode,
        color_scale: scale,
        alpha_scale: 1,
        constant: [1.0; 4],
        update_color_buffer: false,
        update_alpha_buffer: false,
        constant_index: -1,
    }
}

fn material(stages: Vec<TexEnvStage>, blend: i32, alpha_test: bool) -> BannerMaterial {
    let mut m = render::material(None, stages);
    m.alpha_test_enabled = alpha_test;
    m.alpha_test_function = 6;
    m.alpha_test_reference = 0.5;
    m.blend_mode = blend;
    m
}

#[test]
fn a_modulate_stage_reads_texture_0_times_the_stage_constant_and_the_shader_is_opaque() {
    let src = glsl::fragment(
        &material(vec![stage(3, CombinerMode::Modulate, 0, 2)], 0, false),
        [true, false, false],
        &[],
        None,
        false,
    );
    for want in [
        "vec3 c0 = tex0.rgb;",
        "vec3 c1 = uConst[0].rgb;",
        "vec3 rgb = c0 * c1;",
        "rgb * 2.0",
        "texture(uTex0, vec2(vUv0.x, 1.0 - vUv0.y))",
        "vec4 tex1 = vec4(1.0);",
        "oColor = vec4(prev.rgb, 1.0);",
    ] {
        assert!(src.contains(want), "{want} not in\n{src}");
    }
}

#[test]
fn a_blended_material_keeps_the_combiner_alpha_and_an_alpha_test_discards_below_the_reference() {
    let src =
        glsl::fragment(&material(vec![stage(0, CombinerMode::Replace, 0, 1)], 1, true), [false; 3], &[], None, false);
    assert!(src.contains("oColor = prev;"), "{src}");
    assert!(src.contains("if (!(prev.a > 0.5)) discard;"), "{src}");
}

#[test]
fn a_stage_that_updates_the_buffer_is_written_by_the_stage_before_it() {
    let mut second = stage(15, CombinerMode::Replace, 0, 1);
    second.color_sources = [15; 3];
    second.alpha_sources = [15; 3];
    second.update_color_buffer = true;
    let src = glsl::fragment(
        &material(vec![stage(0, CombinerMode::Replace, 0, 1), second], 0, false),
        [false; 3],
        &[],
        None,
        false,
    );
    assert!(src.contains("buf.rgb = next.rgb;"), "{src}");
    assert!(!src.contains("buf.a = next.a;"));
}

#[test]
fn the_vertex_shader_routes_each_slots_uv_set() {
    let src = glsl::vertex([1, 0, 2], false, [0; 3], [0; 3]);
    assert!(src.contains("vUv0 = xf(aUv1, 0, 0);"), "{src}");
    assert!(src.contains("vUv1 = xf(aUv0, 1, 0);"));
    assert!(src.contains("vUv2 = xf(aUv2, 2, 0);"));
}

#[test]
fn pica_colour_blend_factors_map_to_gl_enums_and_alpha_composites_as_coverage() {
    // Source alpha, one minus source alpha on colour; one, one minus source alpha on alpha (coverage).
    let register = (6 << 16) | (7 << 20) | (1 << 24);
    assert_eq!(glsl::blend_factors(register), [0x0302, 0x0303, 1, 0x0303]);
}

#[test]
fn every_table_a_lit_material_or_a_light_reads_gets_one_row_of_value_and_difference_pairs() {
    let scene = render::lit_two_lights();
    let atlas = LutAtlas::of(&scene);
    assert_eq!(atlas.rows(), 2);
    let material = &scene.models[0].meshes[0].material;
    let d0 = &material.lighting.as_ref().unwrap().samplers[lighting::D0].as_ref().unwrap().table;
    let row = atlas.row_of(d0) as usize;
    assert!((atlas.data[(row * 256 + 100) * 4] - d0.values[100]).abs() < 1e-6);
    assert!((atlas.data[(row * 256 + 100) * 4 + 1] - d0.deltas[100]).abs() < 1e-6);
    assert_eq!(atlas.row_of(&scene.lights[1].spot.as_ref().unwrap().table) + atlas.row_of(d0), 1);
}

#[test]
fn a_lit_materials_shader_reads_the_lit_colours_for_sources_1_and_2_and_lights_each_light() {
    let scene = render::lit_two_lights();
    let material = &scene.models[0].meshes[0].material;
    let atlas = LutAtlas::of(&scene);
    let src = glsl::fragment(material, [false; 3], &scene.lights_for(material), Some(&atlas), true);
    for want in ["vec3 c0 = lp.rgb;", "vec3 c1 = ls.rgb;", "uniform sampler2D uLut;", "texelFetch(uLut"] {
        assert!(src.contains(want), "{want} not in\n{src}");
    }
    assert_eq!(src.matches("// light ").count(), 2);
    assert!(glsl::vertex([0; 3], true, [0; 3], [0; 3]).contains("in vec3 aNormal;"));
}

#[test]
fn without_lighting_sources_1_and_2_read_black_as_on_the_cpu() {
    let m = render::material(None, vec![render::stage(1, CombinerMode::Replace)]);
    let src = glsl::fragment(&m, [false; 3], &[], None, false);
    assert!(src.contains("vec3 c0 = vec4(0.0).rgb;"), "{src}");
    assert!(!src.contains("uLut"));
}
