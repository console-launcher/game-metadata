// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Writes the synthetic fixtures the bindings' tests read: `write-fixtures <dir>`.
fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "fixtures".into());
    std::fs::create_dir_all(&dir).expect("create the fixture directory");
    for (name, bytes) in gamemeta_fixtures::samples() {
        let path = std::path::Path::new(&dir).join(name);
        std::fs::create_dir_all(path.parent().expect("a fixture path has a folder")).expect("create a fixture folder");
        std::fs::write(&path, bytes).expect("write a fixture");
        println!("{}", path.display());
    }
}
