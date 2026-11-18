// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The original CPU renderer's tests, one per test: `SoftwareRendererTest` (3) and `LitRenderTest` (2).

use banner3ds_scene::draw::{Player, camera};
use banner3ds_scene::{BannerScene, CombinerMode, TexEnvStage};
use banner3ds_soft::texenv::{self, BLACK};
use banner3ds_soft::{Options, RenderError, Renderer};
use gamemeta_core::RgbaImage;
use gamemeta_fixtures::render::{lit_quad, quad, red_over_blue};

/// The original implementation's `FitCamera.render`: the scene framed to fill the output, at rest.
fn fit_render(scene: &BannerScene, w: u32, h: u32, backdrop: Option<&[u8]>) -> Vec<u8> {
    let r = Renderer::new(scene);
    let cam = camera::fit(scene, &Player::new(scene), w as i32, h as i32);
    let mut out = vec![0u8; (w * h * 4) as usize];
    r.render_with(0.0, w, h, &Options { camera: Some(cam), backdrop, ..Default::default() }, &mut out).unwrap();
    out
}

// SoftwareRendererTest

#[test]
fn a_textured_quad_shows_its_picture_upright_top_row_at_the_top() {
    let img = fit_render(&quad(red_over_blue(), 1.0), 64, 64, None);
    let top = (16 * 64 + 32) * 4;
    let bottom = (48 * 64 + 32) * 4;
    assert!(img[top] > 200 && img[top + 2] < 50, "top is red");
    assert!(img[bottom + 2] > 200 && img[bottom] < 50, "bottom is blue");
}

#[test]
fn a_backdrop_shows_where_the_banner_draws_nothing_and_the_banner_covers_it_where_it_does() {
    let px: Vec<u8> = (0..8 * 8 * 4).map(|i| if i % 4 == 0 || i % 4 == 3 { 255 } else { 0 }).collect();
    let scene = quad(RgbaImage { width: 8, height: 8, pixels: px }, 1.0);
    let backdrop: Vec<u8> = (0..128 * 64 * 4).map(|i| if i % 4 == 1 || i % 4 == 3 { 255 } else { 0 }).collect();
    let img = fit_render(&scene, 128, 64, Some(&backdrop));
    let side = (32 * 128 + 2) * 4;
    let middle = (32 * 128 + 64) * 4;
    assert_eq!(&img[side..side + 4], &[0, 255, 0, 255], "the backdrop shows at the side");
    assert!(img[middle] > 200 && img[middle + 1] < 50, "the quad covers the middle");
}

#[test]
fn interpolate_blends_the_two_colours_by_the_third() {
    let s = TexEnvStage {
        color_sources: [14, 0, 14],
        alpha_sources: [14, 14, 14],
        color_operands: [0; 3],
        alpha_operands: [0; 3],
        color_mode: CombinerMode::Interpolate,
        alpha_mode: CombinerMode::Replace,
        color_scale: 1,
        alpha_scale: 1,
        constant: [1.0, 0.0, 0.0, 1.0],
        update_color_buffer: false,
        update_alpha_buffer: false,
        constant_index: -1,
    };
    // a = constant (red), b = primary (black), c = constant (red): red takes a, green and blue take b.
    let out = texenv::evaluate(&[s], &[0.0, 0.0, 0.0, 1.0], &[None; 3], &[0.0; 4], &BLACK, &BLACK);
    assert_eq!(&out[..3], &[1.0, 0.0, 0.0]);
}

// LitRenderTest

fn replace(src: i32) -> TexEnvStage {
    TexEnvStage {
        color_sources: [src, 14, 14],
        alpha_sources: [src, 14, 14],
        color_operands: [0; 3],
        alpha_operands: [0; 3],
        color_mode: CombinerMode::Replace,
        alpha_mode: CombinerMode::Replace,
        color_scale: 1,
        alpha_scale: 1,
        constant: [1.0; 4],
        update_color_buffer: false,
        update_alpha_buffer: false,
        constant_index: -1,
    }
}

#[test]
fn combiner_sources_1_and_2_read_the_fragment_lit_primary_and_secondary_colours_and_black_with_no_lighting() {
    let primary = [0.25, 0.5, 0.75, 1.0];
    let secondary = [0.1, 0.2, 0.3, 0.4];
    assert_eq!(texenv::evaluate(&[replace(1)], &[0.0; 4], &[None; 3], &[0.0; 4], &primary, &secondary), primary);
    assert_eq!(texenv::evaluate(&[replace(2)], &[0.0; 4], &[None; 3], &[0.0; 4], &primary, &secondary), secondary);
    assert_eq!(texenv::evaluate(&[replace(1)], &[1.0; 4], &[None; 3], &[0.0; 4], &BLACK, &BLACK), [0.0; 4]);
}

#[test]
fn the_cpu_renderer_lights_a_quad_by_its_normal() {
    let centre = |img: &[u8]| img[(16 * 32 + 16) * 4];
    let toward = centre(&fit_render(&lit_quad([0.0, 0.0, -1.0]), 32, 32, None));
    assert!((i32::from(toward) - 128).abs() <= 3, "{toward}");
    assert_eq!(centre(&fit_render(&lit_quad([0.0, 0.0, 1.0]), 32, 32, None)), 0);
}

// Rust API

#[test]
fn a_bad_size_or_buffer_is_refused_and_nothing_is_written() {
    let scene = quad(red_over_blue(), 1.0);
    let r = Renderer::new(&scene);
    let mut out = vec![7u8; 16];
    assert_eq!(r.render(0.0, 0, 4, &mut out), Err(RenderError::BadSize));
    assert_eq!(r.render(0.0, 3, 1, &mut out), Err(RenderError::BadBuffer));
    assert_eq!(r.render(0.0, 1 << 14, 1 << 14, &mut out), Err(RenderError::BadSize));
    assert!(out.iter().all(|&b| b == 7));
    assert_eq!(r.render(0.0, 2, 2, &mut out), Ok(()));
}
