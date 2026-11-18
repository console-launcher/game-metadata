// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The demos for the PlayStation, Switch, Xbox and container formats (CHD).
use super::art;
use super::hash::{hex, sha1};
use super::{File, NAME, put};
use crate::ids;

/// A PSP `PARAM.SFO`.
fn sfo(category: &str) -> Vec<u8> {
    ids::sfo(&[("CATEGORY", category), ("DISC_ID", "ULUS99901"), ("TITLE", NAME)])
}

/// `ICON0.PNG` (144x80) and `PIC1.PNG` (480x272): the kart on the sky.
fn psp_art() -> (Vec<u8>, Vec<u8>) {
    let icon = art::paste(&art::gradient(144, 80, art::SKY, art::SKY_DEEP), &art::icon(0, 72), 36, 4);
    let pic = art::paste(&art::gradient(480, 272, art::SKY, art::SKY_DEEP), &art::icon(1, 224), 128, 24);
    (art::png_bytes(&icon), art::png_bytes(&pic))
}

/// An `EBOOT.PBP`: the 40-byte header, `PARAM.SFO`, `ICON0.PNG`, `PIC1.PNG` (no icon video, no background, no sound, no
/// program: the offsets after the PIC1 all point at the end).
fn pbp(sfo: &[u8], icon0: &[u8], pic1: &[u8]) -> Vec<u8> {
    let sfo_at = 40u32;
    let icon0_at = sfo_at + sfo.len() as u32;
    let pic1_at = icon0_at + icon0.len() as u32;
    let end = pic1_at + pic1.len() as u32;
    let mut out = vec![0, b'P', b'B', b'P'];
    for v in [0x10000, sfo_at, icon0_at, pic1_at, pic1_at, pic1_at, end, end, end, end] {
        out.extend(u32::to_le_bytes(v));
    }
    out.truncate(40);
    out.extend(sfo);
    out.extend(icon0);
    out.extend(pic1);
    out
}

fn playstation(out: &mut Vec<File>) {
    let ps1 = ids::iso9660(&[("SYSTEM.CNF;1", &ids::system_cnf("BOOT = cdrom:\\SLUS_999.01;1"))]);
    put(out, "tux-ps1.bin", ids::raw2352(&ps1, 2));
    put(
        out,
        "tux-ps1.cue",
        b"FILE \"tux-ps1.bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n".to_vec(),
    );
    let ps2 = ids::iso9660(&[("SYSTEM.CNF;1", &ids::system_cnf("BOOT2 = cdrom0:\\SLUS_999.02;1\r\nVER = 1.00"))]);
    // A CHD v5 header whose raw SHA-1 is the PS2 disc's: the data inside is the disc, the demo carries only the header.
    put(out, "tux.chd", ids::chd(&hex(&sha1(&ps2)), 5));
    put(out, "tux-ps2.iso", ps2);
}

fn psp(out: &mut Vec<File>) {
    let (icon0, pic1) = psp_art();
    put(out, "tux.pbp", pbp(&sfo("MG"), &icon0, &pic1));
    let iso = ids::iso9660(&[
        ("PSP_GAME/PARAM.SFO;1", &sfo("UG")),
        ("PSP_GAME/ICON0.PNG;1", &icon0),
        ("PSP_GAME/PIC1.PNG;1", &pic1),
    ]);
    put(out, "tux-psp.iso", iso);
}

/// A Switch `.nsp` (PFS0) holding empty files whose names carry the rights id: the title id is its first 16 digits.
fn switch(out: &mut Vec<File>) {
    put(out, "tux.nsp", ids::pfs0(&["0100f3ff0000a000000000000000000a.tik", "tux.nca"]));
}

/// An Xbox `default.xbe`: header, certificate with the title id, region and the title name (UTF-16).
fn xbox(out: &mut Vec<File>) {
    let mut xbe = ids::xbe(0xFFFE_0001, 1, 0x10000, 0x178);
    for (i, u) in NAME.encode_utf16().enumerate() {
        xbe[0x178 + 0xC + i * 2..0x178 + 0xC + i * 2 + 2].copy_from_slice(&u.to_le_bytes());
    }
    put(out, "tux.xbe", xbe);
}

pub(crate) fn add(out: &mut Vec<File>) {
    psp(out);
    playstation(out);
    switch(out);
    xbox(out);
}
