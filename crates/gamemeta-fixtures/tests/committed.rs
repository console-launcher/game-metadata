// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The committed fixtures the bindings read are exactly what the generator writes.
#[test]
fn the_committed_samples_match_the_generator() {
    for (name, bytes) in gamemeta_fixtures::samples() {
        let path = format!("{}/../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let committed = std::fs::read(&path).unwrap_or_else(|_| {
            panic!("fixtures/{name} (cargo run -p gamemeta-fixtures --bin write-fixtures fixtures)")
        });
        assert!(committed == bytes, "fixtures/{name} is stale: rerun write-fixtures");
    }
}
