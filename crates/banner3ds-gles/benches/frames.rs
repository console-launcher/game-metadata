// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Frames per second at HOME's top-screen size (400 x 240) for every feature banner: the CPU renderer (`banner3ds-soft`), and
//! the GL renderer on the headless EGL context (llvmpipe unless the environment picks a driver), drawn and finished, and drawn
//! and read back. The original implementation's `BannerGlBench` prints the same three columns in milliseconds per frame.
//! `cargo bench -p banner3ds-gles --bench frames [-- <word>]`.

use std::hint::black_box;
use std::time::{Duration, Instant};

use banner3ds_gles::{Gl, Renderer, Viewport};
use banner3ds_scene::Cbmd;
use gamemeta_fixtures::headless::{self, Headless};

const W: u32 = 400;
const H: u32 = 240;

/// Mean milliseconds per call of `f`, timed over at least `budget` after one warm-up call.
fn time(budget: Duration, mut f: impl FnMut(u32)) -> f64 {
    f(0);
    let (start, mut n) = (Instant::now(), 0u32);
    while n < 3 || start.elapsed() < budget {
        f(n);
        n += 1;
    }
    start.elapsed().as_secs_f64() * 1000.0 / f64::from(n)
}

fn main() {
    // SAFETY: single-threaded here, before EGL loads.
    unsafe { headless::prefer_mesa_software() };
    let filter: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect();
    let gl = Headless::new(W as i32, H as i32).and_then(|h| {
        // SAFETY: eglGetProcAddress gives the current ES context's entry points.
        let gl = unsafe { Gl::load(|n| h.proc_address(n)) }.map_err(|m| format!("missing {:?}", m.0))?;
        Ok((h, gl))
    });
    match &gl {
        Ok((h, _)) => println!("gl: {}", h.renderer),
        Err(e) => println!("gl: none ({e})"),
    }
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/scene/features");
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cbmd"))
        .collect();
    names.sort();
    let budget = Duration::from_millis(500);
    let (mut sums, mut count) = ([0.0f64; 3], 0);
    println!("{:20} {:>9} {:>9} {:>9}   ms/frame (fps)", "banner", "soft", "gl", "gl+read");
    for path in names {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        if !filter.is_empty() && !filter.iter().any(|f| name.contains(f.as_str())) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let scene = Cbmd::parse(&bytes).and_then(|c| c.scene()).unwrap();
        let soft = banner3ds_soft::Renderer::new(&scene);
        let frames = soft.frames().max(1.0);
        let mut buf = vec![0u8; (W * H * 4) as usize];
        let cpu = time(budget, |i| {
            soft.render(i as f32 % frames, W, H, &mut buf).unwrap();
            black_box(&buf);
        });
        let (g, gr) = match &gl {
            Ok((h, gl)) => {
                // SAFETY: the headless context is current on this thread for the renderer's whole life.
                let mut r = unsafe { Renderer::new(&scene, *gl) }.unwrap();
                let v = Viewport::of(W as i32, H as i32);
                let g = time(budget, |i| {
                    // SAFETY: as above.
                    unsafe { r.draw(i as f32 % frames, v) };
                    h.finish();
                });
                let gr = time(budget, |i| {
                    // SAFETY: as above.
                    unsafe { r.draw(i as f32 % frames, v) };
                    black_box(h.read(0, 0, W as i32, H as i32));
                });
                // SAFETY: as above.
                unsafe { r.destroy() };
                (g, gr)
            }
            Err(_) => (f64::NAN, f64::NAN),
        };
        println!("{name:20} {cpu:9.3} {g:9.3} {gr:9.3}   ({:.0} / {:.0} / {:.0} fps)", 1e3 / cpu, 1e3 / g, 1e3 / gr);
        for (s, v) in sums.iter_mut().zip([cpu, g, gr]) {
            *s += v;
        }
        count += 1;
    }
    let m = sums.map(|s| s / f64::from(count.max(1)));
    println!(
        "{:20} {:9.3} {:9.3} {:9.3}   ({:.0} / {:.0} / {:.0} fps)",
        "mean",
        m[0],
        m[1],
        m[2],
        1e3 / m[0],
        1e3 / m[1],
        1e3 / m[2]
    );
}
