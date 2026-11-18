// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The committed `demo/` folder, the golden texts and the contact sheet are exactly what the generator writes
//! (`cargo run -p gamemeta-fixtures --bin write-demo`).
use gamemeta_fixtures::showcase::{self, art, report};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("read the folder").map(|e| e.expect("entry")).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = format!("{prefix}{}", e.file_name().to_string_lossy());
        if e.file_type().expect("type").is_dir() {
            walk(&e.path(), &format!("{name}/"), out);
        } else {
            out.push(name);
        }
    }
}

#[test]
fn the_committed_demo_folder_matches_the_generator() {
    let files = showcase::files();
    let mut on_disk = Vec::new();
    walk(&root().join("demo"), "", &mut on_disk);
    let mut wanted: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
    wanted.sort();
    assert_eq!(
        on_disk, wanted,
        "demo/ holds exactly the generated files (cargo run -p gamemeta-fixtures --bin write-demo)"
    );
    for (name, bytes) in &files {
        let committed = std::fs::read(root().join("demo").join(name)).expect("read");
        assert!(committed == *bytes, "demo/{name} is stale: rerun write-demo");
    }
}

#[test]
fn the_golden_texts_and_the_contact_sheet_match_what_the_readers_give() {
    let files = showcase::files();
    let dir = root().join("fixtures/showcase/supertuxkart/formats");
    let mut on_disk = Vec::new();
    walk(&dir.join("golden"), "", &mut on_disk);
    let goldens = showcase::goldens(&files);
    let mut wanted: Vec<String> = goldens.iter().map(|(n, _)| format!("{}.txt", n.replace('/', "__"))).collect();
    wanted.sort();
    assert_eq!(on_disk, wanted, "one golden per demo file");
    for (name, text) in &goldens {
        let path = dir.join("golden").join(format!("{}.txt", name.replace('/', "__")));
        let committed = std::fs::read_to_string(&path).expect("read");
        assert_eq!(committed, *text, "golden for {name} (rerun write-demo and look at the diff)");
    }
    let sheet = art::png_bytes(&report::contact_sheet(&report::images(&files), 8, 2));
    assert!(std::fs::read(dir.join("contact.png")).expect("read") == sheet, "contact.png is stale: rerun write-demo");
}
