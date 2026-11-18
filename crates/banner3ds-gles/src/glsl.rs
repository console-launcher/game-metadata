// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! GLSL ES 3.00 for the renderer, inside WebGL 2's subset (no extensions: `texelFetch`, `dFdx`, float textures read unfiltered
//! and `MIRRORED_REPEAT` are all core). The fragment shader is `banner3ds-soft`'s combiner evaluation written out stage for
//! stage with every source, operand and mode baked in, and its fragment lighting with the light geometry and table rows baked
//! in; a change to one must change the other (the tests pin the shape of the generated code).
//!
//! The renderer poses, skins and places every vertex on the CPU exactly as the soft renderer does, so positions and normals
//! arrive in world space and the shaders only project, transform texture coordinates and shade.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::Write;

use banner3ds_scene::coords::SPHERE;
use banner3ds_scene::draw::light;
use banner3ds_scene::lighting::{D0, D1, FR, MAX_LIGHTS, RB, RG, RR};
use banner3ds_scene::{
    BannerLight, BannerMaterial, BannerScene, CombinerMode, FragmentLightingSetup, LightingLut, LutSampler,
};

/// Bump mode 1: a tangent-space normal map tilts the lit normal.
const BUMP_NORMAL_MAP: i32 = 1;

/// Every lighting table a scene's lit materials and lights read, one 256-texel row each, each texel (value, difference to the
/// next, 0, 0): an RGBA32F texture read with `texelFetch` the way the soft renderer indexes its tables.
#[derive(Debug, Clone, Default)]
pub struct LutAtlas {
    index: BTreeMap<usize, i32>,
    /// `rows() * 256 * 4` floats, row by row.
    pub data: Vec<f32>,
}

impl LutAtlas {
    /// The tables of `scene` (each one once, by identity).
    pub fn of(scene: &BannerScene) -> LutAtlas {
        fn add<'s>(
            tables: &mut Vec<&'s LightingLut>,
            index: &mut BTreeMap<usize, i32>,
            lut: Option<&'s Arc<LightingLut>>,
        ) {
            if let Some(lut) = lut {
                let key = Arc::as_ptr(lut) as usize;
                if let alloc::collections::btree_map::Entry::Vacant(e) = index.entry(key) {
                    e.insert(tables.len() as i32);
                    tables.push(lut);
                }
            }
        }
        let mut tables: Vec<&LightingLut> = Vec::new();
        let mut index = BTreeMap::new();
        for model in &scene.models {
            for mesh in &model.meshes {
                let m = &mesh.material;
                if m.fragment_lighting {
                    if let Some(setup) = &m.lighting {
                        for s in setup.samplers.iter().flatten() {
                            add(&mut tables, &mut index, Some(&s.table));
                        }
                    }
                }
            }
        }
        for l in &scene.lights {
            add(&mut tables, &mut index, l.spot.as_ref().map(|s| &s.table));
            add(&mut tables, &mut index, l.distance.as_ref());
        }
        let mut data = alloc::vec![0f32; tables.len() * 256 * 4];
        for (row, lut) in tables.iter().enumerate() {
            for i in 0..256 {
                data[(row * 256 + i) * 4] = lut.values.get(i).copied().unwrap_or(0.0);
                data[(row * 256 + i) * 4 + 1] = lut.deltas.get(i).copied().unwrap_or(0.0);
            }
        }
        LutAtlas { index, data }
    }

    /// How many tables (texture rows).
    pub fn rows(&self) -> usize {
        self.index.len()
    }

    /// The row of `lut`, or -1 when the scene does not read it.
    pub fn row_of(&self, lut: &Arc<LightingLut>) -> i32 {
        self.index.get(&(Arc::as_ptr(lut) as usize)).copied().unwrap_or(-1)
    }
}

/// The texture-coordinate transform (Maya, SoftImage 1, Max 2), the soft renderer's `shading::transform`.
const XF: &str = "vec2 xf(vec2 uv, int s, int mode) {
  float c = cos(uRot[s]);
  float n = sin(uRot[s]);
  vec2 t = uTrans[s];
  if (mode == 2) {
    vec2 m = vec2(uv.x - t.x - 0.5, uv.y + t.y - 0.5);
    return uScale[s] * vec2(m.x * c - m.y * n, m.x * n + m.y * c) + 0.5;
  }
  if (mode == 1) return uScale[s] * vec2(c * (uv.x - t.x) - n * (uv.y + t.y), n * (uv.x + t.x) + c * (uv.y - t.y));
  vec2 d = uv - 0.5;
  return uScale[s] * (vec2(d.x * c - d.y * n, d.x * n + d.y * c) + 0.5 - uTrans[s]);
}
";

const SLOT_UNIFORMS: &str = "uniform vec2 uScale[3];\nuniform float uRot[3];\nuniform vec2 uTrans[3];\n";

/// The vertex shader. Uniforms: `uMvp` (view-projection: positions arrive in world space); per texture slot `uScale[s]`,
/// `uRot[s]`, `uTrans[s]`. Slot s reads UV set `uv_sets[s]` through transform type `modes[s]`, or, where `mappings[s]` is
/// sphere (only when the mesh has normals), leaves its coordinates to the fragment shader (`vSph`, the view-space normal, with
/// `uView`). `lit` passes the world position and normal on (`aNormal`).
pub fn vertex(uv_sets: [i32; 3], lit: bool, mappings: [i32; 3], modes: [i32; 3]) -> String {
    let mut s = String::from("#version 300 es\nuniform mat4 uMvp;\n");
    s += SLOT_UNIFORMS;
    s += "in vec3 aPos;\nin vec2 aUv0;\nin vec2 aUv1;\nin vec2 aUv2;\nin vec4 aColor;\n";
    s += "out vec2 vUv0;\nout vec2 vUv1;\nout vec2 vUv2;\nout vec4 vColor;\n";
    let sphere = mappings.contains(&SPHERE);
    if lit || sphere {
        s += "in vec3 aNormal;\n";
    }
    if lit {
        s += "out vec3 vWorld;\nout vec3 vNormal;\n";
    }
    if sphere {
        s += "uniform mat4 uView;\nout vec3 vSph;\n";
    }
    s += XF;
    s += "void main() {\n";
    for k in 0..3 {
        if mappings[k] == SPHERE {
            let _ = writeln!(s, "  vUv{k} = vec2(0.0);");
        } else {
            let _ = writeln!(s, "  vUv{k} = xf(aUv{}, {k}, {});", uv_sets[k].clamp(0, 2), modes[k]);
        }
    }
    s += "  vColor = aColor;\n";
    if lit {
        s += "  vWorld = aPos;\n  vNormal = aNormal;\n";
    }
    if sphere {
        s += "  vSph = normalize(mat3(uView) * aNormal);\n";
    }
    s += "  gl_Position = uMvp * vec4(aPos, 1.0);\n}\n";
    s
}

/// Whether slot `s` of `material` maps a sphere environment (which needs normals).
pub fn sphere_slot(material: &BannerMaterial, s: usize, has_normals: bool) -> bool {
    has_normals && material.slots.get(s).and_then(|x| x.as_ref()).is_some_and(|x| x.mapping == SPHERE)
}

/// The fragment shader. `bound` says which of the three texture units hold a texture (an unbound one reads white, as in the
/// soft renderer); uniforms `uTex0..2`, `uConst[stage]` (each stage's constant colour, set per frame so animations reach it)
/// and `uBuffer`. A material with fragment lighting and an `atlas` also lights each of `lights` the soft renderer's way, light
/// geometry and table rows baked in, colour terms from `uGAmb` and `uLAmb/uLDif/uLS0/uLS1[i]`, the eye from `uEye`, the
/// turntable's turn of the baked point and spot light geometry from `uTurn` (directional lights keep their direction).
pub fn fragment(
    material: &BannerMaterial,
    bound: [bool; 3],
    lights: &[&BannerLight],
    atlas: Option<&LutAtlas>,
    has_normals: bool,
) -> String {
    let mut s = String::from("#version 300 es\nprecision highp float;\n");
    s += "uniform sampler2D uTex0;\nuniform sampler2D uTex1;\nuniform sampler2D uTex2;\n";
    s += "uniform vec4 uConst[6];\nuniform vec4 uBuffer;\n";
    s += "in vec2 vUv0;\nin vec2 vUv1;\nin vec2 vUv2;\nin vec4 vColor;\nout vec4 oColor;\n";
    let sphere: [bool; 3] = core::array::from_fn(|k| bound[k] && sphere_slot(material, k, has_normals));
    if sphere.contains(&true) {
        s += SLOT_UNIFORMS;
        s += "in vec3 vSph;\n";
        s += XF;
    }
    let slot = |k: usize| material.slots.get(k).and_then(|x| x.as_ref());
    let uv = |k: usize| if sphere[k] { format!("sUv{k}") } else { format!("vUv{k}") };
    // GL's own sampler where it reads what the soft renderer reads (nearest, no border); the soft renderer's sampling written
    // out where it does not (linear filtering clamps the neighbours of a wrapped coordinate to the edge; the border colour is
    // per texel).
    let soft_sampled = |k: usize| slot(k).is_some_and(|x| x.linear || x.wrap_u == 1 || x.wrap_v == 1);
    let read = |k: usize| match slot(k).filter(|_| soft_sampled(k)) {
        Some(x) => format!(
            "sampleS(uTex{k}, {}, {}, {}, {}, vec4({}, {}, {}, {}))",
            uv(k),
            x.wrap_u,
            x.wrap_v,
            x.linear,
            f(x.border[0]),
            f(x.border[1]),
            f(x.border[2]),
            f(x.border[3])
        ),
        None => format!("texture(uTex{k}, vec2({0}.x, 1.0 - {0}.y))", uv(k)),
    };
    if (0..3).any(|k| bound[k] && soft_sampled(k)) {
        s += SAMPLE;
    }
    let lit = material.fragment_lighting && atlas.is_some();
    let bump = usize::try_from(material.bump_slot).ok().filter(|&b| {
        material.bump_mode == BUMP_NORMAL_MAP
            && has_normals
            && bound.get(b).copied().unwrap_or(false)
            && slot(b).is_some_and(|x| x.mapping != SPHERE)
    });
    match atlas.filter(|_| lit) {
        Some(atlas) => {
            let bumped = bump.map(|b| {
                format!("bumpN(normalize(vNormal), vWorld, vUv{b}, {}, {})", read(b), material.bump_renormalize)
            });
            let n = lights.len().min(MAX_LIGHTS);
            lighting(&mut s, material.lighting.as_ref(), &lights[..n], atlas, has_normals, bumped.as_deref());
        }
        None => s += "void main() {\n",
    }
    for (k, &on) in sphere.iter().enumerate() {
        if on {
            let mode = slot(k).map_or(0, |x| x.matrix_mode);
            let _ = writeln!(s, "  vec2 sUv{k} = xf(normalize(vSph).xy * 0.5 + 0.5, {k}, {mode});");
        }
    }
    for (k, &on) in bound.iter().enumerate() {
        if !on {
            let _ = writeln!(s, "  vec4 tex{k} = vec4(1.0);");
            continue;
        }
        let _ = writeln!(s, "  vec4 tex{k} = {};", read(k));
    }
    s += "  vec4 primary = vColor;\n  vec4 prev = primary;\n  vec4 buf = uBuffer;\n";
    let stages = &material.stages;
    for (i, st) in stages.iter().enumerate() {
        s += "  {\n";
        for k in 0..3 {
            let _ =
                writeln!(s, "    vec3 c{k} = {};", color_operand(st.color_sources[k], st.color_operands[k], i, lit));
        }
        for k in 0..3 {
            let _ =
                writeln!(s, "    float a{k} = {};", alpha_operand(st.alpha_sources[k], st.alpha_operands[k], i, lit));
        }
        let _ = writeln!(s, "    vec3 rgb = {};", color_expr(st.color_mode));
        let _ = writeln!(s, "    float al = {};", alpha_expr(st.alpha_mode));
        let _ = writeln!(
            s,
            "    vec4 next = clamp(vec4(rgb * {}.0, al * {}.0), 0.0, 1.0);",
            st.color_scale, st.alpha_scale
        );
        if let Some(following) = stages.get(i + 1) {
            if following.update_color_buffer {
                s += "    buf.rgb = next.rgb;\n";
            }
            if following.update_alpha_buffer {
                s += "    buf.a = next.a;\n";
            }
        }
        s += "    prev = next;\n  }\n";
    }
    if material.alpha_test_enabled {
        let r = material.alpha_test_reference;
        let r = if r.is_finite() { format!("{r:?}") } else { String::from("0.0") };
        let fail = match material.alpha_test_function {
            0 => Some(String::from("true")),
            1 => None,
            2 => Some(format!("abs(prev.a - {r}) >= 0.002")),
            3 => Some(format!("abs(prev.a - {r}) < 0.002")),
            4 => Some(format!("!(prev.a < {r})")),
            5 => Some(format!("!(prev.a <= {r})")),
            6 => Some(format!("!(prev.a > {r})")),
            _ => Some(format!("!(prev.a >= {r})")),
        };
        if let Some(fail) = fail {
            let _ = writeln!(s, "  if ({fail}) discard;");
        }
    }
    // An opaque material writes alpha 1, as the soft renderer does.
    if material.blend_mode == 0 {
        s += "  oColor = vec4(prev.rgb, 1.0);\n";
    } else {
        s += "  oColor = prev;\n";
    }
    s += "}\n";
    s
}

/// The soft renderer's texture sampling (`raster::sample`): the coordinate wrapped (0 clamp to edge, 1 clamp to border,
/// 2 repeat, 3 mirror), then the nearest texel or the four around it, each clamped to the edge or, along a clamp-to-border
/// axis, the border colour off the texture. Row 0 of the texture is the top (the upload order), as in the soft renderer.
const SAMPLE: &str = "vec4 texelS(sampler2D t, ivec2 size, int x, int y, int wu, int wv, vec4 border) {
  if ((wu == 1 && (x < 0 || x >= size.x)) || (wv == 1 && (y < 0 || y >= size.y))) return border;
  return texelFetch(t, clamp(ivec2(x, y), ivec2(0), size - 1), 0);
}
float wrapS(float c, int mode) {
  if (mode == 2) return c - floor(c);
  if (mode == 3) { float m = c - 2.0 * floor(c / 2.0); return m > 1.0 ? 2.0 - m : m; }
  if (mode == 1) return c;
  return clamp(c, 0.0, 1.0);
}
vec4 sampleS(sampler2D t, vec2 uv, int wu, int wv, bool linear, vec4 border) {
  ivec2 size = textureSize(t, 0);
  float fx = wrapS(uv.x, wu) * float(size.x) - 0.5;
  float fy = (1.0 - wrapS(uv.y, wv)) * float(size.y) - 0.5;
  if (!linear) return texelS(t, size, int(floor(fx + 0.5)), int(floor(fy + 0.5)), wu, wv, border);
  float x0 = floor(fx);
  float y0 = floor(fy);
  int ix = int(x0);
  int iy = int(y0);
  float tx = fx - x0;
  float ty = fy - y0;
  vec4 a = texelS(t, size, ix, iy, wu, wv, border);
  vec4 b = texelS(t, size, ix + 1, iy, wu, wv, border);
  vec4 c = texelS(t, size, ix, iy + 1, wu, wv, border);
  vec4 d = texelS(t, size, ix + 1, iy + 1, wu, wv, border);
  return (a * (1.0 - tx) + b * tx) * (1.0 - ty) + (c * (1.0 - tx) + d * tx) * ty;
}
";

/// A float literal (non-finite values as 0).
fn f(x: f32) -> String {
    if x.is_finite() { format!("float({x:?})") } else { String::from("0.0") }
}

fn vec3(a: [f32; 3]) -> String {
    format!("vec3({}, {}, {})", f(a[0]), f(a[1]), f(a[2]))
}

/// The tangent frame from screen derivatives (for a flat triangle with affine texture coordinates, the per-triangle tangents
/// the soft renderer solves for) and the soft renderer's perturbation.
const BUMP: &str = "vec3 bumpN(vec3 N, vec3 p, vec2 uv, vec4 t, bool renorm) {
  vec3 e1 = dFdx(p); vec3 e2 = dFdy(p); vec2 d1 = dFdx(uv); vec2 d2 = dFdy(uv);
  float det = d1.x * d2.y - d2.x * d1.y;
  if (abs(det) < 1e-12) return N;
  vec3 T = (e1 * d2.y - e2 * d1.y) / det;
  vec3 B = (e2 * d1.x - e1 * d2.x) / det;
  vec3 m = t.rgb * 2.0 - 1.0;
  if (renorm) m.z = sqrt(max(0.0, 1.0 - dot(m.xy, m.xy)));
  vec3 Tp = T - N * dot(N, T);
  if (dot(Tp, Tp) < 1e-12) return N;
  vec3 Tn = normalize(Tp);
  vec3 Bn = cross(N, Tn) * (dot(cross(N, T), B) < 0.0 ? -1.0 : 1.0);
  return normalize(Tn * m.x + Bn * m.y + N * m.z);
}
";

/// The uniforms, the table reader and the start of `main()` up to `lp` and `ls`, the lit primary and secondary colours.
fn lighting(
    s: &mut String,
    setup: Option<&FragmentLightingSetup>,
    lights: &[&BannerLight],
    atlas: &LutAtlas,
    has_normals: bool,
    bumped: Option<&str>,
) {
    let n = lights.len().max(1);
    *s += "uniform sampler2D uLut;\nuniform vec3 uEye;\nuniform mat3 uTurn;\nuniform vec3 uGAmb;\n";
    let _ = write!(
        s,
        "uniform vec3 uLAmb[{n}];\nuniform vec3 uLDif[{n}];\nuniform vec3 uLS0[{n}];\nuniform vec3 uLS1[{n}];\n"
    );
    *s += "in vec3 vWorld;\nin vec3 vNormal;\n";
    *s += "float lut(int row, bool absolute, float x) {
  int i; float f;
  if (absolute) { float v = min(abs(x), 1.0) * 256.0; i = min(int(floor(v)), 255); f = v - float(i); }
  else { float v = clamp(x, -1.0, 1.0) * 128.0; int j = clamp(int(floor(v)), -128, 127); f = v - float(j); i = j & 255; }
  vec2 e = texelFetch(uLut, ivec2(i, row), 0).xy;
  return e.x + e.y * f;
}
";
    if bumped.is_some() {
        *s += BUMP;
    }
    *s += "void main() {\n  vec3 V = normalize(uEye - vWorld);\n";
    match bumped {
        Some(b) => {
            let _ = writeln!(s, "  vec3 N = {b};");
        }
        None if has_normals => *s += "  vec3 N = normalize(vNormal);\n",
        None => *s += "  vec3 N = V;\n",
    }
    *s += "  vec3 pc = uGAmb;\n  vec3 sc = vec3(0.0);\n  float fr = 1.0;\n";
    let layer = setup.map_or(0, |x| x.layer_config);
    let input = |i: i32| match i.clamp(0, 5) {
        0 => "dot(N, H)",
        1 => "dot(V, H)",
        2 => "dot(N, V)",
        3 => "nl",
        4 => "sd",
        _ => "0.0",
    };
    let read = |x: Option<&LutSampler>| match x {
        Some(x) if atlas.row_of(&x.table) >= 0 => {
            format!("({} * lut({}, {}, {}))", f(x.scale), atlas.row_of(&x.table), x.table.absolute, input(x.input))
        }
        _ => String::from("1.0"),
    };
    let samplers = setup.map(|x| &x.samplers);
    let sampler = |i: usize| samplers.and_then(|x| x.get(i)).and_then(|x| x.as_ref());
    let table = |index: usize, enabled: bool| {
        if setup.is_none() || !enabled || !light::available(layer, index) {
            String::from("1.0")
        } else {
            read(sampler(index))
        }
    };
    let reflection = setup.is_some_and(|x| x.reflection);
    let channel = |index: usize| {
        if reflection && sampler(index).is_some() && light::available(layer, index) {
            table(index, true)
        } else {
            String::from("rr")
        }
    };
    for (i, l) in lights.iter().enumerate() {
        let _ = writeln!(s, "  {{ // light {i}");
        if l.kind == 0 {
            let d = l.direction;
            let len = libm_sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
            let dir = if len > 0.0 { [-d[0] / len, -d[1] / len, -d[2] / len] } else { [0.0; 3] };
            let _ = writeln!(s, "    vec3 L = {};\n    float dist = 0.0;", vec3(dir));
        } else {
            let _ = writeln!(s, "    vec3 L = uTurn * {} - vWorld;", vec3(l.position));
            *s += "    float dist = length(L);\n    if (dist > 0.0) L /= dist;\n";
        }
        *s += "    float nl = dot(N, L);\n";
        let _ = writeln!(s, "    float dd = {};", if l.two_sided_diffuse { "abs(nl)" } else { "max(nl, 0.0)" });
        *s += "    vec3 h0 = L + V;\n    float h2 = dot(h0, h0);\n    vec3 H = h2 > 0.0 ? h0 / sqrt(h2) : vec3(0.0);\n";
        let d = l.direction;
        let dl = libm_sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
        let sd = if l.kind == 2 && dl > 0.0 {
            format!("-dot(L, uTurn * {})", vec3([d[0] / dl, d[1] / dl, d[2] / dl]))
        } else {
            String::from("0.0")
        };
        let _ = writeln!(s, "    float sd = {sd};");
        let spot = if l.kind == 2 && l.spot.is_some() && light::available(layer, light::SP) {
            read(l.spot.as_ref())
        } else {
            String::from("1.0")
        };
        let atten = match l.distance.as_ref().filter(|t| atlas.row_of(t) >= 0) {
            Some(t) => format!(
                "lut({}, {}, clamp({} + {} * dist, 0.0, 1.0))",
                atlas.row_of(t),
                t.absolute,
                f(l.attenuation_bias),
                f(l.attenuation_scale)
            ),
            None => String::from("1.0"),
        };
        let _ = writeln!(s, "    float k = {spot} * {atten};");
        let _ = writeln!(s, "    pc += (uLDif[{i}] * dd + uLAmb[{i}]) * k;");
        let _ = writeln!(s, "    float d0 = {};", table(D0, setup.is_some_and(|x| x.distribution0)));
        let _ = writeln!(s, "    float d1 = {};", table(D1, setup.is_some_and(|x| x.distribution1)));
        let _ = writeln!(s, "    float rr = {};", table(RR, reflection));
        let _ = writeln!(s, "    vec3 refl = vec3(rr, {}, {});", channel(RG), channel(RB));
        *s += "    float geo = h2 > 0.0 ? min(max(nl, 0.0) / h2, 1.0) : 0.0;\n";
        let _ = writeln!(s, "    float g0 = {};", if setup.is_some_and(|x| x.geometric0) { "geo" } else { "1.0" });
        let _ = writeln!(s, "    float g1 = {};", if setup.is_some_and(|x| x.geometric1) { "geo" } else { "1.0" });
        let clamp = if setup.is_some_and(|x| x.clamp_highlights) { "(nl <= 0.0 ? 0.0 : 1.0)" } else { "1.0" };
        let _ = writeln!(s, "    float cl = {clamp};");
        let _ = writeln!(s, "    sc += (uLS0[{i}] * d0 * g0 + uLS1[{i}] * refl * d1 * g1) * cl * k;");
        if setup.is_some_and(|x| x.fresnel_selector != 0) && light::available(layer, FR) {
            let _ = writeln!(s, "    fr = {};", read(sampler(FR)));
        }
        *s += "  }\n";
    }
    *s += "  vec4 lp = vec4(clamp(pc, 0.0, 1.0), 1.0);\n  vec4 ls = vec4(clamp(sc, 0.0, 1.0), 1.0);\n";
    let selector = setup.map_or(0, |x| x.fresnel_selector);
    if selector & 1 != 0 {
        *s += "  lp.a = clamp(fr, 0.0, 1.0);\n";
    }
    if selector & 2 != 0 {
        *s += "  ls.a = clamp(fr, 0.0, 1.0);\n";
    }
}

fn libm_sqrt(x: f32) -> f32 {
    banner3ds_scene::draw::math::sqrt(x)
}

fn source(id: i32, stage: usize, lit: bool) -> String {
    match id {
        0 => String::from("primary"),
        1 => String::from(if lit { "lp" } else { "vec4(0.0)" }),
        2 => String::from(if lit { "ls" } else { "vec4(0.0)" }),
        3..=5 => format!("tex{}", id - 3),
        13 => String::from("buf"),
        14 => format!("uConst[{stage}]"),
        15 => String::from("prev"),
        _ => String::from("vec4(1.0)"),
    }
}

fn color_operand(source_id: i32, operand: i32, stage: usize, lit: bool) -> String {
    let s = source(source_id, stage, lit);
    match operand {
        1 => format!("(1.0 - {s}.rgb)"),
        2 => format!("vec3({s}.a)"),
        3 => format!("vec3(1.0 - {s}.a)"),
        4 => format!("vec3({s}.r)"),
        5 => format!("vec3(1.0 - {s}.r)"),
        8 => format!("vec3({s}.g)"),
        9 => format!("vec3(1.0 - {s}.g)"),
        12 => format!("vec3({s}.b)"),
        13 => format!("vec3(1.0 - {s}.b)"),
        _ => format!("{s}.rgb"),
    }
}

fn alpha_operand(source_id: i32, operand: i32, stage: usize, lit: bool) -> String {
    let s = source(source_id, stage, lit);
    match operand {
        1 => format!("(1.0 - {s}.a)"),
        2 => format!("{s}.r"),
        3 => format!("(1.0 - {s}.r)"),
        4 => format!("{s}.g"),
        5 => format!("(1.0 - {s}.g)"),
        6 => format!("{s}.b"),
        7 => format!("(1.0 - {s}.b)"),
        _ => format!("{s}.a"),
    }
}

fn color_expr(mode: CombinerMode) -> &'static str {
    match mode {
        CombinerMode::Replace => "c0",
        CombinerMode::Modulate => "c0 * c1",
        CombinerMode::Add => "c0 + c1",
        CombinerMode::AddSigned => "c0 + c1 - 0.5",
        CombinerMode::Interpolate => "c0 * c2 + c1 * (1.0 - c2)",
        CombinerMode::Subtract => "c0 - c1",
        CombinerMode::DotProduct3Rgb | CombinerMode::DotProduct3Rgba => "vec3(4.0 * dot(c0 - 0.5, c1 - 0.5))",
        CombinerMode::MultAdd => "c0 * c1 + c2",
        CombinerMode::AddMult => "(c0 + c1) * c2",
    }
}

fn alpha_expr(mode: CombinerMode) -> &'static str {
    match mode {
        CombinerMode::Replace => "a0",
        CombinerMode::Modulate => "a0 * a1",
        CombinerMode::Add => "a0 + a1",
        CombinerMode::AddSigned => "a0 + a1 - 0.5",
        CombinerMode::Interpolate => "a0 * a2 + a1 * (1.0 - a2)",
        CombinerMode::Subtract => "a0 - a1",
        CombinerMode::DotProduct3Rgb | CombinerMode::DotProduct3Rgba => "rgb.x",
        CombinerMode::MultAdd => "a0 * a1 + a2",
        CombinerMode::AddMult => "(a0 + a1) * a2",
    }
}

/// PICA blend factor codes as GL enums. Codes 10-13 (the constant colour and alpha ones) read one, as in the soft renderer
/// (the original implementation), not the blend constant.
const FACTORS: [u32; 15] = [0, 1, 0x0300, 0x0301, 0x0306, 0x0307, 0x0302, 0x0303, 0x0304, 0x0305, 1, 1, 1, 1, 0x0308];

/// `[src_rgb, dst_rgb, src_alpha, dst_alpha]` GL blend factors for a PICA BLEND_FUNC register: colour from bits 16-19 and
/// 20-23, alpha as coverage (one, one minus source alpha), or zero, one under a glow, as in the soft renderer.
pub fn blend_factors(register: i32) -> [u32; 4] {
    let factor = |code: i32| FACTORS.get(code as usize).copied().unwrap_or(1);
    let dst = (register >> 20) & 15;
    let glow = light::keeps_destination(dst, register & 7);
    [factor((register >> 16) & 15), factor(dst), if glow { 0 } else { 1 }, if glow { 1 } else { 0x0303 }]
}

/// `[rgb, alpha]` GL blend equations for a PICA BLEND_FUNC register: colour from bits 0-2 (add, subtract, reverse subtract,
/// min, max), alpha adding.
pub fn blend_equations(register: i32) -> [u32; 2] {
    const EQUATIONS: [u32; 5] = [0x8006, 0x800A, 0x800B, 0x8007, 0x8008];
    [EQUATIONS.get((register & 7) as usize).copied().unwrap_or(0x8006), 0x8006]
}

/// HOME's window pass: a full-viewport quad.
pub const WINDOW_VERTEX: &str = "#version 300 es
in vec3 aPos;
void main() { gl_Position = vec4(aPos.xy, 0.0, 1.0); }
";

/// The window's coverage per fragment (the soft renderer's `window_coverage`), drawn with blend (zero, source alpha) so the
/// surface keeps that share of what the banner drew. `uWin` is in GL window pixels (y up).
pub const WINDOW_FRAGMENT: &str = "#version 300 es
precision highp float;
uniform vec4 uWin;
out vec4 oColor;
void main() {
    vec2 d = (gl_FragCoord.xy - uWin.xy) / uWin.zw;
    float g = length(d);
    float grad = length(d / uWin.zw) / max(g, 1e-6);
    float c = g < 1e-6 ? 1.0 : clamp(0.5 - (g - 1.0) / grad, 0.0, 1.0);
    oColor = vec4(c);
}
";
