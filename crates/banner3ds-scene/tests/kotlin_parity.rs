// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The original implementation's parse-level tests, one test per test, on the byte-identical inputs its test builders
//! wrote (fixtures/scene/cases). Pixel and pose tests of the original implementation belong to the renderers.

use banner3ds_scene::{Cbmd, FormatError, cgfx, lz11};
use std::path::PathBuf;

fn case(name: &str) -> Vec<u8> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/scene/cases").join(name);
    ["cgfx", "cbmd"]
        .iter()
        .find_map(|e| std::fs::read(base.with_extension(e)).ok())
        .unwrap_or_else(|| panic!("no fixture {name}"))
}

fn scene(name: &str) -> banner3ds_scene::BannerScene {
    cgfx::parse_or_err(&case(name)).unwrap_or_else(|_| panic!("{name} parses"))
}

fn red_of(scene: Option<banner3ds_scene::BannerScene>) -> u8 {
    scene.expect("a scene").textures["COMMON1"].pixels[0]
}

// ---- CbmdLocalizedTest (3)
mod cbmd_localized {
    use super::*;

    #[test]
    fn every_language_slot_up_to_the_jingle_is_read() {
        let cbmd = Cbmd::parse(&case("cbmd-regional-slots")).unwrap();
        assert_eq!(cbmd.regional.keys().copied().collect::<Vec<_>>(), [0, 9, 15]);
    }

    #[test]
    fn the_scene_takes_the_us_english_textures_over_the_placeholders_else_eur_english_else_the_common_file_alone() {
        // The original implementation's cgfx(n) is the 8x8 COMMON1 texture whose first pixel's red is n.
        assert_eq!(red_of(Cbmd::parse(&case("cbmd-usa-english")).unwrap().scene()), 3);
        assert_eq!(red_of(Cbmd::parse(&case("cbmd-eur-english")).unwrap().scene()), 2);
        assert_eq!(red_of(Cbmd::parse(&case("cbmd-japanese-only")).unwrap().scene()), 1);
        assert_eq!(red_of(Cbmd::parse(&case("cbmd-french-lowest")).unwrap().scene()), 1);
        assert_eq!(red_of(Cbmd::parse(&case("cbmd-common-only")).unwrap().scene()), 1);
    }

    #[test]
    fn a_language_cgfxs_own_models_the_logo_join_the_common_scene() {
        let scene = Cbmd::parse(&case("cbmd-logo-model")).unwrap().scene().unwrap();
        assert_eq!(scene.models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["COMMON", "USA_EN"]);
    }
}

// ---- CgfxBoneWeightTest (1)
#[test]
fn a_bone_weight_stream_of_fewer_than_four_elements_gives_the_missing_influences_no_weight() {
    let s = scene("bone-weights-3");
    let w = s.models[0].meshes[0].vertices.bone_weights.as_ref().unwrap();
    assert_eq!(w[0..4], [0.5, 0.25, 0.25, 0.0]);
    assert_eq!(w[12..16], [0.5, 0.25, 0.25, 0.0], "every vertex");
}

// ---- CgfxConstantsTest (1)
#[test]
fn a_stages_constant_is_the_material_constant_colour_its_index_names_not_the_placeholder_in_the_command() {
    let s = scene("constants-index");
    let stages = &s.models[0].meshes[0].material.stages;
    assert_eq!(stages[0].constant, [0.25, 0.5, 0.75, 1.0]);
    assert_eq!(stages[1].constant, [0.5, 0.5, 0.5, 1.0]);
}

// ---- CgfxDepthTest (1)
#[test]
fn depth_test_and_write_come_from_the_materials_depth_flags_not_the_byte_masked_mask_command() {
    let m = |flags: u8| scene(&format!("depth-{flags}")).models[0].meshes[0].material.clone();
    assert!(m(3).depth_test && m(3).depth_write);
    assert!(m(1).depth_test && !m(1).depth_write);
    assert!(!m(0).depth_test);
}

// ---- CgfxFixedAttributeTest (1)
#[test]
fn a_fixed_vertex_colour_is_its_inline_floats_with_no_scale_applied() {
    let s = scene("fixed-color");
    let c = s.models[0].meshes[0].vertices.colors.as_ref().unwrap();
    assert_eq!(c[0..4], [1.0, 0.5, 0.25, 1.0]);
    assert_eq!(c[12..16], [1.0, 0.5, 0.25, 1.0], "every vertex");
}

// ---- MeshVisibilityTest (1)
#[test]
fn a_mesh_nodes_visibility_is_the_flag_right_after_its_name_in_the_models_visibility_entry() {
    assert!(scene("mesh-visible").models[0].meshes[0].visible);
    assert!(!scene("mesh-hidden").models[0].meshes[0].visible);
}

// ---- ModelTransformTest (1)
#[test]
fn the_model_world_is_the_cached_matrix_scaled_by_the_nodes_scale_its_rotation_and_translation_not_rebuilt() {
    let world = scene("model-transform").models[0].world;
    let expected = [0.89, 0.0, 0.0, 1.0, 0.0, 0.89, 0.0, 2.0, 0.0, 0.0, 0.89, 3.0];
    for i in 0..12 {
        assert!((world[i] - expected[i]).abs() < 1e-6, "element {i}: {}", world[i]);
    }
}

// ---- MultiCurveAnimationTest (4)
mod multi_curve {
    use super::*;

    fn keys(name: &str) -> banner3ds_scene::Keyframes {
        let s = scene(name);
        s.animations.material[0].tracks.iter().find(|t| t.component == 1).expect("a y track").keys.clone()
    }

    #[test]
    fn a_later_curve_of_the_group_plays_after_the_first() {
        let k = keys("curves-sheen");
        assert!((k.sample(10.0) - 0.25).abs() < 1e-4);
        assert!((k.sample(32.0) + 0.25).abs() < 1e-4);
        assert!((k.sample(100.0) + 0.75).abs() < 1e-4);
    }

    #[test]
    fn a_constant_segment_holds_its_value_after_the_keyed_curve() {
        let k = keys("curves-constant-segment");
        assert!((k.sample(90.0) - 1.0).abs() < 1e-4);
        assert!((k.sample(120.0) - 1.5).abs() < 1e-4);
    }

    #[test]
    fn a_stepped_curve_between_linear_ones_holds_its_value() {
        let k = keys("curves-stepped");
        assert!((k.sample(90.0) - 1.0).abs() < 1e-4);
        assert!((k.sample(300.0) - 1.5).abs() < 1e-4);
        assert!((k.sample(469.5) - 1.5).abs() < 1e-4);
        assert!((k.sample(495.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_quantized_curve_whose_first_key_is_all_zero_keeps_its_key_frames() {
        let k = keys("curves-hermite48");
        assert_eq!(k.frames, [0.0, 16.0, 64.0, 128.0]);
        assert!((k.sample(16.0) - 3.6).abs() < 1e-3);
        assert!((k.sample(64.0) - 3.8).abs() < 1e-3);
    }
}

// ---- AnimationTest, the keyframe tests (3 of 5; the clip player and pose are the renderers')
mod animation {
    use banner3ds_scene::Keyframes;

    fn keys(step: bool, slopes: Option<Vec<f32>>) -> Keyframes {
        Keyframes {
            frames: vec![0.0, 10.0],
            values: vec![0.0, 10.0],
            in_slopes: slopes.clone(),
            out_slopes: slopes,
            step,
        }
    }

    #[test]
    fn linear_keys_interpolate_and_clamp_outside_the_range() {
        let k = keys(false, None);
        assert!((k.sample(5.0) - 5.0).abs() < 1e-5);
        assert!((k.sample(-3.0)).abs() < 1e-5);
        assert!((k.sample(99.0) - 10.0).abs() < 1e-5);
    }

    #[test]
    fn step_keys_hold_the_earlier_value_until_the_next_key() {
        assert!((keys(true, None).sample(9.9)).abs() < 1e-5);
    }

    #[test]
    fn hermite_keys_with_zero_slopes_ease_in_and_out() {
        let k = keys(false, Some(vec![0.0, 0.0]));
        assert!((k.sample(5.0) - 5.0).abs() < 1e-5);
        assert!(k.sample(2.0) < 2.0, "eases in slower than linear");
    }
}

// ---- CgfxLightingTest (10)
mod lighting {
    use super::*;
    use banner3ds_scene::lighting::{D0, FR, RR, lookup};

    fn material(s: &banner3ds_scene::BannerScene) -> &banner3ds_scene::BannerMaterial {
        &s.models[0].meshes[0].material
    }

    #[test]
    fn a_lighting_source_in_an_operand_the_stages_mode_never_reads_is_not_a_lighting_feature() {
        assert!(!scene("quad-leftover-sources").unsupported.contains("fragment-lighting"));
    }

    #[test]
    fn the_synthetic_scene_parses_to_one_quad() {
        let s = scene("quad");
        assert_eq!(s.models[0].meshes[0].indices.len(), 6);
        assert!(s.models[0].meshes[0].vertices.normals.is_some());
    }

    #[test]
    fn a_materials_lighting_block_its_tables_and_its_specular_colours_are_read() {
        let s = scene("lighting-block");
        let m = material(&s);
        let l = m.lighting.as_ref().unwrap();
        assert!(
            l.clamp_highlights && l.distribution0 && l.distribution1 && l.geometric0 && !l.geometric1 && l.reflection
        );
        assert_eq!((l.layer_config, l.fresnel_selector), (3, 2));
        let d0 = l.samplers[D0].as_ref().unwrap();
        assert_eq!((d0.input, d0.scale), (0, 0.25));
        assert!(d0.table.absolute);
        assert!((lookup(&d0.table, 0.5) - 0.5).abs() < 0.01);
        let fr = l.samplers[FR].as_ref().unwrap();
        assert_eq!((fr.input, fr.scale, fr.table.absolute), (2, 2.0, false));
        assert!(l.samplers[RR].is_none());
        assert_eq!((m.specular0[0], m.specular1[2]), (0.5, 0.3));
        assert!(!s.unsupported.contains("fragment-lighting"));
    }

    #[test]
    fn a_material_without_fragment_lighting_has_no_lighting_setup() {
        assert!(material(&scene("quad")).lighting.is_none());
    }

    #[test]
    fn enabled_lights_are_read_with_their_kind_colours_position_and_direction() {
        let lights = scene("lights").lights;
        assert_eq!(lights.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["sun", "lamp"]);
        let sun = &lights[0];
        assert_eq!(sun.kind, 0);
        assert!((sun.direction[0] - 0.6).abs() < 1e-4 && (sun.direction[1] + 0.8).abs() < 1e-4);
        assert_eq!((sun.diffuse[1], sun.ambient[0]), (0.7, 0.1));
        let lamp = &lights[1];
        assert_eq!((lamp.kind, lamp.position, lamp.specular0[0]), (1, [1.0, 2.0, 3.0], 0.5));
    }

    #[test]
    fn a_spot_lights_table_and_a_lights_distance_attenuation_are_read() {
        let spot = scene("lights-spot-distance").lights.remove(0);
        let cone = spot.spot.as_ref().unwrap();
        assert_eq!((cone.input, cone.scale), (4, 1.0));
        assert!((lookup(spot.distance.as_ref().unwrap(), 0.25) - 0.75).abs() < 0.01);
        assert!((spot.attenuation_bias + spot.attenuation_scale * 2.0).abs() < 1e-4);
        assert!((spot.attenuation_bias + spot.attenuation_scale * 6.0 - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_normal_mapped_material_parses_its_bump_a_tangent_map_and_a_cos_phi_table_stay_flagged() {
        let bump = scene("bump-normal");
        assert!(!bump.unsupported.contains("fragment-lighting-bump"));
        let m = material(&bump);
        assert_eq!((m.bump_mode, m.bump_slot, m.bump_renormalize), (1, 2, true));
        assert!(scene("bump-tangent").unsupported.contains("fragment-lighting-bump"));
        assert!(scene("lighting-cos-phi").unsupported.contains("lighting-input-cos-phi"));
    }

    #[test]
    fn an_enabled_ambient_light_is_the_scene_ambient_not_a_fragment_light() {
        let s = scene("ambient-lights");
        assert_eq!(s.lights.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["sun"]);
        assert_eq!(s.scene_ambient, [0.15, 0.2, 0.25]);
    }

    #[test]
    fn an_ambient_light_reaches_the_scene_only_through_a_light_set() {
        assert_eq!(scene("ambient-outside-set").scene_ambient, [0.0, 0.0, 0.0], "outside every light set");
        assert_eq!(scene("ambient-in-set").scene_ambient, [0.15, 0.2, 0.25], "listed");
    }
}

// ---- LightSetTest (2 of 4; the other two are the renderers')
mod light_set {
    use super::*;

    fn lights_of(name: &str) -> Vec<String> {
        let s = scene(name);
        s.lights_for(&s.models[0].meshes[0].material).iter().map(|l| l.name.clone()).collect()
    }

    #[test]
    fn a_material_reads_only_the_lights_of_its_set() {
        assert_eq!(lights_of("light-set-0"), ["dim"]);
        assert_eq!(lights_of("light-set-1"), ["bright"]);
    }

    #[test]
    fn with_no_environment_every_light_reaches_the_material_and_a_missing_set_lights_nothing() {
        assert_eq!(lights_of("light-set-none"), ["dim", "bright"]);
        assert!(lights_of("light-set-missing").is_empty());
    }
}

// ---- FileCameraTest (data-level parts of 2 of 7: which camera the environment names; the rest is camera matrices)
mod file_camera {
    use super::*;

    #[test]
    fn only_the_camera_the_scene_environment_names_frames_the_banner() {
        assert_eq!(scene("camera-environment-b").file_camera().unwrap().position[2], 20.0);
        let leftover = scene("camera-leftover");
        assert!(leftover.file_camera().is_none(), "cameras no environment names are authoring leftovers");
        assert_eq!(leftover.cameras.len(), 1);
    }

    #[test]
    fn without_a_camera_there_is_no_file_camera() {
        assert!(scene("quad").file_camera().is_none());
    }
}

// ---- TexCoordConfigTest (3)
mod tex_coord_config {
    use banner3ds_scene::coords::coordinator_of;

    #[test]
    fn textures_0_and_1_read_their_own_coordinators_under_every_config() {
        for config in 0..=5 {
            assert_eq!(coordinator_of(0, config), 0);
            assert_eq!(coordinator_of(1, config), 1);
        }
    }

    #[test]
    fn texture_2_reads_coordinator_1_under_configs_0110_0111_and_0112_else_coordinator_2() {
        assert_eq!((0..=5).map(|c| coordinator_of(2, c)).collect::<Vec<_>>(), [2, 1, 1, 1, 2, 2]);
    }

    #[test]
    fn an_unknown_config_keeps_texture_2_on_its_own_coordinator() {
        assert_eq!((coordinator_of(2, 9), coordinator_of(2, -1)), (2, 2));
    }
}

// ---- PicaTexturesTest (6)
mod pica_textures {
    use banner3ds_scene::pica_textures::{Format, decode};
    use gamemeta_core::RgbaImage;
    use gamemeta_core::threeds::pica::morton_xy;

    fn px(i: &RgbaImage, x: usize, y: usize) -> [u8; 4] {
        let o = (y * i.width as usize + x) * 4;
        i.pixels[o..o + 4].try_into().unwrap()
    }

    #[test]
    fn morton_order_puts_storage_index_1_to_the_right_and_2_below_left_in_the_8x8_tile() {
        for (i, want) in [(0, (0, 0)), (1, (1, 0)), (2, (0, 1)), (3, (1, 1)), (4, (2, 0)), (8, (0, 2)), (63, (7, 7))] {
            assert_eq!(morton_xy(i), want);
        }
    }

    #[test]
    fn rgba8_decodes_each_storage_pixel_to_its_morton_position() {
        let mut data = vec![0u8; 8 * 8 * 4];
        for i in 0..64 {
            data[i * 4] = 0xff;
            data[i * 4 + 3] = i as u8; // A, B, G, R: red carries the index
        }
        let img = decode(&data, 8, 8, Format::Rgba8).unwrap();
        for i in 0..64 {
            let (x, y) = morton_xy(i);
            assert_eq!(px(&img, x, y), [i as u8, 0, 0, 255], "pixel {i}");
        }
    }

    #[test]
    fn rgb565_expands_red_green_and_blue_to_8_bits() {
        let mut data = vec![0u8; 8 * 8 * 2];
        data[1] = 0xf8; // 0xF800 = pure red
        assert_eq!(px(&decode(&data, 8, 8, Format::Rgb565).unwrap(), 0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn an_etc1_block_in_individual_mode_with_base_red_and_a_zero_modifier_index_decodes_to_red_plus_the_small_offset() {
        let block = [0, 0, 0, 0, 0, 0, 0, 0xff];
        let img = decode(&block.repeat(4), 8, 8, Format::Etc1).unwrap();
        assert_eq!(px(&img, 0, 0), [255, 2, 2, 255]);
        assert_eq!(px(&img, 7, 7), [255, 2, 2, 255]);
    }

    #[test]
    fn etc1a4_takes_alpha_from_the_nibble_block_before_each_colour_block() {
        let mut one = vec![0xffu8; 8];
        one.extend([0, 0, 0, 0, 0, 0, 0, 0xff]);
        let img = decode(&one.repeat(4), 8, 8, Format::Etc1A4).unwrap();
        assert_eq!(px(&img, 3, 3)[3], 255);
    }

    #[test]
    fn short_data_and_odd_sizes_give_none_instead_of_panicking() {
        assert!(decode(&[0; 256], 8, 8, Format::Rgba8).is_some(), "exactly enough data decodes");
        assert!(decode(&[0; 10], 8, 8, Format::Rgba8).is_none());
        assert!(decode(&[0; 1000], 9, 8, Format::Rgba8).is_none());
        assert!(decode(&[0; 10], 0, 0, Format::L8).is_none());
    }
}

// ---- BannerContainersTest (5 of 6; the SMDH one is core's)
mod containers {
    use super::*;

    #[test]
    fn lz11_decodes_literals_and_a_back_reference() {
        let data = [0x11, 5, 0, 0, 0x40, b'a', 0x30, 0x00];
        assert_eq!(lz11::decompress(&data, 0, 16 << 20).unwrap(), b"aaaaa");
    }

    #[test]
    fn lz11_refuses_a_size_over_the_cap_and_a_back_reference_before_the_start() {
        assert!(
            lz11::decompress(&[0x11, 5, 0, 0, 0x40, b'a', 0x30, 0x00], 0, 1000).is_ok(),
            "a valid stream under the cap reads"
        );
        assert_eq!(lz11::decompress(&[0x11, 0, 0, 0x7f], 0, 1000), Err(FormatError));
        assert_eq!(lz11::decompress(&[0x11, 4, 0, 0, 0x80, 0x30, 0x05], 0, 16 << 20), Err(FormatError));
    }

    #[test]
    fn cbmd_hands_back_its_common_cgfx_decompressing_it_when_lz11() {
        let cgfx = case("texture-8");
        let cbmd = Cbmd::parse(&case("cbmd-common-only")).unwrap();
        assert_eq!(cbmd.common.as_deref(), Some(&cgfx[..]));
        assert!(Cbmd::parse(&[0; 0x90]).is_none());
        // Compressed: an LZ11 stream of literals only (flags 0 = eight literals per group).
        let mut packed = vec![0x11, cgfx.len() as u8, (cgfx.len() >> 8) as u8, (cgfx.len() >> 16) as u8];
        for chunk in cgfx.chunks(8) {
            packed.push(0);
            packed.extend(chunk);
        }
        let mut banner = vec![0u8; 0x88];
        banner[..4].copy_from_slice(b"CBMD");
        banner[8..12].copy_from_slice(&0x88u32.to_le_bytes());
        banner[0x84..0x88].copy_from_slice(&((0x88 + packed.len()) as u32).to_le_bytes());
        banner.extend(&packed);
        assert_eq!(Cbmd::parse(&banner).unwrap().common.as_deref(), Some(&cgfx[..]));
    }

    #[test]
    fn a_cgfx_with_one_texture_parses_to_that_decoded_texture() {
        let s = cgfx::parse(&case("texture-8")).unwrap();
        assert_eq!(s.textures.keys().collect::<Vec<_>>(), ["COMMON1"]);
        assert_eq!(s.textures["COMMON1"].width, 8);
        assert!(s.models.is_empty());
    }

    #[test]
    fn garbage_and_truncated_cgfx_give_none() {
        assert!(cgfx::parse(&case("texture-8")).is_some(), "the intact file reads");
        assert!(cgfx::parse(&[0; 10]).is_none());
        assert!(cgfx::parse(&vec![7; 0x200]).is_none());
        let ok = case("texture-8");
        for cut in [0x30, 0xA0, ok.len() - 40] {
            let _ = cgfx::parse(&ok[..cut]); // must not panic
        }
    }
}
