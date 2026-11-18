// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wii channel banners: pixel parity with the original renderer on every authored banner (`fixtures/wii-banner`: the original implementation's
//! own `.bin` archives and its PNG frames), the original implementation's 21 authored-banner tests and its `channelBanner` test, ported one per
//! test. There is no real banner behind any of these: they show the port does what the original implementation does.

use std::path::Path;

use gamemeta_core::wii::banner::WiiBanner;
use gamemeta_core::{RgbaImage, SliceSource, png, wii};
use gamemeta_fixtures::wii::{
    PartFile, U8File, WiiRvzOptions, imd5, opening, partition_data, rvz, tpl_plain, u8_archive,
};
use gamemeta_fixtures::wii_banner::{
    BLUE5, GREEN5, Key, Mat, Pane, RED5, Tag, Target, Tex, animation, archive, layout, lz77_wrapped, quadrants,
    tpl_opaque, track,
};
use gamemeta_fixtures::wii_banner_cases::cases;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/wii-banner"))
}

/// The largest difference allowed in any channel of any pixel, 0..255 (0: bit exact).
const MAX_DIFF: u8 = 0;

#[test]
fn the_fixture_writers_give_the_pinned_archives_byte_for_byte() {
    let all = cases();
    assert_eq!(all.len(), 35);
    for c in &all {
        let reference = std::fs::read(root().join(format!("{}.bin", c.name))).unwrap();
        assert!(
            c.archive == reference,
            "{}: writer output differs from the pinned archive (re-run write-wii-banner-fixtures after a layout change)",
            c.name
        );
    }
}

#[test]
fn every_authored_banner_matches_the_reference_renderer() {
    let info = std::fs::read_to_string(root().join("info.txt")).unwrap();
    let manifest = std::fs::read_to_string(root().join("manifest.txt")).unwrap();
    let mut frames = 0;
    let (mut worst, mut sum, mut total) = (0u8, 0u64, 0u64);
    for line in info.lines() {
        let name = line.split(' ').next().unwrap();
        let banner = WiiBanner::open(&std::fs::read(root().join(format!("{name}.bin"))).unwrap())
            .unwrap_or_else(|| panic!("{name} did not open"));
        let expected = format!(
            "{name} start={} skipped={} missing={}",
            banner.start_frames(),
            banner.skipped().iter().map(|(k, v)| format!("{k}:{v}")).collect::<Vec<_>>().join(","),
            banner.missing_textures().join(",")
        );
        assert_eq!(line, expected);
        let mut renders: Vec<(String, f32, f32)> = manifest
            .lines()
            .filter_map(|l| {
                let mut p = l.split(' ');
                let id = p.next()?;
                id.rsplit_once('-').filter(|(n, _)| *n == name)?;
                Some((id.to_string(), p.next()?.parse().ok()?, p.next()?.parse().ok()?))
            })
            .collect();
        renders.push((format!("{name}-poster"), banner.start_frames() as f32, 1.0));
        for (id, frame, scale) in renders {
            let golden = png::decode(&std::fs::read(root().join(format!("{id}.png"))).unwrap()).unwrap();
            let ours = banner.render(frame, scale).unwrap();
            assert_eq!((ours.width, ours.height), (golden.width, golden.height), "{id}");
            let (max, s) = ours
                .pixels
                .iter()
                .zip(&golden.pixels)
                .fold((0u8, 0u64), |(m, s), (a, b)| (m.max(a.abs_diff(*b)), s + u64::from(a.abs_diff(*b))));
            println!(
                "{id:24} frame {frame:6} scale {scale:4} max {max} mean {:.5}",
                s as f64 / ours.pixels.len() as f64
            );
            assert!(max == MAX_DIFF, "{id}: max channel difference {max}");
            worst = worst.max(max);
            sum += s;
            total += ours.pixels.len() as u64;
            frames += 1;
        }
    }
    println!("{frames} frames, max {worst}, mean {:.6}", sum as f64 / total as f64);
    assert_eq!(frames, 78 + 35, "frames in the manifest plus a poster per case");
}

// ---- the original implementation WiiBannerTest, one per test: a 200 x 100 layout, screen centre at pixel (100, 50), layout y up

fn px(image: &RgbaImage, x: usize, y: usize) -> u32 {
    let o = (y * image.width as usize + x) * 4;
    u32::from_be_bytes([image.pixels[o], image.pixels[o + 1], image.pixels[o + 2], image.pixels[o + 3]])
}

fn alpha(image: &RgbaImage, x: usize, y: usize) -> u32 {
    px(image, x, y) & 0xFF
}

fn assert_pixel(expected: u32, image: &RgbaImage, x: usize, y: usize, tolerance: u32) {
    let actual = px(image, x, y);
    for shift in [24, 16, 8, 0] {
        let (e, a) = ((expected >> shift) & 0xFF, (actual >> shift) & 0xFF);
        assert!(e.abs_diff(a) <= tolerance, "pixel ({x}, {y}): expected {expected:#010x}, got {actual:#010x}");
    }
}

const RED: u32 = 0xFF00_00FF;
const GREEN: u32 = 0x00FF_00FF;
const BLUE: u32 = 0x0000_FFFF;
const WHITE: u32 = 0xFFFF_FFFF;

fn open_archive(layout: &[u8], animations: &[(&str, &[u8])], textures: &[(&str, &[u8])]) -> WiiBanner {
    WiiBanner::open(&archive(layout, animations, textures)).expect("opens")
}

fn flat(panes: &[Pane]) -> Vec<u8> {
    layout(200.0, 100.0, &[], &[Mat::new("m")], panes)
}

fn red(w: f32, h: f32) -> Pane {
    Pane { w, h, colors: [RED; 4], ..Pane::new("p") }
}

fn small() -> Pane {
    Pane { w: 20.0, h: 20.0, colors: [RED; 4], ..Pane::new("p") }
}

fn slide(looping: bool) -> Vec<u8> {
    animation(
        60,
        looping,
        &[Target {
            name: "p",
            material: false,
            tags: vec![Tag(
                "RLPA",
                vec![track(0, 0, true, &[Key(0.0, -50.0, 100.0 / 60.0), Key(60.0, 50.0, 100.0 / 60.0)])],
            )],
        }],
        &[],
    )
}

fn k(frame: f32, value: f32) -> Key {
    Key(frame, value, 0.0)
}

fn single(tag: Tag, name: &'static str, material: bool, frames: u16, looping: bool) -> Vec<u8> {
    animation(frames, looping, &[Target { name, material, tags: vec![tag] }], &[])
}

fn textured(texture_names: &[&str], mat: Mat) -> Vec<u8> {
    layout(200.0, 100.0, texture_names, &[mat], &[Pane::new("p")])
}

fn one_map(wrap: u8) -> Mat {
    Mat { textures: vec![Tex(0, wrap)], ..Mat::new("m") }
}

#[test]
fn a_pane_draws_its_colour_where_it_is_and_nothing_around_it() {
    let image = open_archive(&flat(&[red(50.0, 50.0)]), &[], &[]).poster(1.0).unwrap();
    assert_eq!((image.width, image.height), (200, 100));
    assert_pixel(RED, &image, 100, 50, 1);
    assert_pixel(RED, &image, 80, 30, 1);
    assert_eq!(alpha(&image, 60, 50), 0, "left of the pane: transparent");
    assert_eq!(alpha(&image, 100, 90), 0, "below the pane: transparent");
}

#[test]
fn origin_puts_the_panes_anchor_point_at_its_translate() {
    let tl = Pane { origin: 0, w: 50.0, h: 50.0, colors: [RED; 4], ..Pane::new("p") };
    let image = open_archive(&flat(&[tl]), &[], &[]).poster(1.0).unwrap();
    assert_pixel(RED, &image, 125, 75, 1);
    assert_eq!(alpha(&image, 75, 25), 0);
    let br = Pane { origin: 8, w: 50.0, h: 50.0, colors: [RED; 4], ..Pane::new("p") };
    let image = open_archive(&flat(&[br]), &[], &[]).poster(1.0).unwrap();
    assert_pixel(RED, &image, 75, 25, 1);
    assert_eq!(alpha(&image, 125, 75), 0);
}

#[test]
fn children_sit_in_the_parents_space_scaled_and_moved_with_it() {
    let child = Pane { x: 10.0, w: 10.0, h: 10.0, colors: [RED; 4], ..Pane::new("c") };
    let parent = Pane { kind: "pan1", x: 40.0, scale: 2.0, children: vec![child], ..Pane::new("g") };
    let image = open_archive(&flat(&[parent]), &[], &[]).poster(1.0).unwrap();
    assert_pixel(RED, &image, 160, 50, 1);
    assert_pixel(RED, &image, 169, 50, 1);
    assert_eq!(alpha(&image, 140, 50), 0);
    assert_eq!(alpha(&image, 171, 50), 0);
}

#[test]
fn rotation_turns_a_bar_about_its_centre() {
    let bar = Pane { w: 100.0, h: 20.0, rot_z: 90.0, colors: [RED; 4], ..Pane::new("p") };
    let image = open_archive(&flat(&[bar]), &[], &[]).poster(1.0).unwrap();
    assert_pixel(RED, &image, 100, 10, 1);
    assert_eq!(alpha(&image, 140, 50), 0);
}

#[test]
fn vertex_colours_blend_across_the_pane() {
    let gradient = Pane { colors: [0x0000_00FF, WHITE, 0x0000_00FF, WHITE], ..Pane::new("p") };
    let image = open_archive(&flat(&[gradient]), &[], &[]).poster(1.0).unwrap();
    let (left, mid, right) = (px(&image, 60, 50) >> 24, px(&image, 100, 50) >> 24, px(&image, 140, 50) >> 24);
    assert!(left < 40 && (110..=145).contains(&mid) && right > 215, "left={left} mid={mid} right={right}");
}

#[test]
fn a_texture_lands_the_right_way_up_in_the_right_colours() {
    let image = open_archive(&textured(&["q.tpl"], one_map(0)), &[], &[("q.tpl", &quadrants())]).poster(1.0).unwrap();
    assert_pixel(RED, &image, 70, 28, 1);
    assert_pixel(GREEN, &image, 130, 28, 1);
    assert_pixel(BLUE, &image, 70, 72, 1);
    assert_pixel(WHITE, &image, 130, 72, 1);
}

#[test]
fn pane_alpha_reaches_the_children_only_while_they_are_influenced() {
    let child = |influenced: bool| Pane { w: 40.0, h: 40.0, influenced, colors: [RED; 4], ..Pane::new("c") };
    let group = |alpha: u8, visible: bool, influenced: bool| Pane {
        kind: "pan1",
        alpha,
        visible,
        children: vec![child(influenced)],
        ..Pane::new("g")
    };
    let dim = open_archive(&flat(&[group(128, true, true)]), &[], &[]).poster(1.0).unwrap();
    assert_pixel(0xFF00_0080, &dim, 100, 50, 2);
    let full = open_archive(&flat(&[group(128, true, false)]), &[], &[]).poster(1.0).unwrap();
    assert_pixel(RED, &full, 100, 50, 1);
    let hidden = open_archive(&flat(&[group(255, false, true)]), &[], &[]).poster(1.0).unwrap();
    assert_eq!(alpha(&hidden, 100, 50), 0);
}

#[test]
fn text_window_and_bounding_panes_draw_nothing_and_are_counted() {
    let l = flat(&[
        Pane { kind: "txt1", ..Pane::new("t") },
        Pane { kind: "wnd1", ..Pane::new("w") },
        Pane { kind: "bnd1", ..Pane::new("b") },
        red(20.0, 20.0),
    ]);
    let banner = open_archive(&l, &[], &[]);
    assert_eq!(banner.skipped(), [("txt1", 1), ("wnd1", 1), ("bnd1", 1)]);
    assert_pixel(RED, &banner.poster(1.0).unwrap(), 100, 50, 1);
}

#[test]
fn a_texture_the_archive_lacks_is_listed() {
    let banner = open_archive(&textured(&["gone.tpl"], one_map(0)), &[], &[]);
    assert_eq!(banner.missing_textures(), ["gone.tpl"]);
}

// ---- animation

#[test]
fn a_hermite_translate_track_moves_the_pane_between_its_keys() {
    let banner = open_archive(&flat(&[small()]), &[("banner_Start.brlan", &slide(false))], &[]);
    let at = |f: f32| banner.render(f, 1.0).unwrap();
    assert_pixel(RED, &at(0.0), 50, 50, 1);
    assert_eq!(alpha(&at(0.0), 100, 50), 0);
    assert_pixel(RED, &at(30.0), 100, 50, 1);
    assert_pixel(RED, &at(60.0), 150, 50, 1);
    assert_pixel(RED, &at(600.0), 150, 50, 1); // a start animation holds its last frame
}

#[test]
fn the_poster_is_the_start_animation_finished() {
    let banner = open_archive(&flat(&[small()]), &[("banner_Start.brlan", &slide(false))], &[]);
    assert_eq!(banner.start_frames(), 60);
    assert_pixel(RED, &banner.poster(1.0).unwrap(), 150, 50, 1);
}

#[test]
fn a_loop_animation_repeats_and_a_start_animation_does_not() {
    let banner = open_archive(&flat(&[small()]), &[("banner_Loop.brlan", &slide(true))], &[]);
    let (a, b) = (banner.render(30.0, 1.0).unwrap(), banner.render(90.0, 1.0).unwrap());
    assert!(a.pixels == b.pixels, "frame 90 of a 60 frame loop is frame 30");
    assert_pixel(RED, &a, 100, 50, 1);
}

/// 12 of the 20 real banners ship one `banner.brlan` with no start or loop in its name; that one animation is the loop.
#[test]
fn a_single_plain_banner_animation_is_the_loop() {
    let banner = open_archive(&flat(&[small()]), &[("banner.brlan", &slide(true))], &[]);
    assert_eq!((banner.start_frames(), banner.loop_frames()), (0, 60));
    let (a, b) = (banner.render(30.0, 1.0).unwrap(), banner.render(90.0, 1.0).unwrap());
    assert!(a.pixels == b.pixels, "frame 90 of a 60 frame loop is frame 30");
    assert_pixel(RED, &a, 100, 50, 1);
}

#[test]
fn the_loop_begins_after_the_start_animation() {
    let loop_shift = single(Tag("RLPA", vec![track(0, 1, true, &[k(0.0, 0.0), k(60.0, 40.0)])]), "p", false, 60, true);
    let banner = open_archive(
        &flat(&[small()]),
        &[("banner_Start.brlan", &slide(false)), ("banner_Loop.brlan", &loop_shift)],
        &[],
    );
    assert_pixel(RED, &banner.render(60.0, 1.0).unwrap(), 150, 50, 1); // loop frame 0: no shift yet
    assert_pixel(RED, &banner.render(90.0, 1.0).unwrap(), 150, 30, 1); // loop frame 30: 20 up
}

#[test]
fn an_alpha_track_fades_the_pane() {
    let fade = single(Tag("RLVC", vec![track(0, 16, true, &[k(0.0, 0.0), k(60.0, 255.0)])]), "p", false, 60, false);
    let banner = open_archive(&flat(&[red(100.0, 100.0)]), &[("banner_Start.brlan", &fade)], &[]);
    assert_eq!(alpha(&banner.render(0.0, 1.0).unwrap(), 100, 50), 0);
    assert_eq!(alpha(&banner.render(60.0, 1.0).unwrap(), 100, 50), 255);
    assert!(alpha(&banner.render(30.0, 1.0).unwrap(), 100, 50).abs_diff(128) <= 2);
}

#[test]
fn a_visibility_track_shows_and_hides_a_pane_by_steps() {
    let blink = single(Tag("RLVI", vec![track(0, 0, false, &[k(0.0, 0.0), k(10.0, 1.0)])]), "p", false, 20, false);
    let banner = open_archive(&flat(&[red(100.0, 100.0)]), &[("banner_Start.brlan", &blink)], &[]);
    assert_eq!(alpha(&banner.render(5.0, 1.0).unwrap(), 100, 50), 0);
    assert_eq!(alpha(&banner.render(10.0, 1.0).unwrap(), 100, 50), 255);
}

#[test]
fn a_texture_srt_track_scrolls_a_repeating_texture() {
    let halves = tpl_opaque(4, 4, |x, _| if x < 2 { RED5 } else { GREEN5 });
    let mat = Mat { srt: vec![[0.0, 0.0, 0.0, 1.0, 1.0]], ..one_map(1) };
    let scroll = single(Tag("RLTS", vec![track(0, 0, true, &[k(0.0, 0.0), k(60.0, 0.5)])]), "m", true, 60, false);
    let banner = open_archive(&textured(&["h.tpl"], mat), &[("banner_Start.brlan", &scroll)], &[("h.tpl", &halves)]);
    assert_pixel(RED, &banner.render(0.0, 1.0).unwrap(), 70, 50, 1);
    assert_pixel(GREEN, &banner.render(0.0, 1.0).unwrap(), 130, 50, 1);
    assert_pixel(GREEN, &banner.render(60.0, 1.0).unwrap(), 70, 50, 1); // shifted half a texture: the halves swap
    assert_pixel(RED, &banner.render(60.0, 1.0).unwrap(), 130, 50, 1);
}

// ---- the real material layout (counts in the low nibbles, optional blocks in a fixed order; checked on 20 real banners)

/// A material with every optional block (TEV swap, indirect SRTs and stages, TEV stages, alpha compare, blend mode) in front of
/// nothing: the colour comes before all of them, so it reads whatever the other flag bits say.
#[test]
fn a_material_with_every_optional_block_still_reads_its_colour() {
    let mat = Mat { color: Some([0, 255, 0, 255]), full: true, ..Mat::new("m") };
    let pane = Pane { w: 40.0, h: 40.0, colors: [WHITE; 4], ..Pane::new("p") };
    let banner = open_archive(&layout(200.0, 100.0, &[], &[mat], &[pane]), &[], &[]);
    assert_pixel(GREEN, &banner.render(0.0, 1.0).unwrap(), 100, 50, 1);
}

/// Texture map, SRT and coordinate generator counts are the low nibbles of the flag word: one of each, and the map is what draws.
#[test]
fn the_flag_counts_are_the_low_nibbles() {
    let tpl = tpl_opaque(4, 4, |_, _| RED5);
    let mat = Mat { textures: vec![Tex(0, 0)], srt: vec![[0.0, 0.0, 0.0, 1.0, 1.0]], full: true, ..Mat::new("m") };
    let pane = Pane { w: 40.0, h: 40.0, colors: [WHITE; 4], ..Pane::new("p") };
    let banner = open_archive(&layout(200.0, 100.0, &["q.tpl"], &[mat], &[pane]), &[], &[("q.tpl", &tpl)]);
    assert_pixel(RED, &banner.render(0.0, 1.0).unwrap(), 100, 50, 1);
}

/// GX expands an I4 or I8 texel to (I, I, I, I): intensity is the alpha too, so the black texels of a real banner's checker
/// are transparent, not black (the Wii Menu shows the checker of a real banner light grey on white).
#[test]
fn an_intensity_texture_is_transparent_where_it_is_black() {
    let i4 = tpl_plain(8, 8, 0, &[0x00, 0x00, 0xFF, 0xFF].repeat(8));
    let banner = open_archive(&textured(&["i.tpl"], one_map(0)), &[], &[("i.tpl", &i4)]);
    let image = banner.render(0.0, 1.0).unwrap();
    assert_eq!(alpha(&image, 80, 50), 0, "intensity 0 is alpha 0");
    assert_pixel(WHITE, &image, 120, 50, 1);
}

/// A material's first two TEV color registers are what its texture maps black and white to: black + (white - black) * texel,
/// alpha included (the Wii Menu's checker materials set black to light grey, opaque).
#[test]
fn a_materials_black_and_white_colours_map_the_texture() {
    let i4 = tpl_plain(8, 8, 0, &[0x00, 0x00, 0xFF, 0xFF].repeat(8));
    let mat = Mat { black: [0, 0, 255, 255], white: [0, 255, 0, 255], ..one_map(0) };
    let banner = open_archive(&textured(&["i.tpl"], mat), &[], &[("i.tpl", &i4)]);
    let image = banner.render(0.0, 1.0).unwrap();
    assert_pixel(BLUE, &image, 80, 50, 1);
    assert_pixel(GREEN, &image, 120, 50, 1);
}

#[test]
fn a_material_colour_track_tints_the_pane() {
    let l = layout(200.0, 100.0, &[], &[Mat { color: Some([255; 4]), ..Mat::new("m") }], &[Pane::new("p")]);
    let tint = single(
        Tag(
            "RLMC",
            vec![track(0, 0, false, &[k(0.0, 255.0), k(5.0, 0.0)]), track(0, 2, false, &[k(0.0, 255.0), k(5.0, 0.0)])],
        ),
        "m",
        true,
        10,
        false,
    );
    let banner = open_archive(&l, &[("banner_Start.brlan", &tint)], &[]);
    assert_pixel(WHITE, &banner.render(0.0, 1.0).unwrap(), 100, 50, 1);
    assert_pixel(GREEN, &banner.render(5.0, 1.0).unwrap(), 100, 50, 1);
}

#[test]
fn a_texture_pattern_track_swaps_the_texture() {
    let l = textured(&["a.tpl", "b.tpl"], one_map(0));
    let swap = animation(
        10,
        false,
        &[Target {
            name: "m",
            material: true,
            tags: vec![Tag("RLTP", vec![track(0, 0, false, &[k(0.0, 0.0), k(5.0, 1.0)])])],
        }],
        &["a.tpl", "b.tpl"],
    );
    let (a, b) = (tpl_opaque(4, 4, |_, _| RED5), tpl_opaque(4, 4, |_, _| BLUE5));
    let banner = open_archive(&l, &[("banner_Start.brlan", &swap)], &[("a.tpl", &a), ("b.tpl", &b)]);
    assert_pixel(RED, &banner.render(0.0, 1.0).unwrap(), 100, 50, 1);
    assert_pixel(BLUE, &banner.render(5.0, 1.0).unwrap(), 100, 50, 1);
}

// ---- files

#[test]
fn a_banner_archive_opens_through_lz77_and_imd5_wrappers() {
    let plain = archive(&flat(&[red(100.0, 100.0)]), &[], &[]);
    let lz = lz77_wrapped(&plain);
    for wrapped in [plain.clone(), lz.clone(), imd5(&lz)] {
        assert_pixel(RED, &WiiBanner::open(&wrapped).unwrap().poster(1.0).unwrap(), 100, 50, 1);
    }
}

#[test]
fn scale_renders_more_pixels_per_layout_unit() {
    let image = open_archive(&flat(&[red(100.0, 100.0)]), &[], &[]).poster(2.0).unwrap();
    assert_eq!((image.width, image.height), (400, 200));
    assert_pixel(RED, &image, 200, 100, 1);
}

#[test]
fn damaged_or_foreign_data_gives_none_and_never_panics() {
    assert!(WiiBanner::open(&[]).is_none());
    assert!(WiiBanner::open(&[7u8; 64]).is_none());
    assert!(WiiBanner::open(&u8_archive(&[U8File("arc/blyt/banner.brlyt", &[0u8; 40])])).is_none());
    let l = flat(&[red(100.0, 100.0)]);
    for cut in (0..l.len()).step_by(5) {
        let _ = WiiBanner::open(&archive(&l[..cut], &[], &[])); // may or may not open: it must not panic
    }
    let a = slide(false);
    for cut in (0..a.len()).step_by(5) {
        if let Some(b) = WiiBanner::open(&archive(&l, &[("banner_Start.brlan", &a[..cut])], &[])) {
            let _ = b.poster(1.0);
        }
    }
    let mut bent = l.clone();
    let (mid, third) = (bent.len() / 2, bent.len() / 3);
    bent[mid] = 0x7F;
    bent[third] = 0x7F;
    let _ = WiiBanner::open(&archive(&bent, &[], &[]));
}

// ---- WiiMetadataTest `an RVZ's channel banner is drawn on demand from its banner bin`

#[test]
fn an_rvzs_channel_banner_is_drawn_on_demand_from_its_banner_bin() {
    let l = layout(60.0, 40.0, &[], &[Mat::new("m")], &[Pane { w: 20.0, h: 20.0, colors: [RED; 4], ..Pane::new("p") }]);
    let banner_bin = lz77_wrapped(&u8_archive(&[U8File("arc/blyt/banner.brlyt", &l)]));
    let rvz_of = |opening_bnr: &[u8]| {
        let part = partition_data("RZDE01", "Header Title", &[PartFile("opening.bnr", opening_bnr)], None, 0x7C00);
        rvz(&part, &WiiRvzOptions::default()).bytes
    };
    let titles = [("en", "Test Quest")];
    let with = rvz_of(&opening(&titles, None, Some(&banner_bin), |a| a));
    let image = wii::channel_banner(&mut SliceSource::new(&with), "game.rvz").expect("drawn");
    assert_eq!((image.width, image.height), (60, 40));
    let centre = (20 * 60 + 30) * 4;
    assert_eq!(&image.pixels[centre..centre + 4], [255, 0, 0, 255]);
    let without = rvz_of(&opening(&titles, None, None, |a| a));
    assert!(
        wii::channel_banner(&mut SliceSource::new(&without), "game.rvz").is_none(),
        "an opening bnr without a banner bin"
    );
    assert!(wii::channel_banner(&mut SliceSource::new(&with), "game.iso").is_none(), "only an RVZ is read");
    assert!(wii::channel_banner(&mut SliceSource::new(&[0u8; 100]), "game.rvz").is_none());
}

/// The menu's grid tile for a disc is drawn from `meta/icon.bin` (the same archive format as the banner); the Dolphin reference
/// frames are of that tile, so the icon archive comes out of an RVZ the way the banner does.
#[test]
fn an_rvzs_channel_icon_archive_comes_out_of_its_icon_bin() {
    let l = layout(60.0, 40.0, &[], &[Mat::new("m")], &[Pane { w: 20.0, h: 20.0, colors: [RED; 4], ..Pane::new("p") }]);
    let icon_bin = lz77_wrapped(&u8_archive(&[U8File("arc/blyt/icon.brlyt", &l)]));
    let rvz_of = |opening_bnr: &[u8]| {
        let part = partition_data("RZDE01", "Header Title", &[PartFile("opening.bnr", opening_bnr)], None, 0x7C00);
        rvz(&part, &WiiRvzOptions::default()).bytes
    };
    let titles = [("en", "Test Quest")];
    let with = rvz_of(&opening(&titles, Some(&icon_bin), None, |a| a));
    assert_eq!(wii::channel_icon_bin(&mut SliceSource::new(&with)).unwrap(), Some(icon_bin.clone()));
    assert!(WiiBanner::open(&icon_bin).is_some(), "an icon archive opens as a banner");
    let without = rvz_of(&opening(&titles, None, Some(&icon_bin), |a| a));
    assert_eq!(wii::channel_icon_bin(&mut SliceSource::new(&without)).unwrap(), None);
}

#[test]
fn the_banner_bin_comes_out_of_an_opening_bnr_still_wrapped() {
    let l = layout(60.0, 40.0, &[], &[Mat::new("m")], &[]);
    let bin = lz77_wrapped(&u8_archive(&[U8File("arc/blyt/banner.brlyt", &l)]));
    let bnr = opening(&[("en", "T")], None, Some(&bin), |a| a);
    assert_eq!(wii::banner::banner_bin(&bnr).as_deref(), Some(bin.as_slice()));
    assert!(wii::banner::banner_bin(&opening(&[("en", "T")], None, None, |a| a)).is_none());
    assert!(wii::banner::banner_bin(&[0u8; 64]).is_none());
}

/// Found by the fuzzer: a denormal layout size made the caller's scale infinite; the size and the render refuse it, no panic.
#[test]
fn a_scale_that_overflows_is_refused_not_drawn() {
    let banner = open_archive(&layout(1.0e-43, 6.0e-39, &[], &[Mat::new("m")], &[]), &[], &[]);
    let (w, h) = banner.layout_size();
    let scale = 62.0 / w.max(h);
    assert!(!scale.is_finite());
    assert!(banner.pixel_size(scale).is_none());
    assert!(banner.render_into(0.0, scale, &mut []).is_none());
    assert!(banner.poster(scale).is_none());
}
