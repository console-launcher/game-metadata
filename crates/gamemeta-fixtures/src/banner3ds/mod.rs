// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A writer for complete NW4C CGFX (revision 5) files the 3DS HOME Menu loads as a banner, from a scene described in code:
//! one skeletal model (bones, meshes, materials, shapes), textures, and skeletal, material and visibility animations. The
//! layout follows what the NW4C runtime reads: a DATA block of 15 section dictionaries (Patricia trees), self-relative
//! pointers, a string pool, then an IMAG block holding the vertex, index and texture data. Every value written is computed
//! here; nothing is copied from a real banner. A port of the original implementation's test writer, byte for byte (the
//! committed feature banners in `fixtures/scene/features` are its output and the parity test's target).

pub mod cbmd;
pub mod encode;

use std::collections::{HashMap, VecDeque};

use banner3ds_scene::draw::math;
use banner3ds_scene::pica_textures::Format;
use gamemeta_core::RgbaImage;

/// A texture: name, picture, PICA format.
#[derive(Debug, Clone)]
pub struct Texture {
    pub name: String,
    pub image: RgbaImage,
    pub format: Format,
}

/// A combiner stage as its PICA words; `constant_index` picks the material constant it reads.
#[derive(Debug, Clone, Copy)]
pub struct Stage {
    pub sources: i32,
    pub operands: i32,
    pub combiner: i32,
    pub scale: i32,
    pub constant_index: i32,
}

impl Stage {
    /// An unused stage: replace with the previous stage's output.
    pub const PASS_THROUGH: Stage =
        Stage { sources: 0x0E1F_0E1F, operands: 0, combiner: 0, scale: 0, constant_index: 0 };

    /// A stage of `sources` and `combiner`, plain operands, no scale, constant 0.
    pub fn new(sources: i32, combiner: i32) -> Stage {
        Stage { sources, operands: 0, combiner, scale: 0, constant_index: 0 }
    }
}

/// Combiner source words: colour sources 0-2 then alpha sources 0-2 (PICA source codes: 0 primary colour, 3-5 textures 0-2,
/// 13 previous buffer, 14 constant, 15 previous).
pub fn sources(c: [i32; 3], a: [i32; 3]) -> i32 {
    c[0] | (c[1] << 4) | (c[2] << 8) | (a[0] << 16) | (a[1] << 20) | (a[2] << 24)
}

/// The combiner word of a colour and an alpha mode.
pub fn combiner(color: banner3ds_scene::CombinerMode, alpha: banner3ds_scene::CombinerMode) -> i32 {
    color as i32 | ((alpha as i32) << 16)
}

/// A texture unit: the texture by name, PICA wrap codes (0 clamp to edge, 1 border, 2 repeat, 3 mirror), linear filtering.
#[derive(Debug, Clone)]
pub struct Mapper {
    pub texture: String,
    pub wrap_u: i32,
    pub wrap_v: i32,
    pub linear: bool,
    pub border: i32,
}

impl Mapper {
    /// `texture`, repeating, linear, black border.
    pub fn new(texture: &str) -> Mapper {
        Mapper { texture: texture.into(), wrap_u: 2, wrap_v: 2, linear: true, border: 0xFF00_0000u32 as i32 }
    }
}

/// A texture coordinator: UV set, scale, rotation (radians), translation; matrix mode 0 Maya, 1 Softimage, 2 3ds Max.
#[derive(Debug, Clone, Copy)]
pub struct Coord {
    pub uv_set: i32,
    pub scale: [f32; 2],
    pub rotation: f32,
    pub translation: [f32; 2],
    pub matrix_mode: i32,
}

impl Default for Coord {
    fn default() -> Self {
        Coord { uv_set: 0, scale: [1.0, 1.0], rotation: 0.0, translation: [0.0, 0.0], matrix_mode: 0 }
    }
}

/// A material: combiner stages, constants, texture units and fragment state.
#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    pub stages: Vec<Stage>,
    pub constants: Vec<[f32; 4]>,
    pub mappers: Vec<Mapper>,
    pub coords: Vec<Coord>,
    /// 0 none, 1 blend, 3 logic op.
    pub blend_mode: i32,
    /// The BLEND_FUNC word.
    pub blend_function: i32,
    pub logic_op: i32,
    /// The ALPHA_TEST word: bit 0 enable, bits 4-6 function, bits 8-15 reference.
    pub alpha_test: i32,
    pub depth_test: bool,
    pub depth_write: bool,
    /// NW4C culling: 0 front, 1 back, 3 none.
    pub culling: i32,
    /// Render layer: 0 opaque, 1 translucent, 2 subtractive, 3 additive.
    pub translucency: i32,
    pub buffer_color: i32,
    /// TEXENV_UPDATE_BUFFER bits 8-11 colour, 12-15 alpha (stages 1-4 write the buffer).
    pub update_buffer: i32,
    pub emission: [f32; 4],
    pub ambient: [f32; 4],
    pub diffuse: [f32; 4],
    /// Bit 0 fragment lighting, bit 1 vertex lighting, bit 2 hemisphere lighting.
    pub flags: i32,
}

/// The alpha blend function (src alpha, 1 - src alpha).
pub const ALPHA_BLEND: i32 = 0x7676_0000;
/// The additive blend function (src alpha, one).
pub const ADDITIVE_BLEND: i32 = 0x1616_0000;

impl Material {
    /// `name` with `stages`, no textures, opaque, depth tested and written, no culling.
    pub fn new(name: &str, stages: Vec<Stage>) -> Material {
        Material {
            name: name.into(),
            stages,
            constants: vec![[0.0, 0.0, 0.0, 1.0]; 6],
            mappers: Vec::new(),
            coords: Vec::new(),
            blend_mode: 0,
            blend_function: 0x0101_0000,
            logic_op: 0,
            alpha_test: 0x10,
            depth_test: true,
            depth_write: true,
            culling: 3,
            translucency: 0,
            buffer_color: 0xFF00_0000u32 as i32,
            update_buffer: 0,
            emission: [0.0; 4],
            ambient: [1.0; 4],
            diffuse: [1.0; 4],
            flags: 0,
        }
    }

    /// With texture units `mappers` and one default coordinator each.
    pub fn mapped(mut self, mappers: Vec<Mapper>) -> Material {
        self.coords = vec![Coord::default(); mappers.len()];
        self.mappers = mappers;
        self
    }

    /// Blended with `function`, depth not written, the translucent layer (or `layer`).
    pub fn blended(mut self, function: i32, layer: i32) -> Material {
        self.blend_mode = 1;
        self.blend_function = function;
        self.depth_write = false;
        self.translucency = layer;
        self
    }
}

/// One shape: per-vertex arrays (positions xyz, optional uv, rgba colour 0-1, normal) and triangle indices, drawn by `bone`.
#[derive(Debug, Clone, Default)]
pub struct Shape {
    pub positions: Vec<f32>,
    pub indices: Vec<u32>,
    pub bone: usize,
    pub uvs: Option<Vec<f32>>,
    pub colors: Option<Vec<f32>>,
    pub normals: Option<Vec<f32>>,
}

impl Shape {
    fn count(&self) -> usize {
        self.positions.len() / 3
    }
}

/// A mesh: shape and material indices, its node name, draw priority, initial visibility.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub shape: usize,
    pub material: usize,
    pub node: String,
    pub priority: i32,
    pub visible: bool,
}

/// A bone: parent index (-1 root), scale, Euler XYZ radians, translation, billboard mode (0 off).
#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub parent: i32,
    pub scale: [f32; 3],
    pub rotation: [f32; 3],
    pub translation: [f32; 3],
    pub billboard: i32,
}

impl Bone {
    /// `name` under `parent`, at rest.
    pub fn new(name: &str, parent: i32) -> Bone {
        Bone { name: name.into(), parent, scale: [1.0; 3], rotation: [0.0; 3], translation: [0.0; 3], billboard: 0 }
    }
}

/// A curve's keys as (frame, value), linear floats; `step` holds each key until the next.
#[derive(Debug, Clone)]
pub struct Curve {
    pub keys: Vec<(f32, f32)>,
    pub step: bool,
}

impl Curve {
    /// A linear curve through `keys`.
    pub fn linear(keys: &[(f32, f32)]) -> Curve {
        Curve { keys: keys.to_vec(), step: false }
    }

    /// A stepped curve through `keys`.
    pub fn stepped(keys: &[(f32, f32)]) -> Curve {
        Curve { keys: keys.to_vec(), step: true }
    }
}

/// Bone `bone`'s track: `component` 0-2 scale xyz, 3-5 rotation xyz (radians), 6-8 translation xyz.
#[derive(Debug, Clone)]
pub struct BoneTrack {
    pub bone: String,
    pub component: usize,
    pub curve: Curve,
}

/// A material track: `member` of `material` ("TextureCoordinators[0].Translate", "...Scale", "...Rotate",
/// "MaterialColor.Constant0".."Constant5", "MaterialColor.Diffuse"), a curve per animated axis (0 u/r, 1 v/g, 2 b, 3 a).
#[derive(Debug, Clone)]
pub struct MaterialTrack {
    pub material: String,
    pub member: String,
    pub components: Vec<(usize, Curve)>,
}

impl MaterialTrack {
    fn path(&self) -> String {
        format!("Materials[\"{}\"].{}", self.material, self.member)
    }

    /// NW4C element type: float (rotate), vector2 (scale, translate), RGBA (colours).
    fn kind(&self) -> i32 {
        if self.member.ends_with("Rotate") {
            0
        } else if self.member.starts_with("MaterialColor") {
            6
        } else {
            3
        }
    }

    fn axes(&self) -> usize {
        match self.kind() {
            0 => 1,
            6 => 4,
            _ => 2,
        }
    }

    fn component(&self, k: usize) -> Option<&Curve> {
        self.components.iter().rev().find(|(c, _)| *c == k).map(|(_, c)| c)
    }
}

/// A mesh-node visibility track: the node, a stepped 0/1 curve.
#[derive(Debug, Clone)]
pub struct VisibilityTrack {
    pub node: String,
    pub curve: Curve,
}

/// The clip every track belongs to.
#[derive(Debug, Clone)]
pub struct Animation {
    pub name: String,
    pub frames: f32,
    pub looping: bool,
}

/// A banner scene: build it up, then [`Cgfx::build`].
#[derive(Debug, Clone)]
pub struct Cgfx {
    pub model_name: String,
    pub textures: Vec<Texture>,
    pub materials: Vec<Material>,
    pub shapes: Vec<Shape>,
    pub meshes: Vec<Mesh>,
    pub bones: Vec<Bone>,
    pub bone_tracks: Vec<BoneTrack>,
    pub material_tracks: Vec<MaterialTrack>,
    pub visibility_tracks: Vec<VisibilityTrack>,
    pub animation: Animation,
}

impl Default for Cgfx {
    fn default() -> Self {
        Cgfx {
            model_name: "COMMON".into(),
            textures: Vec::new(),
            materials: Vec::new(),
            shapes: Vec::new(),
            meshes: Vec::new(),
            bones: vec![Bone::new("Root", -1)],
            bone_tracks: Vec::new(),
            material_tracks: Vec::new(),
            visibility_tracks: Vec::new(),
            animation: Animation { name: "COMMON".into(), frames: 0.0, looping: true },
        }
    }
}

impl Cgfx {
    /// The CGFX bytes.
    pub fn build(&self) -> Vec<u8> {
        Out::new(self).write()
    }

    /// Index of the bone named `name`.
    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }
}

const SECTIONS: usize = 15;

type Job<'a> = Box<dyn FnOnce(&mut Out<'a>) + 'a>;
type Make<'a> = Box<dyn FnOnce(&mut Out<'a>) -> usize + 'a>;

struct Out<'a> {
    s: &'a Cgfx,
    b: Vec<u8>,
    top: usize,
    strings: Vec<(usize, String)>,
    imag: Vec<(usize, Vec<u8>, usize)>,
    queue: VecDeque<Job<'a>>,
    bone_ats: Option<Vec<usize>>,
}

struct Element {
    kind: i32,
    path: String,
    target: Option<String>,
    sub: Option<String>,
    words: [i32; 5],
    name: Option<String>,
    last: i32,
}

/// Java's `String.hashCode`, which the original implementation seeds its material state hashes with.
fn java_hash(s: &str) -> i32 {
    s.encode_utf16().fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
}

impl<'a> Out<'a> {
    fn new(s: &'a Cgfx) -> Self {
        Out {
            s,
            b: vec![0; 8 << 20],
            top: 0x14 + 8 + 8 * SECTIONS,
            strings: Vec::new(),
            imag: Vec::new(),
            queue: VecDeque::new(),
            bone_ats: None,
        }
    }

    fn alloc(&mut self, size: usize, align: usize) -> usize {
        self.top = self.top.div_ceil(align) * align;
        let at = self.top;
        self.top += size;
        if self.top > self.b.len() {
            self.b.resize(self.top.next_power_of_two(), 0);
        }
        at
    }

    fn a4(&mut self, size: usize) -> usize {
        self.alloc(size, 4)
    }

    fn i32(&mut self, at: usize, v: i32) {
        self.b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn get_i32(&self, at: usize) -> i32 {
        i32::from_le_bytes(self.b[at..at + 4].try_into().unwrap())
    }

    fn u32(&mut self, at: usize, v: u32) {
        self.i32(at, v as i32);
    }

    fn f32(&mut self, at: usize, v: f32) {
        self.b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn floats(&mut self, at: usize, v: &[f32]) {
        for (i, x) in v.iter().enumerate() {
            self.f32(at + 4 * i, *x);
        }
    }

    fn ptr(&mut self, at: usize, target: usize) {
        self.i32(at, if target == 0 { 0 } else { target as i32 - at as i32 });
    }

    fn str(&mut self, at: usize, value: &str) {
        self.strings.push((at, value.into()));
    }

    fn data(&mut self, at: usize, bytes: Vec<u8>, align: usize) {
        self.imag.push((at, bytes, align));
    }

    fn tag(&mut self, at: usize, t: &str) {
        self.b[at..at + t.len()].copy_from_slice(t.as_bytes());
    }

    /// Queues the child's writer and points `at` at it once placed (children after parents, breadth first).
    fn link(&mut self, at: usize, make: Make<'a>) {
        self.queue.push_back(Box::new(move |o: &mut Out<'a>| {
            let t = make(o);
            o.ptr(at, t);
        }));
    }

    fn drain(&mut self) {
        while let Some(job) = self.queue.pop_front() {
            job(self);
        }
    }

    /// A (count, pointer) list of object pointers at `pair_at`.
    fn list(&mut self, pair_at: usize, items: Vec<Make<'a>>) {
        self.i32(pair_at, items.len() as i32);
        if items.is_empty() {
            return;
        }
        self.link(
            pair_at + 4,
            Box::new(move |o: &mut Out<'a>| {
                let arr = o.a4(4 * items.len());
                for (i, make) in items.into_iter().enumerate() {
                    o.link(arr + 4 * i, make);
                }
                arr
            }),
        );
    }

    /// A (count, pointer) dictionary at `pair_at`: a DICT Patricia tree of `entries`.
    fn dict(&mut self, pair_at: usize, entries: Vec<(String, Make<'a>)>) {
        self.i32(pair_at, entries.len() as i32);
        if entries.is_empty() {
            return;
        }
        self.link(pair_at + 4, Box::new(move |o: &mut Out<'a>| o.dict_object(entries)));
    }

    fn dict_object(&mut self, entries: Vec<(String, Make<'a>)>) -> usize {
        let names: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
        let tree = patricia::build(&names);
        let n = entries.len();
        let at = self.a4(12 + 16 * (n + 1));
        self.tag(at, "DICT");
        self.i32(at + 4, (12 + 16 * (n + 1)) as i32);
        self.i32(at + 8, n as i32);
        let node0 = at + 12;
        self.i32(node0, tree[0].ref_bit);
        self.b[node0 + 4..node0 + 6].copy_from_slice(&(tree[0].left as u16).to_le_bytes());
        self.b[node0 + 6..node0 + 8].copy_from_slice(&(tree[0].right as u16).to_le_bytes());
        for (i, (name, make)) in entries.into_iter().enumerate() {
            let node = at + 12 + 16 * (i + 1);
            let t = &tree[i + 1];
            self.i32(node, t.ref_bit);
            self.b[node + 4..node + 6].copy_from_slice(&(t.left as u16).to_le_bytes());
            self.b[node + 6..node + 8].copy_from_slice(&(t.right as u16).to_le_bytes());
            self.str(node + 8, &name);
            self.link(node + 12, make);
        }
        at
    }

    fn write(mut self) -> Vec<u8> {
        let s = self.s;
        self.dict(0x1C, vec![(s.model_name.clone(), Box::new(|o: &mut Out<'a>| o.model()))]);
        let textures: Vec<(String, Make<'a>)> = s
            .textures
            .iter()
            .map(|t| (t.name.clone(), Box::new(move |o: &mut Out<'a>| o.texture(t)) as Make<'a>))
            .collect();
        self.dict(0x1C + 8, textures);
        if !s.bone_tracks.is_empty() {
            self.dict(
                0x1C + 8 * 9,
                vec![(s.animation.name.clone(), Box::new(|o: &mut Out<'a>| o.skeletal_animation()))],
            );
        }
        if !s.material_tracks.is_empty() {
            self.dict(
                0x1C + 8 * 10,
                vec![(s.animation.name.clone(), Box::new(|o: &mut Out<'a>| o.material_animation()))],
            );
        }
        if !s.visibility_tracks.is_empty() {
            self.dict(
                0x1C + 8 * 11,
                vec![(s.animation.name.clone(), Box::new(|o: &mut Out<'a>| o.visibility_animation()))],
            );
        }
        self.drain();
        // The string pool closes DATA; IMAG follows with the GPU data at 0x80 boundaries.
        let mut pool: HashMap<String, usize> = HashMap::new();
        for (at, value) in core::mem::take(&mut self.strings) {
            let p = match pool.get(&value) {
                Some(&p) => p,
                None => {
                    let a = self.alloc(value.len() + 1, 1);
                    self.b[a..a + value.len()].copy_from_slice(value.as_bytes());
                    pool.insert(value, a);
                    a
                }
            };
            self.ptr(at, p);
        }
        let imag_at = self.alloc(0, 8);
        self.top = imag_at + 8;
        for (at, bytes, align) in core::mem::take(&mut self.imag) {
            let d = self.alloc(bytes.len(), align);
            self.b[d..d + bytes.len()].copy_from_slice(&bytes);
            self.ptr(at, d);
        }
        let end = self.alloc(0, 0x80);
        self.tag(imag_at, "IMAG");
        self.i32(imag_at + 4, (end - imag_at) as i32);
        self.tag(0, "CGFX");
        self.b[4..6].copy_from_slice(&0xFEFFu16.to_le_bytes());
        self.b[6..8].copy_from_slice(&0x14u16.to_le_bytes());
        self.i32(8, 0x0500_0000);
        self.i32(0xC, end as i32);
        self.i32(0x10, 2);
        self.tag(0x14, "DATA");
        self.i32(0x18, (imag_at - 0x14) as i32);
        self.b.truncate(end);
        self.b
    }

    /// Mesh node names in first-use order.
    fn nodes(&self) -> Vec<&'a str> {
        let mut out: Vec<&'a str> = Vec::new();
        for m in &self.s.meshes {
            if !out.contains(&m.node.as_str()) {
                out.push(&m.node);
            }
        }
        out
    }

    // ---- model: CMDL skeletal (0x40000092)
    fn model(&mut self) -> usize {
        let s = self.s;
        let at = self.a4(0xE8);
        self.u32(at, 0x4000_0092);
        self.tag(at + 4, "CMDL");
        self.i32(at + 8, 0x0700_0000);
        self.str(at + 0xC, &s.model_name);
        self.i32(at + 0x18, 1);
        self.i32(at + 0x1C, 1);
        self.floats(at + 0x30, &[1.0, 1.0, 1.0]);
        let identity = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        self.floats(at + 0x54, &identity);
        self.floats(at + 0x84, &identity);
        self.i32(at + 0xD4, 0x101);
        let groups = self.anim_groups();
        self.dict(at + 0x28, groups);
        let meshes: Vec<Make<'a>> =
            s.meshes.iter().map(|m| Box::new(move |o: &mut Out<'a>| o.mesh(m, at)) as Make<'a>).collect();
        self.list(at + 0xB4, meshes);
        let materials: Vec<(String, Make<'a>)> = s
            .materials
            .iter()
            .map(|m| (m.name.clone(), Box::new(move |o: &mut Out<'a>| o.material(m)) as Make<'a>))
            .collect();
        self.dict(at + 0xBC, materials);
        let shapes: Vec<Make<'a>> =
            s.shapes.iter().map(|sh| Box::new(move |o: &mut Out<'a>| o.shape(sh)) as Make<'a>).collect();
        self.list(at + 0xC4, shapes);
        if !s.visibility_tracks.is_empty() {
            let entries: Vec<(String, Make<'a>)> = self
                .nodes()
                .into_iter()
                .map(|n| {
                    let visible = s.meshes.iter().find(|m| m.node == n).is_some_and(|m| m.visible);
                    let make: Make<'a> = Box::new(move |o: &mut Out<'a>| {
                        let v = o.a4(8);
                        o.str(v, n);
                        o.i32(v + 4, i32::from(visible));
                        v
                    });
                    (n.to_string(), make)
                })
                .collect();
            self.dict(at + 0xCC, entries);
        }
        self.link(at + 0xE0, Box::new(|o: &mut Out<'a>| o.skeleton()));
        at
    }

    // ---- animation groups: the members a clip may drive, which HOME binds clips through.
    fn anim_groups(&mut self) -> Vec<(String, Make<'a>)> {
        let s = self.s;
        let skeletal: Vec<Element> = s
            .bones
            .iter()
            .map(|b| Element {
                kind: 0x4000_0000,
                path: b.name.clone(),
                target: Some(b.name.clone()),
                sub: None,
                words: [0; 5],
                name: Some(b.name.clone()),
                last: 0,
            })
            .collect();
        let material: Vec<Element> = s
            .material_tracks
            .iter()
            .map(|t| {
                if let Some((unit, what)) = coordinator_member(&t.member) {
                    let (offset, size, op, index) = match what {
                        "Scale" => (0x10, 8, 1, 0),
                        "Rotate" => (0x18, 4, 2, 1),
                        _ => (0x1C, 8, 1, 2),
                    };
                    Element {
                        kind: 0x8000_0000u32 as i32,
                        path: t.path(),
                        target: Some(t.material.clone()),
                        sub: Some(format!("TextureCoordinators[{unit}]")),
                        words: [offset, size, op, 5, index],
                        name: Some(t.material.clone()),
                        last: 0,
                    }
                } else {
                    const COLORS: [&str; 11] = [
                        "Emission",
                        "Ambient",
                        "Diffuse",
                        "Specular0",
                        "Specular1",
                        "Constant0",
                        "Constant1",
                        "Constant2",
                        "Constant3",
                        "Constant4",
                        "Constant5",
                    ];
                    let m = t.member.strip_prefix("MaterialColor.").unwrap_or(&t.member);
                    let k = COLORS.iter().position(|c| *c == m).unwrap_or_else(|| panic!("unknown member {}", t.member))
                        as i32;
                    Element {
                        kind: 0x0800_0000,
                        path: t.path(),
                        target: Some(t.material.clone()),
                        sub: Some("MaterialColor".into()),
                        words: [0x10 * k, 0x10, 0, 1, k],
                        name: Some(t.material.clone()),
                        last: 1,
                    }
                }
            })
            .collect();
        let visibility: Vec<Element> = s
            .visibility_tracks
            .iter()
            .map(|t| Element {
                kind: 0x0008_0000,
                path: format!("MeshNodeVisibilities[\"{}\"].IsVisible", t.node),
                target: None,
                sub: None,
                words: [4, 1, 0, 8, 0],
                name: Some(t.node.clone()),
                last: 8,
            })
            .collect();
        fn group<'a>(
            name: &'static str,
            flags: i32,
            kind: i32,
            ops: &'static [i32],
            timing: i32,
            elements: Vec<Element>,
        ) -> (String, Make<'a>) {
            let make: Make<'a> = Box::new(move |o: &mut Out<'a>| {
                let at = o.a4(0x24 + 4 * ops.len());
                o.u32(at, 0x8000_0000);
                o.i32(at + 4, flags);
                o.str(at + 8, name);
                o.i32(at + 0xC, kind);
                o.i32(at + 0x18, ops.len() as i32);
                o.ptr(at + 0x1C, at + 0x24);
                o.i32(at + 0x20, timing);
                for (i, op) in ops.iter().enumerate() {
                    o.i32(at + 0x24 + 4 * i, *op);
                }
                let entries: Vec<(String, Make<'a>)> = elements
                    .into_iter()
                    .map(|e| {
                        let path = e.path.clone();
                        let make: Make<'a> = Box::new(move |o: &mut Out<'a>| {
                            let el = o.a4(0x30);
                            o.i32(el, e.kind);
                            o.str(el + 4, &e.path);
                            if let Some(t) = &e.target {
                                o.str(el + 8, t);
                            }
                            if let Some(sb) = &e.sub {
                                o.str(el + 0xC, sb);
                            }
                            for (i, w) in e.words.iter().enumerate() {
                                o.i32(el + 0x10 + 4 * i, *w);
                            }
                            if let Some(n) = &e.name {
                                o.str(el + 0x28, n);
                            }
                            o.i32(el + 0x2C, e.last);
                            el
                        });
                        (path, make)
                    })
                    .collect();
                o.dict(at + 0x10, entries);
                at
            });
            (name.into(), make)
        }
        vec![
            group("SkeletalAnimation", 1, 1, &[8], 1, skeletal),
            group("VisibilityAnimation", 0, 3, &[0], 0, visibility),
            // Blend operations by value kind: 0 colours (RGBA), 1 vector2 (scale, translate), 2 float (rotate).
            group("MaterialAnimation", 0, 2, &[3, 5, 2], 1, material),
        ]
    }

    fn mesh(&mut self, m: &'a Mesh, model: usize) -> usize {
        let at = self.a4(0x80);
        self.i32(at, 0x0100_0000);
        self.tag(at + 4, "SOBJ");
        self.str(at + 0xC, "");
        self.i32(at + 0x18, m.shape as i32);
        self.i32(at + 0x1C, m.material as i32);
        self.ptr(at + 0x20, model);
        self.b[at + 0x24] = u8::from(m.visible);
        self.b[at + 0x25] = m.priority as u8;
        let node_index = if self.s.visibility_tracks.is_empty() {
            -1
        } else {
            self.nodes().iter().position(|n| *n == m.node).map_or(-1, |i| i as i32)
        };
        self.b[at + 0x26..at + 0x28].copy_from_slice(&(node_index as i16).to_le_bytes());
        self.str(at + 0x70, &m.node);
        at
    }

    fn skeleton(&mut self) -> usize {
        let s = self.s;
        let at = self.a4(0x2C);
        self.i32(at, 0x0200_0000);
        self.tag(at + 4, "SOBJ");
        self.str(at + 0xC, "");
        self.i32(at + 0x28, 2);
        let entries: Vec<(String, Make<'a>)> = s
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| (b.name.clone(), Box::new(move |o: &mut Out<'a>| o.bone_at(i)) as Make<'a>))
            .collect();
        self.dict(at + 0x18, entries);
        self.link(at + 0x20, Box::new(|o: &mut Out<'a>| o.bone_at(0)));
        at
    }

    /// Bone `i`'s address; the first call writes every bone.
    fn bone_at(&mut self, i: usize) -> usize {
        if self.bone_ats.is_none() {
            let ats = self.write_bones();
            self.bone_ats = Some(ats);
        }
        self.bone_ats.as_ref().unwrap()[i]
    }

    fn write_bones(&mut self) -> Vec<usize> {
        let s = self.s;
        let worlds = bone_worlds(&s.bones);
        let ats: Vec<usize> = s.bones.iter().map(|_| self.a4(0xE0)).collect();
        for (i, bone) in s.bones.iter().enumerate() {
            let p = ats[i];
            self.str(p, &bone.name);
            let children: Vec<usize> = (0..s.bones.len()).filter(|&c| s.bones[c].parent == i as i32).collect();
            let siblings: Vec<usize> =
                (0..s.bones.len()).filter(|&c| s.bones[c].parent == bone.parent && bone.parent >= 0).collect();
            let zero_t = bone.translation.iter().all(|&v| v == 0.0);
            let zero_r = bone.rotation.iter().all(|&v| v == 0.0);
            let unit_s = bone.scale.iter().all(|&v| v == 1.0);
            let mut flags = 0x80 | 0x100;
            if zero_t && zero_r && unit_s {
                flags |= 1;
            }
            if zero_t {
                flags |= 2;
            }
            if zero_r {
                flags |= 4;
            }
            if unit_s {
                flags |= 8;
            }
            if bone.scale[0] == bone.scale[1] && bone.scale[1] == bone.scale[2] {
                flags |= 0x10;
            }
            if s.shapes.iter().any(|sh| sh.bone == i) {
                flags |= 0x40;
            }
            self.i32(p + 4, flags);
            self.i32(p + 8, i as i32);
            self.i32(p + 0xC, bone.parent);
            if bone.parent >= 0 {
                self.ptr(p + 0x10, ats[bone.parent as usize]);
            }
            if let Some(&c) = children.first() {
                self.ptr(p + 0x14, ats[c]);
            }
            if let Some(k) = siblings.iter().position(|&x| x == i) {
                if k > 0 {
                    self.ptr(p + 0x18, ats[siblings[k - 1]]);
                }
                if k + 1 < siblings.len() {
                    self.ptr(p + 0x1C, ats[siblings[k + 1]]);
                }
            }
            self.floats(p + 0x20, &bone.scale);
            self.floats(p + 0x2C, &bone.rotation);
            self.floats(p + 0x38, &bone.translation);
            self.floats(p + 0x44, &math::local(&bone.scale, &bone.rotation, &bone.translation));
            self.floats(p + 0x74, &worlds[i]);
            self.floats(p + 0xA4, &math::invert34(&worlds[i]));
            self.i32(p + 0xD4, bone.billboard);
        }
        ats
    }

    // ---- shape: SOBJ 0x10000001, one sub-mesh drawn by one bone, one interleaved buffer
    fn shape(&mut self, sh: &'a Shape) -> usize {
        let at = self.a4(0x44);
        self.i32(at, 0x1000_0001);
        self.tag(at + 4, "SOBJ");
        self.str(at + 0xC, "");
        let (mut min, mut max) = ([f32::MAX; 3], [-f32::MAX; 3]);
        for v in 0..sh.count() {
            for c in 0..3 {
                min[c] = min[c].min(sh.positions[v * 3 + c]);
                max[c] = max[c].max(sh.positions[v * 3 + c]);
            }
        }
        let obb = self.a4(0x40);
        self.u32(obb, 0x8000_0000);
        self.floats(obb + 4, &core::array::from_fn::<f32, 3, _>(|i| (min[i] + max[i]) / 2.0));
        self.floats(obb + 0x10, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        self.floats(obb + 0x34, &core::array::from_fn::<f32, 3, _>(|i| max[i] - min[i]));
        self.ptr(at + 0x1C, obb);
        // Sub-mesh: bone palette (one bone), skinning 0 (none: the bone's world draws the mesh), one face.
        self.list(at + 0x2C, vec![Box::new(move |o: &mut Out<'a>| o.sub_mesh(sh))]);
        self.list(at + 0x38, vec![Box::new(move |o: &mut Out<'a>| o.interleaved(sh))]);
        at
    }

    fn sub_mesh(&mut self, sh: &'a Shape) -> usize {
        let sub = self.a4(0x14);
        self.i32(sub, 1);
        let bone = sh.bone as i32;
        self.link(
            sub + 4,
            Box::new(move |o: &mut Out<'a>| {
                let p = o.a4(4);
                o.i32(p, bone);
                p
            }),
        );
        self.i32(sub + 8, 0);
        self.list(sub + 0xC, vec![Box::new(move |o: &mut Out<'a>| o.face(sh))]);
        sub
    }

    fn face(&mut self, sh: &'a Shape) -> usize {
        let face = self.a4(0x10);
        self.list(
            face,
            vec![Box::new(move |o: &mut Out<'a>| {
                let desc = o.a4(0x2C);
                let wide = sh.count() > 255;
                let idx: Vec<u8> = if wide {
                    sh.indices.iter().flat_map(|&v| (v as u16).to_le_bytes()).collect()
                } else {
                    sh.indices.iter().map(|&v| v as u8).collect()
                };
                o.i32(desc, if wide { 0x1403 } else { 0x1401 });
                o.i32(desc + 4, 0x100);
                o.i32(desc + 8, idx.len() as i32);
                o.data(desc + 0xC, idx, 0x20);
                desc
            })],
        );
        self.i32(face + 8, 1);
        self.link(face + 0xC, Box::new(|o: &mut Out<'a>| o.a4(4)));
        face
    }

    fn interleaved(&mut self, sh: &'a Shape) -> usize {
        let mut attrs: Vec<(i32, usize, &[f32])> = vec![(0, 3, &sh.positions)];
        if let Some(n) = &sh.normals {
            attrs.push((1, 3, n));
        }
        if let Some(c) = &sh.colors {
            attrs.push((3, 4, c));
        }
        if let Some(u) = &sh.uvs {
            attrs.push((4, 2, u));
        }
        let stride: usize = attrs.iter().map(|a| a.1 * 4).sum();
        let mut bytes = Vec::with_capacity(stride * sh.count());
        for v in 0..sh.count() {
            for (_, n, values) in &attrs {
                for c in 0..*n {
                    bytes.extend_from_slice(&values[v * n + c].to_le_bytes());
                }
            }
        }
        let at = self.a4(0x30);
        self.i32(at, 0x4000_0002);
        self.i32(at + 4, 0x15);
        self.i32(at + 8, 2);
        self.i32(at + 0x14, (stride * sh.count()) as i32);
        self.data(at + 0x18, bytes, 0x20);
        self.i32(at + 0x24, stride as i32);
        let mut offset = 0;
        let items: Vec<Make<'a>> = attrs
            .iter()
            .map(|&(usage, n, _)| {
                let off = offset;
                offset += n * 4;
                Box::new(move |o: &mut Out<'a>| {
                    let p = o.a4(0x34);
                    o.i32(p, 0x4000_0001);
                    o.i32(p + 4, usage);
                    o.i32(p + 0x24, 0x1406);
                    o.i32(p + 0x28, n as i32);
                    o.f32(p + 0x2C, 1.0);
                    o.i32(p + 0x30, off as i32);
                    p
                }) as Make<'a>
            })
            .collect();
        self.list(at + 0x28, items);
        at
    }

    // ---- material: MTOB 0x08000000
    fn material(&mut self, m: &'a Material) -> usize {
        let at = self.a4(0x2D8);
        // 0x2A0: thirteen state hashes, which the runtime compares to skip re-sending unchanged state.
        for k in 0..13 {
            let h = java_hash(&m.name).wrapping_mul(31).wrapping_add(k).wrapping_mul(-0x61c8_8647);
            self.i32(at + 0x2A0 + 4 * k as usize, h);
        }
        self.i32(at, 0x0800_0000);
        self.tag(at + 4, "MTOB");
        self.i32(at + 8, 0x0600_0100);
        self.str(at + 0xC, &m.name);
        self.i32(at + 0x18, m.flags);
        self.i32(at + 0x20, m.translucency);
        let mut colors = vec![m.emission, m.ambient, m.diffuse, [0.0; 4], [0.0; 4]];
        colors.extend((0..6).map(|k| m.constants.get(k).copied().unwrap_or([0.0, 0.0, 0.0, 1.0])));
        for (k, c) in colors.iter().enumerate() {
            self.floats(at + 0x24 + 16 * k, c);
            self.i32(at + 0xD4 + 4 * k, packed(c));
        }
        self.i32(at + 0x108, m.culling);
        self.i32(
            at + 0x110,
            match m.culling {
                0 => 1,
                1 => 2,
                _ => 0,
            },
        );
        self.i32(at + 0x114, 0x0001_0040);
        self.i32(at + 0x118, i32::from(m.depth_test) | if m.depth_write { 2 } else { 0 });
        self.i32(at + 0x11C, if m.depth_test { 0x41 } else { 0x10 });
        self.i32(at + 0x120, 0x0001_0107);
        self.i32(at + 0x124, 0x0300_0000);
        self.i32(at + 0x128, 0x0008_0126);
        self.i32(at + 0x12C, m.blend_mode);
        self.f32(at + 0x13C, 1.0);
        self.i32(at + 0x140, if m.blend_mode == 3 { 0x00E4_0000 } else { 0x00E4_0100 });
        self.u32(at + 0x144, 0x803F_0100);
        self.i32(at + 0x148, m.blend_function);
        self.i32(at + 0x14C, m.logic_op);
        self.u32(at + 0x150, 0xFF00_0000);
        self.u32(at + 0x158, 0xFF00_0000);
        self.i32(at + 0x15C, 0x000D_0105);
        self.i32(at + 0x164, 0x000F_0106);
        self.i32(at + 0x168, m.coords.len() as i32);
        for k in 0..3 {
            let c = at + 0x16C + 0x58 * k;
            let coord = m.coords.get(k);
            if let Some(coord) = coord {
                self.i32(c, coord.uv_set);
                self.i32(c + 0xC, coord.matrix_mode);
                self.floats(c + 0x10, &coord.scale);
                self.f32(c + 0x18, coord.rotation);
                self.floats(c + 0x1C, &coord.translation);
            }
            self.floats(c + 0x28, &coord_matrix(&coord.copied().unwrap_or_default()));
        }
        for (k, mp) in m.mappers.iter().enumerate() {
            let p = self.mapper(mp, k);
            self.ptr(at + 0x274 + 4 * k, p);
        }
        let shader = self.a4(0x20);
        self.u32(shader, 0x8000_0001);
        self.tag(shader + 4, "SHDR");
        self.i32(shader + 8, 0x0500_0000);
        self.str(shader + 0xC, "");
        self.str(shader + 0x18, "DefaultShader");
        self.ptr(at + 0x284, shader);
        let f = self.fragment(m);
        self.ptr(at + 0x288, f);
        at
    }

    fn mapper(&mut self, mp: &'a Mapper, unit: usize) -> usize {
        let at = self.a4(0x8C);
        self.u32(at, 0x8000_0000);
        let base = [0x80u32, 0x90, 0x98][unit];
        let kind = [0x8Ei32, 0x96, 0x9E][unit];
        self.i32(at + 0x14, 0x0001_0000 | kind);
        self.i32(at + 0x18, mp.border);
        self.u32(at + 0x1C, 0x809F_0000 | (base + 1));
        let filter = if mp.linear { 6 } else { 0 }; // mag and min linear
        self.i32(at + 0x24, filter | (mp.wrap_v << 8) | (mp.wrap_u << 12));
        self.i32(at + 0x48, 0x38);
        let r = at + 0x4C;
        self.i32(r, 0x2000_0004);
        self.tag(r + 4, "TXOB");
        self.i32(r + 8, 0x0500_0000);
        self.str(r + 0xC, "");
        self.str(r + 0x18, &mp.texture);
        self.ptr(at + 8, r);
        let sampler = at + 0x6C;
        self.u32(sampler, 0x8000_0000);
        self.ptr(sampler + 4, at);
        self.i32(sampler + 8, i32::from(mp.linear));
        self.f32(sampler + 0x18, 1.0);
        self.ptr(at + 0xC, sampler);
        at
    }

    fn fragment(&mut self, m: &'a Material) -> usize {
        let at = self.a4(0xF4);
        self.f32(at + 0xC, 1.0);
        let table = self.a4(0x18);
        self.ptr(at + 0x28, table);
        let regs = [0xC0u32, 0xC8, 0xD0, 0xD8, 0xF0, 0xF8];
        for (k, reg) in regs.iter().enumerate() {
            let st = m.stages.get(k).copied().unwrap_or(Stage::PASS_THROUGH);
            let te = at + 0x2C + 0x1C * k;
            self.i32(te, st.constant_index);
            self.i32(te + 4, st.sources);
            self.u32(te + 8, 0x804F_0000 | reg);
            self.i32(te + 0xC, st.operands);
            self.i32(te + 0x10, st.combiner);
            self.u32(te + 0x14, 0xFF00_0000);
            self.i32(te + 0x18, st.scale);
        }
        self.i32(at + 0xD4, m.alpha_test);
        self.i32(at + 0xD8, 0x000F_0104);
        self.i32(at + 0xDC, m.buffer_color);
        self.i32(at + 0xE0, 0x000F_00FD);
        self.i32(at + 0xE4, m.update_buffer);
        self.i32(at + 0xE8, 0x0002_00E0);
        self.i32(at + 0xEC, 0x400);
        self.i32(at + 0xF0, 0x0002_01C3);
        at
    }

    // ---- texture: TXOB image 0x20000011, its pixels in IMAG
    fn texture(&mut self, t: &'a Texture) -> usize {
        let at = self.a4(0x5C);
        let f = t.format;
        self.i32(at, 0x2000_0011);
        self.tag(at + 4, "TXOB");
        self.i32(at + 8, 0x0500_0000);
        self.str(at + 0xC, &t.name);
        self.i32(at + 0x18, t.image.height as i32);
        self.i32(at + 0x1C, t.image.width as i32);
        let (gl_format, gl_type) = gl_enums(f);
        self.i32(at + 0x20, gl_format);
        self.i32(at + 0x24, gl_type);
        self.i32(at + 0x28, 1);
        self.i32(at + 0x34, f as i32);
        let image = at + 0x3C;
        let pixels = encode::encode(&t.image, f);
        self.i32(image, t.image.height as i32);
        self.i32(image + 4, t.image.width as i32);
        self.i32(image + 8, pixels.len() as i32);
        self.data(image + 0xC, pixels, 0x80);
        self.i32(image + 0x14, f.bits_per_pixel() as i32);
        self.ptr(at + 0x38, image);
        at
    }

    // ---- animations (CANM, revision 5).
    fn animation(&mut self, target_group: &str, members: Vec<(String, Make<'a>)>) -> usize {
        let a = &self.s.animation;
        let at = self.a4(0x28);
        self.tag(at, "CANM");
        self.i32(at + 4, 0x0500_0000);
        self.str(at + 8, &a.name);
        self.str(at + 0xC, target_group);
        self.i32(at + 0x10, i32::from(a.looping));
        self.f32(at + 0x14, a.frames);
        self.dict(at + 0x18, members);
        at
    }

    /// A float curve: start, end, format (0xC4 linear, 0xC0 stepped), key count, 1/span, keys from the start.
    fn curve(&mut self, c: &Curve) -> usize {
        let (start, end) = (c.keys[0].0, c.keys[c.keys.len() - 1].0);
        let at = self.a4(0x14 + 8 * c.keys.len());
        self.f32(at, start);
        self.f32(at + 4, end);
        self.i32(at + 8, if c.step { 0xC0 } else { 0xC4 });
        self.i32(at + 0xC, c.keys.len() as i32);
        self.f32(at + 0x10, if end > start { 1.0 / (end - start) } else { 0.0 });
        for (k, (frame, value)) in c.keys.iter().enumerate() {
            self.f32(at + 0x14 + 8 * k, frame - start);
            self.f32(at + 0x18 + 8 * k, *value);
        }
        at
    }

    fn group(&mut self, c: &'a Curve) -> usize {
        let at = self.a4(0x18);
        self.f32(at, c.keys[0].0);
        self.f32(at + 4, c.keys[c.keys.len() - 1].0);
        self.i32(at + 0xC, 4);
        self.i32(at + 0x10, 1);
        self.link(at + 0x14, Box::new(move |o: &mut Out<'a>| o.curve(c)));
        at
    }

    fn skeletal_animation(&mut self) -> usize {
        let s = self.s;
        let mut order: Vec<&'a str> = Vec::new();
        for t in &s.bone_tracks {
            if !order.contains(&t.bone.as_str()) {
                order.push(&t.bone);
            }
        }
        let members: Vec<(String, Make<'a>)> = order
            .into_iter()
            .map(|bone| {
                let make: Make<'a> = Box::new(move |o: &mut Out<'a>| {
                    // Transform (type 5): slots scale xyz, rotation xyz, an unused one, translation xyz; unanimated ones
                    // hold the rest pose.
                    let b = s.bones.iter().find(|b| b.name == bone).expect("track of an unknown bone");
                    let rest: Vec<f32> = b.scale.iter().chain(&b.rotation).chain(&b.translation).copied().collect();
                    let e = o.a4(0x14 + 4 * 10);
                    let mut flags = 1 << (16 + 6);
                    o.str(e + 4, bone);
                    o.str(e + 8, bone);
                    o.i32(e + 0x10, 5);
                    for (k, rest_k) in rest.iter().enumerate() {
                        let slot = if k >= 6 { k + 1 } else { k };
                        let t = s.bone_tracks.iter().rev().find(|t| t.bone == bone && t.component == k);
                        match t {
                            Some(t) => o.link(e + 0x14 + 4 * slot, Box::new(move |o: &mut Out<'a>| o.group(&t.curve))),
                            None => {
                                flags |= 1 << (6 + slot);
                                o.f32(e + 0x14 + 4 * slot, *rest_k);
                            }
                        }
                    }
                    o.i32(e, flags);
                    e
                });
                (bone.to_string(), make)
            })
            .collect();
        self.animation("SkeletalAnimation", members)
    }

    fn material_animation(&mut self) -> usize {
        let s = self.s;
        let members: Vec<(String, Make<'a>)> = s
            .material_tracks
            .iter()
            .map(|t| {
                let make: Make<'a> = Box::new(move |o: &mut Out<'a>| {
                    let axes = t.axes();
                    let e = o.a4(0x14 + 4 * axes);
                    let mut flags = 0;
                    o.str(e + 4, &t.path());
                    o.str(e + 8, &t.material);
                    let member = t.member.rsplit_once('.').map_or(t.member.as_str(), |(a, _)| a);
                    o.str(e + 0xC, member);
                    o.i32(e + 0x10, t.kind());
                    for k in 0..axes {
                        match t.component(k) {
                            Some(c) => o.link(e + 0x14 + 4 * k, Box::new(move |o: &mut Out<'a>| o.group(c))),
                            None => flags |= 1 << (axes + k),
                        }
                    }
                    o.i32(e, flags);
                    e
                });
                (t.path(), make)
            })
            .collect();
        self.animation("MaterialAnimation", members)
    }

    fn visibility_animation(&mut self) -> usize {
        let s = self.s;
        let members: Vec<(String, Make<'a>)> = s
            .visibility_tracks
            .iter()
            .map(|t| {
                let path = format!("MeshNodeVisibilities[\"{}\"].IsVisible", t.node);
                let p2 = path.clone();
                let make: Make<'a> = Box::new(move |o: &mut Out<'a>| {
                    // Boolean (type 2): a pointer to (start, end, repeat, pad), then one bit per frame, 32 to a word.
                    let e = o.a4(0x1C);
                    o.str(e + 4, &p2);
                    o.str(e + 8, &t.node);
                    o.str(e + 0xC, "IsVisible");
                    o.i32(e + 0x10, 2);
                    let frames = (s.animation.frames.ceil() as usize).max(1);
                    o.link(
                        e + 0x14,
                        Box::new(move |o: &mut Out<'a>| {
                            let h = o.a4(0x10);
                            o.f32(h, 0.0);
                            o.f32(h + 4, frames as f32);
                            h
                        }),
                    );
                    o.link(
                        e + 0x18,
                        Box::new(move |o: &mut Out<'a>| {
                            let bits = o.a4(frames.div_ceil(32) * 4);
                            for f in 0..frames {
                                let v = t
                                    .curve
                                    .keys
                                    .iter()
                                    .rev()
                                    .find(|k| k.0 <= f as f32)
                                    .map_or(t.curve.keys[0].1, |k| k.1);
                                if v >= 0.5 {
                                    let w = bits + f / 32 * 4;
                                    let old = o.get_i32(w);
                                    o.i32(w, old | (1 << (f % 32)));
                                }
                            }
                            bits
                        }),
                    );
                    e
                });
                (path, make)
            })
            .collect();
        self.animation("VisibilityAnimation", members)
    }
}

/// "TextureCoordinators[<unit>].<Scale|Rotate|Translate>" split into its unit digit and member.
fn coordinator_member(member: &str) -> Option<(char, &str)> {
    let rest = member.strip_prefix("TextureCoordinators[")?;
    let mut chars = rest.chars();
    let unit = chars.next().filter(char::is_ascii_digit)?;
    let what = chars.as_str().strip_prefix("].")?;
    matches!(what, "Scale" | "Rotate" | "Translate").then_some((unit, what))
}

fn packed(c: &[f32; 4]) -> i32 {
    let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as i32;
    ch(c[0]) | (ch(c[1]) << 8) | (ch(c[2]) << 16) | (ch(c[3]) << 24)
}

fn bone_worlds(bones: &[Bone]) -> Vec<math::Mat34> {
    let mut out: Vec<math::Mat34> = Vec::with_capacity(bones.len());
    for b in bones {
        let local = math::local(&b.scale, &b.rotation, &b.translation);
        let w = if b.parent < 0 { local } else { math::multiply34(&out[b.parent as usize], &local) };
        out.push(w);
    }
    out
}

/// The coordinator's 3x4 texture matrix (rows u', v', w), as real files store it: the affine map the coordinator makes.
fn coord_matrix(c: &Coord) -> [f32; 12] {
    let t = |u, v| banner3ds_soft::shading::transform(u, v, &c.scale, c.rotation, &c.translation, c.matrix_mode);
    let (o, x, y) = (t(0.0, 0.0), t(1.0, 0.0), t(0.0, 1.0));
    [x[0] - o[0], y[0] - o[0], 0.0, o[0], x[1] - o[1], y[1] - o[1], 0.0, o[1], 0.0, 0.0, 1.0, 0.0]
}

fn gl_enums(f: Format) -> (i32, i32) {
    match f {
        Format::Rgba8 => (0x6752, 0x1401),
        Format::Rgb8 => (0x6754, 0x1401),
        Format::Rgba5551 => (0x6752, 0x8034),
        Format::Rgb565 => (0x6754, 0x8363),
        Format::Rgba4 => (0x6752, 0x8033),
        Format::La8 => (0x6758, 0x1401),
        Format::L8 => (0x6757, 0x1401),
        Format::A8 => (0x6756, 0x1401),
        Format::La4 => (0x6758, 0x6760),
        Format::L4 => (0x6757, 0x6761),
        Format::A4 => (0x6756, 0x6761),
        Format::Etc1 => (0x675A, 0),
        Format::Etc1A4 => (0x675B, 0),
        Format::Hilo8 => panic!("no GL enums for HILO8"),
    }
}

/// NW4C's Patricia dictionary: node 0 is the root (ref bit -1); a name's bit n is bit (n & 7) of its character (n >> 3), 0
/// past the end.
pub mod patricia {
    /// A tree node.
    #[derive(Debug, Clone)]
    pub struct Node {
        pub ref_bit: i32,
        pub left: usize,
        pub right: usize,
        pub name: String,
    }

    fn bit(name: &[u8], n: i32) -> i32 {
        let c = (n as u32 >> 3) as usize;
        if c < name.len() { i32::from(name[c] >> (n & 7)) & 1 } else { 0 }
    }

    fn highest_diff(a: &[u8], b: &[u8]) -> i32 {
        let top = (a.len().max(b.len()) * 8) as i32 - 1;
        (0..=top).rev().find(|&n| bit(a, n) != bit(b, n)).unwrap_or_else(|| panic!("duplicate dictionary name"))
    }

    /// The tree over `names`, node 0 the root, node i the (i-1)th name.
    pub fn build(names: &[&str]) -> Vec<Node> {
        let mut nodes = vec![Node { ref_bit: -1, left: 0, right: 0, name: String::new() }];
        let r = |nodes: &[Node], i: usize| nodes[i].ref_bit as u32;
        for name in names {
            let nb = name.as_bytes();
            let step =
                |nodes: &[Node], n: usize| if bit(nb, nodes[n].ref_bit) == 1 { nodes[n].right } else { nodes[n].left };
            let (mut p, mut n) = (0, nodes[0].left);
            while r(&nodes, p) > r(&nodes, n) {
                p = n;
                n = step(&nodes, n);
            }
            let diff = highest_diff(nb, nodes[n].name.as_bytes());
            (p, n) = (0, nodes[0].left);
            while r(&nodes, p) > r(&nodes, n) && r(&nodes, n) > diff as u32 {
                p = n;
                n = step(&nodes, n);
            }
            let x = nodes.len();
            let (left, right) = if bit(nb, diff) == 1 { (n, x) } else { (x, n) };
            nodes.push(Node { ref_bit: diff, left, right, name: (*name).into() });
            if p == 0 {
                nodes[0].left = x;
            } else if bit(nb, nodes[p].ref_bit) == 1 {
                nodes[p].right = x;
            } else {
                nodes[p].left = x;
            }
        }
        nodes
    }
}
