// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The cartridge readers (GB/GBC, GBA, N64, SNES, NES, Mega Drive/32X, SMS/GG, zips of them) on arbitrary bytes: the first byte
//! picks the file name, the rest is the file. Never panics, never reads more than the headers (and the streamed checksums read
//! only inside the file), what comes back is well formed, and the N64 RA hash agrees with itself however the file is cut up.
#![no_main]

use gamemeta_core::carts::{self, CartHeader, Family};
use gamemeta_core::ids::{RaHasher, ra_hash};
use gamemeta_core::{ByteSource, SliceSource, read};
use libfuzzer_sys::fuzz_target;

const NAMES: [&str; 22] = [
    "a.gb", "a.gbc", "a.sgb", "a.gba", "a.agb", "a.z64", "a.v64", "a.n64", "a.sfc", "a.smc", "a.nes", "a.md", "a.gen", "a.bin",
    "a.smd", "a.32x", "a.sms", "a.gg", "a.zip", "A.GB", "a.xyz", "noext",
];

struct Counting<'a> {
    inner: SliceSource<'a>,
    asked: u64,
}

impl ByteSource for Counting<'_> {
    fn size(&self) -> u64 {
        self.inner.size()
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), gamemeta_core::ReadError> {
        self.asked += buf.len() as u64;
        self.inner.read_at(offset, buf)
    }
}

fn well_formed(header: &CartHeader) {
    let json = header.to_json();
    assert!(json.starts_with("{\"family\":\"") && json.ends_with('}'));
    assert!(!header.fields().is_empty());
}

fuzz_target!(|data: &[u8]| {
    let Some((&pick, file)) = data.split_first() else { return };
    let name = NAMES[usize::from(pick) % NAMES.len()];
    let mut src = Counting { inner: SliceSource::new(file), asked: 0 };
    let family = Family::for_file(name);
    if let Some(info) = read(&mut src, name) {
        assert!(info.titles.len() <= 2);
        for (_, t) in &info.titles {
            assert!(!t.name.is_empty() && t.name == t.name.trim());
        }
        if family.is_some() && name != "a.zip" {
            // A header read only ever looks at the start of the file, and the SNES windows at three offsets.
            assert!(src.asked <= 0x5000, "asked for {} bytes", src.asked);
            assert!(info.cart.is_some(), "a cartridge file with a result has its header");
        }
        if let Some(header) = &info.cart {
            well_formed(header);
        }
        assert!(info.ids.game_id.as_ref().is_none_or(|g| !g.is_empty() && g.is_ascii()));
        assert!(info.ids.maker_code.as_ref().is_none_or(|g| !g.is_empty()));
    }
    // The streamed checksums stay inside the file.
    let _ = carts::checksum_matches(&mut SliceSource::new(file), name);
    // The pure parsers on the raw bytes, whatever the extension.
    let _ = carts::gb::parse(file);
    let _ = carts::gba::parse(file);
    let _ = carts::n64::parse(file);
    let _ = carts::nes::parse(file);
    let _ = carts::snes::parse(file, file.len() as u64);
    let _ = carts::snes::parse(file, file.len() as u64 + 512);
    let _ = carts::md::parse(file, carts::md::Layout::Plain);
    let _ = carts::md::deinterleave(file);
    for at in carts::sms::OFFSETS {
        let _ = carts::sms::parse(file.get(at as usize..).unwrap_or(&[]), at, pick & 1 == 0);
    }
    // The N64 RA hash agrees with itself however the file is cut up.
    let whole = ra_hash(&mut SliceSource::new(file), "a.z64");
    let chunked = RaHasher::for_file("a.z64", Some(file.len() as u64)).and_then(|mut h| {
        for piece in file.chunks(1 + usize::from(pick)) {
            h.update(piece);
        }
        h.finish()
    });
    assert_eq!(whole, chunked);
});
