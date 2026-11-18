// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Rewrites the authored banner archives of `fixtures/wii-banner`: `write-wii-banner-fixtures [dir]`. The PNGs next to them
//! are the pixel goldens and stay as they are; run this only when the writer's layout changes and the goldens still hold.
fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "fixtures/wii-banner".into());
    for c in gamemeta_fixtures::wii_banner_cases::cases() {
        let path = std::path::Path::new(&dir).join(format!("{}.bin", c.name));
        std::fs::write(&path, &c.archive).expect("write an archive");
        println!("{}", path.display());
    }
}
