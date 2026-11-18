// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The SuperTuxKart demo set: one small file per format the readers take, built in code from SuperTuxKart's free-licensed
//! art (credits in the set's `CREDITS.md`, written from [`CREDITS`]). Headers and art only, no game code. Ids are homebrew
//! style: maker "00" or none, codes and title ids picked at the top of each range no retail title uses (nothing was checked
//! against an external registry): DS and GBA game code `TUXE` (the readers want letters or digits, so no `####`), 3DS unique id 0xFF3FF, Wii U `1FF3FF00`, PS serials `999.xx`.

pub mod art;
mod carts;
pub mod docs;
pub mod hash;
mod nintendo;
mod other;
pub mod report;

use crate::Le;

/// What a caller can add to the set.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The 3DS banner (a CBMD file) that goes into `tux.3ds` and `tux.cia` as the ExeFS `banner` entry. This is the hook for
    /// the 3DS banner: [`files`] passes [`BANNER3DS`]; `write-demo --banner3ds <file>` another.
    pub banner3ds: Option<Vec<u8>>,
}

/// The name shown for the game.
pub const NAME: &str = "SuperTuxKart";
/// The publisher / maker line.
pub const TEAM: &str = "SuperTuxKart Team";

/// "Kart racing" in the twelve languages of the 3DS and Wii U (slot order: ja, en, fr, de, it, es, zhs, ko, nl, pt, ru, zht);
/// the DS uses the first eight, the GameCube six of them.
pub const SUBTITLES: [&str; 12] = [
    "カートレース",
    "Kart racing",
    "Course de karts",
    "Kart-Rennen",
    "Corsa di kart",
    "Carreras de karts",
    "卡丁车竞速",
    "카트 레이싱",
    "Kartracen",
    "Corrida de karts",
    "Гонки на картах",
    "卡丁車競速",
];

/// `NAME` and the subtitle in slot `slot`, on two lines.
pub fn two_lines(slot: usize) -> String {
    format!("{NAME}\n{}", SUBTITLES[slot])
}

/// A path and its bytes.
pub type File = (String, Vec<u8>);

/// Every file of the demo set: path below `demo/` (a folder's files carry its name), bytes. Includes `README.md` and
/// `CREDITS.md`.
pub fn files() -> Vec<File> {
    build(&Options { banner3ds: Some(BANNER3DS.to_vec()) })
}

/// The SuperTuxKart showcase banner (fixtures/showcase/supertuxkart, built by [`crate::stk_banner`]): the default 3DS banner.
pub const BANNER3DS: &[u8] = include_bytes!("../../../../fixtures/showcase/supertuxkart/banner.cbmd");

/// The set, with `options`.
pub fn build(options: &Options) -> Vec<File> {
    let mut out: Vec<File> = Vec::new();
    nintendo::add(&mut out, options);
    other::add(&mut out);
    carts::add(&mut out);
    out.push(("CREDITS.md".into(), docs::credits().into_bytes()));
    out.push(("README.md".into(), docs::readme().into_bytes()));
    out
}

/// The golden texts of the set: (file path, text), one per demo file (the docs and a folder's non-entry files are left out; a
/// folder is read through its \`.rpx\`).
pub fn goldens(files: &[File]) -> Vec<(String, String)> {
    files.iter().filter(|(n, _)| is_demo_entry(n)).map(|(n, _)| (n.clone(), report::golden(files, n))).collect()
}

/// Whether \`path\` is a file a reader opens by itself: not the docs, not the other files of a Wii U title folder.
pub fn is_demo_entry(path: &str) -> bool {
    !path.ends_with(".md") && (!path.starts_with("tux-wiiu/") || path.ends_with(".rpx"))
}

pub(crate) fn put(out: &mut Vec<File>, name: &str, bytes: Vec<u8>) {
    out.push((name.to_string(), bytes));
}

/// A zero-filled buffer with `bytes` written at `at`.
pub(crate) fn le(size: usize) -> Le {
    Le::new(size)
}
