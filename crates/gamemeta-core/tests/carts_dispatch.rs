// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Dispatch by extension and magic, zips holding one ROM, bounded reads, and the N64 RetroAchievements hash.
mod carts_common;

use carts_common::{Counting, Failing, info, load};
use gamemeta_core::carts::Family;
use gamemeta_core::ids::{RaHasher, ra_hash};
use gamemeta_core::{Format, Platform, SliceSource, format_for, read};
use gamemeta_fixtures::carts as fx;
use gamemeta_fixtures::zip::zip;

#[test]
fn extensions_select_a_family_in_any_case() {
    for (name, family) in [
        ("a.gb", Family::GameBoy),
        ("a.GBC", Family::GameBoy),
        ("a.sgb", Family::GameBoy),
        ("a.gba", Family::Gba),
        ("a.agb", Family::Gba),
        ("a.z64", Family::N64),
        ("a.v64", Family::N64),
        ("a.n64", Family::N64),
        ("a.sfc", Family::Snes),
        ("a.smc", Family::Snes),
        ("a.swc", Family::Snes),
        ("a.fig", Family::Snes),
        ("a.nes", Family::Nes),
        ("a.md", Family::MegaDrive),
        ("a.gen", Family::MegaDrive),
        ("a.bin", Family::MegaDrive),
        ("a.smd", Family::MegaDrive),
        ("a.32x", Family::MegaDrive),
        ("a.sms", Family::MasterSystem),
        ("a.GG", Family::MasterSystem),
    ] {
        assert_eq!(format_for(name), Some(Format::Cart(family)), "{name}");
    }
    assert_eq!(format_for("a.nds"), Some(Format::Ds));
    assert_eq!(format_for("a.3ds"), Some(Format::ThreeDs));
    assert_eq!(format_for("a.txt"), None);
}

#[test]
fn a_zip_with_one_rom_reads_the_rom_inside_stored_or_deflated() {
    let gba = fx::gba::Spec::default().build();
    let md = fx::md::Spec::default().build();
    let sms = fx::sms::Spec::default().build();
    let snes = fx::snes::Spec { copier: true, ..fx::snes::Spec::new(fx::snes::Layout::Hi) }.build();
    for deflate in [false, true] {
        let platform = |entries: &[(&str, &[u8], bool)]| info(&zip(entries), "set.zip").map(|i| i.platform);
        assert_eq!(platform(&[("game.gba", &gba, deflate)]), Some(Platform::GameBoyAdvance));
        assert_eq!(platform(&[("game.md", &md, deflate)]), Some(Platform::MegaDrive));
        assert_eq!(platform(&[("game.sms", &sms, deflate)]), Some(Platform::MasterSystem));
        assert_eq!(platform(&[("game.smc", &snes, deflate)]), Some(Platform::Snes));
    }
    let i = load(&zip(&[("game.gba", &gba, true)]), "set.zip");
    assert_eq!(i.title(), Some("SYNTHGAME"));
    assert_eq!(i.ids.game_id.as_deref(), Some("ASYE"));
}

#[test]
fn a_zip_skips_other_entries_and_a_zip_without_a_rom_is_nothing() {
    let gb = fx::gb::Spec::default().build();
    let both = zip(&[("readme.txt", b"hello", false), ("roms/", b"", false), ("game.gb", &gb, true)]);
    assert_eq!(load(&both, "set.zip").platform, Platform::GameBoy);
    assert!(info(&zip(&[("readme.txt", b"hello", false)]), "set.zip").is_none());
    assert!(info(&zip(&[("game.gb", &[0u8; 0x200], false)]), "set.zip").is_none());
    let whole = zip(&[("game.gb", &gb, true)]);
    assert!(info(&whole[..whole.len() / 2], "set.zip").is_none());
}

#[test]
fn a_zipped_snes_exhirom_is_decompressed_far_enough() {
    let ex = fx::snes::Spec::new(fx::snes::Layout::ExHi).build();
    let i = load(&zip(&[("game.sfc", &ex, true)]), "set.zip");
    assert_eq!(i.title(), Some("SYNTH SNES TEST"));
}

#[test]
fn a_nds_zip_is_still_a_ds_game() {
    use gamemeta_fixtures::nds;
    let i = load(&nds::zip("a.nds", &nds::sample_nds(), true), "a.zip");
    assert!(matches!(i.platform, Platform::Ds | Platform::Dsi), "{:?}", i.platform);
}

#[test]
fn readers_ask_for_headers_only() {
    let big = |n: usize, mut v: Vec<u8>| {
        v.resize(n, 0x5A);
        v
    };
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("a.gb", big(4 << 20, fx::gb::Spec::default().build())),
        ("a.gba", big(8 << 20, fx::gba::Spec::default().build())),
        ("a.z64", big(8 << 20, fx::n64::Spec::default().build())),
        ("a.sfc", big(2 << 20, fx::snes::Spec::new(fx::snes::Layout::Hi).build())),
        ("a.nes", big(1 << 20, fx::nes::Spec { prg: 2, ..Default::default() }.build())),
        ("a.md", big(4 << 20, fx::md::Spec::default().build())),
        ("a.smd", big(4 << 20 | 512, fx::md::to_smd(&fx::md::Spec::default().build()))),
        ("a.sms", big(1 << 20, fx::sms::Spec::default().build())),
    ];
    for (name, bytes) in &files {
        let mut source = Counting::new(bytes);
        assert!(read(&mut source, name).is_some(), "{name}");
        assert!(source.asked <= 0x5000, "{name} asked for {} bytes", source.asked);
    }
}

#[test]
fn a_failing_source_reads_as_nothing() {
    let good: [(&str, Vec<u8>); 8] = [
        ("a.gb", fx::gb::Spec::default().build()),
        ("a.gba", fx::gba::Spec::default().build()),
        ("a.z64", fx::n64::Spec::default().build()),
        ("a.sfc", fx::snes::Spec::new(fx::snes::Layout::Lo).build()),
        ("a.nes", fx::nes::Spec { prg: 1, ..Default::default() }.build()),
        ("a.md", fx::md::Spec::default().build()),
        ("a.sms", fx::sms::Spec::default().build()),
        ("a.zip", zip(&[("g.gb", &fx::gb::Spec::default().build(), false)])),
    ];
    for (name, bytes) in &good {
        assert!(info(bytes, name).is_some(), "{name} reads from a working source");
        assert!(read(&mut Failing(bytes.len() as u64), name).is_none(), "{name}");
    }
}

#[test]
fn the_n64_ra_hash_is_of_the_big_endian_image_whatever_the_file_order() {
    let z = fx::n64::Spec::default().build();
    let want = ra_hash(&mut SliceSource::new(&z), "x.gba").expect("whole-file hash of the big-endian bytes");
    let hash = |bytes: &[u8], name: &str| ra_hash(&mut SliceSource::new(bytes), name);
    assert_eq!(hash(&z, "a.z64").as_deref(), Some(want.as_str()));
    assert_eq!(hash(&fx::n64::to_v64(&z), "a.v64").as_deref(), Some(want.as_str()));
    assert_eq!(hash(&fx::n64::to_n64(&z), "a.n64").as_deref(), Some(want.as_str()));
    assert_eq!(
        hash(&fx::n64::to_v64(&z), "a.n64").as_deref(),
        Some(want.as_str()),
        "the magic decides, not the extension"
    );
}

#[test]
fn the_n64_ra_hasher_streams_in_any_chunk_size() {
    let z = fx::n64::Spec::default().build();
    let v = fx::n64::to_v64(&z);
    let want = ra_hash(&mut SliceSource::new(&z), "x.z64");
    for chunk in [1, 3, 5, 4096] {
        let mut hasher = RaHasher::for_file("a.v64", Some(v.len() as u64)).expect("n64 is hashed");
        v.chunks(chunk).for_each(|c| hasher.update(c));
        assert_eq!(hasher.finish(), want, "chunk {chunk}");
    }
}

#[test]
fn an_n64_file_with_no_known_magic_has_no_ra_hash() {
    let mut z = fx::n64::Spec::default().build();
    assert!(ra_hash(&mut SliceSource::new(&z), "a.z64").is_some(), "a known magic is hashed");
    z[0] = 0x00;
    assert_eq!(ra_hash(&mut SliceSource::new(&z), "a.z64"), None);
    assert_eq!(ra_hash(&mut SliceSource::new(&[]), "a.z64"), None);
    assert_eq!(ra_hash(&mut SliceSource::new(&[0x80, 0x37]), "a.v64"), None);
}
