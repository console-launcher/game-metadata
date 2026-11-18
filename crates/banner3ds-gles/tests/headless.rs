// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The renderer on a real GLES 3 context the test makes itself: EGL on Mesa's surfaceless platform (no X, no Wayland, no GPU)
//! with a pbuffer, the software rasteriser llvmpipe forced (`LIBGL_ALWAYS_SOFTWARE`, and glvnd pointed at Mesa's vendor file
//! when the environment does not choose). One context for the whole run on the main thread (`harness = false`), so the
//! environment is set before anything else runs. Pass a word to run only the checks whose name contains it.
//!
//! The checks: the original implementation's quad test (the quad against the CPU renderer, upright), every feature banner at
//! three frames and every drawable case against `banner3ds-soft` within [`TOLERANCE`], every generated program compiling,
//! a viewport inside a larger framebuffer, a destroyed renderer's context reused, and the showcase banner against HOME.

use std::path::Path;

use banner3ds_gles::{DrawOptions, Gl, Renderer, Viewport};
use banner3ds_scene::{BannerScene, Cbmd, draw::camera};
use gamemeta_core::{RgbaImage, png};
use gamemeta_fixtures::headless::{self, Headless};
use gamemeta_fixtures::{reference, render};

/// GL against the soft renderer, per frame: at most this share of pixels whose worst channel differs by more than 8, and at
/// most this mean difference per channel (0..255). See docs/design.md for why.
const TOLERANCE: Tolerance = Tolerance { share: 0.01, mean: 0.5 };

struct Tolerance {
    share: f64,
    mean: f64,
}

/// Per-channel differences between two same-size RGBA images (the original implementation's benchmark diff).
#[derive(Debug, Clone, Copy, Default)]
struct Diff {
    max: u8,
    mean: f64,
    covered_mean: f64,
    share: f64,
}

impl std::fmt::Display for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "max {:3} mean {:.3} covered-mean {:.2} >8: {:.2}%",
            self.max,
            self.mean,
            self.covered_mean,
            self.share * 100.0
        )
    }
}

fn diff(a: &[u8], b: &[u8]) -> Diff {
    assert_eq!(a.len(), b.len());
    let pixels = a.len() / 4;
    let (mut max, mut sum, mut covered, mut covered_sum, mut over) = (0u8, 0u64, 0u64, 0u64, 0u64);
    for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
        let worst = pa.iter().zip(pb).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        let d: u64 = pa.iter().zip(pb).map(|(x, y)| u64::from(x.abs_diff(*y))).sum();
        sum += d;
        max = max.max(worst);
        if worst > 8 {
            over += 1;
        }
        if pa[3] > 0 || pb[3] > 0 {
            covered += 1;
            covered_sum += d;
        }
    }
    Diff {
        max,
        mean: sum as f64 / (pixels * 4) as f64,
        covered_mean: if covered == 0 { 0.0 } else { covered_sum as f64 / (covered * 4) as f64 },
        share: over as f64 / pixels as f64,
    }
}

/// The test context and the renderer's GL table loaded from it.
struct Ctx {
    headless: Headless,
    gl: Gl,
}

impl std::ops::Deref for Ctx {
    type Target = Headless;
    fn deref(&self) -> &Headless {
        &self.headless
    }
}

impl Ctx {
    /// One GL frame of `scene` at `frame` into the `width` x `height` region at the origin (camera: HOME's view or `camera`).
    fn render(
        &self,
        scene: &BannerScene,
        frame: f32,
        width: i32,
        height: i32,
        camera: Option<camera::Camera>,
    ) -> Vec<u8> {
        // SAFETY: the context is current on this thread for the renderer's whole life.
        unsafe {
            let mut r = Renderer::new(scene, self.gl).expect("drawable");
            assert!(r.log().is_empty(), "{:?}", r.log());
            r.draw_with(frame, Viewport::of(width, height), &DrawOptions { camera, clip_frame: None });
            self.finish();
            let px = self.read(0, 0, width, height);
            r.destroy();
            px
        }
    }
}

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures"))
}

fn soft(scene: &BannerScene, frame: f32, width: u32, height: u32, cam: Option<camera::Camera>) -> Vec<u8> {
    let mut out = vec![0u8; (width * height * 4) as usize];
    let options = banner3ds_soft::Options { camera: cam, ..Default::default() };
    banner3ds_soft::Renderer::new(scene).render_with(frame, width, height, &options, &mut out).unwrap();
    out
}

/// With `GM_GL_SHOTS=<dir>`, both frames as PNGs there (for looking at a difference).
fn shots(id: &str, gl: &[u8], cpu: &[u8], width: u32, height: u32) {
    let Some(dir) = std::env::var_os("GM_GL_SHOTS") else { return };
    for (tag, px) in [("gl", gl), ("cpu", cpu)] {
        let image = RgbaImage { width, height, pixels: px.to_vec() };
        std::fs::write(Path::new(&dir).join(format!("{id}-{tag}.png")), png::encode(&image).unwrap()).unwrap();
    }
}

fn within(d: &Diff) -> bool {
    d.share <= TOLERANCE.share && d.mean <= TOLERANCE.mean
}

/// The reference's `DesktopGlRendererTest`: the textured quad (half 30, framed by the fitted camera at 64 x 64) like the CPU
/// renderer on all but 2% of pixels, upright: red over blue.
fn gl_draws_the_textured_quad_like_the_cpu_renderer(ctx: &Ctx) -> Result<(), String> {
    let scene = render::quad(render::red_over_blue(), 30.0);
    let player = banner3ds_scene::draw::Player::new(&scene);
    let cam = camera::fit(&scene, &player, 64, 64);
    let gl = ctx.render(&scene, 0.0, 64, 64, Some(cam.clone()));
    let cpu = soft(&scene, 0.0, 64, 64, Some(cam));
    let d = diff(&gl, &cpu);
    println!("    quad: {d}");
    if d.share >= 0.02 {
        return Err(format!("GL and CPU differ on {:.2}% of pixels: {d}", d.share * 100.0));
    }
    let (top, bottom) = ((16 * 64 + 32) * 4, (48 * 64 + 32) * 4);
    if !(gl[top] > 200 && gl[top + 2] < 50) {
        return Err("top is not red".into());
    }
    if !(gl[bottom + 2] > 200 && gl[bottom] < 50) {
        return Err("bottom is not blue".into());
    }
    Ok(())
}

fn every_feature_banner_matches_the_soft_renderer(ctx: &Ctx) -> Result<(), String> {
    let manifest = std::fs::read_to_string(root().join("render/manifest.txt")).unwrap();
    let (mut count, mut fails, mut worst) = (0, Vec::new(), Diff::default());
    for line in manifest.lines() {
        let (id, t) = line.split_once(' ').unwrap();
        let t: f32 = t.parse().unwrap();
        let name = id.rsplit_once('-').unwrap().0;
        let bytes = std::fs::read(root().join(format!("scene/features/{name}.cbmd"))).unwrap();
        let scene = Cbmd::parse(&bytes).and_then(|c| c.scene()).unwrap();
        let (gl, cpu) = (ctx.render(&scene, t, 400, 240, None), soft(&scene, t, 400, 240, None));
        let d = diff(&gl, &cpu);
        println!("    {id:24} t={t:5} {d}");
        shots(id, &gl, &cpu, 400, 240);
        worst = Diff {
            max: worst.max.max(d.max),
            mean: worst.mean.max(d.mean),
            covered_mean: worst.covered_mean.max(d.covered_mean),
            share: worst.share.max(d.share),
        };
        count += 1;
        if !within(&d) {
            fails.push(format!("{id}: {d}"));
        }
    }
    println!("    {}/{count} within tolerance; worst {worst}", count - fails.len());
    if count != 54 {
        return Err(format!("{count} frames, want 54"));
    }
    if fails.is_empty() { Ok(()) } else { Err(fails.join("\n")) }
}

fn every_drawable_case_matches_the_soft_renderer(ctx: &Ctx) -> Result<(), String> {
    let manifest = std::fs::read_to_string(root().join("render/cases/manifest.txt")).unwrap();
    let (mut count, mut fails, mut worst) = (0, Vec::new(), Diff::default());
    for file in manifest.lines() {
        let bytes = std::fs::read(root().join(format!("scene/cases/{file}"))).unwrap();
        let scene = if file.ends_with(".cbmd") {
            Cbmd::parse(&bytes).and_then(|c| c.scene())
        } else {
            banner3ds_scene::cgfx::parse(&bytes)
        }
        .unwrap();
        let player = banner3ds_scene::draw::Player::new(&scene);
        let cam = camera::fit(&scene, &player, 128, 128);
        let d = diff(&ctx.render(&scene, 0.0, 128, 128, Some(cam.clone())), &soft(&scene, 0.0, 128, 128, Some(cam)));
        println!("    {file:30} {d}");
        worst = Diff {
            max: worst.max.max(d.max),
            mean: worst.mean.max(d.mean),
            covered_mean: worst.covered_mean.max(d.covered_mean),
            share: worst.share.max(d.share),
        };
        count += 1;
        if !within(&d) {
            fails.push(format!("{file}: {d}"));
        }
    }
    println!("    {}/{count} within tolerance; worst {worst}", count - fails.len());
    if count != 40 {
        return Err(format!("{count} cases, want 40"));
    }
    if fails.is_empty() { Ok(()) } else { Err(fails.join("\n")) }
}

/// A viewport inside a larger framebuffer: the banner lands there, cleared there only; the rest keeps what it had.
fn a_viewport_draws_only_inside_itself(ctx: &Ctx) -> Result<(), String> {
    let scene = render::quad(render::red_over_blue(), 30.0);
    let player = banner3ds_scene::draw::Player::new(&scene);
    let cam = camera::fit(&scene, &player, 64, 64);
    let alone = ctx.render(&scene, 0.0, 64, 64, Some(cam.clone()));
    ctx.fill([0.0, 1.0, 0.0, 1.0]);
    // SAFETY: the context is current.
    unsafe {
        let mut r = Renderer::new(&scene, ctx.gl).expect("drawable");
        r.draw_with(
            0.0,
            Viewport { x: 100, y: 50, width: 64, height: 64 },
            &DrawOptions { camera: Some(cam), clip_frame: None },
        );
        r.destroy();
    }
    if ctx.read(100, 50, 64, 64) != alone {
        return Err("the viewport's pixels differ from a frame drawn at the origin".into());
    }
    let around = ctx.read(0, 0, 300, 200);
    for y in 0..200usize {
        for x in 0..300usize {
            let gl_y = 199 - y;
            let inside = (100..164).contains(&x) && (50..114).contains(&gl_y);
            if !inside && around[(y * 300 + x) * 4..(y * 300 + x) * 4 + 4] != [0, 255, 0, 255] {
                return Err(format!("pixel ({x}, {gl_y}) outside the viewport changed"));
            }
        }
    }
    Ok(())
}

/// Destroying a renderer and making another in the same context draws the same frame.
fn a_destroyed_renderers_context_takes_another(ctx: &Ctx) -> Result<(), String> {
    let bytes = std::fs::read(root().join("scene/features/bone-anim.cbmd")).unwrap();
    let scene = Cbmd::parse(&bytes).and_then(|c| c.scene()).unwrap();
    let a = ctx.render(&scene, 30.0, 400, 240, None);
    let b = ctx.render(&scene, 30.0, 400, 240, None);
    if a != b {
        return Err("a second renderer drew a different frame".into());
    }
    if a.iter().all(|&x| x == 0) {
        return Err("nothing drawn".into());
    }
    Ok(())
}

/// The SuperTuxKart showcase banner against HOME in an emulator (fixtures/showcase/supertuxkart/banner.home), scored like the soft
/// renderer's check in gamemeta-fixtures' tests/stk_banner.rs: a banner over `reference::MATCH` is a mismatch.
fn the_showcase_banner_matches_its_home_reference(ctx: &Ctx) -> Result<(), String> {
    let dir = root().join("showcase/supertuxkart");
    let bytes = std::fs::read(dir.join("banner.cbmd")).unwrap();
    let scene = Cbmd::parse(&bytes).and_then(|c| c.scene()).unwrap();
    let keys = reference::keyframes(&dir.join("banner.home"));
    let fit = reference::fit(&keys, 16, 8, |frame| {
        ctx.render(&scene, frame, reference::WIDTH as i32, reference::HEIGHT as i32, None)
    });
    print!("    gles vs HOME: {fit}");
    for k in &keys {
        let frame = k.clip as f32 + fit.offset as f32;
        let (w, h) = (reference::WIDTH, reference::HEIGHT);
        let gl = ctx.render(&scene, frame, w as i32, h as i32, None);
        shots(&format!("showcase-c{:04}", k.clip), &gl, &soft(&scene, frame, w, h, None), w, h);
    }
    if fit.score > reference::MATCH {
        return Err(format!("mean {:.2} > {}", fit.score, reference::MATCH));
    }
    fit.check(gamemeta_fixtures::stk_banner::KNOWN_MISMATCHES)
}

fn main() {
    if !cfg!(target_os = "linux") {
        println!("headless GL: skipped (the EGL surfaceless check runs on Linux)");
        return;
    }
    // SAFETY: single-threaded here, before EGL loads.
    unsafe { headless::prefer_mesa_software() };
    let filter: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect();
    let ctx = match Headless::new(512, 512).and_then(|h| {
        // SAFETY: eglGetProcAddress gives the current ES context's entry points (core ones included: EGL 1.5).
        let gl = unsafe { Gl::load(|n| h.proc_address(n)) }.map_err(|m| format!("missing {:?}", m.0))?;
        Ok(Ctx { headless: h, gl })
    }) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("headless GL: no context: {e}");
            std::process::exit(1);
        }
    };
    println!("headless GL: EGL surfaceless pbuffer, {}", ctx.renderer);
    type Check = fn(&Ctx) -> Result<(), String>;
    let checks: [(&str, Check); 6] = [
        ("gl_draws_the_textured_quad_like_the_cpu_renderer", gl_draws_the_textured_quad_like_the_cpu_renderer),
        ("every_feature_banner_matches_the_soft_renderer", every_feature_banner_matches_the_soft_renderer),
        ("every_drawable_case_matches_the_soft_renderer", every_drawable_case_matches_the_soft_renderer),
        ("a_viewport_draws_only_inside_itself", a_viewport_draws_only_inside_itself),
        ("a_destroyed_renderers_context_takes_another", a_destroyed_renderers_context_takes_another),
        ("the_showcase_banner_matches_its_home_reference", the_showcase_banner_matches_its_home_reference),
    ];
    let (mut passed, mut failed) = (0, 0);
    for (name, check) in checks {
        if !filter.is_empty() && !filter.iter().any(|f| name.contains(f.as_str())) {
            continue;
        }
        match check(&ctx) {
            Ok(()) => {
                println!("test {name} ... ok");
                passed += 1;
            }
            Err(e) => {
                println!("test {name} ... FAILED\n{e}");
                failed += 1;
            }
        }
    }
    println!("test result: {}. {passed} passed; {failed} failed", if failed == 0 { "ok" } else { "FAILED" });
    drop(ctx);
    if failed > 0 {
        std::process::exit(101);
    }
}
