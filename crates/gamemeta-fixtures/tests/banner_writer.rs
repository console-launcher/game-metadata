// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The CGFX/CBMD writer reproduces the committed feature banners (`fixtures/scene/features`, written by the original implementation
//! implementation's writer) byte for byte, from the same scenes rebuilt here.

use std::f64::consts::PI;

use banner3ds_scene::CombinerMode;
use banner3ds_scene::pica_textures::Format;
use gamemeta_core::RgbaImage;
use gamemeta_fixtures::banner3ds::{
    ALPHA_BLEND, Bone, BoneTrack, Cgfx, Coord, Curve, Mapper, Material, MaterialTrack, Mesh, Shape, Stage,
    VisibilityTrack, cbmd, combiner, sources,
};

const LOOP: f32 = 120.0;

fn image(size: u32, argb: impl Fn(i32, i32) -> u32) -> RgbaImage {
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size as i32 {
        for x in 0..size as i32 {
            let c = argb(x, y);
            px.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8]);
        }
    }
    RgbaImage { width: size, height: size, pixels: px }
}

fn argb(a: i32, r: i32, g: i32, b: i32) -> u32 {
    ((a.clamp(0, 255) as u32) << 24)
        | ((r.clamp(0, 255) as u32) << 16)
        | ((g.clamp(0, 255) as u32) << 8)
        | b.clamp(0, 255) as u32
}

fn wall() -> RgbaImage {
    image(32, |x, y| {
        let light = (x / 8 + y / 8) % 2 == 0;
        if (x + y) % 32 < 4 {
            argb(255, 0xC8, 0xD4, 0xA0)
        } else if light {
            argb(255, 0x8C, 0xA8, 0xC4)
        } else {
            argb(255, 0x34, 0x4C, 0x6C)
        }
    })
}

fn art() -> RgbaImage {
    image(32, |x, y| {
        let (dx, dy) = (x as f64 - 15.5, y as f64 - 15.5);
        let r = dx.hypot(dy) / 15.5;
        let h = (dy.atan2(dx) / (2.0 * PI) + 1.0) % 1.0;
        let ch = |o: f64| (127.5 + 127.5 * (2.0 * PI * (h - o)).cos()) as i32;
        argb(((1.0 - r) * 2.2 * 255.0) as i32, ch(0.0), ch(1.0 / 3.0), ch(2.0 / 3.0))
    })
}

fn ramp() -> RgbaImage {
    image(32, |x, _| argb(x * 255 / 31, 0xF0, 0x90, 0x30))
}

fn stripes() -> RgbaImage {
    image(32, |x, _| match x / 4 % 3 {
        0 => argb(255, 0xF0, 0xE0, 0x40),
        1 => argb(255, 0x30, 0xA0, 0x60),
        _ => argb(255, 0xF0, 0xF0, 0xF0),
    })
}

fn arrow() -> RgbaImage {
    image(32, |x, y| {
        let up = 31 - y;
        let head = (18..=28).contains(&up) && (x as f64 - 15.5).abs() < (29 - up) as f64 * 0.9;
        let shaft = (4..=18).contains(&up) && (13..=18).contains(&x);
        if head || shaft { argb(255, 0xF8, 0xF0, 0xE0) } else { argb(255, 0x60, 0x30, 0x80) }
    })
}

fn quad(cx: f32, cy: f32, w: f32, h: f32, z: f32) -> Vec<f32> {
    vec![
        cx - w / 2.0,
        cy - h / 2.0,
        z,
        cx + w / 2.0,
        cy - h / 2.0,
        z,
        cx + w / 2.0,
        cy + h / 2.0,
        z,
        cx - w / 2.0,
        cy + h / 2.0,
        z,
    ]
}

const UV: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
const TRIS: [u32; 6] = [0, 1, 2, 0, 2, 3];
const TEXTURE: [i32; 3] = [3, 0, 0];
const CONSTANT: [i32; 3] = [14, 0, 0];

fn normals() -> Vec<f32> {
    (0..12).map(|i| if i % 3 == 2 { 1.0 } else { 0.0 }).collect()
}

fn replace(src: [i32; 3], constant: i32) -> Stage {
    Stage {
        constant_index: constant,
        ..Stage::new(sources(src, src), combiner(CombinerMode::Replace, CombinerMode::Replace))
    }
}

fn tex(name: &str, image: RgbaImage, format: Format) -> gamemeta_fixtures::banner3ds::Texture {
    gamemeta_fixtures::banner3ds::Texture { name: name.into(), image, format }
}

fn banner(frames: f32) -> Cgfx {
    let mut c = Cgfx::default();
    c.animation.frames = frames;
    c.textures.push(tex("wall", wall(), Format::Rgb565));
    let mut m = Material::new("Backdrop", vec![replace(TEXTURE, 0)]).mapped(vec![Mapper::new("wall")]);
    m.coords = vec![Coord { scale: [2.0, 1.25], ..Coord::default() }];
    c.materials.push(m);
    c.bones.push(Bone { billboard: 1, ..Bone::new("Back", 0) });
    c.shapes.push(Shape {
        positions: quad(0.0, 1.0, 42.0, 26.0, -1.0),
        indices: TRIS.to_vec(),
        bone: 1,
        uvs: Some(UV.to_vec()),
        normals: Some(normals()),
        colors: None,
    });
    c.meshes.push(Mesh { shape: 0, material: 0, node: "Back".into(), priority: 0, visible: true });
    c
}

struct Tile<'n> {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    node: &'n str,
    z: f32,
    colors: Option<Vec<f32>>,
}

fn tile(c: &mut Cgfx, m: Material, t: Tile) {
    let b = c.bone_index(t.node).unwrap_or_else(|| {
        c.bones.push(Bone { billboard: 1, ..Bone::new(t.node, 0) });
        c.bones.len() - 1
    });
    c.materials.push(m);
    c.shapes.push(Shape {
        positions: quad(t.x, t.y, t.w, t.h, t.z),
        indices: TRIS.to_vec(),
        bone: b,
        uvs: Some(UV.to_vec()),
        colors: t.colors,
        normals: Some(normals()),
    });
    c.meshes.push(Mesh {
        shape: c.shapes.len() - 1,
        material: c.materials.len() - 1,
        node: t.node.into(),
        priority: 0,
        visible: true,
    });
}

fn at(x: f32, y: f32, w: f32, h: f32) -> Tile<'static> {
    Tile { x, y, w, h, node: "Front", z: 0.0, colors: None }
}

fn row(n: usize, pitch: f32) -> Vec<f32> {
    (0..n).map(|i| (i as f32 - (n as f32 - 1.0) / 2.0) * pitch).collect()
}

fn textured(name: &str, texture: &str) -> Material {
    Material::new(name, vec![replace(TEXTURE, 0)]).mapped(vec![Mapper::new(texture)])
}

fn flat(name: &str, rgba: [f32; 4]) -> Material {
    Material { constants: vec![rgba; 6], ..Material::new(name, vec![replace(CONSTANT, 0)]) }
}

fn scenes(name: &str) -> Cgfx {
    match name {
        "alpha-test" => {
            let mut c = banner(0.0);
            c.textures.push(tex("ramp", ramp(), Format::Rgba8));
            let tests = [1 | (6 << 4) | (64 << 8), 1 | (6 << 4) | (128 << 8), 1 | (4 << 4) | (128 << 8)];
            for (i, x) in row(3, 9.0).into_iter().enumerate() {
                let m = Material { alpha_test: tests[i], ..textured(&format!("Test{i}"), "ramp") };
                tile(&mut c, m, at(x, 0.0, 8.0, 8.0));
            }
            c
        }
        "tex-etc1" => {
            let mut c = banner(0.0);
            let list = [(Format::Etc1, "etc1"), (Format::Etc1A4, "etc1a4")];
            for (i, x) in row(2, 11.0).into_iter().enumerate() {
                let (f, n) = list[i];
                c.textures.push(tex(&format!("art_{n}"), art(), f));
                let m = textured(&format!("Fmt{i}"), &format!("art_{n}")).blended(ALPHA_BLEND, 1);
                tile(&mut c, m, at(x, 0.0, 9.0, 9.0));
            }
            c
        }
        "tex-luma-formats" => {
            let mut c = banner(0.0);
            let list = [
                (Format::La8, "la8"),
                (Format::L8, "l8"),
                (Format::A8, "a8"),
                (Format::La4, "la4"),
                (Format::L4, "l4"),
                (Format::A4, "a4"),
            ];
            let side = 28.0 / 6.0 - 0.6;
            for (i, x) in row(6, 28.0 / 6.0).into_iter().enumerate() {
                let (f, n) = list[i];
                c.textures.push(tex(&format!("art_{n}"), art(), f));
                let m = textured(&format!("Fmt{i}"), &format!("art_{n}")).blended(ALPHA_BLEND, 1);
                tile(&mut c, m, at(x, 0.0, side, side));
            }
            c
        }
        "combiner-buffer" => {
            let mut c = banner(0.0);
            c.textures.push(tex("ramp", ramp(), Format::Rgba8));
            let vertex = vec![0.3, 0.9, 1.0, 1.0, 0.3, 0.9, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
            let stages = vec![
                Stage::new(sources([3, 0, 0], [3, 0, 0]), combiner(CombinerMode::Modulate, CombinerMode::Modulate)),
                Stage {
                    constant_index: 1,
                    ..Stage::new(
                        sources([14, 0, 0], [14, 0, 0]),
                        combiner(CombinerMode::Replace, CombinerMode::Replace),
                    )
                },
                Stage {
                    operands: 2 << 8,
                    constant_index: 2,
                    ..Stage::new(
                        sources([15, 13, 3], [14, 14, 14]),
                        combiner(CombinerMode::Interpolate, CombinerMode::Replace),
                    )
                },
            ];
            let mut constants = vec![[0.0, 0.0, 0.0, 1.0], [0.9, 0.2, 0.5, 1.0], [1.0, 1.0, 1.0, 1.0]];
            constants.extend([[0.0, 0.0, 0.0, 1.0]; 3]);
            let m = Material { constants, update_buffer: 0x0100 | 0x1000, ..Material::new("Buffer", stages) }
                .mapped(vec![Mapper::new("ramp")]);
            tile(&mut c, m, Tile { colors: Some(vertex), ..at(0.0, 0.0, 20.0, 9.0) });
            c
        }
        "uv-rotate-scale" => {
            let mut c = banner(LOOP);
            c.textures.push(tex("arrow", arrow(), Format::Rgb565));
            tile(&mut c, textured("Turn", "arrow"), at(0.0, 0.0, 10.0, 10.0));
            let turn = Curve::linear(&[(0.0, 0.0), (LOOP, (2.0 * PI) as f32)]);
            c.material_tracks.push(MaterialTrack {
                material: "Turn".into(),
                member: "TextureCoordinators[0].Rotate".into(),
                components: vec![(0, turn)],
            });
            let breathe = Curve::linear(&[(0.0, 1.0), (LOOP / 2.0, 1.6), (LOOP, 1.0)]);
            c.material_tracks.push(MaterialTrack {
                material: "Turn".into(),
                member: "TextureCoordinators[0].Scale".into(),
                components: vec![(0, breathe.clone()), (1, breathe)],
            });
            c
        }
        "color-anim" => {
            let mut c = banner(LOOP);
            tile(&mut c, flat("Pulse", [1.0, 0.0, 0.5, 1.0]), at(0.0, 0.0, 12.0, 9.0));
            c.material_tracks.push(MaterialTrack {
                material: "Pulse".into(),
                member: "MaterialColor.Constant0".into(),
                components: vec![
                    (0, Curve::linear(&[(0.0, 1.0), (LOOP / 2.0, 0.0), (LOOP, 1.0)])),
                    (1, Curve::linear(&[(0.0, 0.0), (LOOP / 2.0, 1.0), (LOOP, 0.0)])),
                ],
            });
            c
        }
        "bone-anim" => {
            let mut c = banner(LOOP);
            c.textures.push(tex("arrow", arrow(), Format::Rgb565));
            tile(&mut c, textured("Lift", "arrow"), Tile { node: "Lift", ..at(0.0, -3.0, 6.0, 6.0) });
            let bt = |component, keys: &[(f32, f32)]| BoneTrack {
                bone: "Lift".into(),
                component,
                curve: Curve::linear(keys),
            };
            c.bone_tracks.push(bt(7, &[(0.0, 0.0), (LOOP / 2.0, 6.0), (LOOP, 0.0)]));
            c.bone_tracks.push(bt(0, &[(0.0, 1.0), (LOOP / 2.0, 1.8), (LOOP, 1.0)]));
            c
        }
        "visibility" => {
            let mut c = banner(LOOP);
            tile(&mut c, flat("Left", [0.95, 0.75, 0.1, 1.0]), Tile { node: "Left", ..at(-6.0, 0.0, 9.0, 9.0) });
            tile(&mut c, flat("Right", [0.1, 0.8, 0.6, 1.0]), Tile { node: "Right", ..at(6.0, 0.0, 9.0, 9.0) });
            c.visibility_tracks
                .push(VisibilityTrack { node: "Left".into(), curve: Curve::stepped(&[(0.0, 1.0), (LOOP / 2.0, 0.0)]) });
            c.visibility_tracks.push(VisibilityTrack {
                node: "Right".into(),
                curve: Curve::stepped(&[(0.0, 0.0), (LOOP / 2.0, 1.0)]),
            });
            c
        }
        "uv-scroll" => {
            let mut c = banner(LOOP);
            c.textures.push(tex("stripes", stripes(), Format::Rgb565));
            tile(&mut c, textured("Scroll", "stripes"), at(0.0, 0.0, 22.0, 9.0));
            c.material_tracks.push(MaterialTrack {
                material: "Scroll".into(),
                member: "TextureCoordinators[0].Translate".into(),
                components: vec![(0, Curve::linear(&[(0.0, 0.0), (LOOP, 1.0)]))],
            });
            c
        }
        _ => unreachable!("{name}"),
    }
}

fn committed(name: &str) -> Vec<u8> {
    let path = format!("{}/../../fixtures/scene/features/{name}.cbmd", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn rebuilds_the_feature_banners_byte_for_byte() {
    let names = [
        "alpha-test",
        "tex-etc1",
        "tex-luma-formats",
        "combiner-buffer",
        "uv-rotate-scale",
        "color-anim",
        "bone-anim",
        "visibility",
        "uv-scroll",
    ];
    let mut bad = Vec::new();
    for name in names {
        let cgfx = scenes(name).build();
        let want = committed(name);
        let want_cgfx = banner3ds_scene::lz11::decompress(&want, 0x88, 8 << 20).expect("committed banner decompresses");
        if cgfx != want_cgfx {
            let first = cgfx.iter().zip(&want_cgfx).position(|(a, b)| a != b);
            bad.push(format!(
                "{name}: cgfx {} vs {} bytes, first difference at {first:x?}",
                cgfx.len(),
                want_cgfx.len()
            ));
        } else if cbmd::write(&cgfx) != want {
            bad.push(format!("{name}: CGFX equal, CBMD differs"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn localized_slots_point_at_their_own_cgfx() {
    let common = scenes("color-anim").build();
    let title = scenes("visibility").build();
    let banner = cbmd::write_localized(&common, &[(0, &title), (1, &title)]);
    let parsed = banner3ds_scene::Cbmd::parse(&banner).expect("parses");
    assert!(parsed.localized().is_some());
    let at = |slot: usize| u32::from_le_bytes(banner[0xC + 4 * slot..0x10 + 4 * slot].try_into().unwrap()) as usize;
    assert!(at(0) > 0x88 && at(1) > at(0));
    assert_eq!(banner3ds_scene::lz11::decompress(&banner, at(1), 8 << 20).unwrap(), title);
}
