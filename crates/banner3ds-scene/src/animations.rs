// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Animation parsing: the skeletal, material and visibility animation dictionaries of a CGFX into [`AnimTrack`]s. Layout
//! facts per SPICA (Unlicense): an animation holds a dictionary of elements, each a named target (a bone, a
//! `Materials["x"]...` path, a `MeshNodeVisibilities["x"].IsVisible` path) with a value of one primitive type; a curve is a
//! constant or a keyframe group in one of eight quantizations.

use crate::cgfx::{Budget, dict};
use crate::model::{AnimTrack, BannerAnimation, BannerAnimations, Keyframes, TrackKind};
use crate::reader::{FormatError, R, Reader};
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

const MAX_FRAMES: f32 = 20_000.0;
const MAX_KEYS: i64 = 20_000;
const MAX_CURVES: i32 = 64;
const MAX_ANIMATIONS: usize = 256;

const T_FLOAT: i32 = 0;
const T_BOOLEAN: i32 = 2;
const T_VECTOR2D: i32 = 3;
const T_VECTOR3D: i32 = 4;
const T_TRANSFORM: i32 = 5;
const T_RGBA: i32 = 6;
const T_QUAT_TRANSFORM: i32 = 8;

pub(crate) fn parse(
    r: &Reader<'_>,
    dict_base: i64,
    unsupported: &mut BTreeSet<String>,
    budget: &Budget,
) -> R<BannerAnimations> {
    let version = r.u32(8)?;
    let mut group = |index: i64| -> R<Vec<BannerAnimation>> {
        let mut out = Vec::new();
        for (name, at) in dict(r, dict_base + 8 * index)?.into_iter().take(MAX_ANIMATIONS) {
            if at == 0 {
                continue;
            }
            // An animation that does not read is dropped, the rest stay.
            match parse_animation(r, version, &name, at, unsupported, budget) {
                Ok(a) => out.push(a),
                Err(_) => {
                    unsupported.insert("animation-damaged".into());
                }
            }
        }
        Ok(out)
    };
    Ok(BannerAnimations { skeletal: group(9)?, material: group(10)?, visibility: group(11)? })
}

// Object: 0 magic, 4 revision, 8 name, 0xC target group name, 0x10 loop mode, 0x14 frame count, 0x18 elements (count, DICT pointer).
fn parse_animation(
    r: &Reader<'_>,
    version: u32,
    name: &str,
    at: i64,
    unsupported: &mut BTreeSet<String>,
    budget: &Budget,
) -> R<BannerAnimation> {
    let looping = r.u32(at + 0x10)? != 0;
    let f = r.f32(at + 0x14)?;
    let frames = f.clamp(0.0, MAX_FRAMES);
    let mut tracks = Vec::new();
    for (path, element) in dict(r, at + 0x18)? {
        if element == 0 {
            continue;
        }
        read_element(r, version, &path, element, &mut tracks, unsupported, budget)?;
    }
    Ok(BannerAnimation { name: name.into(), frames, looping, tracks })
}

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// `prefix` + group 1 (one or more, no line terminators) + `middle` + group 2 (one or more) with greedy group 1: the last
/// `middle` that leaves both groups non-empty.
fn split_greedy<'a>(path: &'a str, prefix: &str, middle: &str) -> Option<(&'a str, &'a str)> {
    if path.chars().any(is_line_terminator) {
        return None;
    }
    let rest = path.strip_prefix(prefix)?;
    let mut from = rest.len();
    while let Some(i) = rest[..from].rfind(middle) {
        if i >= 1 && i + middle.len() < rest.len() {
            return Some((&rest[..i], &rest[i + middle.len()..]));
        }
        from = i + middle.len() - 1;
        if i == 0 {
            break;
        }
    }
    None
}

/// `MeshNodeVisibilities["x"].IsVisible`.
fn visibility_target(path: &str) -> Option<&str> {
    if path.chars().any(is_line_terminator) {
        return None;
    }
    let inner = path.strip_prefix("MeshNodeVisibilities[\"")?.strip_suffix("\"].IsVisible")?;
    (!inner.is_empty()).then_some(inner)
}

/// `TextureCoordinators[d].(Scale|Rotate|Translate)`.
fn coordinator(property: &str) -> Option<(i32, TrackKind)> {
    let rest = property.strip_prefix("TextureCoordinators[")?;
    let b = rest.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_digit() || b[1] != b']' || b[2] != b'.' {
        return None;
    }
    let kind = match &rest[3..] {
        "Scale" => TrackKind::MaterialScale,
        "Rotate" => TrackKind::MaterialRotate,
        "Translate" => TrackKind::MaterialTranslate,
        _ => return None,
    };
    Some((i32::from(b[0] - b'0'), kind))
}

/// `MaterialColor.\w+`: the colour's name.
fn color_name(property: &str) -> Option<&str> {
    let name = property.strip_prefix("MaterialColor.")?;
    (!name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')).then_some(name)
}

fn read_element(
    r: &Reader<'_>,
    version: u32,
    path: &str,
    at: i64,
    out: &mut Vec<AnimTrack>,
    unsupported: &mut BTreeSet<String>,
    budget: &Budget,
) -> R<()> {
    let legacy = version <= 0x0500_0000;
    let type_at = at + if legacy { 0x10 } else { 0x8 };
    let content = type_at + 4;
    let kind = r.i32(type_at)?;
    let flags = r.u32(at)?;
    let bit = |n: usize| (flags >> n) & 1 != 0;

    let visible = visibility_target(path);
    let material = split_greedy(path, "Materials[\"", "\"].");
    if kind == T_BOOLEAN && visible.is_some() {
        if let Some(t) = bool_track(r, content, visible.unwrap_or_default(), budget)? {
            out.push(t);
        }
    } else if kind == T_TRANSFORM {
        let kinds = [
            Some((TrackKind::BoneScale, 0)),
            Some((TrackKind::BoneScale, 1)),
            Some((TrackKind::BoneScale, 2)),
            Some((TrackKind::BoneRotation, 0)),
            Some((TrackKind::BoneRotation, 1)),
            Some((TrackKind::BoneRotation, 2)),
            None,
            Some((TrackKind::BoneTranslation, 0)),
            Some((TrackKind::BoneTranslation, 1)),
            Some((TrackKind::BoneTranslation, 2)),
        ];
        for (i, k) in kinds.iter().enumerate() {
            let Some((track_kind, component)) = *k else { continue };
            if bit(16 + i) {
                continue;
            }
            if let Some(keys) = curve(r, content + 4 * i as i64, bit(6 + i), budget)? {
                out.push(AnimTrack { target: path.into(), kind: track_kind, component, keys, slot: 0 });
            }
        }
    } else if kind == T_QUAT_TRANSFORM {
        quat_tracks(r, content, flags, path, out, budget)?;
    } else if let (Some((target, property)), true) =
        (material, matches!(kind, T_VECTOR2D | T_FLOAT | T_RGBA | T_VECTOR3D))
    {
        let coord = coordinator(property);
        let color = color_name(property);
        if let Some((slot, track_kind)) = coord {
            let axes = if kind == T_FLOAT { 1 } else { 2 };
            for (c, keys) in vector(r, content, flags, axes, budget)?.into_iter().enumerate() {
                if let Some(keys) = keys {
                    out.push(AnimTrack { target: target.into(), kind: track_kind, component: c as i32, keys, slot });
                }
            }
        } else if let (Some(name), true) = (color, kind == T_RGBA) {
            let track_kind = if name == "Diffuse" {
                Some(TrackKind::MaterialDiffuse)
            } else if name.starts_with("Constant") {
                Some(TrackKind::MaterialConstant)
            } else {
                None
            };
            if let Some(track_kind) = track_kind {
                let slot = name.strip_prefix("Constant").unwrap_or(name).parse::<i32>().unwrap_or(0);
                for (c, keys) in vector(r, content, flags, 4, budget)?.into_iter().enumerate() {
                    if let Some(keys) = keys {
                        out.push(AnimTrack {
                            target: target.into(),
                            kind: track_kind,
                            component: c as i32,
                            keys,
                            slot,
                        });
                    }
                }
            } else {
                unsupported.insert(format!("material-animation:{name}"));
            }
        } else {
            unsupported.insert("material-animation".into());
        }
    } else {
        unsupported.insert(format!("animation-element-{kind}"));
    }
    Ok(())
}

/// `n` axis curves whose slots are at `content`; the flags word has a constant bit per axis, then an absent bit per axis.
fn vector(r: &Reader<'_>, content: i64, flags: u32, n: usize, budget: &Budget) -> R<Vec<Option<Keyframes>>> {
    let mut result = Vec::with_capacity(n);
    for axis in 0..n {
        let constant = (flags >> axis) & 1 != 0;
        let missing = (flags >> (n + axis)) & 1 != 0;
        result.push(if missing { None } else { curve(r, content + 4 * axis as i64, constant, budget)? });
    }
    Ok(result)
}

fn constant_keys(frame: f32, value: f32) -> Keyframes {
    Keyframes { frames: vec![frame], values: vec![value], in_slopes: None, out_slopes: None, step: false }
}

/// A curve slot at `slot`: the constant value itself, or a pointer to a keyframe group.
fn curve(r: &Reader<'_>, slot: i64, constant: bool, budget: &Budget) -> R<Option<Keyframes>> {
    if constant {
        return Ok(Some(constant_keys(0.0, r.f32(slot)?)));
    }
    match r.ptr(slot)? {
        0 => Ok(None),
        group => read_group(r, group, budget),
    }
}

// Group: 0 start, 4 end, 8 pre/post repeat, 0xA padding, 0xC curve flags (bit 1 constant), then the constant value, or
// 0x10 curve count, 0x14 one pointer per curve. A curve: start, end, format flags, key count, 1/duration, [scale, offset, frame
// scale], keys (or, with format bit 0, one constant value); its key frames count from its own start. Several curves play one
// after another (a coin sheen: hold, sweep, hold).
fn read_group(r: &Reader<'_>, at: i64, budget: &Budget) -> R<Option<Keyframes>> {
    let curve_flags = r.u32(at + 0xC)?;
    if curve_flags & 2 != 0 {
        return Ok(Some(constant_keys(0.0, r.f32(at + 0x10)?)));
    }
    let count = (r.u32(at + 0x10)? as i32).clamp(1, MAX_CURVES);
    let mut curves = Vec::new();
    for i in 0..i64::from(count) {
        let block = r.ptr(at + 0x14 + 4 * i)?;
        if block != 0 {
            if let Some(k) = read_curve(r, block, budget)? {
                curves.push(k);
            }
        }
    }
    Ok(match curves.len() {
        0 => None,
        1 => curves.pop(),
        _ => Some(join(&curves)),
    })
}

/// Consecutive curves as one: a curve's first key at the frame the previous one ended on replaces that one's last. A stepped
/// curve among interpolated ones keeps its steps as a hold key on each step's frame.
fn join(curves: &[Keyframes]) -> Keyframes {
    let mut frames: Vec<f32> = Vec::new();
    let mut values: Vec<f32> = Vec::new();
    let hermite = curves.iter().all(|c| c.in_slopes.is_some());
    let stepped = curves.iter().all(|c| c.step);
    let mut in_slopes: Vec<f32> = Vec::new();
    let mut out_slopes: Vec<f32> = Vec::new();
    for c in curves {
        for i in 0..c.frames.len() {
            if frames.last().is_some_and(|&last| c.frames[i] <= last) {
                frames.pop();
                values.pop();
                if hermite {
                    in_slopes.pop();
                    out_slopes.pop();
                }
            }
            if c.step && !stepped && i > 0 {
                frames.push(c.frames[i]);
                values.push(c.values[i - 1]);
            }
            frames.push(c.frames[i]);
            values.push(c.values[i]);
            if hermite {
                in_slopes.push(c.in_slopes.as_ref().map_or(0.0, |s| s[i]));
                out_slopes.push(c.out_slopes.as_ref().map_or(0.0, |s| s[i]));
            }
        }
    }
    Keyframes {
        frames,
        values,
        in_slopes: hermite.then_some(in_slopes),
        out_slopes: hermite.then_some(out_slopes),
        step: stepped,
    }
}

fn read_curve(r: &Reader<'_>, block: i64, budget: &Budget) -> R<Option<Keyframes>> {
    let start = r.f32(block)?;
    let mut p = block + 8; // start, end frame (the keys carry their own frames, from the start)
    let format = r.u32(p)?;
    // Flag bit 0: a constant segment, its value where the key count would be.
    if format & 1 != 0 {
        return Ok(Some(constant_keys(start, r.f32(p + 4)?)));
    }
    let count = r.i32(p + 4)?;
    if !(0..=MAX_KEYS as i32).contains(&count) {
        return Err(FormatError);
    }
    p += 12; // format flags, key count, 1/duration
    let quant = (format >> 5) as i32;
    let linear = format & 4 != 0;
    let keys = if matches!(quant, 0 | 3 | 6) {
        read_keys(r, p, quant, count, linear, false, false, budget)?
    } else {
        // The quantized formats carry a value scale and offset, and (per the format description) a frame scale. Some files leave
        // the frame scale out, so of the layouts that give sorted frames inside the clip the one whose last key lands nearest
        // the curve's end is taken. A stored frame scale is positive.
        let span = r.f32(block + 4)? - start;
        let with = read_keys(r, p, quant, count, linear, true, true, budget)?.filter(|k| is_sane(&k.frames));
        let without = read_keys(r, p, quant, count, linear, true, false, budget)?.filter(|k| is_sane(&k.frames));
        let distance = |k: &Keyframes| match k.frames.last() {
            Some(&last) if span.is_finite() => (last - span).abs(),
            _ => 0.0,
        };
        match (with, without) {
            (Some(a), Some(b)) => Some(if distance(&b) < distance(&a) { b } else { a }),
            (a, b) => a.or(b),
        }
    };
    let Some(mut keys) = keys else { return Ok(None) };
    if start == 0.0 || !start.is_finite() {
        return Ok(Some(keys));
    }
    for f in &mut keys.frames {
        *f += start;
    }
    Ok(Some(keys))
}

fn is_sane(frames: &[f32]) -> bool {
    for (i, &f) in frames.iter().enumerate() {
        if f.is_nan() || f < 0.0 || f > MAX_FRAMES {
            return false;
        }
        if i > 0 && f < frames[i - 1] {
            return false;
        }
    }
    true
}

fn i16_of(v: i32) -> f32 {
    f32::from(v as u16 as i16)
}

#[allow(clippy::too_many_arguments)]
fn read_keys(
    r: &Reader<'_>,
    start: i64,
    quant: i32,
    count: i32,
    linear: bool,
    scaled: bool,
    frame_scale_stored: bool,
    budget: &Budget,
) -> R<Option<Keyframes>> {
    let mut p = start;
    let (mut value_scale, mut value_offset, mut frame_scale) = (1.0f32, 0.0f32, 1.0f32);
    if scaled {
        value_scale = r.f32(p)?;
        value_offset = r.f32(p + 4)?;
        p += 8;
        if frame_scale_stored {
            frame_scale = r.f32(p)?;
            p += 4;
            if frame_scale.is_nan() || frame_scale <= 0.0 || !frame_scale.is_finite() {
                return Ok(None);
            }
        }
    }
    let n = count as usize;
    budget.spend(n * 5)?;
    let hermite = quant <= 5;
    let mut frames = Vec::with_capacity(n);
    let mut values = Vec::with_capacity(n);
    let mut in_slopes = Vec::with_capacity(if hermite { n } else { 0 });
    let mut out_slopes = Vec::with_capacity(if hermite { n } else { 0 });
    for _ in 0..n {
        let (frame, value, mut in_s, mut out_s);
        match quant {
            0 => {
                frame = r.f32(p)?;
                value = r.f32(p + 4)?;
                in_s = r.f32(p + 8)?;
                out_s = r.f32(p + 12)?;
                p += 16;
            }
            1 => {
                let v = r.u32(p)?;
                frame = (v & 0xfff) as f32;
                value = ((v >> 12) & 0xfffff) as f32;
                in_s = i16_of(r.u16(p + 4)?) / 256.0;
                out_s = i16_of(r.u16(p + 6)?) / 256.0;
                p += 8;
            }
            2 => {
                frame = r.u8(p)? as f32;
                value = r.u16(p + 1)? as f32;
                let s = r.u8(p + 3)? | (r.u8(p + 4)? << 8) | (r.u8(p + 5)? << 16);
                in_s = ((s << 20) >> 20) as f32 / 32.0;
                out_s = ((s << 8) >> 20) as f32 / 32.0;
                p += 6;
            }
            3 => {
                frame = r.f32(p)?;
                value = r.f32(p + 4)?;
                in_s = r.f32(p + 8)?;
                out_s = in_s;
                p += 12;
            }
            4 => {
                frame = r.u16(p)? as f32 / 32.0;
                value = r.u16(p + 2)? as f32;
                in_s = i16_of(r.u16(p + 4)?) / 256.0;
                out_s = in_s;
                p += 6;
            }
            5 => {
                frame = r.u8(p)? as f32;
                let vs = r.u8(p + 1)? | (r.u8(p + 2)? << 8) | (r.u8(p + 3)? << 16);
                value = (vs & 0xfff) as f32;
                in_s = ((vs << 8) >> 20) as f32 / 32.0;
                out_s = in_s;
                p += 4;
            }
            6 => {
                frame = r.f32(p)?;
                value = r.f32(p + 4)?;
                in_s = 0.0;
                out_s = 0.0;
                p += 8;
            }
            7 => {
                let v = r.u32(p)?;
                frame = (v & 0xfff) as f32;
                value = ((v >> 12) & 0xfffff) as f32;
                in_s = 0.0;
                out_s = 0.0;
                p += 4;
            }
            _ => return Err(FormatError),
        }
        if !hermite {
            in_s = 0.0;
            out_s = 0.0;
        }
        frames.push(frame * frame_scale);
        values.push(value * value_scale + value_offset);
        if hermite {
            in_slopes.push(in_s);
            out_slopes.push(out_s);
        }
    }
    Ok(Some(Keyframes {
        frames,
        values,
        in_slopes: hermite.then_some(in_slopes),
        out_slopes: hermite.then_some(out_slopes),
        step: !hermite && !linear,
    }))
}

// Boolean: a pointer to (start, end, pre, post, pad), then a pointer to one bit per frame, packed 32 to a little-endian word.
fn bool_track(r: &Reader<'_>, content: i64, target: &str, budget: &Budget) -> R<Option<AnimTrack>> {
    let head = r.ptr(content)?;
    let bits = r.ptr(content + 4)?;
    if head == 0 || bits == 0 {
        return Ok(None);
    }
    let start = r.f32(head)?;
    let end = r.f32(head + 4)?;
    let count = (end - start) as i64;
    if count <= 0 || count > MAX_KEYS {
        return Ok(None);
    }
    budget.spend(count as usize * 2)?;
    let frames: Vec<f32> = (0..count).map(|i| start + i as f32).collect();
    let mut values = Vec::with_capacity(count as usize);
    for i in 0..count {
        values.push(if (r.u32(bits + 4 * (i / 32))? >> (i % 32)) & 1 != 0 { 1.0 } else { 0.0 });
    }
    Ok(Some(AnimTrack {
        target: target.into(),
        kind: TrackKind::MeshVisibility,
        component: 0,
        keys: Keyframes { frames, values, in_slopes: None, out_slopes: None, step: true },
        slot: 0,
    }))
}

// Quaternion transform: flags (before the content) tell which of scale, rotation, translation exist or are constant; the content
// holds pointers to rotation, translation and scale blocks (start, end, curve ptr, constant, then one entry per frame).
fn quat_tracks(
    r: &Reader<'_>,
    content: i64,
    flags: u32,
    target: &str,
    out: &mut Vec<AnimTrack>,
    budget: &Budget,
) -> R<()> {
    // The pointers sit in the order rotation, translation, scale; their flag bits run translation, rotation, scale, from bit 0.
    let pointers = [r.ptr(content + 8)?, r.ptr(content)?, r.ptr(content + 4)?]; // scale, rotation, translation
    let constant_bits = [2usize, 1, 0];
    let missing_bits = [5usize, 4, 3];
    let widths = [3i64, 4, 3];
    let kinds = [TrackKind::BoneScale, TrackKind::BoneRotationQuat, TrackKind::BoneTranslation];
    for e in 0..3 {
        if (flags >> missing_bits[e]) & 1 != 0 {
            continue;
        }
        let at = pointers[e];
        if at == 0 {
            continue;
        }
        let start = r.f32(at)?;
        let end = r.f32(at + 4)?;
        let is_constant = r.u32(at + 12)? != 0 || (flags >> constant_bits[e]) & 1 != 0;
        let count = if is_constant { 1 } else { (end - start) as i64 };
        if count <= 0 || count > MAX_KEYS {
            continue;
        }
        let stride = widths[e] * 4 + 4;
        budget.spend(count as usize * (widths[e] as usize + 1) * 2)?;
        let frames: Vec<f32> = (0..count).map(|i| start + i as f32).collect();
        for c in 0..widths[e] {
            let mut values = Vec::with_capacity(count as usize);
            for i in 0..count {
                values.push(r.f32(at + 16 + stride * i + 4 * c)?);
            }
            out.push(AnimTrack {
                target: target.into(),
                kind: kinds[e],
                component: c as i32,
                keys: Keyframes { frames: frames.clone(), values, in_slopes: None, out_slopes: None, step: false },
                slot: 0,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_paths_split_like_the_greedy_reference_regex() {
        assert_eq!(
            split_greedy("Materials[\"a\"].MaterialColor.Diffuse", "Materials[\"", "\"]."),
            Some(("a", "MaterialColor.Diffuse"))
        );
        // Greedy: the last `"].` that leaves a non-empty tail.
        assert_eq!(split_greedy("Materials[\"a\"].b\"].c", "Materials[\"", "\"]."), Some(("a\"].b", "c")));
        assert_eq!(split_greedy("Materials[\"\"].x", "Materials[\"", "\"]."), None);
        assert_eq!(split_greedy("Materials[\"a\"].", "Materials[\"", "\"]."), None);
        assert_eq!(split_greedy("Materials[\"a\nb\"].x", "Materials[\"", "\"]."), None);
        assert_eq!(split_greedy("Bone", "Materials[\"", "\"]."), None);
    }

    #[test]
    fn property_and_visibility_paths() {
        assert_eq!(coordinator("TextureCoordinators[2].Rotate"), Some((2, TrackKind::MaterialRotate)));
        assert_eq!(coordinator("TextureCoordinators[a].Rotate"), None);
        assert_eq!(coordinator("TextureCoordinators[1].Shear"), None);
        assert_eq!(color_name("MaterialColor.Constant3"), Some("Constant3"));
        assert_eq!(color_name("MaterialColor."), None);
        assert_eq!(color_name("MaterialColor.a-b"), None);
        assert_eq!(visibility_target("MeshNodeVisibilities[\"logo\"].IsVisible"), Some("logo"));
        assert_eq!(visibility_target("MeshNodeVisibilities[\"\"].IsVisible"), None);
    }
}
