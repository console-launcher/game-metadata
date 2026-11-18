// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the library reads, by platform and file extension: the one list a page, a CLI or a README should show.

use alloc::vec::Vec;

use crate::carts::Family;
use crate::ids::RA_WHOLE;

/// One platform (or family) and the file extensions (lower case, no dot) the library reads for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Support {
    /// A display name.
    pub platform: &'static str,
    /// The extensions, in a stable order.
    pub extensions: Vec<&'static str>,
}

/// Every platform the library reads and its extensions, built from the readers' own extension lists. A file with one of these
/// extensions is read (or, for a hash-only system, hashed); the header decides whether a particular file yields anything.
pub fn supported_formats() -> Vec<Support> {
    let row = |platform, extensions: &[&'static str]| Support { platform, extensions: extensions.to_vec() };
    let mut rows = Vec::new();
    rows.push(row("Nintendo 3DS", &crate::threeds::EXTENSIONS));
    let mut ds = crate::nds::ROM_EXTENSIONS.to_vec();
    ds.push("zip");
    rows.push(Support { platform: "Nintendo DS, DSi", extensions: ds });
    // The GameCube and Wii readers share several extensions: each is listed once.
    let mut discs = crate::gc::EXTENSIONS.to_vec();
    discs.extend(crate::wii::EXTENSIONS.iter().filter(|e| !crate::gc::EXTENSIONS.contains(e)));
    rows.push(Support { platform: "Nintendo GameCube, Wii", extensions: discs });
    rows.push(row("Nintendo Wii U (also a game folder)", &crate::wiiu::EXTENSIONS));
    for family in Family::ALL {
        rows.push(row(family.name(), family.extensions()));
    }
    rows.push(row("PlayStation, PSP", &["iso", "bin", "img", "cue", "pbp"]));
    rows.push(row("PlayStation Vita", &["vpk"]));
    rows.push(row("Nintendo Switch", &["nsp"]));
    rows.push(row("Xbox", &["xbe"]));
    rows.push(row("CHD disc images", &["chd"]));
    // Systems the RetroAchievements hash covers that have no header reader.
    let hash_only: Vec<&'static str> =
        RA_WHOLE.iter().copied().filter(|e| !rows.iter().any(|r| r.extensions.contains(e))).collect();
    rows.push(Support { platform: "Other systems (checksum only)", extensions: hash_only });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{self, RaHasher};

    fn listed() -> Vec<&'static str> {
        supported_formats().into_iter().flat_map(|s| s.extensions).collect()
    }

    /// Whether any reader accepts a file called `x.<ext>`: the format table, the ids readers or the RetroAchievements hash.
    fn accepted(name: &mut alloc::string::String, ext: &str) -> bool {
        name.clear();
        name.push_str("x.");
        name.push_str(ext);
        crate::format_for(name).is_some()
            || ids::EXTENSIONS.contains(&ext)
            || RaHasher::for_file(name, Some(1 << 20)).is_some()
    }

    #[test]
    fn the_platform_rows_are_there_and_every_extension_is_listed_once() {
        let rows = supported_formats();
        for platform in [
            "Nintendo DS, DSi",
            "Nintendo 3DS",
            "Game Boy Advance",
            "Nintendo 64",
            "NES",
            "Sega Genesis, 32X",
            "Master System, Game Gear",
        ] {
            assert!(rows.iter().any(|r| r.platform == platform), "no row for {platform}");
        }
        let ds = rows.iter().find(|r| r.platform == "Nintendo DS, DSi").unwrap();
        assert_eq!(ds.extensions, ["nds", "dsi", "srl", "ids", "zip"]);
        let mut all = listed();
        let total = all.len();
        all.sort_unstable();
        all.dedup();
        // `iso` and `bin` are read as several things, so they may sit in more than one row; nothing else repeats.
        let repeats = total - all.len();
        assert!(repeats <= 4, "{repeats} repeated extensions in {total}");
    }

    #[test]
    fn every_listed_extension_is_read_by_something() {
        let mut name = alloc::string::String::new();
        for ext in listed() {
            assert!(accepted(&mut name, ext), "{ext} is listed but no reader takes it");
        }
    }

    #[test]
    fn every_extension_any_reader_takes_is_listed() {
        // Every name of up to four letters or digits: a reader that takes an extension the list lacks fails here.
        let listed = listed();
        let alphabet: Vec<char> = ('a'..='z').chain('0'..='9').collect();
        let mut name = alloc::string::String::new();
        let mut ext = alloc::string::String::new();
        let mut missing = Vec::new();
        let mut walk = |ext: &str, missing: &mut Vec<alloc::string::String>| {
            if accepted(&mut name, ext) && !listed.contains(&ext) {
                missing.push(ext.into());
            }
        };
        for &a in &alphabet {
            ext.clear();
            ext.push(a);
            walk(&ext, &mut missing);
            for &b in &alphabet {
                ext.truncate(1);
                ext.push(b);
                walk(&ext, &mut missing);
                for &c in &alphabet {
                    ext.truncate(2);
                    ext.push(c);
                    walk(&ext, &mut missing);
                    for &d in &alphabet {
                        ext.truncate(3);
                        ext.push(d);
                        walk(&ext, &mut missing);
                    }
                }
            }
        }
        assert!(missing.is_empty(), "read but not listed: {missing:?}");
    }

    #[test]
    fn the_ids_extension_list_is_the_readers_match() {
        // The `match ext` in `ids::read_with` is the readers; its arms must be exactly `ids::EXTENSIONS`.
        let source = include_str!("ids/mod.rs");
        let body =
            source.split("match ext.as_str() {").nth(1).expect("the match").split("        _ => {}").next().unwrap();
        let mut arms: Vec<&str> = body
            .lines()
            .filter(|l| l.starts_with("        \"") && l.contains("=>"))
            .flat_map(|l| l.split("=>").next().unwrap().split('|'))
            .map(|s| s.trim().trim_matches('"'))
            .collect();
        arms.sort_unstable();
        let mut listed: Vec<&str> = ids::EXTENSIONS.to_vec();
        listed.sort_unstable();
        assert_eq!(arms, listed);
    }
}
