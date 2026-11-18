// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The scene model: everything a renderer needs of one banner, as plain data. Field for field the original implementation
//! implementation's `BannerScene` (names in snake case), minus the derived helpers a renderer owns (rest-pose matrices,
//! the camera matrices, posing).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use gamemeta_core::RgbaImage;

/// Everything a renderer needs of one banner CGFX: models, decoded textures and the animations.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerScene {
    /// The models, in dictionary order (a CBMD's language file appends its own, the logo).
    pub models: Vec<BannerModel>,
    /// Decoded textures by name (CGFX texture names are unique per file).
    pub textures: BTreeMap<String, RgbaImage>,
    /// Skeletal, material and visibility clips.
    pub animations: BannerAnimations,
    /// Features the file uses that a renderer may not draw (`"dot3-combiner"`, `"camera-frustum"`, ...).
    pub unsupported: BTreeSet<String>,
    /// The enabled fragment lights.
    pub lights: Vec<BannerLight>,
    /// The scene environment's light sets (index to light names); `None` without an environment, where every light reaches every material.
    pub light_sets: Option<BTreeMap<i32, Vec<String>>>,
    /// The file's cameras.
    pub cameras: Vec<BannerFileCamera>,
    /// The scene environment's cameras by name, in index order.
    pub environment_cameras: Vec<String>,
    /// The enabled vertex lights ("CVLT").
    pub vertex_lights: Vec<VertexLight>,
    /// The enabled hemisphere lights ("CHLT").
    pub hemisphere_lights: Vec<HemisphereLight>,
    /// The summed RGB of the enabled ambient lights ("CALT") a light set lists (all of them without an environment).
    pub scene_ambient: [f32; 3],
}

impl BannerScene {
    /// A scene with only the given models and textures.
    pub fn new(models: Vec<BannerModel>, textures: BTreeMap<String, RgbaImage>) -> Self {
        BannerScene {
            models,
            textures,
            animations: BannerAnimations::default(),
            unsupported: BTreeSet::new(),
            lights: Vec::new(),
            light_sets: None,
            cameras: Vec::new(),
            environment_cameras: Vec::new(),
            vertex_lights: Vec::new(),
            hemisphere_lights: Vec::new(),
            scene_ambient: [0.0; 3],
        }
    }

    /// The camera the banner frames itself with: the first one its scene environment names, `None` without one. A camera
    /// no environment names is an authoring leftover.
    pub fn file_camera(&self) -> Option<&BannerFileCamera> {
        self.environment_cameras.iter().find_map(|n| self.cameras.iter().find(|c| &c.name == n))
    }

    /// The lights a lit `material` reads: its light set's (none when the set is missing), or every light without an environment.
    pub fn lights_for(&self, material: &BannerMaterial) -> Vec<&BannerLight> {
        self.in_set(material, &self.lights, |l| &l.name)
    }

    /// The vertex lights `material`'s light set names (all of them without an environment).
    pub fn vertex_lights_for(&self, material: &BannerMaterial) -> Vec<&VertexLight> {
        self.in_set(material, &self.vertex_lights, |l| &l.name)
    }

    /// The first hemisphere light `material`'s light set names.
    pub fn hemisphere_for(&self, material: &BannerMaterial) -> Option<&HemisphereLight> {
        self.in_set(material, &self.hemisphere_lights, |l| &l.name).into_iter().next()
    }

    fn in_set<'a, T>(&self, material: &BannerMaterial, all: &'a [T], name: impl Fn(&T) -> &String) -> Vec<&'a T> {
        let Some(sets) = &self.light_sets else { return all.iter().collect() };
        let Some(names) = sets.get(&material.light_set) else { return Vec::new() };
        names.iter().filter_map(|n| all.iter().find(|l| name(l) == n)).collect()
    }
}

/// One model: meshes drawn with one skeleton.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerModel {
    /// Dictionary name.
    pub name: String,
    /// 3x4 row-major world transform of the model node: the cached world with the node's scale on top.
    pub world: [f32; 12],
    /// One entry per sub-mesh of every mesh.
    pub meshes: Vec<BannerMesh>,
    /// The skeleton (empty for a non-skeletal model).
    pub bones: Vec<BannerBone>,
    /// Model flag bit 0.
    pub visible: bool,
}

/// One bone of a skeleton.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerBone {
    /// Dictionary name.
    pub name: String,
    /// Parent bone index, -1 for a root.
    pub parent: i32,
    /// Rest scale.
    pub scale: [f32; 3],
    /// Rest rotation, Euler XYZ radians.
    pub rotation: [f32; 3],
    /// Rest translation.
    pub translation: [f32; 3],
    /// 3x4 row-major rest world transform.
    pub world: [f32; 12],
    /// 3x4 row-major inverse of [`world`](Self::world).
    pub inverse_world: [f32; 12],
    /// Billboard mode: 0 off, 1 world, 2 world viewpoint, 3 screen, 4 screen viewpoint, 5 Y-axial, 6 Y-axial viewpoint.
    pub billboard: i32,
    /// Bone flags: 1 identity, 2 translation zero, 4 rotation zero, 8 scale one, 0x10 uniform scale, 0x20 segment scale compensate, 0x40 needs rendering.
    pub flags: i32,
}

impl BannerBone {
    /// [`flags`](Self::flags) bit: the bone is the identity.
    pub const IDENTITY: i32 = 1;
    /// [`flags`](Self::flags) bit: Maya-style segment scale compensation (the parent's own scale does not carry down).
    pub const SEGMENT_SCALE_COMPENSATE: i32 = 0x20;
}

/// How a sub-mesh's vertices follow the skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skinning {
    /// Not skinned.
    None,
    /// One bone per vertex.
    Rigid,
    /// Weighted bones.
    Smooth,
}

impl Skinning {
    /// The name the scene dump uses.
    pub fn name(self) -> &'static str {
        match self {
            Skinning::None => "None",
            Skinning::Rigid => "Rigid",
            Skinning::Smooth => "Smooth",
        }
    }

    pub(crate) fn from_index(i: i32) -> Skinning {
        match i {
            1 => Skinning::Rigid,
            2 => Skinning::Smooth,
            _ => Skinning::None,
        }
    }
}

/// One shape's decoded vertices, shared by the meshes drawing its sub-meshes.
#[derive(Debug, Clone, PartialEq)]
pub struct VertexData {
    /// Vertex count.
    pub count: usize,
    /// 3 per vertex, the shape's position offset already added.
    pub positions: Vec<f32>,
    /// 3 per vertex.
    pub normals: Option<Vec<f32>>,
    /// 2 per vertex.
    pub uv0: Option<Vec<f32>>,
    /// 2 per vertex.
    pub uv1: Option<Vec<f32>>,
    /// 2 per vertex.
    pub uv2: Option<Vec<f32>>,
    /// 4 per vertex, RGBA.
    pub colors: Option<Vec<f32>>,
    /// 4 per vertex: indices into the sub-mesh's bone palette, `None` without skinning.
    pub bone_indices: Option<Vec<i32>>,
    /// 4 per vertex; a missing influence has weight 0.
    pub bone_weights: Option<Vec<f32>>,
}

/// One sub-mesh drawn with one material.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerMesh {
    /// `"<model>/<node>"`.
    pub name: String,
    /// The shape's vertices.
    pub vertices: Arc<VertexData>,
    /// Triangle list into [`vertices`](Self::vertices).
    pub indices: Vec<u32>,
    /// Skinning kind.
    pub skinning: Skinning,
    /// Palette slot to skeleton bone index.
    pub bone_palette: Vec<i32>,
    /// The material.
    pub material: Arc<BannerMaterial>,
    /// The mesh node's name.
    pub node_name: String,
    /// The mesh's own flag and the model's mesh-node visibility entry.
    pub visible: bool,
    /// Draw order priority.
    pub render_priority: i32,
}

/// A texture-environment combiner function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombinerMode {
    /// Source 0.
    Replace,
    /// 0 * 1.
    Modulate,
    /// 0 + 1.
    Add,
    /// 0 + 1 - 0.5.
    AddSigned,
    /// Interpolate 0 and 1 by 2.
    Interpolate,
    /// 0 - 1.
    Subtract,
    /// Dot product, RGB only.
    DotProduct3Rgb,
    /// Dot product, RGBA.
    DotProduct3Rgba,
    /// (0 * 1) + 2.
    MultAdd,
    /// (0 + 1) * 2.
    AddMult,
}

impl CombinerMode {
    const ALL: [CombinerMode; 10] = [
        CombinerMode::Replace,
        CombinerMode::Modulate,
        CombinerMode::Add,
        CombinerMode::AddSigned,
        CombinerMode::Interpolate,
        CombinerMode::Subtract,
        CombinerMode::DotProduct3Rgb,
        CombinerMode::DotProduct3Rgba,
        CombinerMode::MultAdd,
        CombinerMode::AddMult,
    ];

    /// The mode register value `i`; an unknown one reads as `Replace`.
    pub fn from_index(i: i32) -> CombinerMode {
        usize::try_from(i).ok().and_then(|i| Self::ALL.get(i).copied()).unwrap_or(CombinerMode::Replace)
    }

    /// The name the scene dump uses.
    pub fn name(self) -> &'static str {
        match self {
            CombinerMode::Replace => "Replace",
            CombinerMode::Modulate => "Modulate",
            CombinerMode::Add => "Add",
            CombinerMode::AddSigned => "AddSigned",
            CombinerMode::Interpolate => "Interpolate",
            CombinerMode::Subtract => "Subtract",
            CombinerMode::DotProduct3Rgb => "DotProduct3Rgb",
            CombinerMode::DotProduct3Rgba => "DotProduct3Rgba",
            CombinerMode::MultAdd => "MultAdd",
            CombinerMode::AddMult => "AddMult",
        }
    }
}

/// One of the six PICA texture-environment stages. Sources: 0 primary, 1-2 fragment lighting, 3-6 texture 0-3, 13 previous buffer, 14 constant, 15 previous.
#[derive(Debug, Clone, PartialEq)]
pub struct TexEnvStage {
    /// Colour sources 0..2.
    pub color_sources: [i32; 3],
    /// Alpha sources 0..2.
    pub alpha_sources: [i32; 3],
    /// Colour operands: 0 colour, 1 1-colour, 2 alpha, 3 1-alpha, 4 red, 5 1-red, 8 green, 9 1-green, 12 blue, 13 1-blue.
    pub color_operands: [i32; 3],
    /// Alpha operands: 0 alpha, 1 1-alpha, 2 red, 3 1-red, 4 green, 5 1-green, 6 blue, 7 1-blue.
    pub alpha_operands: [i32; 3],
    /// Colour combiner.
    pub color_mode: CombinerMode,
    /// Alpha combiner.
    pub alpha_mode: CombinerMode,
    /// Colour scale: 1, 2 or 4.
    pub color_scale: i32,
    /// Alpha scale: 1, 2 or 4.
    pub alpha_scale: i32,
    /// RGBA 0..1 of this stage's constant.
    pub constant: [f32; 4],
    /// The stage writes the colour buffer.
    pub update_color_buffer: bool,
    /// The stage writes the alpha buffer.
    pub update_alpha_buffer: bool,
    /// Which of the material's constant colours 0..5 this stage reads (what a `ConstantN` animation changes); -1 when unknown.
    pub constant_index: i32,
}

/// Texture coordinate generation constants (coordinator +4 and +0xC).
pub mod coords {
    /// The mesh's own UVs.
    pub const UV: i32 = 0;
    /// Camera sphere environment map.
    pub const SPHERE: i32 = 2;
    /// Transform type: Maya.
    pub const MAYA: i32 = 0;
    /// Transform type: Softimage.
    pub const SOFTIMAGE: i32 = 1;
    /// Transform type: 3ds Max.
    pub const MAX: i32 = 2;

    /// The coordinator texture unit `unit` reads under the material's texture coordinate config `config` (material +0x1C):
    /// only texture 2 can borrow one, coordinator 1 under configs 1 to 3.
    pub fn coordinator_of(unit: i32, config: i32) -> i32 {
        if unit == 2 && (1..=3).contains(&config) { 1 } else { unit }
    }
}

/// One texture unit of a material.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureSlot {
    /// Texture name in [`BannerScene::textures`].
    pub texture: String,
    /// Wrap mode of U (PICA: 0 clamp to edge, 1 clamp to border, 2 repeat, 3 mirror).
    pub wrap_u: i32,
    /// Wrap mode of V.
    pub wrap_v: i32,
    /// Linear filtering.
    pub linear: bool,
    /// Which UV set of the vertices feeds this slot.
    pub uv_set: i32,
    /// Coordinator scale.
    pub scale: [f32; 2],
    /// Coordinator rotation, radians.
    pub rotation: f32,
    /// Coordinator translation.
    pub translation: [f32; 2],
    /// Mapping method: 0 the mesh's UVs, 1 camera cube env, 2 camera sphere env, 3 projection.
    pub mapping: i32,
    /// RGBA 0..1 a clamp-to-border axis reads outside [0, 1].
    pub border: [f32; 4],
    /// The coordinator's transform type ([`coords::MAYA`], [`coords::SOFTIMAGE`], [`coords::MAX`]).
    pub matrix_mode: i32,
}

/// One material.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerMaterial {
    /// Dictionary name.
    pub name: String,
    /// The three texture units.
    pub slots: Vec<Option<TextureSlot>>,
    /// The six combiner stages.
    pub stages: Vec<TexEnvStage>,
    /// Combiner buffer colour, RGBA 0..1.
    pub buffer_color: [f32; 4],
    /// Diffuse colour.
    pub diffuse: [f32; 4],
    /// Ambient colour.
    pub ambient: [f32; 4],
    /// Emission colour.
    pub emission: [f32; 4],
    /// Fragment lighting on (material flag bit 0).
    pub fragment_lighting: bool,
    /// Alpha test on.
    pub alpha_test_enabled: bool,
    /// Alpha test function (PICA compare 0..7).
    pub alpha_test_function: i32,
    /// Alpha test reference, 0..1.
    pub alpha_test_reference: f32,
    /// 0 none (opaque), 1 blend, 2 separate blend, 3 logic op.
    pub blend_mode: i32,
    /// PICA blend function register value.
    pub blend_function: i32,
    /// Blend constant colour (always zero: the file keeps it in a command nobody reads).
    pub blend_color: [f32; 4],
    /// 0 cull front faces, 1 cull back faces, 2 cull everything, 3 cull nothing.
    pub face_culling: i32,
    /// Depth test on.
    pub depth_test: bool,
    /// Depth write on.
    pub depth_write: bool,
    /// The file's translucency kind word (mat +0x20).
    pub translucency: i32,
    /// Specular colour 0.
    pub specular0: [f32; 4],
    /// Specular colour 1.
    pub specular1: [f32; 4],
    /// The fragment-lighting setup; `None` when [`fragment_lighting`](Self::fragment_lighting) is off.
    pub lighting: Option<FragmentLightingSetup>,
    /// PICA logic op for blend mode 3 (3 copy).
    pub logic_op: i32,
    /// The scene environment light set this material reads.
    pub light_set: i32,
    /// Bump: 0 none, 1 the bump slot is a tangent-space normal map, 2 tangent map.
    pub bump_mode: i32,
    /// The texture slot (0..2) the bump reads.
    pub bump_slot: i32,
    /// Rebuild the normal map's z from x and y.
    pub bump_renormalize: bool,
    /// NW4C vertex-shader lighting into colour source 0 (material flag bit 1).
    pub vertex_lighting: bool,
    /// NW4C hemisphere lighting into colour source 0 (material flag bit 2).
    pub hemisphere_lighting: bool,
}

/// A PICA lighting look-up table: 256 entries, each a value and the difference to the next one (what the hardware interpolates
/// with). `absolute` tables are indexed by |x| over [0, 1]; the others by x over [-1, 1], entries 0-127 for x >= 0 and 128-255 for x < 0.
#[derive(Debug, Clone, PartialEq)]
pub struct LightingLut {
    /// 256 values, 0..1.
    pub values: Vec<f32>,
    /// 256 differences to the next entry.
    pub deltas: Vec<f32>,
    /// Indexed by |x|.
    pub absolute: bool,
}

impl LightingLut {
    /// From the 24-bit PICA entries: the value in bits 0-11 (0.12 fixed), the difference sign-magnitude in bits 12-23 (0.11
    /// magnitude in bits 12-22, bit 23 set when negative). Missing entries read as 0.
    pub fn from_entries(entries: &[i32], absolute: bool) -> LightingLut {
        let entry = |i: usize| entries.get(i).copied().unwrap_or(0);
        let values = (0..256).map(|i| (entry(i) & 0xFFF) as f32 / 4095.0).collect();
        let deltas = (0..256)
            .map(|i| {
                let e = entry(i);
                let magnitude = ((e >> 12) & 0x7FF) as f32 / 2047.0;
                if (e >> 23) & 1 != 0 { -magnitude } else { magnitude }
            })
            .collect();
        LightingLut { values, deltas, absolute }
    }
}

/// One table a material or light reads: `input` picks the dot product that indexes it (0 N.H, 1 V.H, 2 N.V, 3 L.N, 4 -L.spot
/// direction, 5 cos phi) and `scale` multiplies what it returns.
#[derive(Debug, Clone, PartialEq)]
pub struct LutSampler {
    /// The table.
    pub table: Arc<LightingLut>,
    /// The input.
    pub input: i32,
    /// The scale: 0.25, 0.5, 1, 2, 4 or 8.
    pub scale: f32,
}

/// A material's fragment-lighting setup.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentLightingSetup {
    /// Clamp highlights.
    pub clamp_highlights: bool,
    /// Distribution 0 on.
    pub distribution0: bool,
    /// Distribution 1 on.
    pub distribution1: bool,
    /// Geometric factor 0 on.
    pub geometric0: bool,
    /// Geometric factor 1 on.
    pub geometric1: bool,
    /// Reflection on.
    pub reflection: bool,
    /// Which tables the hardware reads (CONFIG0 bits 4-7).
    pub layer_config: i32,
    /// 0 none, 1 primary alpha, 2 secondary alpha, 3 both.
    pub fresnel_selector: i32,
    /// RR, RG, RB, D0, D1, FR (see [`lighting`]); `None` where the material has none.
    pub samplers: Vec<Option<LutSampler>>,
}

/// Fragment-lighting constants and the table lookup.
pub mod lighting {
    use super::LightingLut;

    /// Sampler index: reflection red.
    pub const RR: usize = 0;
    /// Sampler index: reflection green.
    pub const RG: usize = 1;
    /// Sampler index: reflection blue.
    pub const RB: usize = 2;
    /// Sampler index: distribution 0.
    pub const D0: usize = 3;
    /// Sampler index: distribution 1.
    pub const D1: usize = 4;
    /// Sampler index: Fresnel.
    pub const FR: usize = 5;
    /// LUT input: cos phi (not drawn by the original renderers).
    pub const INPUT_CP: i32 = 5;
    /// Most lights a PICA light environment holds.
    pub const MAX_LIGHTS: usize = 8;

    /// Reads `lut` at `x` the way the hardware indexes it, interpolating with the entry's stored difference.
    pub fn lookup(lut: &LightingLut, x: f32) -> f32 {
        let (index, frac) = if lut.absolute {
            let v = x.abs().min(1.0) * 256.0;
            let i = (v as usize).min(255);
            (i, v - i as f32)
        } else {
            let v = x.clamp(-1.0, 1.0) * 128.0;
            let t = v as i32; // truncates toward zero; floor below
            let i = (if (t as f32) > v { t - 1 } else { t }).clamp(-128, 127);
            ((i & 255) as usize, v - i as f32)
        };
        match (lut.values.get(index), lut.deltas.get(index)) {
            (Some(v), Some(d)) => v + d * frac,
            _ => 0.0,
        }
    }
}

/// A fragment light, in world space. Colours RGBA 0..1.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerLight {
    /// Dictionary name.
    pub name: String,
    /// 0 directional, 1 point, 2 spot.
    pub kind: i32,
    /// World position.
    pub position: [f32; 3],
    /// Where the light points.
    pub direction: [f32; 3],
    /// Ambient colour.
    pub ambient: [f32; 4],
    /// Diffuse colour.
    pub diffuse: [f32; 4],
    /// Specular colour 0.
    pub specular0: [f32; 4],
    /// Specular colour 1.
    pub specular1: [f32; 4],
    /// Spot table (input 4 by default), read for kind 2.
    pub spot: Option<LutSampler>,
    /// Distance attenuation table, indexed by clamp(bias + scale * distance, 0, 1).
    pub distance: Option<Arc<LightingLut>>,
    /// Distance attenuation bias.
    pub attenuation_bias: f32,
    /// Distance attenuation scale.
    pub attenuation_scale: f32,
    /// Two-sided diffuse (never set by the file reader).
    pub two_sided_diffuse: bool,
}

/// A vertex light ("CVLT"), in world space.
#[derive(Debug, Clone, PartialEq)]
pub struct VertexLight {
    /// Dictionary name.
    pub name: String,
    /// 0 directional, 1 point, 2 spot.
    pub kind: i32,
    /// World position.
    pub position: [f32; 3],
    /// Where the light points.
    pub direction: [f32; 3],
    /// Ambient colour.
    pub ambient: [f32; 4],
    /// Diffuse colour.
    pub diffuse: [f32; 4],
    /// Distance attenuation 1 / (constant + linear d + quadratic d^2).
    pub attenuation: [f32; 3],
}

/// A hemisphere light ("CHLT"): `sky` where a normal faces `direction`, `ground` where it faces away.
#[derive(Debug, Clone, PartialEq)]
pub struct HemisphereLight {
    /// Dictionary name.
    pub name: String,
    /// Ground colour.
    pub ground: [f32; 4],
    /// Sky colour.
    pub sky: [f32; 4],
    /// Up direction.
    pub direction: [f32; 3],
    /// Lerp factor.
    pub lerp: f32,
}

/// How a file camera aims.
#[derive(Debug, Clone, PartialEq)]
pub enum CameraView {
    /// Looks at `target`, rolled `twist` radians about the line of sight.
    Aim {
        /// The point looked at.
        target: [f32; 3],
        /// Roll, radians.
        twist: f32,
    },
    /// Looks at `target` with `up` up.
    LookAt {
        /// The point looked at.
        target: [f32; 3],
        /// The up vector.
        up: [f32; 3],
    },
    /// Turned by Euler XYZ radians, looking down its own -Z with +Y up.
    Rotate {
        /// Euler XYZ, radians.
        rotation: [f32; 3],
    },
}

/// A file camera's projection.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraProjection {
    /// Orthographic (else perspective).
    pub orthographic: bool,
    /// Near plane.
    pub near: f32,
    /// Far plane.
    pub far: f32,
    /// Width over height.
    pub aspect: f32,
    /// The vertical field of view in radians, or the height an orthographic camera shows.
    pub fov_or_height: f32,
}

/// A CGFX camera ("CCAM") as the file stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerFileCamera {
    /// Dictionary name.
    pub name: String,
    /// Where it stands.
    pub position: [f32; 3],
    /// How it aims.
    pub view: CameraView,
    /// Its projection.
    pub projection: CameraProjection,
}

/// Animations by kind.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BannerAnimations {
    /// Bone clips.
    pub skeletal: Vec<BannerAnimation>,
    /// Material clips (UV, colours).
    pub material: Vec<BannerAnimation>,
    /// Mesh visibility clips.
    pub visibility: Vec<BannerAnimation>,
}

/// A clip: `frames` long at 60 per second, with a track per animated target and component.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerAnimation {
    /// Dictionary name.
    pub name: String,
    /// Length in frames.
    pub frames: f32,
    /// Loops.
    pub looping: bool,
    /// The tracks.
    pub tracks: Vec<AnimTrack>,
}

/// What a track drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    /// A bone's scale.
    BoneScale,
    /// A bone's Euler rotation.
    BoneRotation,
    /// A bone's translation.
    BoneTranslation,
    /// A bone's quaternion rotation.
    BoneRotationQuat,
    /// A material texture coordinator's translation.
    MaterialTranslate,
    /// A material texture coordinator's scale.
    MaterialScale,
    /// A material texture coordinator's rotation.
    MaterialRotate,
    /// A material's diffuse colour.
    MaterialDiffuse,
    /// A material's constant colour.
    MaterialConstant,
    /// A mesh node's visibility.
    MeshVisibility,
}

impl TrackKind {
    /// The name the scene dump uses.
    pub fn name(self) -> &'static str {
        match self {
            TrackKind::BoneScale => "BoneScale",
            TrackKind::BoneRotation => "BoneRotation",
            TrackKind::BoneTranslation => "BoneTranslation",
            TrackKind::BoneRotationQuat => "BoneRotationQuat",
            TrackKind::MaterialTranslate => "MaterialTranslate",
            TrackKind::MaterialScale => "MaterialScale",
            TrackKind::MaterialRotate => "MaterialRotate",
            TrackKind::MaterialDiffuse => "MaterialDiffuse",
            TrackKind::MaterialConstant => "MaterialConstant",
            TrackKind::MeshVisibility => "MeshVisibility",
        }
    }
}

/// One animated value: a bone, material or mesh node, and which of its components.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimTrack {
    /// The bone, material or mesh node name.
    pub target: String,
    /// What it drives.
    pub kind: TrackKind,
    /// For vectors 0..2 = x, y, z; for colours 0..3 = r, g, b, a; for visibility 0.
    pub component: i32,
    /// The curve.
    pub keys: Keyframes,
    /// The texture coordinator or constant colour index.
    pub slot: i32,
}

/// Sorted keys, sampled by [`sample`](Self::sample): linear, hermite (with slopes) or step.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframes {
    /// Key frames, ascending.
    pub frames: Vec<f32>,
    /// One value per key.
    pub values: Vec<f32>,
    /// Hermite in slopes.
    pub in_slopes: Option<Vec<f32>>,
    /// Hermite out slopes.
    pub out_slopes: Option<Vec<f32>>,
    /// Hold each key until the next.
    pub step: bool,
}

impl Keyframes {
    /// The value at `frame`: clamped outside the keys, between them stepped, hermite when both slope arrays exist, else linear.
    pub fn sample(&self, frame: f32) -> f32 {
        let n = self.frames.len();
        if n == 0 || self.values.len() < n {
            return 0.0;
        }
        if n == 1 || frame <= self.frames[0] {
            return self.values[0];
        }
        if frame >= self.frames[n - 1] {
            return self.values[n - 1];
        }
        let mut hi = 1;
        while hi < n - 1 && self.frames[hi] < frame {
            hi += 1;
        }
        let lo = hi - 1;
        if self.step {
            return self.values[lo];
        }
        let dt = self.frames[hi] - self.frames[lo];
        let t = if dt == 0.0 { 0.0 } else { (frame - self.frames[lo]) / dt };
        let (Some(out), Some(inn)) = (&self.out_slopes, &self.in_slopes) else {
            return self.values[lo] + (self.values[hi] - self.values[lo]) * t;
        };
        let (Some(&o), Some(&i)) = (out.get(lo), inn.get(hi)) else { return self.values[lo] };
        let t2 = t * t;
        let t3 = t2 * t;
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.values[lo]
            + (t3 - 2.0 * t2 + t) * o * dt
            + (-2.0 * t3 + 3.0 * t2) * self.values[hi]
            + (t3 - t2) * i * dt
    }
}
