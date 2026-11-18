// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Synthetic cartridge ROMs: a header built from a `Spec` with every checksum computed (so a test breaks one on
//! purpose), a patterned body so checksums are not trivial. No game data: titles and ids are made up.

fn pattern(i: usize) -> u8 {
    ((i * 31 + 7) % 251) as u8
}

fn padded(text: &str, len: usize, fill: u8) -> Vec<u8> {
    let mut v = text.as_bytes().to_vec();
    v.resize(len, fill);
    v.truncate(len);
    v
}

/// Game Boy / Game Boy Color.
pub mod gb {
    use super::{padded, pattern};

    /// The Nintendo logo bitmap at 0x104 (an independent copy of the documented bytes).
    pub const LOGO: [u8; 48] = [
        0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D, 0x00, 0x08,
        0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99, 0xBB, 0xBB, 0x67, 0x63,
        0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
    ];

    /// What a header holds.
    #[derive(Debug, Clone)]
    pub struct Spec {
        /// The title (16 characters, 15 with a Color flag, 11 with a manufacturer code).
        pub title: &'static str,
        /// A manufacturer code at 0x13F.
        pub manufacturer: Option<&'static str>,
        /// 0x143: 0x80, 0xC0 or 0.
        pub cgb: u8,
        /// Super Game Boy support (0x146 = 3).
        pub sgb: bool,
        /// 0x14B.
        pub old_licensee: u8,
        /// 0x144, two characters (used when `old_licensee` is 0x33).
        pub new_licensee: &'static str,
        /// 0x147.
        pub cart_type: u8,
        /// 0x148.
        pub rom_size: u8,
        /// 0x149.
        pub ram_size: u8,
        /// 0x14A.
        pub destination: u8,
        /// 0x14C.
        pub version: u8,
        /// The file's length.
        pub len: usize,
    }

    impl Default for Spec {
        fn default() -> Self {
            Spec {
                title: "SYNTH TEST",
                manufacturer: None,
                cgb: 0,
                sgb: false,
                old_licensee: 0x01,
                new_licensee: "",
                cart_type: 0x13,
                rom_size: 0,
                ram_size: 3,
                destination: 1,
                version: 2,
                len: 0x8000,
            }
        }
    }

    impl Spec {
        /// The ROM, with the logo, the header checksum and the global checksum right.
        pub fn build(&self) -> Vec<u8> {
            let mut r: Vec<u8> = (0..self.len.max(0x150)).map(pattern).collect();
            r[0x100..0x150].fill(0);
            r[0x104..0x134].copy_from_slice(&LOGO);
            match self.manufacturer {
                Some(code) => {
                    r[0x134..0x13F].copy_from_slice(&padded(self.title, 11, 0));
                    r[0x13F..0x143].copy_from_slice(&padded(code, 4, 0));
                }
                None => {
                    let width = if self.cgb != 0 { 15 } else { 16 };
                    r[0x134..0x134 + width].copy_from_slice(&padded(self.title, width, 0));
                }
            }
            r[0x143] = self.cgb;
            r[0x144..0x146].copy_from_slice(&padded(self.new_licensee, 2, 0));
            r[0x146] = if self.sgb { 3 } else { 0 };
            r[0x147] = self.cart_type;
            r[0x148] = self.rom_size;
            r[0x149] = self.ram_size;
            r[0x14A] = self.destination;
            r[0x14B] = self.old_licensee;
            r[0x14C] = self.version;
            r[0x14D] = r[0x134..=0x14C].iter().fold(0u8, |x, &b| x.wrapping_sub(b).wrapping_sub(1));
            let sum = r
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != 0x14E && *i != 0x14F)
                .fold(0u16, |s, (_, &b)| s.wrapping_add(u16::from(b)));
            r[0x14E..0x150].copy_from_slice(&sum.to_be_bytes());
            r
        }
    }
}

/// Game Boy Advance.
pub mod gba {
    use super::{padded, pattern};

    /// What a header holds.
    #[derive(Debug, Clone)]
    pub struct Spec {
        /// Up to 12 characters.
        pub title: &'static str,
        /// The 4-character game code.
        pub code: &'static str,
        /// The 2-character maker code.
        pub maker: &'static str,
        /// 0xBC.
        pub version: u8,
        /// The file's length.
        pub len: usize,
    }

    impl Default for Spec {
        fn default() -> Self {
            Spec { title: "SYNTHGAME", code: "ASYE", maker: "01", version: 1, len: 0x400 }
        }
    }

    impl Spec {
        /// The ROM with the fixed byte and the complement right.
        pub fn build(&self) -> Vec<u8> {
            let mut r: Vec<u8> = (0..self.len.max(0xC0)).map(pattern).collect();
            r[..0xC0].fill(0);
            r[..4].copy_from_slice(&[0x2E, 0, 0, 0xEA]);
            r[0xA0..0xAC].copy_from_slice(&padded(self.title, 12, 0));
            r[0xAC..0xB0].copy_from_slice(&padded(self.code, 4, 0));
            r[0xB0..0xB2].copy_from_slice(&padded(self.maker, 2, 0));
            r[0xB2] = 0x96;
            r[0xBC] = self.version;
            let sum = r[0xA0..=0xBC].iter().fold(0u8, |x, &b| x.wrapping_add(b));
            r[0xBD] = 0u8.wrapping_sub(0x19).wrapping_sub(sum);
            r
        }
    }
}

/// Nintendo 64.
pub mod n64 {
    use super::{padded, pattern};

    /// What a header holds.
    #[derive(Debug, Clone)]
    pub struct Spec {
        /// Up to 20 characters.
        pub title: &'static str,
        /// 0x3B.
        pub category: u8,
        /// The 2-character game id at 0x3C.
        pub id: &'static str,
        /// 0x3E.
        pub country: u8,
        /// 0x3F.
        pub version: u8,
        /// CRC1.
        pub crc1: u32,
        /// CRC2.
        pub crc2: u32,
        /// The file's length.
        pub len: usize,
    }

    impl Default for Spec {
        fn default() -> Self {
            Spec {
                title: "SYNTH 64 TEST",
                category: b'N',
                id: "ST",
                country: b'E',
                version: 1,
                crc1: 0x1122_3344,
                crc2: 0x5566_7788,
                len: 0x2000,
            }
        }
    }

    impl Spec {
        /// The image in the native big-endian order (`.z64`).
        pub fn build(&self) -> Vec<u8> {
            let mut r: Vec<u8> = (0..self.len.max(0x40)).map(pattern).collect();
            r[..0x40].fill(0);
            r[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
            r[4..8].copy_from_slice(&0x0000_000Fu32.to_be_bytes());
            r[8..12].copy_from_slice(&0x8000_1000u32.to_be_bytes());
            r[0x10..0x14].copy_from_slice(&self.crc1.to_be_bytes());
            r[0x14..0x18].copy_from_slice(&self.crc2.to_be_bytes());
            r[0x20..0x34].copy_from_slice(&padded(self.title, 20, b' '));
            r[0x3B] = self.category;
            r[0x3C..0x3E].copy_from_slice(&padded(self.id, 2, 0));
            r[0x3E] = self.country;
            r[0x3F] = self.version;
            r
        }
    }

    /// A big-endian image as `.v64` stores it: every pair of bytes swapped.
    pub fn to_v64(z64: &[u8]) -> Vec<u8> {
        let mut v = z64.to_vec();
        v.chunks_exact_mut(2).for_each(|p| p.swap(0, 1));
        v
    }

    /// A big-endian image as `.n64` stores it: every word reversed.
    pub fn to_n64(z64: &[u8]) -> Vec<u8> {
        let mut v = z64.to_vec();
        v.chunks_exact_mut(4).for_each(|w| w.reverse());
        v
    }
}

/// Super Nintendo.
pub mod snes {
    use super::{padded, pattern};

    /// Where the header sits.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Layout {
        /// 0x7FC0.
        Lo,
        /// 0xFFC0.
        Hi,
        /// 0x40FFC0.
        ExHi,
    }

    /// What a header holds.
    #[derive(Debug, Clone)]
    pub struct Spec {
        /// Where the header goes.
        pub layout: Layout,
        /// Title bytes (JIS X 0201), padded to 21 with spaces.
        pub title: Vec<u8>,
        /// +0x15.
        pub map_mode: u8,
        /// +0x16.
        pub rom_type: u8,
        /// +0x17.
        pub rom_size: u8,
        /// +0x18.
        pub ram_size: u8,
        /// +0x19.
        pub destination: u8,
        /// +0x1A (0x33: the extended header below is written).
        pub developer: u8,
        /// The extended header's maker code.
        pub maker: &'static str,
        /// The extended header's game code.
        pub game_code: &'static str,
        /// +0x1B.
        pub version: u8,
        /// +0x1E.
        pub checksum: u16,
        /// Write the complement as the checksum's inverse (else equal to it).
        pub complement_ok: bool,
        /// A 512-byte copier header in front.
        pub copier: bool,
    }

    impl Spec {
        /// Plausible defaults for `layout`.
        pub fn new(layout: Layout) -> Self {
            Spec {
                layout,
                title: b"SYNTH SNES TEST".to_vec(),
                map_mode: match layout {
                    Layout::Lo => 0x20,
                    Layout::Hi => 0x21,
                    Layout::ExHi => 0x25,
                },
                rom_type: 0x02,
                rom_size: 0x09,
                ram_size: 0x03,
                destination: 1,
                developer: 0x01,
                maker: "01",
                game_code: "SYNT",
                version: 0,
                checksum: 0xABCD,
                complement_ok: true,
                copier: false,
            }
        }

        /// The file's header offset, without any copier header.
        pub fn base(&self) -> usize {
            match self.layout {
                Layout::Lo => 0x7FC0,
                Layout::Hi => 0xFFC0,
                Layout::ExHi => 0x40FFC0,
            }
        }

        /// The ROM image.
        pub fn build(&self) -> Vec<u8> {
            let mut r: Vec<u8> = (0..self.base() + 0x40).map(pattern).collect();
            let b = self.base();
            r[b - 0x10..b + 0x40].fill(0);
            r[b..b + 21].copy_from_slice(&padded_bytes(&self.title, 21, b' '));
            r[b + 0x15] = self.map_mode;
            r[b + 0x16] = self.rom_type;
            r[b + 0x17] = self.rom_size;
            r[b + 0x18] = self.ram_size;
            r[b + 0x19] = self.destination;
            r[b + 0x1A] = self.developer;
            r[b + 0x1B] = self.version;
            let complement = if self.complement_ok { !self.checksum } else { self.checksum };
            r[b + 0x1C..b + 0x1E].copy_from_slice(&complement.to_le_bytes());
            r[b + 0x1E..b + 0x20].copy_from_slice(&self.checksum.to_le_bytes());
            r[b + 0x3C..b + 0x3E].copy_from_slice(&0x8000u16.to_le_bytes());
            if self.developer == 0x33 {
                r[b - 0x10..b - 0x0E].copy_from_slice(&padded(self.maker, 2, b' '));
                r[b - 0x0E..b - 0x0A].copy_from_slice(&padded(self.game_code, 4, b' '));
            }
            if self.copier {
                let mut with = vec![0u8; 512];
                with.extend(r);
                return with;
            }
            r
        }
    }

    fn padded_bytes(b: &[u8], len: usize, fill: u8) -> Vec<u8> {
        let mut v = b.to_vec();
        v.resize(len, fill);
        v
    }
}

/// NES.
pub mod nes {
    /// What a header holds.
    #[derive(Debug, Clone, Default)]
    pub struct Spec {
        /// NES 2.0 instead of iNES.
        pub nes2: bool,
        /// PRG units (16 KiB; NES 2.0: 12 bits).
        pub prg: u16,
        /// CHR units (8 KiB; NES 2.0: 12 bits).
        pub chr: u16,
        /// The mapper (8 bits, NES 2.0: 12).
        pub mapper: u16,
        /// NES 2.0 submapper.
        pub submapper: u8,
        /// Byte 6 bit 0.
        pub vertical: bool,
        /// Byte 6 bit 1.
        pub battery: bool,
        /// Byte 6 bit 2 (a 512-byte trainer is appended).
        pub trainer: bool,
        /// Byte 6 bit 3.
        pub four_screen: bool,
        /// Byte 7 bits 0-1.
        pub console: u8,
        /// NES 2.0 byte 12.
        pub timing: u8,
        /// iNES byte 9 bit 0.
        pub pal: bool,
        /// iNES byte 8.
        pub prg_ram_units: u8,
        /// NES 2.0 shift counts: PRG RAM, PRG NVRAM, CHR RAM, CHR NVRAM.
        pub ram_shifts: [u8; 4],
        /// NES 2.0 byte 13 low nibble (extended console type).
        pub extended_console: u8,
    }

    impl Spec {
        /// The header (and the trainer, if any). No ROM data follows: the reader reads 16 bytes.
        pub fn build(&self) -> Vec<u8> {
            let mut h = vec![0u8; 16];
            h[..4].copy_from_slice(b"NES\x1A");
            h[4] = self.prg as u8;
            h[5] = self.chr as u8;
            h[6] = ((self.mapper & 0xF) as u8) << 4
                | u8::from(self.vertical)
                | u8::from(self.battery) << 1
                | u8::from(self.trainer) << 2
                | u8::from(self.four_screen) << 3;
            h[7] = (self.mapper & 0xF0) as u8 | if self.nes2 { 0x08 } else { 0 } | (self.console & 3);
            if self.nes2 {
                h[8] = ((self.mapper >> 8) & 0xF) as u8 | self.submapper << 4;
                h[9] = ((self.prg >> 8) & 0xF) as u8 | (((self.chr >> 8) & 0xF) as u8) << 4;
                h[10] = self.ram_shifts[0] | self.ram_shifts[1] << 4;
                h[11] = self.ram_shifts[2] | self.ram_shifts[3] << 4;
                h[12] = self.timing;
                h[13] = self.extended_console;
            } else {
                h[8] = self.prg_ram_units;
                h[9] = u8::from(self.pal);
            }
            if self.trainer {
                h.extend([0xEE; 512]);
            }
            h
        }
    }
}

/// Genesis / Mega Drive, 32X.
pub mod md {
    use super::padded;

    /// What a header holds.
    #[derive(Debug, Clone)]
    pub struct Spec {
        /// The 16-character system string.
        pub system: &'static str,
        /// The 16-character copyright field.
        pub copyright: &'static str,
        /// The domestic title (encoded as Shift_JIS).
        pub domestic: &'static str,
        /// The overseas title.
        pub overseas: &'static str,
        /// The 14-character serial.
        pub serial: &'static str,
        /// The I/O support field.
        pub io: &'static str,
        /// The region field.
        pub region: &'static str,
        /// The file's length (a multiple of 16 KiB so it can be interleaved).
        pub len: usize,
    }

    impl Default for Spec {
        fn default() -> Self {
            Spec {
                system: "SEGA MEGA DRIVE",
                copyright: "(C)SEGA 1991.APR",
                domestic: "シンセゲーム",
                overseas: "SYNTH DRIVE GAME",
                serial: "GM 00001009-02",
                io: "J",
                region: "JUE",
                len: 0x8000,
            }
        }
    }

    impl Spec {
        /// A plain image with the checksum right.
        pub fn build(&self) -> Vec<u8> {
            let mut r: Vec<u8> = (0..self.len.max(0x200)).map(|i| ((i * 13 + 5) % 253) as u8).collect();
            r[0x100..0x200].fill(b' ');
            r[0x100..0x110].copy_from_slice(&padded(self.system, 16, b' '));
            r[0x110..0x120].copy_from_slice(&padded(self.copyright, 16, b' '));
            let (jp, _, _) = encoding_rs::SHIFT_JIS.encode(self.domestic);
            r[0x120..0x150].copy_from_slice(&{
                let mut v = jp.to_vec();
                v.resize(48, b' ');
                v
            });
            r[0x150..0x180].copy_from_slice(&padded(self.overseas, 48, b' '));
            r[0x180..0x18E].copy_from_slice(&padded(self.serial, 14, b' '));
            r[0x190..0x1A0].copy_from_slice(&padded(self.io, 16, b' '));
            r[0x1A0..0x1A4].copy_from_slice(&0u32.to_be_bytes());
            let last = (r.len() - 1) as u32;
            r[0x1A4..0x1A8].copy_from_slice(&last.to_be_bytes());
            r[0x1A8..0x1AC].copy_from_slice(&0x00FF_0000u32.to_be_bytes());
            r[0x1AC..0x1B0].copy_from_slice(&0x00FF_FFFFu32.to_be_bytes());
            r[0x1F0..0x200].copy_from_slice(&padded(self.region, 16, b' '));
            let sum = r[0x200..]
                .chunks(2)
                .fold(0u16, |s, w| s.wrapping_add(u16::from_be_bytes([w[0], *w.get(1).unwrap_or(&0)])));
            r[0x18E..0x190].copy_from_slice(&sum.to_be_bytes());
            r
        }
    }

    /// A plain image as a Super Magic Drive file: 512-byte header, 16 KiB blocks with the odd bytes first.
    pub fn to_smd(plain: &[u8]) -> Vec<u8> {
        assert_eq!(plain.len() % 0x4000, 0, "an SMD image is whole 16 KiB blocks");
        let mut out = vec![0u8; 512];
        out[0] = (plain.len() / 0x4000) as u8;
        out[1] = 3;
        out[8] = 0xAA;
        out[9] = 0xBB;
        out[10] = 6;
        for block in plain.chunks(0x4000) {
            out.extend(block.iter().skip(1).step_by(2));
            out.extend(block.iter().step_by(2));
        }
        out
    }
}

/// Master System / Game Gear.
pub mod sms {
    use super::pattern;

    /// What a header holds.
    #[derive(Debug, Clone)]
    pub struct Spec {
        /// Where `TMR SEGA` goes: 0x7FF0, 0x3FF0 or 0x1FF0.
        pub at: usize,
        /// The product number (5 BCD digits).
        pub product: u32,
        /// The version nibble.
        pub version: u8,
        /// The region nibble (3 to 7).
        pub region: u8,
        /// The size nibble.
        pub size: u8,
    }

    impl Default for Spec {
        fn default() -> Self {
            Spec { at: 0x7FF0, product: 26_001, version: 1, region: 4, size: 0xC }
        }
    }

    impl Spec {
        /// The file's length: the size the size nibble names.
        pub fn rom_len(&self) -> usize {
            match self.size {
                0xA => 0x2000,
                0xB => 0x4000,
                0xC => 0x8000,
                0xD => 0xC000,
                0xE => 0x10000,
                0xF => 0x20000,
                _ => 0x40000,
            }
        }

        /// The ROM with the checksum right (every byte but the header's 16, up to the size).
        pub fn build(&self) -> Vec<u8> {
            let mut r: Vec<u8> = (0..self.rom_len()).map(pattern).collect();
            let at = self.at;
            r[at..at + 16].fill(0);
            r[at..at + 8].copy_from_slice(b"TMR SEGA");
            r[at + 8..at + 10].copy_from_slice(&[0x20, 0x20]);
            let bcd = |n: u32| (((n / 10) % 10) << 4 | (n % 10)) as u8;
            r[at + 0xC] = bcd(self.product % 100);
            r[at + 0xD] = bcd(self.product / 100 % 100);
            r[at + 0xE] = ((self.product / 10_000) as u8) << 4 | (self.version & 0xF);
            r[at + 0xF] = self.region << 4 | (self.size & 0xF);
            let sum = r
                .iter()
                .enumerate()
                .filter(|(i, _)| *i < at || *i >= at + 16)
                .fold(0u16, |s, (_, &b)| s.wrapping_add(u16::from(b)));
            r[at + 0xA..at + 0xC].copy_from_slice(&sum.to_le_bytes());
            r
        }
    }
}
