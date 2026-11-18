// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The SuperTuxKart showcase banner (`fixtures/showcase/supertuxkart/`): Tux in his kart racing toward the viewer on a
//! looping stretch of track, built from the game's own free-licensed art (credited in that folder's `CREDITS.md`) and the
//! feature set the renderers support. The scene is a treadmill: the kart holds its place while the road, curbs, trees and an
//! item box stream past, so one clip loops seamlessly. Everything hangs off one world billboard at the origin, which keeps
//! HOME's turntable from turning the scene (the billboard root carries its turn to the camera to the bones below it).

use std::collections::BTreeMap;
use std::f32::consts::PI;
use std::path::Path;

use banner3ds_scene::CombinerMode::{self, Add, Interpolate, Modulate, Replace};
use banner3ds_scene::pica_textures::Format;
use gamemeta_core::RgbaImage;

use crate::banner3ds::{
    ADDITIVE_BLEND, ALPHA_BLEND, Bone, BoneTrack, Cgfx, Coord, Curve, Mapper, Material, MaterialTrack, Mesh, Shape,
    Stage, Texture, cbmd, combiner, sources,
};
use crate::reference;

/// Clip length in frames (60 to the second on HOME): one loop.
pub const LOOP: f32 = 240.0;
/// How far the ground scrolls per loop: whole road, grass, curb and tree tiles, so every stream loops.
const TRAVEL: f32 = 80.0;
const TREE_SPACING: f32 = 10.0;
const ROAD_TILE: f32 = 20.0;
const GRASS_TILE: f32 = 10.0;
/// The kart model's scale (the source is in metres, about 1.4 long).
const KART: f32 = 10.0;
const ROAD_Y: f32 = -8.0;
/// The kart's place along the road (behind the item box at the origin).
const KART_Z: f32 = -5.0;
/// The kart's turn toward the viewer's left (radians about Y): a three-quarter view.
const YAW: f32 = 0.35;
/// The scene's tilt toward the viewer (radians about X): a look down onto the track.
const TILT: f32 = 0.12;
const ROAD_HALF: f32 = 8.0;
const GRASS_HALF: f32 = 96.0;
/// Width of the mown stripes in the grass.
const STRIPE: f32 = 12.0;
const NEAR_Z: f32 = 40.0;
const FAR_Z: f32 = -320.0;
/// Haze: full at `HAZE_FAR`, none nearer than `HAZE_NEAR`.
const HAZE_FAR: f32 = -220.0;
const HAZE_NEAR: f32 = -60.0;
const HAZE: [f32; 4] = [0.78, 0.86, 0.95, 1.0];
/// The tree rows' distance from the road's centre, and their tint.
const TREES_X: f32 = 13.0;
const TREE_TINT: [f32; 3] = [0.8, 0.85, 0.78];
/// A scale that hides a part: HOME ignores visibility clips, so the box and the spark come and go by scale.
const GONE: f32 = 0.001;
/// The item box's height (it hovers over the road ahead of Tux), and the frame it pops.
const BOX_Y: f32 = 0.5;
const BOX_HIT: f32 = 160.0;
/// The logo's centre and width (the window's top shows y up to about 10 at z = 0).
const LOGO_AT: [f32; 3] = [0.0, 7.3, 0.5];
const LOGO_W: f32 = 11.0;
/// Light for the kart's baked shading (towards the light, normalised in `shade`).
const LIGHT: [f32; 3] = [-0.4, 0.8, 0.6];

/// A kart-space point (the model scaled, facing the viewer) turned by `YAW` and set on the road at `KART_Z`.
fn on_kart(p: [f32; 3]) -> [f32; 3] {
    let q = yawed(p);
    [q[0], q[1] + ROAD_Y, q[2] + KART_Z]
}

/// `p` turned by `YAW` about Y.
fn yawed(p: [f32; 3]) -> [f32; 3] {
    let (c, s) = (YAW.cos(), YAW.sin());
    [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]]
}

/// The gift box turned a little so two faces show.
fn box_yaw(p: [f32; 3]) -> [f32; 3] {
    let (c, s) = (0.6f32.cos(), 0.6f32.sin());
    [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]]
}

/// A quad in kart space (see `quad`), placed with `on_kart`.
fn kart_quad(c: [f32; 3], w: f32, h: f32, uv: [f32; 4], bone: usize) -> Shape {
    let mut q = quad(c, w, h, uv, bone, None);
    for v in q.positions.chunks_mut(3) {
        let p = on_kart([v[0], v[1], v[2]]);
        v.copy_from_slice(&p);
    }
    q
}

/// The tilt toward the viewer baked into every vertex but the sky's (the bones are billboards and drop rotations).
fn finish(c: &mut Cgfx, sky: usize) {
    let (co, si) = (TILT.cos(), TILT.sin());
    for s in c.shapes.iter_mut().filter(|s| s.bone != sky) {
        for v in s.positions.chunks_mut(3) {
            (v[1], v[2]) = (co * v[1] - si * v[2], si * v[1] + co * v[2]);
        }
    }
}

/// A model part from the source mesh file.
#[derive(Debug, Clone, Default)]
pub struct Part {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub uvs: Vec<f32>,
    pub indices: Vec<u32>,
}

/// A parsed `.mesh` file (written by `source/export_blend.py`).
#[derive(Debug, Clone, Default)]
pub struct Source {
    pub parts: BTreeMap<String, Part>,
    pub origins: BTreeMap<String, [f32; 3]>,
    pub nitro: Vec<[f32; 3]>,
}

/// Parses the exporter's text format; `None` on any malformed line.
pub fn parse_mesh(text: &str) -> Option<Source> {
    let mut out = Source::default();
    let mut current: Option<String> = None;
    let f = |s: &str| s.parse::<f32>().ok();
    for line in text.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.first().copied() {
            Some("part") => {
                let name = w.get(1)?.to_string();
                out.parts.insert(name.clone(), Part::default());
                current = Some(name);
            }
            Some("v") if w.len() == 9 => {
                let p = out.parts.get_mut(current.as_ref()?)?;
                let v: Vec<f32> = w[1..].iter().map(|s| f(s)).collect::<Option<_>>()?;
                p.positions.extend_from_slice(&v[0..3]);
                p.normals.extend_from_slice(&v[3..6]);
                p.uvs.extend_from_slice(&v[6..8]);
            }
            Some("t") if w.len() == 4 => {
                let p = out.parts.get_mut(current.as_ref()?)?;
                for s in &w[1..] {
                    p.indices.push(s.parse().ok()?);
                }
            }
            Some("origin") if w.len() == 5 => {
                out.origins.insert(w[1].into(), [f(w[2])?, f(w[3])?, f(w[4])?]);
            }
            Some("nitro") if w.len() == 4 => out.nitro.push([f(w[1])?, f(w[2])?, f(w[3])?]),
            None => {}
            _ => return None,
        }
    }
    Some(out)
}

/// The banner's textures: file in `assets/`, name, format.
pub const TEXTURES: [(&str, Format); 16] = [
    ("tuxkart", Format::Etc1),
    ("tux_body", Format::Etc1),
    ("engine", Format::Etc1),
    ("racetrack_grass", Format::Etc1),
    ("pino", Format::Etc1A4),
    ("wheel", Format::Rgb565),
    ("nitro-particle", Format::Rgb565),
    ("sky2", Format::Rgb8),
    ("racetrack_asphalt", Format::Rgba5551),
    ("gift-box", Format::Rgba8),
    ("flaring-star", Format::Rgba4),
    ("smoke", Format::A8),
    ("item_spark", Format::La8),
    ("city_checker", Format::L4),
    ("tux_shadow", Format::L8),
    ("logo_slim", Format::Rgba4),
];

fn load_png(path: &Path) -> RgbaImage {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    gamemeta_core::png::decode(&bytes).unwrap_or_else(|| panic!("{}: not an 8-bit RGB(A) PNG", path.display()))
}

/// `img` composited over white (luminance formats drop alpha: the shadow becomes a dark blob on white, for multiplying).
fn on_white(img: &mut RgbaImage) {
    for p in img.pixels.chunks_mut(4) {
        let a = u32::from(p[3]);
        for c in &mut p[..3] {
            *c = ((u32::from(*c) * a + 255 * (255 - a) + 127) / 255) as u8;
        }
        p[3] = 255;
    }
}

/// The right half of `img`.
fn right_half(img: &RgbaImage) -> RgbaImage {
    let (w, h) = (img.width / 2, img.height);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let row = ((y * img.width + w) * 4) as usize;
        pixels.extend_from_slice(&img.pixels[row..row + (w * 4) as usize]);
    }
    RgbaImage { width: w, height: h, pixels }
}

fn load_mesh(path: &Path) -> Source {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    parse_mesh(&text).unwrap_or_else(|| panic!("{}: malformed", path.display()))
}

/// The showcase folder in this repository.
pub fn folder() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/showcase/supertuxkart")
}

/// Keyframes of `banner.home/` that score over [`reference::MATCH`] on our renderers, with the reason (soft and gles share
/// them). Measured 2026-11-17 on the emulator 2126.2; the original Kotlin renderer scores the same (3.04 at c0430).
pub const KNOWN_MISMATCHES: &[reference::Known] = &[(
    430,
    5.0,
    "the spark burst (an LA8 texture added to a fading gold constant, blended additively): HOME draws it dimmer with its rings \
     showing; soft, gles and the original Kotlin renderer all saturate it to a white disc",
)];

/// The banner (CBMD) built from `dir` (the showcase folder): its title, SuperTuxKart's logo, reads the same in every language,
/// so all 30 language slots point at the one scene.
pub fn banner(dir: &Path) -> Vec<u8> {
    cbmd::write_every_language(&scene(dir).build())
}

fn stage(src: [i32; 3], alpha: [i32; 3], color: CombinerMode, alpha_mode: CombinerMode) -> Stage {
    Stage::new(sources(src, alpha), combiner(color, alpha_mode))
}

/// Texture 0 times the vertex colour (colour and alpha).
fn lit() -> Vec<Stage> {
    vec![stage([3, 0, 0], [3, 0, 0], Modulate, Modulate)]
}

/// Texture 0 times the vertex colour, then hazed by the vertex alpha through the combiner buffer: stage 1 stores stage 0's
/// result in the buffer and puts out the haze constant, stage 2 mixes buffer and haze by the primary alpha.
fn hazed() -> Vec<Stage> {
    vec![
        stage([3, 0, 0], [3, 0, 0], Modulate, Replace),
        Stage { constant_index: 1, ..stage([14, 0, 0], [14, 0, 0], Replace, Replace) },
        Stage { operands: 2 << 8, constant_index: 1, ..stage([13, 15, 0], [14, 0, 0], Interpolate, Replace) },
    ]
}

/// Multiply blending (destination colour, zero).
const MULTIPLY_BLEND: i32 = 0x0404_0000;
/// TEXENV_UPDATE_BUFFER: stage 1 stores its input (stage 0's result) in the buffer, colour and alpha.
const BUFFER_STAGE1: i32 = 0x0100 | 0x1000;

fn constants(c0: [f32; 4], c1: [f32; 4]) -> Vec<[f32; 4]> {
    let mut v = vec![c0, c1];
    v.extend([[0.0, 0.0, 0.0, 1.0]; 4]);
    v
}

fn haze_alpha(z: f32) -> f32 {
    ((z - HAZE_FAR) / (HAZE_NEAR - HAZE_FAR)).clamp(0.0, 1.0)
}

/// Baked diffuse light for a normal: ambient plus a key light.
fn shade(n: &[f32]) -> f32 {
    let l = LIGHT;
    let len = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
    let d = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) / len;
    (0.55 + 0.55 * d.max(0.0)).min(1.0)
}

struct Builder {
    c: Cgfx,
}

impl Builder {
    fn bone(&mut self, name: &str, parent: &str, translation: [f32; 3]) -> usize {
        let p = self.c.bone_index(parent).expect("parent bone") as i32;
        self.c.bones.push(Bone { translation, billboard: 1, ..Bone::new(name, p) });
        self.c.bones.len() - 1
    }

    fn material(&mut self, m: Material) -> usize {
        self.c.materials.push(m);
        self.c.materials.len() - 1
    }

    fn mesh(&mut self, shape: Shape, material: usize, node: &str) {
        self.c.shapes.push(shape);
        self.c.meshes.push(Mesh {
            shape: self.c.shapes.len() - 1,
            material,
            node: node.into(),
            priority: 0,
            visible: true,
        });
    }

    fn track(&mut self, bone: &str, component: usize, keys: &[(f32, f32)]) {
        self.c.bone_tracks.push(BoneTrack { bone: bone.into(), component, curve: Curve::linear(keys) });
    }

    fn uv_track(&mut self, material: &str, member: &str, components: Vec<(usize, Curve)>) {
        self.c.material_tracks.push(MaterialTrack { material: material.into(), member: member.into(), components });
    }
}

/// A quad in the XY plane facing +Z: centre, size, UV rectangle (u0, v0, u1, v1).
fn quad(c: [f32; 3], w: f32, h: f32, uv: [f32; 4], bone: usize, color: Option<[f32; 4]>) -> Shape {
    let (x0, x1, y0, y1) = (c[0] - w / 2.0, c[0] + w / 2.0, c[1] - h / 2.0, c[1] + h / 2.0);
    Shape {
        positions: vec![x0, y0, c[2], x1, y0, c[2], x1, y1, c[2], x0, y1, c[2]],
        indices: vec![0, 1, 2, 0, 2, 3],
        bone,
        uvs: Some(vec![uv[0], uv[1], uv[2], uv[1], uv[2], uv[3], uv[0], uv[3]]),
        colors: color.map(|c| c.repeat(4)),
        normals: None,
    }
}

/// A ground strip on y = `y` from x0 to x1, cut every `step` along z from FAR_Z to NEAR_Z; UVs tile every `tile` units along
/// z (u along z and v 0..1 across when `across`, else u from x and v along z), the vertex colour `color(x, z)` with the haze
/// in alpha.
fn ground(
    (x0, x1): (f32, f32),
    y: f32,
    step: f32,
    tile: f32,
    across: bool,
    bone: usize,
    color: impl Fn(f32, f32) -> [f32; 3],
) -> Shape {
    let mut s = Shape { bone, uvs: Some(Vec::new()), colors: Some(Vec::new()), ..Shape::default() };
    let rows = ((NEAR_Z - FAR_Z) / step).round() as u32;
    for r in 0..=rows {
        let z = FAR_Z + r as f32 * step;
        for x in [x0, x1] {
            s.positions.extend_from_slice(&[x, y, z]);
            let uv = if across { [-z / tile, if x == x0 { 0.0 } else { 1.0 }] } else { [x / tile, -z / tile] };
            s.uvs.as_mut().unwrap().extend_from_slice(&uv);
            let c = color(x, z);
            s.colors.as_mut().unwrap().extend_from_slice(&[c[0], c[1], c[2], haze_alpha(z)]);
        }
        if r > 0 {
            let b = 2 * r;
            s.indices.extend_from_slice(&[b - 2, b - 1, b + 1, b - 2, b + 1, b]);
        }
    }
    s
}

/// The full scene from the showcase folder `dir`.
pub fn scene(dir: &Path) -> Cgfx {
    let kart = load_mesh(&dir.join("source/tux-kart.mesh"));
    let gift = load_mesh(&dir.join("source/gift-box.mesh"));
    let mut b = Builder { c: Cgfx::default() };
    b.c.animation.frames = LOOP;
    for (name, format) in TEXTURES {
        let mut image = load_png(&dir.join(format!("assets/{name}.png")));
        if format == Format::L8 {
            on_white(&mut image);
        }
        b.c.textures.push(Texture { name: name.into(), image, format });
    }
    // The pine row: pino's foliage half on its own, so it tiles along the tree rows.
    let pino = load_png(&dir.join("assets/pino.png"));
    b.c.textures.push(Texture { name: "pino_row".into(), image: right_half(&pino), format: Format::Etc1A4 });

    // Bones. HOME holds a banner still (no turntable) only when every bone with a mesh is a billboard at x = z = 0 and no clip
    // moves a bone off the Y axis: every bone here is a world billboard at the origin, animated only in y, scale and
    // visibility. Places, the three-quarter view and the tilt toward the viewer are baked into the vertices (`finish`).
    b.c.bones[0].billboard = 1;
    let sky = b.bone("Sky", "Root", [0.0, 0.0, 0.0]);
    let world = b.bone("World", "Root", [0.0, 0.0, 0.0]);

    // ---- sky: SuperTuxKart's sky2 far back, a flare rotating on it (additive, UV rotate + scale).
    let m =
        b.material(Material::new("Sky", vec![stage([3, 0, 0], [3, 0, 0], Replace, Replace)]).mapped(vec![Mapper {
            wrap_u: 0,
            wrap_v: 0,
            ..Mapper::new("sky2")
        }]));
    b.mesh(quad([0.0, 40.0, -300.0], 380.0, 150.0, [0.0, 0.0, 1.0, 1.0], sky, None), m, "Sky");
    let m = b.material(
        Material::new("Flare", vec![stage([3, 0, 0], [3, 0, 0], Replace, Replace)])
            .mapped(vec![Mapper { wrap_u: 0, wrap_v: 0, ..Mapper::new("flaring-star") }])
            .blended(ADDITIVE_BLEND, 3),
    );
    b.c.materials[m].coords[0].matrix_mode = 2;
    b.mesh(quad([-95.0, 75.0, -290.0], 60.0, 60.0, [0.0, 0.0, 1.0, 1.0], sky, None), m, "Sky");
    b.uv_track("Flare", "TextureCoordinators[0].Rotate", vec![(0, Curve::linear(&[(0.0, 0.0), (LOOP, 2.0 * PI)]))]);
    let breathe = Curve::linear(&[(0.0, 1.0), (LOOP / 2.0, 0.8), (LOOP, 1.0)]);
    b.uv_track("Flare", "TextureCoordinators[0].Scale", vec![(0, breathe.clone()), (1, breathe)]);

    // ---- SuperTuxKart's logo over the sky, upright, in front of the box's drop (alpha blended).
    let m = b.material(
        Material::new("Logo", vec![stage([3, 0, 0], [3, 0, 0], Replace, Replace)])
            .mapped(vec![Mapper { wrap_u: 0, wrap_v: 0, ..Mapper::new("logo_slim") }])
            .blended(ALPHA_BLEND, 1),
    );
    b.mesh(quad(LOGO_AT, LOGO_W, LOGO_W / 2.0, [0.0, 0.0, 1.0, 1.0], sky, None), m, "Logo");

    // ---- ground: grass (mown stripes in the vertex colour) and asphalt, both hazed through the combiner buffer, both
    // scrolling toward the viewer; red-and-white curbs from the checker (L4 times a constant).
    let road_scroll = -TRAVEL / ROAD_TILE;
    let m = b.material(Material {
        constants: constants([0.0, 0.0, 0.0, 1.0], HAZE),
        update_buffer: BUFFER_STAGE1,
        ..Material::new("Grass", hazed()).mapped(vec![Mapper {
            wrap_u: 3,
            wrap_v: 3,
            ..Mapper::new("racetrack_grass")
        }])
    });
    let stripes = |x: f32, _z: f32| {
        if ((x / STRIPE).floor() as i32).rem_euclid(2) == 0 { [1.0, 1.0, 1.0] } else { [0.82, 0.9, 0.8] }
    };
    for (x0, x1) in [(-GRASS_HALF, -ROAD_HALF - 1.0), (ROAD_HALF + 1.0, GRASS_HALF)] {
        let mut cols = Vec::new();
        let mut x = x0;
        while x < x1 {
            cols.push(x);
            x = (STRIPE * ((x / STRIPE).floor() + 1.0)).min(x1);
        }
        cols.push(x1);
        for w in cols.windows(2) {
            let mid = (w[0] + w[1]) / 2.0;
            b.mesh(
                ground((w[0], w[1]), ROAD_Y - 0.05, 40.0, GRASS_TILE, false, world, |_, z| stripes(mid, z)),
                m,
                "Ground",
            );
        }
    }
    b.uv_track(
        "Grass",
        "TextureCoordinators[0].Translate",
        vec![(1, Curve::linear(&[(0.0, 0.0), (LOOP, -TRAVEL / GRASS_TILE)]))],
    );
    let m = b.material(Material {
        constants: constants([0.0, 0.0, 0.0, 1.0], HAZE),
        update_buffer: BUFFER_STAGE1,
        ..Material::new("Road", hazed()).mapped(vec![Mapper { wrap_v: 0, ..Mapper::new("racetrack_asphalt") }])
    });
    b.mesh(ground((-ROAD_HALF, ROAD_HALF), ROAD_Y, 10.0, ROAD_TILE, true, world, |_, _| [1.0; 3]), m, "Ground");
    b.uv_track(
        "Road",
        "TextureCoordinators[0].Translate",
        vec![(0, Curve::linear(&[(0.0, 0.0), (LOOP, road_scroll)]))],
    );
    let curb = Material {
        constants: constants([0.92, 0.12, 0.1, 1.0], HAZE),
        update_buffer: BUFFER_STAGE1,
        ..Material::new(
            "Curb",
            vec![
                stage([3, 14, 0], [0, 0, 0], CombinerMode::AddMult, Replace),
                Stage { constant_index: 1, ..stage([14, 0, 0], [14, 0, 0], Replace, Replace) },
                Stage { operands: 2 << 8, constant_index: 1, ..stage([13, 15, 0], [14, 0, 0], Interpolate, Replace) },
            ],
        )
        .mapped(vec![Mapper { wrap_u: 3, wrap_v: 3, linear: false, ..Mapper::new("city_checker") }])
    };
    let m = b.material(curb);
    for x in [-ROAD_HALF - 1.0, ROAD_HALF] {
        b.mesh(ground((x, x + 1.0), ROAD_Y + 0.02, 10.0, 2.0, true, world, |_, _| [1.0; 3]), m, "Ground");
    }
    b.uv_track(
        "Curb",
        "TextureCoordinators[0].Translate",
        vec![(0, Curve::linear(&[(0.0, 0.0), (LOOP, -TRAVEL / 2.0)]))],
    );

    // ---- trees: rows of pines down both sides, alpha-tested foliage (ETC1A4) scrolling away with the road, fading into the
    // haze through their vertex colours.
    let leaves = b.material(Material {
        alpha_test: 1 | (6 << 4) | (128 << 8),
        ..Material::new("Leaves", lit()).mapped(vec![Mapper { wrap_v: 0, ..Mapper::new("pino_row") }])
    });
    for (i, x) in [-TREES_X, TREES_X, -TREES_X - 9.0, TREES_X + 9.0].into_iter().enumerate() {
        let mut s = Shape { bone: world, uvs: Some(Vec::new()), colors: Some(Vec::new()), ..Shape::default() };
        let rows = ((NEAR_Z - FAR_Z) / 20.0).round() as u32;
        let (h, shift) = if i < 2 { (11.0, 0.0) } else { (14.0, 0.5) };
        for r in 0..=rows {
            let z = FAR_Z + r as f32 * 20.0;
            let k = haze_alpha(z);
            let c: Vec<f32> = (0..3).map(|j| HAZE[j] + (TREE_TINT[j] - HAZE[j]) * k).collect();
            for (y, v) in [(ROAD_Y, 1.0), (ROAD_Y + h, 0.0)] {
                s.positions.extend_from_slice(&[x, y, z]);
                s.uvs.as_mut().unwrap().extend_from_slice(&[-z / TREE_SPACING + shift, v]);
                s.colors.as_mut().unwrap().extend_from_slice(&[c[0], c[1], c[2], 1.0]);
            }
            if r > 0 {
                let b = 2 * r;
                s.indices.extend_from_slice(&[b - 2, b - 1, b + 1, b - 2, b + 1, b]);
            }
        }
        b.mesh(s, leaves, "Trees");
    }
    b.uv_track(
        "Leaves",
        "TextureCoordinators[0].Translate",
        vec![(0, Curve::linear(&[(0.0, 0.0), (LOOP, -TRAVEL / TREE_SPACING)]))],
    );

    // ---- the kart: SuperTuxKart's Tux kart in a three-quarter view, shading baked into the vertex colours, bobbing.
    let kart_bone = b.bone("Kart", "World", [0.0, 0.0, 0.0]);
    let bob: Vec<(f32, f32)> =
        (0..=16).map(|i| (i as f32 * LOOP / 16.0, if i % 2 == 0 { 0.0 } else { 0.18 })).collect();
    b.track("Kart", 7, &bob);
    let tex_of = |name: &str| match name {
        "engine" => "engine",
        n if n.starts_with("Wheel") => "wheel",
        n if n.starts_with("tux.") => "tux_body",
        _ => "tuxkart",
    };
    let mut kart_materials: BTreeMap<&str, usize> = BTreeMap::new();
    for t in ["tuxkart", "engine", "wheel", "tux_body"] {
        let name = format!("Kart_{t}");
        let m = b.material(Material { culling: 1, ..Material::new(&name, lit()).mapped(vec![Mapper::new(t)]) });
        kart_materials.insert(t, m);
    }
    let origin = |n: &str| kart.origins.get(n).copied().unwrap_or([0.0; 3]);
    let scaled = |p: [f32; 3]| [p[0] * KART, p[1] * KART, p[2] * KART];
    for (name, part) in &kart.parts {
        let positions: Vec<f32> =
            part.positions.chunks(3).flat_map(|p| on_kart([p[0] * KART, p[1] * KART, p[2] * KART])).collect();
        let colors: Vec<f32> = part
            .normals
            .chunks(3)
            .flat_map(|n| {
                let n = yawed([n[0], n[1], n[2]]);
                let s = shade(&n);
                [s, s, s, 1.0]
            })
            .collect();
        let shape = Shape {
            positions,
            indices: part.indices.clone(),
            bone: kart_bone,
            uvs: Some(part.uvs.clone()),
            colors: Some(colors),
            normals: None,
        };
        b.mesh(shape, kart_materials[tex_of(name)], "Kart");
    }
    // Its shadow on the road (L8 on white, multiplied) and the nitro flames (additive, flickering, scrolling).
    let m = b.material(
        Material::new("Shadow", vec![stage([3, 0, 0], [3, 0, 0], Replace, Replace)])
            .mapped(vec![Mapper { wrap_u: 0, wrap_v: 0, ..Mapper::new("tux_shadow") }])
            .blended(MULTIPLY_BLEND, 1),
    );
    let mut shadow = quad([0.0, 0.0, 0.0], 0.95 * KART, 1.45 * KART, [0.0, 1.0, 1.0, 0.0], kart_bone, None);
    for v in shadow.positions.chunks_mut(3) {
        let p = on_kart([v[0], 0.05, -v[1] - 0.05 * KART]);
        v.copy_from_slice(&p);
    }
    b.mesh(shadow, m, "Kart");
    let m = b.material(Material {
        constants: constants([0.5, 0.75, 1.0, 1.0], HAZE),
        ..Material::new("Nitro", vec![stage([3, 14, 0], [3, 14, 0], Modulate, Modulate)])
            .mapped(vec![Mapper::new("nitro-particle")])
            .blended(ADDITIVE_BLEND, 3)
    });
    for n in &kart.nitro {
        let p = scaled(*n);
        b.mesh(kart_quad([p[0], p[1] + 1.6, p[2] - 0.4], 2.6, 4.4, [0.0, 0.0, 1.0, 2.0], kart_bone), m, "Kart");
    }
    b.uv_track("Nitro", "TextureCoordinators[0].Translate", vec![(1, Curve::linear(&[(0.0, 0.0), (LOOP, -8.0)]))]);
    let flicker: Vec<(f32, f32)> = (0..=24).map(|i| (i as f32 * LOOP / 24.0, [1.0, 0.55, 0.85, 0.6][i % 4])).collect();
    b.uv_track(
        "Nitro",
        "MaterialColor.Constant0",
        vec![
            (0, Curve::linear(&flicker.iter().map(|&(f, v)| (f, v * 0.5)).collect::<Vec<_>>())),
            (1, Curve::linear(&flicker)),
        ],
    );
    // Dust behind the rear wheels: the smoke puff (A8 alpha over a constant colour, alpha blended), pulsing on its own clip.
    let m = b.material(Material {
        constants: constants([0.85, 0.82, 0.75, 0.55], HAZE),
        ..Material::new("Dust", vec![stage([14, 0, 0], [3, 14, 0], Replace, Modulate)])
            .mapped(vec![Mapper { wrap_u: 0, wrap_v: 0, ..Mapper::new("smoke") }])
            .blended(ALPHA_BLEND, 1)
    });
    for o in ["WheelRear.L", "WheelRear.R"] {
        let p = scaled(origin(o));
        b.mesh(kart_quad([p[0] * 1.1, 0.9, p[2] - 1.8], 2.4, 2.0, [0.0, 0.0, 1.0, 1.0], kart_bone), m, "Kart");
    }
    b.uv_track(
        "Dust",
        "MaterialColor.Constant0",
        vec![(
            3,
            Curve::linear(
                &(0..=8).map(|i| (i as f32 * LOOP / 8.0, if i % 2 == 0 { 0.6 } else { 0.25 })).collect::<Vec<_>>(),
            ),
        )],
    );

    // ---- the item box: SuperTuxKart's gift box hovering ahead of the kart, tinted through a cycling constant; it drops in,
    // bobs, and at BOX_HIT pops (scale) into a spark burst (LA8 added to a gold constant that fades, additive, growing, turning).
    b.bone("ItemBox", "World", [0.0, BOX_Y, 0.0]);
    let mut drop = vec![(0.0, BOX_Y + 14.0), (16.0, BOX_Y - 0.4), (24.0, BOX_Y + 0.3), (30.0, BOX_Y)];
    drop.extend((1..=8).map(|i| (30.0 + i as f32 * 16.0, BOX_Y + if i % 2 == 0 { 0.0 } else { 0.5 })));
    drop.push((LOOP, BOX_Y));
    b.track("ItemBox", 7, &drop);
    for c in 0..3 {
        b.track(
            "ItemBox",
            c,
            &[(0.0, 0.4), (16.0, 1.0), (BOX_HIT - 8.0, 1.0), (BOX_HIT, 1.35), (BOX_HIT + 4.0, GONE), (LOOP, GONE)],
        );
    }
    let m = b.material(Material {
        constants: constants([1.0, 1.0, 1.0, 1.0], HAZE),
        culling: 1,
        ..Material::new("ItemBox", vec![stage([3, 14, 0], [3, 14, 0], Modulate, Modulate)])
            .mapped(vec![Mapper::new("gift-box")])
    });
    let gift_part = gift.parts.values().next().expect("gift box part");
    let box_bone = b.c.bone_index("ItemBox").unwrap();
    let g = 4.0;
    b.mesh(
        Shape {
            positions: gift_part.positions.chunks(3).flat_map(|v| box_yaw([v[0] * g, v[1] * g, v[2] * g])).collect(),
            indices: gift_part.indices.clone(),
            bone: box_bone,
            uvs: Some(gift_part.uvs.clone()),
            colors: None,
            normals: None,
        },
        m,
        "ItemBox",
    );
    let hue = |phase: f32| -> Vec<(f32, f32)> {
        (0..=12).map(|i| (i as f32 * LOOP / 12.0, 0.65 + 0.35 * (2.0 * PI * (i as f32 / 6.0 + phase)).cos())).collect()
    };
    b.uv_track(
        "ItemBox",
        "MaterialColor.Constant0",
        vec![(0, Curve::linear(&hue(0.0))), (1, Curve::linear(&hue(1.0 / 3.0))), (2, Curve::linear(&hue(2.0 / 3.0)))],
    );
    let spark = b.bone("Spark", "World", [0.0, BOX_Y, 0.0]);
    for c in 0..3 {
        b.track(
            "Spark",
            c,
            &[
                (0.0, GONE),
                (BOX_HIT, GONE),
                (BOX_HIT + 36.0, 1.3),
                (BOX_HIT + 40.0, 1.3),
                (BOX_HIT + 41.0, GONE),
                (LOOP, GONE),
            ],
        );
    }
    let m = b.material(
        Material {
            constants: constants([0.45, 0.32, 0.05, 1.0], HAZE),
            ..Material::new("Spark", vec![stage([3, 14, 0], [3, 0, 0], Add, Replace)])
        }
        .mapped(vec![Mapper { wrap_u: 0, wrap_v: 0, ..Mapper::new("item_spark") }])
        .blended(ADDITIVE_BLEND, 3),
    );
    b.c.materials[m].coords = vec![Coord { matrix_mode: 2, ..Coord::default() }];
    b.mesh(quad([0.0, 0.0, 1.5], 10.0, 10.0, [0.0, 0.0, 1.0, 1.0], spark, None), m, "Spark");
    b.uv_track("Spark", "TextureCoordinators[0].Rotate", vec![(0, Curve::linear(&[(0.0, 0.0), (LOOP, PI)]))]);
    let fade = |v: f32| Curve::linear(&[(0.0, v), (BOX_HIT + 8.0, v), (BOX_HIT + 40.0, 0.0), (LOOP, 0.0)]);
    b.uv_track("Spark", "MaterialColor.Constant0", vec![(0, fade(0.45)), (1, fade(0.32)), (2, fade(0.05))]);
    finish(&mut b.c, sky);
    b.c
}
