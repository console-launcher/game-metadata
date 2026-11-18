// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Rust snippet of the README, compiled so it cannot rot (`scripts/readme-snippets` checks the README shows exactly this).
//! It is built, not run: `cargo run -p gamemeta-core --example readme` needs a `game.3ds` in the current directory.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // readme:begin
    use gamemeta_core::{SliceSource, read};

    let bytes = std::fs::read("game.3ds")?;
    if let Some(info) = read(&mut SliceSource::new(&bytes), "game.3ds") {
        println!("{:?}", info.title());
    }
    // readme:end
    Ok(())
}
