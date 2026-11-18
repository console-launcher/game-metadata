// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The banner scene parser on arbitrary bytes: CBMD containers (LZ11 or raw CGFX), bare CGFX files, and the scene dump. Never
//! panics, never allocates without bound; what comes back is well formed (indices inside their vertex arrays, one vertex
//! attribute value per vertex, pictures as big as their size says).
#![no_main]

use banner3ds_scene::{Cbmd, cgfx, dump};
use libfuzzer_sys::fuzz_target;

fn check(scene: &banner3ds_scene::BannerScene) {
    for model in &scene.models {
        for mesh in &model.meshes {
            let v = &mesh.vertices;
            assert_eq!(v.positions.len(), v.count * 3);
            assert!(mesh.indices.len() % 3 == 0);
            assert!(mesh.indices.iter().all(|&i| (i as usize) < v.count));
            if let Some(n) = &v.normals {
                assert_eq!(n.len(), v.count * 3);
            }
            if let Some(c) = &v.colors {
                assert_eq!(c.len(), v.count * 4);
            }
            if let Some(b) = &v.bone_indices {
                assert_eq!(b.len(), v.count * 4);
            }
        }
    }
    for image in scene.textures.values() {
        assert_eq!(image.pixels.len(), (image.width * image.height * 4) as usize);
    }
    for a in scene.animations.skeletal.iter().chain(&scene.animations.material).chain(&scene.animations.visibility) {
        for t in &a.tracks {
            assert_eq!(t.keys.frames.len(), t.keys.values.len());
            // Sampling any frame must not panic.
            let _ = t.keys.sample(a.frames * 0.5);
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if let Some(cbmd) = Cbmd::parse(data) {
        if let Some(scene) = cbmd.scene() {
            check(&scene);
            let _ = dump::scene_json(&scene);
        }
    }
    if let Some(scene) = cgfx::parse(data) {
        check(&scene);
        let _ = dump::scene_json(&scene);
    }
    let _ = cgfx::textures(data);
});
