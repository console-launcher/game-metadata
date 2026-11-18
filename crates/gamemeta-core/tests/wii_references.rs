// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Channel banners against reference frames, from an optional private reference set that is not part of this repository
//! (`docs/wii-references.md`): point `GM_WII_REFS` at it (`scripts/dolphin/fetch-refs --env` fills a cache). Each entry's icon
//! archive is drawn at the frames its `meta.json` lists and the reference tile's window compared with the captured frame (mean
//! absolute difference per channel, 0..255). Without the set (`GM_WII_REFS` unset and no cache) every test here prints "skipped"
//! and passes. Entries are named by an opaque id (the first 8 hex digits of the MD5 of the entry's name), so nothing here says what
//! they are.

use std::path::PathBuf;

use gamemeta_core::png;
use gamemeta_core::wii::banner::WiiBanner;

/// An entry whose mean difference stays under this matches the reference tile: the renderer is right about its pictures, keyframes
/// and materials; what remains is the tile's resampling and the frame it shows being a frame or two off.
const MATCH_MEAN: f64 = 20.0;

/// Entries the renderer does not match yet, by id, with the ceiling their worst sample is held under (the measured mean plus a little)
/// and why. A listed entry that gets under `MATCH_MEAN` fails the test: remove it from the list. One that gets worse fails too.
const KNOWN_MISMATCH: &[(&str, f64, &str)] = &[
    ("948b721d", 33.0, "a background is drawn darker and noisier: its TEV stages (not rendered) shape it"),
    ("1ec4fbfe", 74.0, "one phase of a colour animation: the colour comes through TEV stages (not rendered)"),
    (
        "51c54f44",
        72.0,
        "pictures pair a colour texture with an alpha texture combined by TEV stages (not rendered): the scene is missing",
    ),
    ("d15f8253", 40.0, "a layer is missing: its material blends by TEV stages (not rendered)"),
];

/// The entry's opaque id: the first 8 hex digits of the MD5 of its name.
fn id(slug: &str) -> String {
    format!("{:x}", md5::compute(slug.as_bytes()))[..8].to_string()
}

fn library() -> Option<PathBuf> {
    let dir = std::env::var_os("GM_WII_REFS")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/gamemeta-wii-refs")))?;
    dir.join("banners").is_dir().then_some(dir)
}

/// The number after `"key":` at or after `from` in `json`, and where it ended. The library writes its meta.json itself: this reads
/// that shape only.
fn number(json: &str, key: &str, from: usize) -> Option<(f64, usize)> {
    let at = from + json[from..].find(&format!("\"{key}\":"))? + key.len() + 3;
    let rest = json[at..].trim_start();
    let start = json.len() - rest.len();
    let len = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == '+'))
        .unwrap_or(rest.len());
    Some((rest[..len].parse().ok()?, start + len))
}

struct Sample {
    tile_frame: u32,
    icon_frame: f32,
}

struct Meta {
    scale: f32,
    window: [usize; 4],
    samples: Vec<Sample>,
}

fn meta(json: &str) -> Meta {
    let scale = number(json, "scale", 0).expect("scale").0 as f32;
    let w = json.find("\"window\":").expect("window");
    let window = {
        let mut v = [0usize; 4];
        let mut at = json[w..].find('[').unwrap() + w;
        for (i, slot) in v.iter_mut().enumerate() {
            let rest = json[at + 1..].trim_start();
            let len = rest.find(|c: char| !c.is_ascii_digit()).unwrap();
            *slot = rest[..len].parse().unwrap_or_else(|_| panic!("window {i}"));
            at = json.len() - rest.len() + len;
        }
        v
    };
    let mut samples = Vec::new();
    let from = json.find("\"samples\":").expect("samples");
    // The keys of a sample come in sorted order and not always the same set: read each key's values in turn.
    let (mut tiles, mut icons) = (Vec::new(), Vec::new());
    let mut at = from;
    while let Some((v, end)) = number(json, "tile_frame", at) {
        tiles.push(v as u32);
        at = end;
    }
    at = from;
    while let Some((v, end)) = number(json, "icon_frame", at) {
        icons.push(v as f32);
        at = end;
    }
    assert_eq!(tiles.len(), icons.len(), "every sample has both frames");
    for (tile_frame, icon_frame) in tiles.into_iter().zip(icons) {
        samples.push(Sample { tile_frame, icon_frame });
    }
    Meta { scale, window, samples }
}

/// The tile's window and the render's centred window of the same size, as mean absolute difference over the colour channels, and the
/// largest difference of any channel of any pixel.
fn compare(banner: &WiiBanner, meta: &Meta, tile: &gamemeta_core::RgbaImage, sample: &Sample) -> (f64, u8) {
    let render = banner.render(sample.icon_frame, meta.scale).expect("renders");
    let [x0, y0, x1, y1] = meta.window;
    let (tw, th) = (x1 - x0, y1 - y0);
    let (rx, ry) = ((render.width as usize - tw) / 2, (render.height as usize - th) / 2);
    let (mut sum, mut max) = (0u64, 0u8);
    for y in 0..th {
        for x in 0..tw {
            let t = ((y0 + y) * tile.width as usize + x0 + x) * 4;
            let r = ((ry + y) * render.width as usize + rx + x) * 4;
            for c in 0..3 {
                let d = tile.pixels[t + c].abs_diff(render.pixels[r + c]);
                sum += u64::from(d);
                max = max.max(d);
            }
        }
    }
    (sum as f64 / (tw * th * 3) as f64, max)
}

fn entries(root: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root.join("refs"))
        .map(|d| d.filter_map(|e| Some(e.ok()?.file_name().to_str()?.to_string())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn every_entry_is_drawn_like_its_reference_tile_or_is_a_known_mismatch() {
    let Some(root) = library() else {
        eprintln!("skipped: no reference set (GM_WII_REFS)");
        return;
    };
    let mut failures = Vec::new();
    for slug in entries(&root) {
        let id = id(&slug);
        let bytes = std::fs::read(root.join(format!("banners/{slug}/icon.bin")))
            .unwrap_or_else(|_| panic!("{id}: no icon.bin"));
        let banner = WiiBanner::open(&bytes).unwrap_or_else(|| panic!("{id}: icon.bin does not open"));
        let meta = meta(&std::fs::read_to_string(root.join(format!("refs/{slug}/meta.json"))).unwrap());
        assert!(!meta.samples.is_empty(), "{id}: no samples");
        let (mut worst, mut worst_max) = (0f64, 0u8);
        let mut means = Vec::new();
        for s in &meta.samples {
            let tile = png::decode(&std::fs::read(root.join(format!("refs/{slug}/f{}.png", s.tile_frame))).unwrap())
                .expect("frame");
            let (mean, max) = compare(&banner, &meta, &tile, s);
            means.push(format!("{mean:.1}"));
            worst = worst.max(mean);
            worst_max = worst_max.max(max);
        }
        eprintln!("{id}: mean [{}] max {worst_max}", means.join(" "));
        match KNOWN_MISMATCH.iter().find(|(t, _, _)| *t == id) {
            None if worst > MATCH_MEAN => failures.push(format!("{id}: mean {worst:.1} is over {MATCH_MEAN}")),
            Some((_, ceiling, why)) if worst > *ceiling => {
                failures.push(format!("{id}: mean {worst:.1} is over its known-mismatch ceiling {ceiling} ({why})"))
            }
            Some((_, _, why)) if worst <= MATCH_MEAN => {
                failures.push(format!("{id}: now matches (mean {worst:.1}): remove it from KNOWN_MISMATCH ({why})"))
            }
            _ => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_archive_in_the_set_opens_and_draws() {
    let Some(root) = library() else {
        eprintln!("skipped: no reference set (GM_WII_REFS)");
        return;
    };
    let mut opened = 0;
    for entry in std::fs::read_dir(root.join("banners")).unwrap().flatten() {
        for name in ["banner.bin", "icon.bin"] {
            let Ok(bytes) = std::fs::read(entry.path().join(name)) else { continue };
            let slug = id(&entry.file_name().to_string_lossy());
            let banner = WiiBanner::open(&bytes).unwrap_or_else(|| panic!("{slug}/{name} does not open"));
            let poster = banner.poster(0.5).unwrap_or_else(|| panic!("{slug}/{name} has no poster"));
            assert!(poster.pixels.iter().any(|&b| b != 0), "{slug}/{name}: the poster is blank");
            opened += 1;
        }
    }
    eprintln!("{opened} archives open and draw");
}
