// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The cartridge demos: valid headers titled SUPERTUXKART (where the field allows), no art, the smallest size each header
//! allows, checksums right where a reader checks them. The signature bytes a header must carry to be recognised (the Game Boy
//! logo bytes, `TMR SEGA`, `SEGA MEGA DRIVE`) are format magic, not art.
use super::File;
use super::put;
use crate::carts as c;

const TITLE: &str = "SUPERTUXKART";

/// SNES: the LoROM image with its real checksum (the byte sum with the checksum pair counted as FF FF 00 00) and complement.
fn snes() -> Vec<u8> {
    let spec = |checksum, complement_ok| c::snes::Spec {
        title: TITLE.as_bytes().to_vec(),
        developer: 0x33,
        maker: "00",
        game_code: "TUXE",
        rom_size: 5,
        ram_size: 0,
        rom_type: 0,
        checksum,
        complement_ok,
        ..c::snes::Spec::new(c::snes::Layout::Lo)
    };
    let blank = spec(0, false).build();
    let sum = blank.iter().fold(0u16, |s, &b| s.wrapping_add(u16::from(b))).wrapping_add(0x1FE);
    spec(sum, true).build()
}

pub(crate) fn add(out: &mut Vec<File>) {
    let gb = |cgb| c::gb::Spec {
        title: TITLE,
        cgb,
        old_licensee: 0,
        cart_type: 0,
        ram_size: 0,
        destination: 1,
        version: 0,
        ..Default::default()
    };
    put(out, "tux.gb", gb(0).build());
    put(out, "tux.gbc", gb(0xC0).build());
    put(out, "tux.gba", c::gba::Spec { title: TITLE, code: "TUXE", maker: "00", version: 0, len: 0xC0 }.build());
    let z64 = c::n64::Spec { title: TITLE, id: "TX", crc1: 0, crc2: 0, version: 0, len: 0x1000, ..Default::default() }
        .build();
    put(out, "tux.v64", c::n64::to_v64(&z64));
    put(out, "tux.n64", c::n64::to_n64(&z64));
    put(out, "tux.z64", z64);
    put(out, "tux.sfc", snes());
    let mut nes = c::nes::Spec { prg: 1, ..Default::default() }.build();
    nes.extend(vec![0u8; 16 * 1024]);
    put(out, "tux.nes", nes);
    put(
        out,
        "tux.md",
        c::md::Spec {
            system: "SEGA MEGA DRIVE",
            copyright: "(C)TUX  2026.NOV",
            domestic: TITLE,
            overseas: TITLE,
            serial: "GM 99999901-00",
            io: "J",
            region: "JUE",
            len: 0x400,
        }
        .build(),
    );
    let sms = |region| c::sms::Spec { at: 0x3FF0, product: 99_901, version: 0, region, size: 0xB };
    put(out, "tux.sms", sms(4).build());
    put(out, "tux.gg", sms(6).build());
}
