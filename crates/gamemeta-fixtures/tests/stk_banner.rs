// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The SuperTuxKart showcase banner (fixtures/showcase/supertuxkart): the committed banner is what the builder makes, its scene
//! dumps to the committed golden JSON, and the soft renderer draws it like HOME did (`banner.home/`, captured by
//! the capture tooling in scripts/) each keyframe within the match bar or its known-mismatch ceiling
//! (`stk_banner::KNOWN_MISMATCHES`).

use banner3ds_scene::{Cbmd, dump};
use gamemeta_fixtures::{reference, stk_banner};

#[test]
fn the_committed_banner_is_what_the_builder_makes() {
    let dir = stk_banner::folder();
    let built = stk_banner::banner(&dir);
    let committed = std::fs::read(dir.join("banner.cbmd")).unwrap();
    assert!(built == committed, "fixtures/showcase/supertuxkart/banner.cbmd is stale: run its build.sh");
}

#[test]
fn every_language_slot_shows_the_scene() {
    let bytes = std::fs::read(stk_banner::folder().join("banner.cbmd")).unwrap();
    for slot in 0..30 {
        let at = u32::from_le_bytes(bytes[0xC + 4 * slot..0x10 + 4 * slot].try_into().unwrap());
        assert_eq!(at, 0x88, "language slot {slot}");
    }
}

#[test]
fn the_banner_dumps_to_the_golden_scene() {
    let dir = stk_banner::folder();
    let got = dump::banner_json(&std::fs::read(dir.join("banner.cbmd")).unwrap());
    let path = dir.join("banner.json");
    if std::env::var_os("GM_WRITE_GOLDEN").is_some() {
        std::fs::write(&path, format!("{}\n", got.trim_end())).unwrap();
    }
    let want = std::fs::read_to_string(&path).expect("banner.json (write it with GM_WRITE_GOLDEN=1)");
    assert!(got.trim_end() == want.trim_end(), "the scene dump drifted from banner.json");
}

/// The banner within HOME's limits measured in an emulator: the decompressed scene under 512 KiB (a 552,192-byte one is not shown).
#[test]
fn the_banner_fits_homes_scene_limit() {
    let bytes = std::fs::read(stk_banner::folder().join("banner.cbmd")).unwrap();
    let cgfx = banner3ds_scene::lz11::decompress(&bytes, 0x88, 8 << 20).unwrap();
    assert!(cgfx.len() <= 512 * 1024, "{} bytes", cgfx.len());
}

#[test]
fn soft_draws_the_banner_like_home() {
    let dir = stk_banner::folder();
    let scene = Cbmd::parse(&std::fs::read(dir.join("banner.cbmd")).unwrap()).and_then(|c| c.scene()).unwrap();
    let keys = reference::keyframes(&dir.join("banner.home"));
    assert_eq!(keys.len(), 5, "keyframes");
    let renderer = banner3ds_soft::Renderer::new(&scene);
    let fit = reference::fit(&keys, 16, 8, |frame| {
        let mut out = vec![0u8; (reference::WIDTH * reference::HEIGHT * 4) as usize];
        renderer.render(frame, reference::WIDTH, reference::HEIGHT, &mut out).unwrap();
        out
    });
    println!("soft vs HOME: {fit}");
    assert!(fit.score <= reference::MATCH, "mean {:.2} > {}", fit.score, reference::MATCH);
    fit.check(stk_banner::KNOWN_MISMATCHES).unwrap_or_else(|e| panic!("{e}"));
}
