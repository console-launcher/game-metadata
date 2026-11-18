// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The id readers (ISO 9660, PlayStation, PSP, CHD, zip, NSP, Xbox, GameCube containers, cue sheets, RA hashes) on arbitrary
//! bytes: the first byte picks the file name, the rest is the file, and every sibling file the readers ask for is the same bytes.
//! Never panics, and what comes back is well formed.
#![no_main]

use gamemeta_core::ids::{self, DiscIdResult, Opener, RaHasher};
use gamemeta_core::{ByteSource, SliceSource};
use libfuzzer_sys::fuzz_target;

const NAMES: [&str; 32] = [
    "a.iso", "a.gcm", "a.rvz", "a.wia", "a.wbfs", "a.ciso", "a.gcz", "a.bin", "a.img", "a.cue", "EBOOT.PBP", "a.wad", "a.3ds",
    "a.cia", "a.nsp", "a.wua", "game/code/a.rpx", "default.xbe", "a.zip", "a.chd", "a.nes", "a.sfc", "a.smc", "a.gb", "a.md",
    "a.a26", "a.xyz", "A.ISO", "a.CUE", "noext", "a.tgc", "a.cxi",
];

/// Every sibling a reader asks for is the file itself.
struct Same<'a>(&'a [u8]);

impl Opener for Same<'_> {
    fn open(&mut self, _path: &str) -> Option<Box<dyn ByteSource + '_>> {
        Some(Box::new(SliceSource::new(self.0)))
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&pick, file)) = data.split_first() else { return };
    let name = NAMES[usize::from(pick) % NAMES.len()];
    let mut src = SliceSource::new(file);
    if let Some(found) = ids::read_with(&mut src, name, &mut Same(file)) {
        assert!(!found.is_empty());
        assert!(found.game_id.as_ref().is_none_or(|g| g.len() == 6 || g.len() == 4));
        assert!(found.chd_sha1.as_ref().is_none_or(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())));
        assert!(found.zip_crc.as_ref().is_none_or(|c| c.len() == 8));
        assert_eq!(found.zip_crc.is_some(), found.zip_size.is_some());
        assert!(found.psp_title.as_ref().is_none_or(|t| t.len() <= ids::psp::MAX_READ));
    }
    // The streaming hasher agrees with itself however the file is cut up.
    let size = file.len() as u64;
    let whole = ids::ra_hash(&mut SliceSource::new(file), name);
    let chunked = RaHasher::for_file(name, Some(size)).and_then(|mut h| {
        for piece in file.chunks(1 + usize::from(pick)) {
            h.update(piece);
        }
        h.finish()
    });
    assert_eq!(whole, chunked);
    if let DiscIdResult::Ok { id, .. } = ids::disc_id(file) {
        assert!(id.len() == 6 || id.len() == 8);
    }
    assert_eq!(src.size(), size);
});
