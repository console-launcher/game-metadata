// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A CGFX into a [`BannerScene`]. Field layouts follow 3dbrew and SPICA's CtrGfx classes (Unlicense); offsets are byte
//! offsets from the object start (every object begins with a 4-byte type id, a revision, the name pointer and an 8-byte
//! metadata dictionary, so its own fields start at 0x18). Anything unreadable is a [`FormatError`], which [`parse`] turns
//! into `None`.

use crate::animations;
use crate::model::*;
use crate::pica_commands;
use crate::pica_textures::{self, Format};
use crate::reader::{FormatError, R, Reader};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::Cell;
use gamemeta_core::RgbaImage;

const MAGIC: u32 = 0x5846_4743; // "CGFX"
const MAX_ENTRIES: u32 = 4096;
const MAX_VERTICES: i64 = 200_000;
const MAX_TEXTURE_SIDE: i32 = 1024;
const TYPE_ATTRIBUTE: u32 = 0x4000_0001;
const TYPE_INTERLEAVED: u32 = 0x4000_0002;
const TYPE_FIXED: u32 = 0x8000_0000;
const TEXENV_REGISTERS: [i32; 6] = [0xC0, 0xC8, 0xD0, 0xD8, 0xF0, 0xF8];
/// GPUREG_LIGHTING_LUTINPUT_SCALE codes: 0 x1, 1 x2, 2 x4, 3 x8, 6 x0.25, 7 x0.5 (4 and 5 unused).
const LUT_SCALES: [f32; 8] = [1.0, 2.0, 4.0, 8.0, 0.0, 0.0, 0.25, 0.5];
/// What one parse may allocate in total, in 4-byte units (vertex floats, indices, texture pixels, keys): 256 MiB. A hostile
/// file can point thousands of dictionary entries at the same data; the original reader allocates without bound.
const BUDGET_UNITS: usize = 64 << 20;

/// Allocation allowance of one parse.
pub(crate) struct Budget(Cell<usize>);

impl Budget {
    fn new() -> Budget {
        Budget(Cell::new(BUDGET_UNITS))
    }

    /// Takes `units`, or fails when the allowance is spent.
    pub(crate) fn spend(&self, units: usize) -> R<()> {
        let left = self.0.get().checked_sub(units).ok_or(FormatError)?;
        self.0.set(left);
        Ok(())
    }
}

type Luts = BTreeMap<String, BTreeMap<String, Arc<LightingLut>>>;

/// The scene of a CGFX, `None` when it does not read.
pub fn parse(bytes: &[u8]) -> Option<BannerScene> {
    parse_with_localized(bytes, None)
}

/// `bytes` merged with `localized` (a CBMD language CGFX): its textures replace the common placeholders by name, and its
/// own models (the logo) and animations join the scene.
pub fn parse_with_localized(bytes: &[u8], localized: Option<&[u8]>) -> Option<BannerScene> {
    let mut scene = parse_or_err(bytes).ok()?;
    let Some(localized) = localized else { return Some(scene) };
    match parse_or_err(localized) {
        Err(_) => {
            // Not a scene of its own: only its textures count.
            for (name, image) in textures_or_err(localized).ok()? {
                scene.textures.insert(name, image);
            }
        }
        Ok(loc) => {
            scene.models.extend(loc.models);
            scene.textures.extend(loc.textures);
            scene.animations.skeletal.extend(loc.animations.skeletal);
            scene.animations.material.extend(loc.animations.material);
            scene.animations.visibility.extend(loc.animations.visibility);
            scene.unsupported.extend(loc.unsupported);
            scene.lights.extend(loc.lights);
            if scene.light_sets.is_none() {
                scene.light_sets = loc.light_sets;
            }
            if scene.cameras.is_empty() {
                scene.cameras = loc.cameras;
            }
            if scene.environment_cameras.is_empty() {
                scene.environment_cameras = loc.environment_cameras;
            }
            scene.vertex_lights.extend(loc.vertex_lights);
            scene.hemisphere_lights.extend(loc.hemisphere_lights);
            for k in 0..3 {
                scene.scene_ambient[k] += loc.scene_ambient[k];
            }
        }
    }
    Some(scene)
}

/// The scene of a CGFX, or why it does not read.
pub fn parse_or_err(bytes: &[u8]) -> R<BannerScene> {
    let r = Reader(bytes);
    if bytes.len() < 0x20 || r.u32(0)? != MAGIC {
        return Err(FormatError);
    }
    // 0x14: the "DATA" section header (magic, length); then 16 dictionaries as (count, pointer) pairs.
    let dict_base = 0x1C;
    let budget = Budget::new();
    let mut unsupported = BTreeSet::new();
    let textures = parse_textures(&r, dict_base + 8, &budget)?;
    let luts = parse_luts(&r, dict_base + 16)?;
    let mut models = Vec::new();
    for (name, at) in dict(&r, dict_base)? {
        models.push(parse_model(&r, &name, at, &luts, &mut unsupported, &budget)?);
    }
    let animations = animations::parse(&r, dict_base, &mut unsupported, &budget)?;
    let lights = parse_lights(&r, dict_base + 48, &luts, &mut unsupported)?;
    let light_sets = parse_light_sets(&r, dict_base + 64)?;
    let cameras = parse_cameras(&r, dict_base + 40, &mut unsupported)?;
    let environment_cameras = parse_environment_cameras(&r, dict_base + 64)?;
    let vertex_lights = parse_vertex_lights(&r, dict_base + 48)?;
    let hemisphere_lights = parse_hemisphere_lights(&r, dict_base + 48)?;
    let listed: Option<BTreeSet<&str>> =
        light_sets.as_ref().map(|s| s.values().flatten().map(String::as_str).collect());
    let scene_ambient = parse_scene_ambient(&r, dict_base + 48, listed.as_ref())?;
    Ok(BannerScene {
        models,
        textures,
        animations,
        unsupported,
        lights,
        light_sets,
        cameras,
        environment_cameras,
        vertex_lights,
        hemisphere_lights,
        scene_ambient,
    })
}

/// Only the textures of a CGFX (what a CBMD language file carries), empty when it does not read.
pub fn textures(bytes: &[u8]) -> BTreeMap<String, RgbaImage> {
    textures_or_err(bytes).unwrap_or_default()
}

fn textures_or_err(bytes: &[u8]) -> R<BTreeMap<String, RgbaImage>> {
    let r = Reader(bytes);
    if bytes.len() < 0x20 || r.u32(0)? != MAGIC {
        return Ok(BTreeMap::new());
    }
    parse_textures(&r, 0x1C + 8, &Budget::new())
}

// ---- dictionaries and lists

/// (name, object pointer) of each entry of the dictionary whose (count, pointer) pair sits at `pair_at`.
pub(crate) fn dict(r: &Reader<'_>, pair_at: i64) -> R<Vec<(String, i64)>> {
    let count = r.u32(pair_at)?;
    let at = r.ptr(pair_at + 4)?;
    if count == 0 || at == 0 {
        return Ok(Vec::new());
    }
    if count > MAX_ENTRIES || r.u32(at)? != 0x5443_4944 {
        return Err(FormatError);
    }
    let n = r.u32(at + 8)? as i32;
    if !(0..=MAX_ENTRIES as i32).contains(&n) {
        return Err(FormatError);
    }
    let mut out = Vec::with_capacity(n as usize);
    for i in 1..=i64::from(n) {
        let node = at + 12 + 16 * i;
        let name = match r.ptr(node + 8)? {
            0 => String::new(),
            p => r.cstr(p)?,
        };
        out.push((name, r.ptr(node + 12)?));
    }
    Ok(out)
}

/// Object pointers of the list whose (count, pointer) pair sits at `pair_at`.
pub(crate) fn list(r: &Reader<'_>, pair_at: i64) -> R<Vec<i64>> {
    let count = r.u32(pair_at)?;
    let at = r.ptr(pair_at + 4)?;
    if count == 0 || at == 0 {
        return Ok(Vec::new());
    }
    if count > MAX_ENTRIES {
        return Err(FormatError);
    }
    (0..i64::from(count)).map(|i| r.ptr(at + 4 * i)).collect()
}

fn vec<const N: usize>(r: &Reader<'_>, p: i64) -> R<[f32; N]> {
    let mut out = [0.0; N];
    for (i, v) in out.iter_mut().enumerate() {
        *v = r.f32(p + 4 * i as i64)?;
    }
    Ok(out)
}

fn name_at(r: &Reader<'_>, p: i64) -> R<Option<String>> {
    match r.ptr(p)? {
        0 => Ok(None),
        q => r.cstr(q).map(Some),
    }
}

// ---- textures: type 0x20000011 (image). 0x18 height, 0x1C width, 0x34 hardware format, 0x38 pointer to image data
// (0 height, 4 width, 8 byte count, 0xC pointer to the pixels). The image data is the base level followed by mip levels.

fn parse_textures(r: &Reader<'_>, pair_at: i64, budget: &Budget) -> R<BTreeMap<String, RgbaImage>> {
    let mut out = BTreeMap::new();
    for (name, at) in dict(r, pair_at)? {
        if at == 0 || r.u32(at)? != 0x2000_0011 {
            continue;
        }
        let height = r.i32(at + 0x18)?;
        let width = r.i32(at + 0x1C)?;
        let Some(format) = Format::of(r.i32(at + 0x34)?) else { continue };
        if !(8..=MAX_TEXTURE_SIDE).contains(&width) || !(8..=MAX_TEXTURE_SIDE).contains(&height) {
            continue;
        }
        let image = r.ptr(at + 0x38)?;
        if image == 0 {
            continue;
        }
        let size = r.i32(image + 8)?;
        let pixels = r.ptr(image + 0xC)?;
        if pixels == 0 || size <= 0 {
            continue;
        }
        let need = pica_textures::byte_size(width as usize, height as usize, format);
        if (size as usize) < need {
            continue;
        }
        budget.spend(width as usize * height as usize)?;
        if let Some(image) =
            pica_textures::decode(r.slice(pixels, need as i64)?, width as usize, height as usize, format)
        {
            out.insert(name, image);
        }
    }
    Ok(out)
}

// ---- lighting look-up tables (dictionary 2): LUT sets (0x18 samplers dict); a sampler: 8 absolute flag, 0xC command byte
// count, 0x10 pointer to a PICA command buffer that writes its 256 entries to GPUREG_LIGHTING_LUT_DATA (0x1C8-0x1CF).

fn parse_luts(r: &Reader<'_>, pair_at: i64) -> R<Luts> {
    let mut out = Luts::new();
    for (set_name, at) in dict(r, pair_at)? {
        let mut set = BTreeMap::new();
        if at != 0 {
            for (name, s) in dict(r, at + 0x18)? {
                if s == 0 {
                    continue;
                }
                let words = i64::from((r.i32(s + 0xC)? / 4).clamp(0, 1024));
                let cmds = r.ptr(s + 0x10)?;
                if cmds == 0 {
                    continue;
                }
                let entries = pica_commands::writes(r, cmds, words, 0x1C8, 0x1CF)?;
                if entries.len() >= 256 {
                    set.insert(name, Arc::new(LightingLut::from_entries(&entries[..256], r.i32(s + 8)? != 0)));
                }
            }
        }
        out.insert(set_name, set);
    }
    Ok(out)
}

/// A LUT reference: 0 type, 4 LUT set name, 8 sampler name.
fn lut_by_reference(r: &Reader<'_>, reference: i64, luts: &Luts) -> R<Option<Arc<LightingLut>>> {
    if reference == 0 {
        return Ok(None);
    }
    let Some(set) = name_at(r, reference + 4)? else { return Ok(None) };
    let Some(sampler) = name_at(r, reference + 8)? else { return Ok(None) };
    Ok(luts.get(&set).and_then(|s| s.get(&sampler)).cloned())
}

/// A table use: 0 input, 4 PICA scale code, 8 pointer to the LUT reference. `None` when unreadable.
fn lut_sampler(r: &Reader<'_>, at: i64, luts: &Luts) -> R<Option<LutSampler>> {
    let input = r.i32(at)?;
    let code = r.i32(at + 4)?;
    let Some(&scale) = usize::try_from(code).ok().and_then(|c| LUT_SCALES.get(c)) else { return Ok(None) };
    if !(0..=5).contains(&input) || scale == 0.0 {
        return Ok(None);
    }
    Ok(lut_by_reference(r, r.ptr(at + 8)?, luts)?.map(|table| LutSampler { table, input, scale }))
}

// ---- cameras (dictionary 5), "CCAM": the node transform as a model's (0x48 translation), then 0xB4 view kind (0 aim,
// 1 look-at, 2 rotate), 0xB8 projection kind (0 perspective, 1 frustum, 2 orthographic), 0xBC view (pointer), 0xC0 projection
// (pointer). A view: 0 type, 4 flags, 8 target (aim, look-at) or Euler XYZ (rotate), then 0x14 twist (aim) or up (look-at).
// A projection: 0 type, 4 near, 8 far, 0xC aspect, 0x10 fovY in radians or the orthographic height.

fn parse_cameras(r: &Reader<'_>, pair_at: i64, unsupported: &mut BTreeSet<String>) -> R<Vec<BannerFileCamera>> {
    let mut out = Vec::new();
    for (name, at) in dict(r, pair_at)? {
        if at == 0 || r.u32(at + 4)? != 0x4D41_4343 {
            continue;
        }
        let v = r.ptr(at + 0xBC)?;
        let p = r.ptr(at + 0xC0)?;
        let projection_kind = r.i32(at + 0xB8)?;
        if v == 0 || p == 0 || projection_kind == 1 {
            unsupported.insert(if projection_kind == 1 { "camera-frustum" } else { "camera-damaged" }.into());
            continue;
        }
        let view = match r.i32(at + 0xB4)? {
            0 => CameraView::Aim { target: vec(r, v + 8)?, twist: r.f32(v + 0x14)? },
            1 => CameraView::LookAt { target: vec(r, v + 8)?, up: vec(r, v + 0x14)? },
            2 => CameraView::Rotate { rotation: vec(r, v + 8)? },
            _ => {
                unsupported.insert("camera-view".into());
                continue;
            }
        };
        let projection = CameraProjection {
            orthographic: projection_kind == 2,
            near: r.f32(p + 4)?,
            far: r.f32(p + 8)?,
            aspect: r.f32(p + 0xC)?,
            fov_or_height: r.f32(p + 0x10)?,
        };
        out.push(BannerFileCamera { name, position: vec(r, at + 0x48)?, view, projection });
    }
    Ok(out)
}

/// The first scene environment's cameras (CENV 0x18, a list; an entry: 0 index, 4 the camera's name), by index.
fn parse_environment_cameras(r: &Reader<'_>, pair_at: i64) -> R<Vec<String>> {
    let Some(&(_, env)) = dict(r, pair_at)?.first() else { return Ok(Vec::new()) };
    if env == 0 {
        return Ok(Vec::new());
    }
    let mut named = Vec::new();
    for e in list(r, env + 0x18)?.into_iter().take(16) {
        if let Some(name) = name_at(r, e + 4)? {
            named.push((r.i32(e)?, name));
        }
    }
    named.sort_by_key(|n| n.0);
    Ok(named.into_iter().map(|n| n.1).collect())
}

// ---- fragment lights (dictionary 6), "CFLT": the node transform as a model's (0x48 translation, 0x84 world 3x4), then
// 0xB4 enabled, 0xB8 kind, 0xBC ambient, 0xCC diffuse, 0xDC specular 0, 0xEC specular 1 (float RGBA), 0x10C direction,
// 0x118 distance-attenuation LUT reference, 0x11C spot table use, 0x120 attenuation start, 0x124 end.

fn parse_lights(r: &Reader<'_>, pair_at: i64, luts: &Luts, unsupported: &mut BTreeSet<String>) -> R<Vec<BannerLight>> {
    let mut out = Vec::new();
    for (name, at) in dict(r, pair_at)? {
        if at == 0 || r.u32(at + 4)? != 0x544C_4643 || r.i32(at + 0xB4)? == 0 {
            continue;
        }
        let kind = r.i32(at + 0xB8)?.clamp(0, 2);
        let position = node_position(r, at)?;
        let distance_ref = r.ptr(at + 0x118)?;
        let distance = lut_by_reference(r, distance_ref, luts)?;
        if distance_ref != 0 && distance.is_none() {
            unsupported.insert("light-distance-attenuation".into());
        }
        let spot_at = r.ptr(at + 0x11C)?;
        let spot = if spot_at == 0 { None } else { lut_sampler(r, spot_at, luts)? };
        if kind == 2 && spot_at != 0 && spot.is_none() {
            unsupported.insert("light-spot".into());
        }
        let start = r.f32(at + 0x120)?;
        let end = r.f32(at + 0x124)?;
        let range = end - start;
        out.push(BannerLight {
            name,
            kind,
            position,
            direction: vec(r, at + 0x10C)?,
            ambient: vec(r, at + 0xBC)?,
            diffuse: vec(r, at + 0xCC)?,
            specular0: vec(r, at + 0xDC)?,
            specular1: vec(r, at + 0xEC)?,
            spot,
            distance,
            attenuation_bias: if range > 0.0 { -start / range } else { 0.0 },
            attenuation_scale: if range > 0.0 { 1.0 / range } else { 0.0 },
            two_sided_diffuse: false,
        });
    }
    Ok(out)
}

/// A light node's position: its world transform's translation, else its own (0x84 world 3x4, 0x48 translation).
fn node_position(r: &Reader<'_>, at: i64) -> R<[f32; 3]> {
    let world: [f32; 12] = vec(r, at + 0x84)?;
    let position = [world[3], world[7], world[11]];
    if position.iter().any(|&x| x != 0.0) { Ok(position) } else { vec(r, at + 0x48) }
}

// ---- vertex lights (dictionary 6), "CVLT": 0xB4 enabled, 0xB8 kind, 0xBC ambient, 0xCC diffuse (float RGBA), 0xDC
// direction, 0xE8 distance attenuation constant, linear, quadratic.

fn parse_vertex_lights(r: &Reader<'_>, pair_at: i64) -> R<Vec<VertexLight>> {
    let mut out = Vec::new();
    for (name, at) in dict(r, pair_at)? {
        if at == 0 || r.u32(at + 4)? != 0x544C_5643 || r.i32(at + 0xB4)? == 0 {
            continue;
        }
        out.push(VertexLight {
            name,
            kind: r.i32(at + 0xB8)?.clamp(0, 2),
            position: node_position(r, at)?,
            direction: vec(r, at + 0xDC)?,
            ambient: vec(r, at + 0xBC)?,
            diffuse: vec(r, at + 0xCC)?,
            attenuation: vec(r, at + 0xE8)?,
        });
    }
    Ok(out)
}

// ---- hemisphere lights (dictionary 6), "CHLT": 0xB4 enabled, 0xB8 ground and 0xC8 sky colours (float RGBA), 0xD8
// direction, 0xE4 lerp factor.

fn parse_hemisphere_lights(r: &Reader<'_>, pair_at: i64) -> R<Vec<HemisphereLight>> {
    let mut out = Vec::new();
    for (name, at) in dict(r, pair_at)? {
        if at == 0 || r.u32(at + 4)? != 0x544C_4843 || r.i32(at + 0xB4)? == 0 {
            continue;
        }
        out.push(HemisphereLight {
            name,
            ground: vec(r, at + 0xB8)?,
            sky: vec(r, at + 0xC8)?,
            direction: vec(r, at + 0xD8)?,
            lerp: r.f32(at + 0xE4)?,
        });
    }
    Ok(out)
}

// ---- ambient lights (dictionary 6), "CALT": 0xB4 enabled, 0xB8 colour (float RGBA). The enabled ones a light set lists
// (`listed`; all of them with no scene environment) sum into the scene ambient.

fn parse_scene_ambient(r: &Reader<'_>, pair_at: i64, listed: Option<&BTreeSet<&str>>) -> R<[f32; 3]> {
    let mut sum = [0.0f32; 3];
    for (name, at) in dict(r, pair_at)? {
        if at == 0 || r.u32(at + 4)? != 0x544C_4143 {
            continue;
        }
        if listed.is_some_and(|l| !l.contains(name.as_str())) || r.i32(at + 0xB4)? == 0 {
            continue;
        }
        for (k, s) in sum.iter_mut().enumerate() {
            let v = r.f32(at + 0xB8 + 4 * k as i64)?;
            *s += if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        }
    }
    Ok(sum)
}

// ---- scene environments (dictionary 8), the first one's light sets. CENV: 0x20 light sets (list); a set: 0 index,
// 4 lights (list); a light entry: 4 the light's name. `None` without an environment.

fn parse_light_sets(r: &Reader<'_>, pair_at: i64) -> R<Option<BTreeMap<i32, Vec<String>>>> {
    let Some(&(_, env)) = dict(r, pair_at)?.first() else { return Ok(None) };
    if env == 0 {
        return Ok(None);
    }
    let mut out = BTreeMap::new();
    for set in list(r, env + 0x20)?.into_iter().take(16) {
        let mut names = Vec::new();
        for e in list(r, set + 4)?.into_iter().take(lighting::MAX_LIGHTS * 4) {
            if let Some(name) = name_at(r, e + 4)? {
                names.push(name);
            }
        }
        out.insert(r.i32(set)?, names);
    }
    Ok(Some(out))
}

// ---- models. CMDL 0x40000012 / skeletal 0x40000092. 0x30 scale, 0x3C rotation, 0x48 translation, 0x54 local 3x4, 0x84
// world 3x4; 0xB4 meshes (list), 0xBC materials (dict), 0xC4 shapes (list), 0xCC mesh-node visibilities (dict of name,
// visible), 0xD4 flags, 0xE0 skeleton pointer (skeletal only).

fn parse_model(
    r: &Reader<'_>,
    name: &str,
    at: i64,
    luts: &Luts,
    unsupported: &mut BTreeSet<String>,
    budget: &Budget,
) -> R<BannerModel> {
    let kind = r.u32(at)?;
    if kind != 0x4000_0012 && kind != 0x4000_0092 {
        return Err(FormatError);
    }
    // The cached world at 0x84 with the node's scale (0x30) on top.
    let cached: [f32; 12] = vec(r, at + 0x84)?;
    let node_scale: [f32; 3] = vec(r, at + 0x30)?;
    let mut world = [0.0; 12];
    for (i, w) in world.iter_mut().enumerate() {
        *w = if i % 4 == 3 { cached[i] } else { cached[i] * node_scale[i % 4] };
    }
    let mut materials = Vec::new();
    for (n, p) in dict(r, at + 0xBC)? {
        materials.push(Arc::new(parse_material(r, &n, p, luts, unsupported)?));
    }
    let shapes = list(r, at + 0xC4)?;
    // A mesh-node visibility entry: 0 name, 4 visible.
    let mut visibilities = BTreeMap::new();
    for (n, p) in dict(r, at + 0xCC)? {
        visibilities.insert(n, r.i32(p + 4)? != 0);
    }
    let bones = if kind == 0x4000_0092 {
        match r.ptr(at + 0xE0)? {
            0 => Vec::new(),
            p => parse_skeleton(r, p)?,
        }
    } else {
        Vec::new()
    };
    let mut shape_data: BTreeMap<i32, ShapeDecode> = BTreeMap::new();
    let mut meshes = Vec::new();
    for mesh_at in list(r, at + 0xB4)? {
        let shape_index = r.i32(mesh_at + 0x18)?;
        let material_index = r.i32(mesh_at + 0x1C)?;
        let Some(&shape_at) = usize::try_from(shape_index).ok().and_then(|i| shapes.get(i)) else { continue };
        let Some(material) = usize::try_from(material_index).ok().and_then(|i| materials.get(i)) else { continue };
        let node_name = name_at(r, mesh_at + 0x70)?.unwrap_or_default();
        let visible = r.u8(mesh_at + 0x24)? != 0 && visibilities.get(&node_name).copied().unwrap_or(true);
        let priority = r.u8(mesh_at + 0x25)?;
        if let alloc::collections::btree_map::Entry::Vacant(e) = shape_data.entry(shape_index) {
            e.insert(decode_shape(r, shape_at, budget)?);
        }
        for sub in &shape_data[&shape_index].sub_meshes {
            meshes.push(BannerMesh {
                name: format!("{name}/{node_name}"),
                vertices: shape_data[&shape_index].vertices.clone(),
                indices: sub.indices.clone(),
                skinning: sub.skinning,
                bone_palette: sub.palette.clone(),
                material: material.clone(),
                node_name: node_name.clone(),
                visible,
                render_priority: priority,
            });
        }
    }
    Ok(BannerModel { name: name.into(), world, meshes, bones, visible: r.i32(at + 0xD4)? & 1 != 0 })
}

/// 0x18 bones (dict), 0x20 root bone pointer. Bone: 0 name, 4 flags, 8 index, 0xC parent index, 0x20 scale, 0x2C rotation,
/// 0x38 translation, 0x74 world 3x4, 0xA4 inverse world 3x4, 0xD4 billboard mode.
fn parse_skeleton(r: &Reader<'_>, at: i64) -> R<Vec<BannerBone>> {
    let mut out = Vec::new();
    for (name, b) in dict(r, at + 0x18)? {
        out.push(BannerBone {
            name,
            parent: r.i32(b + 0xC)?,
            scale: vec(r, b + 0x20)?,
            rotation: vec(r, b + 0x2C)?,
            translation: vec(r, b + 0x38)?,
            world: vec(r, b + 0x74)?,
            inverse_world: vec(r, b + 0xA4)?,
            billboard: r.i32(b + 0xD4)?,
            flags: r.i32(b + 4)?,
        });
    }
    Ok(out)
}

// ---- shapes. 0x18 flags, 0x1C bounding box, 0x20 position offset (3 floats), 0x2C sub-meshes (list), 0x38 vertex buffers
// (list). Sub-mesh: 0 bone palette (count, pointer to ints), 8 skinning, 0xC faces (list). Face: 0 descriptors (list).
// Descriptor: 0 GL index type, 4 primitive mode (byte), 8 byte count, 0xC pointer to the indices.

struct SubMeshDecode {
    indices: Vec<u32>,
    skinning: Skinning,
    palette: Vec<i32>,
}

struct ShapeDecode {
    vertices: Arc<VertexData>,
    sub_meshes: Vec<SubMeshDecode>,
}

/// One attribute's values: `count` vertices of `elements` components, or (count -1) one fixed vector repeated.
struct AttrStream {
    name: i32,
    elements: i32,
    count: i64,
    source: Source,
}

enum Source {
    /// No data: zeros.
    Empty,
    Fixed(Vec<f32>),
    Buffer {
        format: i32,
        data: i64,
        stride: i64,
        offset: i64,
        size: i64,
        scale: f32,
    },
}

impl AttrStream {
    fn value(&self, r: &Reader<'_>, vertex: i64, component: i64) -> R<f32> {
        match &self.source {
            Source::Empty => Ok(0.0),
            Source::Fixed(values) => {
                Ok(usize::try_from(component).ok().and_then(|c| values.get(c)).copied().unwrap_or(0.0))
            }
            Source::Buffer { format, data, stride, offset, size, scale } => {
                let p = data + vertex * stride + offset + component * size;
                let raw = match format {
                    0x1400 => f32::from(r.u8(p)? as u8 as i8),
                    0x1401 => r.u8(p)? as f32,
                    0x1402 => f32::from(r.u16(p)? as u16 as i16),
                    _ => r.f32(p)?,
                };
                Ok(raw * scale)
            }
        }
    }
}

fn decode_shape(r: &Reader<'_>, at: i64, budget: &Budget) -> R<ShapeDecode> {
    let offset: [f32; 3] = vec(r, at + 0x20)?;
    let mut streams = Vec::new();
    for vb in list(r, at + 0x38)? {
        match r.u32(vb)? {
            TYPE_ATTRIBUTE => streams.push(read_attribute(r, vb, r.i32(vb + 0x14)?, r.ptr(vb + 0x18)?, None, 0)?),
            TYPE_INTERLEAVED => {
                let stride = r.i32(vb + 0x24)?;
                let len = r.i32(vb + 0x14)?;
                let data = r.ptr(vb + 0x18)?;
                for a in list(r, vb + 0x28)? {
                    streams.push(read_attribute(r, a, len, data, Some(i64::from(stride)), r.i32(a + 0x30)?)?);
                }
            }
            TYPE_FIXED => streams.push(read_fixed(r, vb)?),
            _ => {}
        }
    }
    let count = streams.iter().find(|s| s.count >= 0).map_or(0, |s| s.count);
    if count <= 0 || count > MAX_VERTICES {
        return Err(FormatError);
    }
    // A stream with fewer elements than `width` reads the rest as 0, and a colour's missing alpha as 1. Bone weights pad with 0.
    let floats = |name: i32, width: i64, missing_alpha: f32| -> R<Option<Vec<f32>>> {
        let Some(s) = streams.iter().find(|s| s.name == name) else { return Ok(None) };
        budget.spend((count * width) as usize)?;
        let mut out = Vec::with_capacity((count * width) as usize);
        for v in 0..count {
            for c in 0..width {
                out.push(if c < i64::from(s.elements) {
                    s.value(r, v, c)?
                } else if c == 3 && width == 4 {
                    missing_alpha
                } else {
                    0.0
                });
            }
        }
        Ok(Some(out))
    };
    let mut positions = floats(0, 3, 1.0)?.ok_or(FormatError)?;
    for p in positions.chunks_mut(3) {
        p[0] += offset[0];
        p[1] += offset[1];
        p[2] += offset[2];
    }
    let bone_indices = match streams.iter().find(|s| s.name == 7) {
        None => None,
        Some(s) => {
            budget.spend(count as usize * 4)?;
            let mut out = Vec::with_capacity(count as usize * 4);
            for i in 0..count * 4 {
                out.push(if i % 4 < i64::from(s.elements) { s.value(r, i / 4, i % 4)? as i32 } else { 0 });
            }
            Some(out)
        }
    };
    let vertices = VertexData {
        count: count as usize,
        positions,
        normals: floats(1, 3, 1.0)?,
        uv0: floats(4, 2, 1.0)?,
        uv1: floats(5, 2, 1.0)?,
        uv2: floats(6, 2, 1.0)?,
        colors: floats(3, 4, 1.0)?,
        bone_indices,
        bone_weights: floats(8, 4, 0.0)?,
    };
    let mut sub_meshes = Vec::new();
    for sub in list(r, at + 0x2C)? {
        let palette_count = (r.u32(sub)? as i32).clamp(0, 64);
        let palette_ptr = r.ptr(sub + 4)?;
        let palette = (0..i64::from(palette_count)).map(|i| r.i32(palette_ptr + 4 * i)).collect::<R<Vec<_>>>()?;
        let skinning = Skinning::from_index(r.i32(sub + 8)?);
        let mut indices = Vec::new();
        for face in list(r, sub + 0xC)? {
            for desc in list(r, face)? {
                read_face(r, desc, count, &mut indices, budget)?;
            }
        }
        sub_meshes.push(SubMeshDecode { indices, skinning, palette });
    }
    Ok(ShapeDecode { vertices: Arc::new(vertices), sub_meshes })
}

fn read_face(r: &Reader<'_>, desc: i64, vertex_count: i64, out: &mut Vec<u32>, budget: &Budget) -> R<()> {
    let wide = r.u32(desc)? == 0x1403;
    let mode = r.u8(desc + 4)?;
    let size = r.i32(desc + 8)?;
    let data = r.ptr(desc + 0xC)?;
    if data == 0 || size <= 0 {
        return Ok(());
    }
    let n = i64::from(if wide { size / 2 } else { size });
    budget.spend(n as usize * 3)?;
    let mut idx = Vec::new();
    for i in 0..n {
        let v = if wide { r.u16(data + 2 * i)? } else { r.u8(data + i)? };
        if i64::from(v) >= vertex_count {
            return Err(FormatError);
        }
        idx.push(v as u32);
    }
    let n = idx.len();
    match mode {
        0 => out.extend_from_slice(&idx[..n - n % 3]),
        1 => {
            for i in 0..n.saturating_sub(2) {
                if i % 2 == 0 {
                    out.extend_from_slice(&[idx[i], idx[i + 1], idx[i + 2]]);
                } else {
                    out.extend_from_slice(&[idx[i + 1], idx[i], idx[i + 2]]);
                }
            }
        }
        2 => {
            for i in 1..n.saturating_sub(1) {
                out.extend_from_slice(&[idx[0], idx[i], idx[i + 1]]);
            }
        }
        _ => {}
    }
    Ok(())
}

fn read_attribute(
    r: &Reader<'_>,
    at: i64,
    len: i32,
    data: i64,
    interleaved_stride: Option<i64>,
    offset: i32,
) -> R<AttrStream> {
    let name = r.i32(at + 4)?;
    let format = r.u32(at + 0x24)? as i32;
    let elements = r.i32(at + 0x28)?.clamp(1, 4);
    let scale = r.f32(at + 0x2C)?;
    let size: i64 = match format {
        0x1400 | 0x1401 => 1,
        0x1402 => 2,
        0x1406 => 4,
        _ => return Err(FormatError),
    };
    let stride = interleaved_stride.unwrap_or(i64::from(elements) * size);
    if data == 0 || stride <= 0 {
        return Ok(AttrStream { name, elements, count: 0, source: Source::Empty });
    }
    Ok(AttrStream {
        name,
        elements,
        count: i64::from(len) / stride,
        source: Source::Buffer { format, data, stride, offset: i64::from(offset), size, scale },
    })
}

/// Fixed attribute: 4 name, 0xC format, 0x10 elements, 0x14 zero (no scale: the floats are the values), 0x18 float count,
/// 0x1C pointer to the floats.
fn read_fixed(r: &Reader<'_>, at: i64) -> R<AttrStream> {
    let name = r.i32(at + 4)?;
    let elements = r.i32(at + 0x10)?.min(r.i32(at + 0x18)?).clamp(1, 4);
    let vec_at = r.ptr(at + 0x1C)?;
    let values = if vec_at == 0 {
        Vec::new()
    } else {
        (0..i64::from(elements)).map(|i| r.f32(vec_at + 4 * i)).collect::<R<Vec<_>>>()?
    };
    Ok(AttrStream { name, elements, count: -1, source: Source::Fixed(values) })
}

// ---- materials (0x08000000). 0x18 flags, 0x24 colours (11 float vectors: emission, ambient, diffuse, spec0, spec1,
// constant 0-5), 0x104 rasterization (4 culling), 0x118 depth (0 flags: 1 test, 2 write), 0x12C blend (0 mode), 0x16C three
// texture coords of 0x58 bytes (0 source UV set, 0x10 scale, 0x18 rotation, 0x1C translation), 0x274 three texture-mapper
// pointers (8 texture reference, 0x10 PICA commands), 0x288 fragment shader pointer (0 buffer colour, 0x10 lighting,
// 0x2C six 0x1C-byte texture environments, 0xD4 alpha test commands). Lighting block: 0x10 flags (1 clamp highlights,
// 2 D0, 4 D1, 8 geometric factor 0, 0x10 geometric factor 1, 0x20 reflection), 0x14 layer configuration, 0x18 Fresnel
// selector, 0x1C bump texture, 0x20 bump mode, 0x24 renormalise, 0x28 pointer to six table-use pointers.

fn scale_of(v: i32) -> i32 {
    match v {
        1 => 2,
        2 => 4,
        _ => 1,
    }
}

fn operand_count(mode: CombinerMode) -> usize {
    match mode {
        CombinerMode::Replace => 1,
        CombinerMode::Interpolate | CombinerMode::MultAdd | CombinerMode::AddMult => 3,
        _ => 2,
    }
}

fn reads_lighting(s: &TexEnvStage) -> bool {
    let lit = |v: &i32| (1..=2).contains(v);
    s.color_sources.iter().take(operand_count(s.color_mode)).any(lit)
        || (s.color_mode != CombinerMode::DotProduct3Rgba
            && s.alpha_sources.iter().take(operand_count(s.alpha_mode)).any(lit))
}

fn rgba8(word: i32) -> [f32; 4] {
    [
        (word & 255) as f32 / 255.0,
        ((word >> 8) & 255) as f32 / 255.0,
        ((word >> 16) & 255) as f32 / 255.0,
        ((word >> 24) & 255) as f32 / 255.0,
    ]
}

fn parse_material(
    r: &Reader<'_>,
    name: &str,
    at: i64,
    luts: &Luts,
    unsupported: &mut BTreeSet<String>,
) -> R<BannerMaterial> {
    if r.u32(at)? != 0x0800_0000 {
        return Err(FormatError);
    }
    let flags = r.i32(at + 0x18)?;
    let mut colors = [[0.0f32; 4]; 11];
    for (i, c) in colors.iter_mut().enumerate() {
        *c = vec(r, at + 0x24 + 16 * i as i64)?;
    }
    let culling = r.i32(at + 0x108)?;
    // Depth flags: bit 0 test, bit 1 write (the byte-masked command after them never says whether to write).
    let depth_flags = r.i32(at + 0x118)?;
    let blend_mode = r.i32(at + 0x12C)?;
    let blend_commands = pica_commands::read(r, at + 0x140, 6)?;
    let mut slots = Vec::new();
    for i in 0..3i64 {
        let mapper = r.ptr(at + 0x274 + 4 * i)?;
        if mapper == 0 {
            slots.push(None);
            continue;
        }
        let reference = r.ptr(mapper + 8)?;
        let path = if reference == 0 { String::new() } else { name_at(r, reference + 0x18)?.unwrap_or_default() };
        let cmds = pica_commands::read(r, mapper + 0x10, 14)?;
        // The unit's param and border colour commands (texture unit 0: 0x83 and 0x81, 1: 0x93 and 0x91, 2: 0x9B and 0x99).
        let unit = [0x83, 0x93, 0x9B].into_iter().find(|u| cmds.contains_key(u));
        let param = unit.and_then(|u| cmds.get(&u).copied()).unwrap_or(0) as u32;
        let border_word = unit.and_then(|u| cmds.get(&(u - 2)).copied()).unwrap_or(0) as u32;
        let coord = at + 0x16C + 0x58 * i64::from(coords::coordinator_of(i as i32, r.i32(at + 0x1C)?));
        let matrix_mode = r.i32(coord + 0xC)?;
        slots.push(Some(TextureSlot {
            texture: path,
            wrap_v: ((param >> 8) & 3) as i32,
            wrap_u: ((param >> 12) & 3) as i32,
            linear: param & 2 != 0,
            uv_set: r.i32(coord)?,
            scale: [r.f32(coord + 0x10)?, r.f32(coord + 0x14)?],
            rotation: r.f32(coord + 0x18)?,
            translation: [r.f32(coord + 0x1C)?, r.f32(coord + 0x20)?],
            mapping: r.i32(coord + 4)?,
            matrix_mode: if (0..=2).contains(&matrix_mode) { matrix_mode } else { coords::MAYA },
            border: core::array::from_fn(|k| ((border_word >> (8 * k)) & 0xFF) as f32 / 255.0),
        }));
    }
    let frag = r.ptr(at + 0x288)?;
    let mut stages: Vec<TexEnvStage> = Vec::new();
    let mut buffer_color = [0.0; 4];
    let (mut alpha_test, mut alpha_function, mut alpha_ref) = (false, 1, 0.0);
    let mut lighting_setup = None;
    let (mut bump_mode, mut bump_slot, mut bump_renormalize) = (0, 0, false);
    if frag != 0 && flags & 1 != 0 {
        let lf = r.i32(frag + 0x10)?;
        bump_mode = r.i32(frag + 0x20)?;
        bump_slot = r.i32(frag + 0x1C)?.clamp(0, 2);
        bump_renormalize = r.i32(frag + 0x24)? != 0;
        let table = r.ptr(frag + 0x28)?;
        let mut samplers = Vec::new();
        for i in 0..6 {
            samplers.push(match table {
                0 => None,
                _ => match r.ptr(table + 4 * i)? {
                    0 => None,
                    p => lut_sampler(r, p, luts)?,
                },
            });
        }
        lighting_setup = Some(FragmentLightingSetup {
            clamp_highlights: lf & 1 != 0,
            distribution0: lf & 2 != 0,
            distribution1: lf & 4 != 0,
            geometric0: lf & 8 != 0,
            geometric1: lf & 0x10 != 0,
            reflection: lf & 0x20 != 0,
            layer_config: r.i32(frag + 0x14)?.clamp(0, 7),
            fresnel_selector: r.i32(frag + 0x18)? & 3,
            samplers,
        });
    }
    if frag != 0 {
        let update_commands = pica_commands::read(r, frag + 0xDC, 6)?;
        let update = update_commands.get(&0x0E0).copied().unwrap_or(0);
        if let Some(&c) = update_commands.get(&0x0FD) {
            buffer_color = rgba8(c);
        }
        for (s, &base) in TEXENV_REGISTERS.iter().enumerate() {
            let te = frag + 0x2C + 0x1C * s as i64;
            let constant_index = r.i32(te)?;
            let cmd = pica_commands::read(r, te + 4, 6)?;
            let reg = |k: i32| cmd.get(&(base + k)).copied().unwrap_or(0);
            let (src, operand, combiner, color, scale) = (reg(0), reg(1), reg(2), reg(3), reg(4));
            let constant =
                if (0..=5).contains(&constant_index) { colors[5 + constant_index as usize] } else { rgba8(color) };
            let stage = TexEnvStage {
                color_sources: [src & 15, (src >> 4) & 15, (src >> 8) & 15],
                alpha_sources: [(src >> 16) & 15, (src >> 20) & 15, (src >> 24) & 15],
                color_operands: [operand & 15, (operand >> 4) & 15, (operand >> 8) & 15],
                alpha_operands: [(operand >> 12) & 7, (operand >> 16) & 7, (operand >> 20) & 7],
                color_mode: CombinerMode::from_index(combiner & 15),
                alpha_mode: CombinerMode::from_index((combiner >> 16) & 15),
                color_scale: scale_of(scale & 3),
                alpha_scale: scale_of((scale >> 16) & 3),
                constant,
                update_color_buffer: (1..=4).contains(&s) && update & (0x100 << (s - 1)) != 0,
                update_alpha_buffer: (1..=4).contains(&s) && update & (0x1000 << (s - 1)) != 0,
                constant_index,
            };
            if !(0..=5).contains(&constant_index) && stage.constant.iter().all(|&v| v == 0.0) {
                unsupported.insert("texenv-constant-source".into());
            }
            stages.push(stage);
        }
        if let Some(&at2) = pica_commands::read(r, frag + 0xD4, 2)?.get(&0x104) {
            alpha_test = at2 & 1 != 0;
            alpha_function = (at2 >> 4) & 7;
            alpha_ref = ((at2 >> 8) & 255) as f32 / 255.0;
        }
    }
    if stages.iter().any(|s| matches!(s.color_mode, CombinerMode::DotProduct3Rgb | CombinerMode::DotProduct3Rgba)) {
        unsupported.insert("dot3-combiner".into());
    }
    if let Some(l) = &lighting_setup {
        if stages.iter().any(reads_lighting) {
            // 1 is the tangent-space normal map the renderers draw.
            if bump_mode != 0 && bump_mode != 1 {
                unsupported.insert("fragment-lighting-bump".into());
            }
            if l.samplers.iter().any(|s| s.as_ref().is_some_and(|s| s.input == lighting::INPUT_CP)) {
                unsupported.insert("lighting-input-cos-phi".into());
            }
        }
    }
    if blend_mode == 3 {
        unsupported.insert("logic-op-blend".into());
    }
    Ok(BannerMaterial {
        name: name.into(),
        slots,
        stages,
        buffer_color,
        diffuse: colors[2],
        ambient: colors[1],
        emission: colors[0],
        fragment_lighting: flags & 1 != 0,
        alpha_test_enabled: alpha_test,
        alpha_test_function: alpha_function,
        alpha_test_reference: alpha_ref,
        blend_mode,
        blend_function: blend_commands.get(&0x101).copied().unwrap_or(0),
        blend_color: [0.0; 4],
        face_culling: culling,
        depth_test: depth_flags & 1 != 0,
        depth_write: depth_flags & 2 != 0,
        translucency: r.i32(at + 0x20)?,
        specular0: colors[3],
        specular1: colors[4],
        lighting: lighting_setup,
        logic_op: 3,
        light_set: r.i32(at + 0x298)?,
        bump_mode,
        bump_slot,
        bump_renormalize,
        vertex_lighting: flags & 2 != 0,
        hemisphere_lighting: flags & 4 != 0,
    })
}
