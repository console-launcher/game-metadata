// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Writes the SuperTuxKart showcase banner: `write-stk-banner [<showcase dir>] [-o <banner.cbmd>]` (defaults: the repo's
//! `fixtures/showcase/supertuxkart`, its `banner.cbmd`). Same inputs, same bytes.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut dir = gamemeta_fixtures::stk_banner::folder();
    let mut out = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "-o" {
            out = it.next().map(std::path::PathBuf::from);
        } else {
            dir = a.into();
        }
    }
    let out = out.unwrap_or_else(|| dir.join("banner.cbmd"));
    let bytes = gamemeta_fixtures::stk_banner::banner(&dir);
    std::fs::write(&out, &bytes).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    eprintln!("{}: {} bytes", out.display(), bytes.len());
}
