// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Writes the SuperTuxKart demo set (the 3DS files with the showcase banner unless `--banner3ds` names another): `write-demo [--banner3ds <file>] [--images <dir>] [<demo dir> [<showcase dir>]]`.
//! The demo files go to `<demo dir>` (default `demo`), the golden texts to `<showcase dir>/golden/` and the contact sheet
//! to `<showcase dir>/contact.png` (default `fixtures/showcase/supertuxkart/formats`); `--images` also writes each
//! extracted image as its own PNG (for a page; not committed).
use gamemeta_fixtures::showcase::{self, art, report};
use std::path::Path;

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("a path has a folder")).expect("create a folder");
    std::fs::write(path, bytes).expect("write a file");
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take = |flag: &str| {
        let at = args.iter().position(|a| a == flag)?;
        args.remove(at);
        Some(args.remove(at))
    };
    let banner3ds = Some(
        take("--banner3ds")
            .map_or_else(|| showcase::BANNER3DS.to_vec(), |p| std::fs::read(p).expect("read the banner")),
    );
    let images_dir = take("--images");
    let demo = args.first().cloned().unwrap_or_else(|| "demo".into());
    let showcase_dir = args.get(1).cloned().unwrap_or_else(|| "fixtures/showcase/supertuxkart/formats".into());
    let files = showcase::build(&showcase::Options { banner3ds });
    for (name, bytes) in &files {
        write(&Path::new(&demo).join(name), bytes);
    }
    for name in showcase::goldens(&files) {
        write(
            &Path::new(&showcase_dir).join("golden").join(format!("{}.txt", name.0.replace('/', "__"))),
            name.1.as_bytes(),
        );
    }
    let images = report::images(&files);
    let sheet = report::contact_sheet(&images, 8, 2);
    write(&Path::new(&showcase_dir).join("contact.png"), &art::png_bytes(&sheet));
    if let Some(dir) = images_dir {
        for (n, (file, label, image)) in images.iter().enumerate() {
            let name = format!("{n:02}-{}-{}.png", file.replace('/', "_"), label.replace(' ', "-"));
            write(&Path::new(&dir).join(name), &art::png_bytes(image));
        }
    }
    println!("{} files in {demo}, {} images on the sheet", files.len(), images.len());
}
