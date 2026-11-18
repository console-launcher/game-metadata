// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The CPU renderer on arbitrary banners at small sizes: byte 0 and 1 are the width and height (1 to 64), byte 2 the
//! frame, the rest a CBMD or bare CGFX file. Never panics (hostile meshes, bones, textures, LUTs and animation keys), and
//! the frame does not depend on what the output buffer held before.
#![no_main]

use banner3ds_scene::{BannerScene, Cbmd, cgfx};
use libfuzzer_sys::fuzz_target;

fn draw(scene: &BannerScene, frame: f32, width: u32, height: u32) {
    let len = (width * height * 4) as usize;
    let mut a = vec![0u8; len];
    let mut b = vec![0xA5u8; len];
    let r = banner3ds_soft::Renderer::new(scene);
    assert!(r.render(frame, width, height, &mut a).is_ok());
    assert!(r.render(frame, width, height, &mut b).is_ok());
    assert!(a == b, "the frame depends on the buffer's old contents");
    assert!(r.render(frame, width, height, &mut a[1..]).is_err());
}

fuzz_target!(|data: &[u8]| {
    let [w, h, f, rest @ ..] = data else { return };
    let (width, height, frame) = (u32::from(w % 64) + 1, u32::from(h % 64) + 1, f32::from(*f));
    if let Some(scene) = Cbmd::parse(rest).and_then(|c| c.scene()) {
        draw(&scene, frame, width, height);
    }
    if let Some(scene) = cgfx::parse(rest) {
        draw(&scene, frame, width, height);
    }
});
