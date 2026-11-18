// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pixel parity with the original CPU renderer: each of the 18 feature banners (`fixtures/scene/features`) at three clip frames
//! (`fixtures/render/manifest.txt`: 0, mid-loop and the loop's last frame; a still banner only at 0) rendered by the original implementation
//! at 400 x 240 through HOME's view over transparent black (`fixtures/render/<name>-<i>.png`, a still banner's 1 and 2 being its 0).

use std::path::Path;

use banner3ds_scene::Cbmd;
use gamemeta_core::png;

/// The largest difference allowed in any channel of any pixel, 0..255.
const MAX_DIFF: u8 = 1;
/// The largest mean difference allowed per channel over a frame.
const MEAN_DIFF: f64 = 0.01;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures"))
}

#[test]
fn every_feature_banner_matches_the_reference_renderer() {
    let manifest = std::fs::read_to_string(root().join("render/manifest.txt")).unwrap();
    let mut failures = Vec::new();
    let (mut worst, mut worst_mean, mut count, mut within) = (0u8, 0f64, 0, 0);
    for line in manifest.lines() {
        let (id, t) = line.split_once(' ').unwrap();
        let t: f32 = t.parse().unwrap();
        let (name, _) = id.rsplit_once('-').unwrap();
        let bytes = std::fs::read(root().join(format!("scene/features/{name}.cbmd"))).unwrap();
        let scene = Cbmd::parse(&bytes).and_then(|c| c.scene()).unwrap();
        let golden_path = root().join(format!("render/{id}.png"));
        let golden_path = if golden_path.exists() { golden_path } else { root().join(format!("render/{name}-0.png")) };
        let golden = png::decode(&std::fs::read(golden_path).unwrap()).unwrap();
        assert_eq!((golden.width, golden.height), (400, 240));
        let mut ours = vec![0u8; 400 * 240 * 4];
        banner3ds_soft::render(&scene, t, 400, 240, &mut ours).unwrap();
        let (max, sum) = ours
            .iter()
            .zip(&golden.pixels)
            .fold((0u8, 0u64), |(m, s), (a, b)| (m.max(a.abs_diff(*b)), s + u64::from(a.abs_diff(*b))));
        let mean = sum as f64 / ours.len() as f64;
        println!("{id:24} t={t:6} max {max:3} mean {mean:.5}");
        worst = worst.max(max);
        worst_mean = worst_mean.max(mean);
        count += 1;
        if max <= MAX_DIFF && mean <= MEAN_DIFF {
            within += 1;
        } else {
            failures.push(format!("{id}: max {max} mean {mean:.4}"));
        }
    }
    println!("{within}/{count} within tolerance; worst max {worst}, worst mean {worst_mean:.5}");
    assert_eq!(count, 54);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The single-feature scenes of `fixtures/scene/cases` that parse and draw something (lighting, bump maps, billboards, bones,
/// cameras, ...), each rendered by the original implementation at rest at 128 x 128 through a camera fitted to its bounds
/// (`fixtures/render/cases/<name>.png`).
#[test]
fn every_drawable_case_matches_the_reference_renderer() {
    let manifest = std::fs::read_to_string(root().join("render/cases/manifest.txt")).unwrap();
    let mut failures = Vec::new();
    let (mut worst, mut worst_mean, mut count) = (0u8, 0f64, 0);
    for file in manifest.lines() {
        let bytes = std::fs::read(root().join(format!("scene/cases/{file}"))).unwrap();
        let scene = if file.ends_with(".cbmd") {
            Cbmd::parse(&bytes).and_then(|c| c.scene())
        } else {
            banner3ds_scene::cgfx::parse(&bytes)
        }
        .unwrap();
        let name = file.rsplit_once('.').unwrap().0;
        let golden = png::decode(&std::fs::read(root().join(format!("render/cases/{name}.png"))).unwrap()).unwrap();
        let r = banner3ds_soft::Renderer::new(&scene);
        let cam = banner3ds_scene::draw::camera::fit(&scene, r.player(), 128, 128);
        let mut ours = vec![0u8; 128 * 128 * 4];
        r.render_with(0.0, 128, 128, &banner3ds_soft::Options { camera: Some(cam), ..Default::default() }, &mut ours)
            .unwrap();
        let (max, sum) = ours
            .iter()
            .zip(&golden.pixels)
            .fold((0u8, 0u64), |(m, s), (a, b)| (m.max(a.abs_diff(*b)), s + u64::from(a.abs_diff(*b))));
        let mean = sum as f64 / ours.len() as f64;
        println!("{name:28} max {max:3} mean {mean:.5}");
        worst = worst.max(max);
        worst_mean = worst_mean.max(mean);
        count += 1;
        if max > MAX_DIFF || mean > MEAN_DIFF {
            failures.push(format!("{name}: max {max} mean {mean:.4}"));
        }
    }
    println!("{}/{count} within tolerance; worst max {worst}, worst mean {worst_mean:.5}", count - failures.len());
    assert_eq!(count, 40);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
