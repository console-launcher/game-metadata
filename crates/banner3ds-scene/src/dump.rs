// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The deterministic JSON dump of a scene: sorted keys, floats to 6 significant digits, textures as size and SHA-256 of
//! their RGBA. The parity tests compare it with the original implementation's.

use crate::cbmd::Cbmd;
use crate::model::*;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;
use gamemeta_core::RgbaImage;

/// A float to 6 significant digits (the exact value rounded half to even), plain notation for exponents -5..=9, else
/// `d.ddde<exp>`; zero is `0`; NaN and the infinities are the JSON strings `"NaN"`, `"Infinity"`, `"-Infinity"`.
pub fn number(x: f32) -> String {
    if x.is_nan() {
        return "\"NaN\"".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "\"Infinity\"" } else { "\"-Infinity\"" }.to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    // Rust formats the exact value of the (widened, so exact) float, rounding half to even.
    let sci = format!("{:.5e}", f64::from(x).abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let e: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let mut out = String::new();
    if x < 0.0 {
        out.push('-');
    }
    if (-5..=9).contains(&e) {
        if e >= 0 {
            let int_len = e as usize + 1;
            if digits.len() <= int_len {
                out.push_str(digits);
                out.extend(core::iter::repeat_n('0', int_len - digits.len()));
            } else {
                out.push_str(&digits[..int_len]);
                out.push('.');
                out.push_str(&digits[int_len..]);
            }
        } else {
            out.push_str("0.");
            out.extend(core::iter::repeat_n('0', (-e - 1) as usize));
            out.push_str(digits);
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let _ = write!(out, "e{e}");
    }
    out
}

/// Lower-case hex SHA-256.
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut message: Vec<u8> = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = String::new();
    for word in h {
        let _ = write!(out, "{word:08x}");
    }
    out
}

// ---- the writer: strings built bottom-up, object keys sorted

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || (c as u32) >= 0x7F => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn obj(mut fields: Vec<(&str, String)>) -> String {
    fields.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = String::from("{");
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&quote(k));
        out.push(':');
        out.push_str(v);
    }
    out.push('}');
    out
}

fn arr(items: impl IntoIterator<Item = String>) -> String {
    let mut out = String::from("[");
    for (i, v) in items.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&v);
    }
    out.push(']');
    out
}

fn fa(a: &[f32]) -> String {
    arr(a.iter().map(|&x| number(x)))
}

fn ia(a: &[i32]) -> String {
    arr(a.iter().map(|x| x.to_string()))
}

fn b(v: bool) -> String {
    if v { "true" } else { "false" }.to_string()
}

fn opt<T>(v: &Option<T>, f: impl FnOnce(&T) -> String) -> String {
    v.as_ref().map_or_else(|| "null".to_string(), f)
}

fn fopt(v: &Option<Vec<f32>>) -> String {
    opt(v, |a| fa(a))
}

// ---- the scene

fn texture(i: &RgbaImage) -> String {
    obj(vec![
        ("height", i.height.to_string()),
        ("sha256", quote(&sha256_hex(&i.pixels))),
        ("width", i.width.to_string()),
    ])
}

fn lut(l: &LightingLut) -> String {
    obj(vec![("absolute", b(l.absolute)), ("deltas", fa(&l.deltas)), ("values", fa(&l.values))])
}

fn sampler(s: &LutSampler) -> String {
    obj(vec![("input", s.input.to_string()), ("scale", number(s.scale)), ("table", lut(&s.table))])
}

fn lighting(l: &FragmentLightingSetup) -> String {
    obj(vec![
        ("clampHighlights", b(l.clamp_highlights)),
        ("distribution0", b(l.distribution0)),
        ("distribution1", b(l.distribution1)),
        ("geometric0", b(l.geometric0)),
        ("geometric1", b(l.geometric1)),
        ("reflection", b(l.reflection)),
        ("layerConfig", l.layer_config.to_string()),
        ("fresnelSelector", l.fresnel_selector.to_string()),
        ("samplers", arr(l.samplers.iter().map(|s| opt(s, sampler)))),
    ])
}

fn stage(s: &TexEnvStage) -> String {
    obj(vec![
        ("colorSources", ia(&s.color_sources)),
        ("alphaSources", ia(&s.alpha_sources)),
        ("colorOperands", ia(&s.color_operands)),
        ("alphaOperands", ia(&s.alpha_operands)),
        ("colorMode", quote(s.color_mode.name())),
        ("alphaMode", quote(s.alpha_mode.name())),
        ("colorScale", s.color_scale.to_string()),
        ("alphaScale", s.alpha_scale.to_string()),
        ("constant", fa(&s.constant)),
        ("updateColorBuffer", b(s.update_color_buffer)),
        ("updateAlphaBuffer", b(s.update_alpha_buffer)),
        ("constantIndex", s.constant_index.to_string()),
    ])
}

fn slot(s: &TextureSlot) -> String {
    obj(vec![
        ("texture", quote(&s.texture)),
        ("wrapU", s.wrap_u.to_string()),
        ("wrapV", s.wrap_v.to_string()),
        ("linear", b(s.linear)),
        ("uvSet", s.uv_set.to_string()),
        ("scale", fa(&s.scale)),
        ("rotation", number(s.rotation)),
        ("translation", fa(&s.translation)),
        ("mapping", s.mapping.to_string()),
        ("border", fa(&s.border)),
        ("matrixMode", s.matrix_mode.to_string()),
    ])
}

fn material(m: &BannerMaterial) -> String {
    obj(vec![
        ("name", quote(&m.name)),
        ("slots", arr(m.slots.iter().map(|s| opt(s, slot)))),
        ("stages", arr(m.stages.iter().map(stage))),
        ("bufferColor", fa(&m.buffer_color)),
        ("diffuse", fa(&m.diffuse)),
        ("ambient", fa(&m.ambient)),
        ("emission", fa(&m.emission)),
        ("fragmentLighting", b(m.fragment_lighting)),
        ("alphaTestEnabled", b(m.alpha_test_enabled)),
        ("alphaTestFunction", m.alpha_test_function.to_string()),
        ("alphaTestReference", number(m.alpha_test_reference)),
        ("blendMode", m.blend_mode.to_string()),
        ("blendFunction", m.blend_function.to_string()),
        ("blendColor", fa(&m.blend_color)),
        ("faceCulling", m.face_culling.to_string()),
        ("depthTest", b(m.depth_test)),
        ("depthWrite", b(m.depth_write)),
        ("translucency", m.translucency.to_string()),
        ("specular0", fa(&m.specular0)),
        ("specular1", fa(&m.specular1)),
        ("lighting", opt(&m.lighting, lighting)),
        ("logicOp", m.logic_op.to_string()),
        ("lightSet", m.light_set.to_string()),
        ("bumpMode", m.bump_mode.to_string()),
        ("bumpSlot", m.bump_slot.to_string()),
        ("bumpRenormalize", b(m.bump_renormalize)),
        ("vertexLighting", b(m.vertex_lighting)),
        ("hemisphereLighting", b(m.hemisphere_lighting)),
    ])
}

fn vertices(v: &VertexData) -> String {
    obj(vec![
        ("count", v.count.to_string()),
        ("positions", fa(&v.positions)),
        ("normals", fopt(&v.normals)),
        ("uv0", fopt(&v.uv0)),
        ("uv1", fopt(&v.uv1)),
        ("uv2", fopt(&v.uv2)),
        ("colors", fopt(&v.colors)),
        ("boneIndices", opt(&v.bone_indices, |a| ia(a))),
        ("boneWeights", fopt(&v.bone_weights)),
    ])
}

fn mesh(m: &BannerMesh) -> String {
    obj(vec![
        ("name", quote(&m.name)),
        ("vertices", vertices(&m.vertices)),
        ("indices", arr(m.indices.iter().map(|i| i.to_string()))),
        ("skinning", quote(m.skinning.name())),
        ("bonePalette", ia(&m.bone_palette)),
        ("material", material(&m.material)),
        ("nodeName", quote(&m.node_name)),
        ("visible", b(m.visible)),
        ("renderPriority", m.render_priority.to_string()),
    ])
}

fn bone(x: &BannerBone) -> String {
    obj(vec![
        ("name", quote(&x.name)),
        ("parent", x.parent.to_string()),
        ("scale", fa(&x.scale)),
        ("rotation", fa(&x.rotation)),
        ("translation", fa(&x.translation)),
        ("world", fa(&x.world)),
        ("inverseWorld", fa(&x.inverse_world)),
        ("billboard", x.billboard.to_string()),
        ("flags", x.flags.to_string()),
    ])
}

fn model(m: &BannerModel) -> String {
    obj(vec![
        ("name", quote(&m.name)),
        ("world", fa(&m.world)),
        ("meshes", arr(m.meshes.iter().map(mesh))),
        ("bones", arr(m.bones.iter().map(bone))),
        ("visible", b(m.visible)),
    ])
}

fn keys(k: &Keyframes) -> String {
    obj(vec![
        ("frames", fa(&k.frames)),
        ("values", fa(&k.values)),
        ("inSlopes", fopt(&k.in_slopes)),
        ("outSlopes", fopt(&k.out_slopes)),
        ("step", b(k.step)),
    ])
}

fn track(t: &AnimTrack) -> String {
    obj(vec![
        ("target", quote(&t.target)),
        ("kind", quote(t.kind.name())),
        ("component", t.component.to_string()),
        ("keys", keys(&t.keys)),
        ("slot", t.slot.to_string()),
    ])
}

fn animation(a: &BannerAnimation) -> String {
    obj(vec![
        ("name", quote(&a.name)),
        ("frames", number(a.frames)),
        ("loop", b(a.looping)),
        ("tracks", arr(a.tracks.iter().map(track))),
    ])
}

fn light(l: &BannerLight) -> String {
    obj(vec![
        ("name", quote(&l.name)),
        ("kind", l.kind.to_string()),
        ("position", fa(&l.position)),
        ("direction", fa(&l.direction)),
        ("ambient", fa(&l.ambient)),
        ("diffuse", fa(&l.diffuse)),
        ("specular0", fa(&l.specular0)),
        ("specular1", fa(&l.specular1)),
        ("spot", opt(&l.spot, sampler)),
        ("distance", opt(&l.distance, |t| lut(t))),
        ("attenuationBias", number(l.attenuation_bias)),
        ("attenuationScale", number(l.attenuation_scale)),
        ("twoSidedDiffuse", b(l.two_sided_diffuse)),
    ])
}

fn vertex_light(l: &VertexLight) -> String {
    obj(vec![
        ("name", quote(&l.name)),
        ("kind", l.kind.to_string()),
        ("position", fa(&l.position)),
        ("direction", fa(&l.direction)),
        ("ambient", fa(&l.ambient)),
        ("diffuse", fa(&l.diffuse)),
        ("attenuation", fa(&l.attenuation)),
    ])
}

fn hemisphere_light(l: &HemisphereLight) -> String {
    obj(vec![
        ("name", quote(&l.name)),
        ("ground", fa(&l.ground)),
        ("sky", fa(&l.sky)),
        ("direction", fa(&l.direction)),
        ("lerp", number(l.lerp)),
    ])
}

fn camera(c: &BannerFileCamera) -> String {
    let view = match &c.view {
        CameraView::Aim { target, twist } => {
            obj(vec![("kind", quote("aim")), ("target", fa(target)), ("twist", number(*twist))])
        }
        CameraView::LookAt { target, up } => {
            obj(vec![("kind", quote("lookAt")), ("target", fa(target)), ("up", fa(up))])
        }
        CameraView::Rotate { rotation } => obj(vec![("kind", quote("rotate")), ("rotation", fa(rotation))]),
    };
    let p = &c.projection;
    let projection = obj(vec![
        ("orthographic", b(p.orthographic)),
        ("near", number(p.near)),
        ("far", number(p.far)),
        ("aspect", number(p.aspect)),
        ("fovOrHeight", number(p.fov_or_height)),
    ]);
    obj(vec![("name", quote(&c.name)), ("position", fa(&c.position)), ("view", view), ("projection", projection)])
}

/// The dump of one scene.
pub fn scene_json(s: &BannerScene) -> String {
    let textures = {
        let mut out = String::from("{");
        for (i, (name, image)) in s.textures.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&quote(name));
            out.push(':');
            out.push_str(&texture(image));
        }
        out.push('}');
        out
    };
    let light_sets = opt(&s.light_sets, |sets| {
        arr(sets.iter().map(|(k, names)| arr([k.to_string(), arr(names.iter().map(|n| quote(n)))])))
    });
    obj(vec![
        ("models", arr(s.models.iter().map(model))),
        ("textures", textures),
        (
            "animations",
            obj(vec![
                ("skeletal", arr(s.animations.skeletal.iter().map(animation))),
                ("material", arr(s.animations.material.iter().map(animation))),
                ("visibility", arr(s.animations.visibility.iter().map(animation))),
            ]),
        ),
        ("unsupported", arr(s.unsupported.iter().map(|u| quote(u)))),
        ("lights", arr(s.lights.iter().map(light))),
        ("lightSets", light_sets),
        ("cameras", arr(s.cameras.iter().map(camera))),
        ("environmentCameras", arr(s.environment_cameras.iter().map(|n| quote(n)))),
        ("vertexLights", arr(s.vertex_lights.iter().map(vertex_light))),
        ("hemisphereLights", arr(s.hemisphere_lights.iter().map(hemisphere_light))),
        ("sceneAmbient", fa(&s.scene_ambient)),
    ])
}

/// The dump of a CBMD banner: `{"cbmd": {...} | null, "scene": {...} | null}`.
pub fn banner_json(cbmd: &[u8]) -> String {
    let parsed = Cbmd::parse(cbmd);
    let head = opt(&parsed, |c| {
        obj(vec![("common", b(c.common.is_some())), ("regional", arr(c.regional.keys().map(|k| k.to_string())))])
    });
    let scene = parsed.as_ref().and_then(Cbmd::scene);
    obj(vec![("cbmd", head), ("scene", opt(&scene, scene_json))])
}

/// The dump of a bare CGFX (`cbmd` is `null`).
pub fn cgfx_json(cgfx: &[u8]) -> String {
    obj(vec![("cbmd", "null".to_string()), ("scene", opt(&crate::cgfx::parse(cgfx), scene_json))])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_six_significant_digits() {
        let cases: [(f32, &str); 14] = [
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-2.5, "-2.5"),
            (0.1, "0.1"),
            (1.0 / 3.0, "0.333333"),
            (123456.7, "123457"),
            (1234567.0, "1234570"),
            (1000005.0, "1000000"), // an exact tie: half to even
            (1000015.0, "1000020"),
            (1e-5, "0.00001"),
            (1.5e-7, "1.5e-7"),
            (3.0e12, "3e12"),
            (-8.25e11, "-8.25e11"),
        ];
        for (x, want) in cases {
            assert_eq!(number(x), want, "{x:e}");
        }
        assert_eq!(number(f32::NAN), "\"NaN\"");
        assert_eq!(number(f32::INFINITY), "\"Infinity\"");
        assert_eq!(number(f32::NEG_INFINITY), "\"-Infinity\"");
    }

    #[test]
    fn sha256_matches_the_standard_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = [b'a'; 1000];
        assert_eq!(sha256_hex(&long), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
    }
}
