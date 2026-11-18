// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reading a scene from a file: a bare CBMD or CGFX, or a 3DS file (NCSD, CIA, NCCH) holding the banner in its ExeFS.

use banner3ds_scene::source::{SceneError, read};
use gamemeta_core::SliceSource;
use gamemeta_fixtures as fx;
use std::fs;

fn feature(name: &str) -> Vec<u8> {
    fs::read(format!("{}/../../fixtures/scene/features/{name}.cbmd", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn scene_of(bytes: &[u8]) -> Result<banner3ds_scene::BannerScene, SceneError> {
    read(&mut SliceSource::new(bytes))
}

#[test]
fn a_bare_cbmd_reads() {
    let scene = scene_of(&feature("alpha-test")).unwrap();
    assert!(!scene.models.is_empty());
}

#[test]
fn a_bare_cgfx_reads() {
    let bytes = fs::read(format!("{}/../../fixtures/scene/cases/quad.cgfx", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(scene_of(&bytes).unwrap().models.len(), 1);
}

#[test]
fn a_3ds_file_gives_its_exefs_banner() {
    let exefs = fx::exefs(&[("banner", &feature("alpha-test"))]);
    let want = scene_of(&feature("alpha-test")).unwrap();
    for file in [fx::ncsd(&fx::plain_ncch(&exefs)), fx::cia(&fx::plain_ncch(&exefs)), fx::plain_ncch(&exefs)] {
        assert_eq!(scene_of(&file).unwrap(), want);
    }
}

#[test]
fn an_encrypted_3ds_file_is_encrypted_and_never_decrypted() {
    let noise: Vec<u8> = (0..0x400).map(|i: i32| (i * 31 + 7) as u8).collect();
    assert_eq!(scene_of(&fx::ncsd(&fx::ncch(&noise, 0))).unwrap_err(), SceneError::Encrypted);
}

#[test]
fn files_without_a_banner_are_not_found() {
    assert_eq!(scene_of(&[]).unwrap_err(), SceneError::NotFound);
    assert_eq!(scene_of(b"just some text").unwrap_err(), SceneError::NotFound);
    let no_banner = fx::plain_ncch(&fx::exefs(&[("icon", &[0; 16])]));
    assert_eq!(scene_of(&no_banner).unwrap_err(), SceneError::NotFound);
    // A CBMD whose CGFX does not parse.
    assert_eq!(scene_of(&fx::cbmd(b"garbage", &[])).unwrap_err(), SceneError::NotFound);
}
