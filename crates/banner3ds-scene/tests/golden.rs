// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Parity with the original implementation: every fixture's scene dumps to exactly the JSON the original implementation wrote
//! (fixtures/scene/*/NAME.json). The inputs are the 18 original feature banners (`features/`) and the original test
//! builders' output (`cases/`, see fixtures/scene/README.md).

use banner3ds_scene::dump;
use std::fs;
use std::path::PathBuf;

fn dir(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/scene").join(sub)
}

/// (name, input bytes, golden) of every `.json` in `sub` that has a `.cbmd` or `.cgfx` beside it.
fn fixtures(sub: &str) -> Vec<(String, bool, Vec<u8>, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir(sub)).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let cbmd = path.with_extension("cbmd");
        let (is_cbmd, input) = if cbmd.exists() {
            (true, fs::read(cbmd).unwrap())
        } else {
            (false, fs::read(path.with_extension("cgfx")).unwrap())
        };
        out.push((name, is_cbmd, input, fs::read_to_string(&path).unwrap()));
    }
    out.sort();
    out
}

fn check(sub: &str, want_count: usize) {
    let all = fixtures(sub);
    assert_eq!(all.len(), want_count, "fixtures in {sub}");
    let mut bad = Vec::new();
    for (name, is_cbmd, input, golden) in &all {
        let got = if *is_cbmd { dump::banner_json(input) } else { dump::cgfx_json(input) };
        if got.trim_end() != golden.trim_end() {
            bad.push(name.clone());
        }
    }
    assert!(bad.is_empty(), "{} of {} differ from the original implementation: {bad:?}", bad.len(), all.len());
}

#[test]
fn the_18_feature_banners_dump_like_the_reference() {
    check("features", 18);
}

#[test]
fn the_builder_cases_dump_like_the_reference() {
    check("cases", 53);
}
