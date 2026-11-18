// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Synthetic game files built in code, so no game data is ever in the repository. The 3DS builders mirror the
//! original reader's test fixtures (same layouts, same defaults) so the two test suites assert on the same bytes.

pub mod banner3ds;
pub mod carts;
pub mod gc;
#[cfg(feature = "gl")]
pub mod headless;
pub mod ids;
pub mod nds;
pub mod reference;
pub mod render;
pub mod showcase;
pub mod stk_banner;
pub mod wii;
pub mod wii_banner;
pub mod wii_banner_cases;
pub mod wiiu;
pub mod zip;

/// A little-endian byte buffer with put helpers.
#[derive(Debug, Clone)]
pub struct Le(pub Vec<u8>);

impl Le {
    /// `size` zero bytes.
    pub fn new(size: usize) -> Self {
        Le(vec![0; size])
    }
    /// Writes `b` at `at`.
    pub fn bytes(&mut self, at: usize, b: &[u8]) -> &mut Self {
        self.0[at..at + b.len()].copy_from_slice(b);
        self
    }
    /// Writes a u16 at `at`.
    pub fn u16(&mut self, at: usize, v: u16) -> &mut Self {
        self.bytes(at, &v.to_le_bytes())
    }
    /// Writes a u32 at `at`.
    pub fn u32(&mut self, at: usize, v: u32) -> &mut Self {
        self.bytes(at, &v.to_le_bytes())
    }
    /// Writes a u64 at `at`.
    pub fn u64(&mut self, at: usize, v: u64) -> &mut Self {
        self.bytes(at, &v.to_le_bytes())
    }
}

/// An ExeFS holding `files` (name, bytes): the 0x200 header, then the files back to back.
pub fn exefs(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut h = Le::new(0x200);
    let mut offset = 0u32;
    for (i, (name, data)) in files.iter().enumerate() {
        h.bytes(i * 16, name.as_bytes()).u32(i * 16 + 8, offset).u32(i * 16 + 12, data.len() as u32);
        offset += data.len() as u32;
    }
    let mut out = h.0;
    for (_, data) in files {
        out.extend_from_slice(data);
    }
    out
}

/// An NCCH whose ExeFS follows the 0x200 header, in media units; `flags7` is NCCH flag byte 7 (4 = NoCrypto).
pub fn ncch(exefs: &[u8], flags7: u8) -> Vec<u8> {
    let units = exefs.len().div_ceil(0x200);
    let mut h = Le::new(0x200);
    h.bytes(0x100, b"NCCH").bytes(0x188 + 7, &[flags7]).u32(0x1A0, 1).u32(0x1A4, units as u32);
    let mut out = h.0;
    out.extend_from_slice(exefs);
    out.resize(0x200 + units * 0x200, 0);
    out
}

/// A plain NCCH (NoCrypto set) around `exefs`.
pub fn plain_ncch(exefs: &[u8]) -> Vec<u8> {
    ncch(exefs, 4)
}

/// Writes the program (title) id into an NCCH header.
pub fn with_program_id(mut ncch: Vec<u8>, id: u64) -> Vec<u8> {
    ncch[0x118..0x120].copy_from_slice(&id.to_le_bytes());
    ncch
}

/// Writes the product code into an NCCH header.
pub fn with_product_code(mut ncch: Vec<u8>, code: &str) -> Vec<u8> {
    ncch[0x150..0x150 + code.len()].copy_from_slice(code.as_bytes());
    ncch
}

/// An NCSD cartridge image with `ncch` as partition 0 at 0x4000.
pub fn ncsd(ncch: &[u8]) -> Vec<u8> {
    let mut h = Le::new(0x4000);
    h.bytes(0x100, b"NCSD").u32(0x120, 0x4000 / 0x200).u32(0x124, (ncch.len() / 0x200) as u32);
    let mut out = h.0;
    out.extend_from_slice(ncch);
    out
}

/// A CIA with empty cert, ticket, TMD and meta sections and `content` at 0x2040.
pub fn cia(content: &[u8]) -> Vec<u8> {
    let mut h = Le::new(0x2040);
    h.u32(0, 0x2020);
    let mut out = h.0;
    out.extend_from_slice(content);
    out
}

/// A CBMD with `common` and, per language slot, a regional CGFX (contents stored as given).
pub fn cbmd(common: &[u8], regional: &[(usize, &[u8])]) -> Vec<u8> {
    let mut h = Le::new(0x88);
    h.bytes(0, b"CBMD").u32(8, 0x88);
    let mut at = 0x88 + common.len();
    let mut body = common.to_vec();
    let mut sorted = regional.to_vec();
    sorted.sort_by_key(|r| r.0);
    for (slot, cgfx) in sorted {
        h.u32(0xC + 4 * slot, at as u32);
        at += cgfx.len();
        body.extend_from_slice(cgfx);
    }
    h.u32(0x84, at as u32);
    let mut out = h.0;
    out.extend_from_slice(&body);
    out
}

/// A CBMD like [`cbmd`] with a jingle (a BCWAV file) after the CGFX files, as 3DS banners have.
pub fn cbmd_with_jingle(common: &[u8], regional: &[(usize, &[u8])], jingle: &[u8]) -> Vec<u8> {
    let mut out = cbmd(common, regional);
    let at = out.len() as u32;
    out[0x84..0x88].copy_from_slice(&at.to_le_bytes());
    out.extend_from_slice(jingle);
    out
}

/// A mono-or-more BCWAV (the 3DS sound file) of 16-bit PCM: `channels` is one slice of samples per channel.
pub fn bcwav_pcm16(rate: u32, channels: &[&[i16]]) -> Vec<u8> {
    let data: Vec<Vec<u8>> = channels.iter().map(|c| c.iter().flat_map(|s| s.to_le_bytes()).collect()).collect();
    bcwav(1, rate, channels[0].len() as u32, &data, None)
}

/// A BCWAV of 8-bit PCM (signed), one slice per channel.
pub fn bcwav_pcm8(rate: u32, channels: &[&[i8]]) -> Vec<u8> {
    let data: Vec<Vec<u8>> = channels.iter().map(|c| c.iter().map(|&s| s as u8).collect()).collect();
    bcwav(0, rate, channels[0].len() as u32, &data, None)
}

/// A BCWAV of DSP-ADPCM: per channel the coefficients and the raw 8-byte frames; `samples` is the sample count.
pub fn bcwav_dsp(rate: u32, samples: u32, channels: &[([i16; 16], &[u8])]) -> Vec<u8> {
    let data: Vec<Vec<u8>> = channels.iter().map(|c| c.1.to_vec()).collect();
    let coefs: Vec<[i16; 16]> = channels.iter().map(|c| c.0).collect();
    bcwav(2, rate, samples, &data, Some(&coefs))
}

/// The CWAV container: header, INFO (wave info, channel references, channel info) and DATA.
fn bcwav(encoding: u8, rate: u32, samples: u32, data: &[Vec<u8>], coefs: Option<&[[i16; 16]]>) -> Vec<u8> {
    let n = data.len();
    // INFO: 8 header + 0x14 wave info (to 0x1C) + count + n refs + n channel infos (0x14 sample ref .. + 0x2E adpcm info).
    let table = 0x1C;
    let infos = table + 4 + 8 * n; // first channel info, relative to INFO
    let info_len = infos + n * 0x40;
    let info_len = info_len.next_multiple_of(0x20);
    let mut info = Le::new(info_len);
    info.bytes(0, b"INFO").u32(4, info_len as u32);
    info.bytes(8, &[encoding, 0, 0, 0]).u32(0xC, rate).u32(0x10, 0).u32(0x14, samples);
    info.u32(table, n as u32);
    let mut sample_at = 0usize;
    for (i, d) in data.iter().enumerate() {
        let ci = infos + 0x40 * i;
        info.u16(table + 4 + 8 * i, 0x7100).u32(table + 8 + 8 * i, (ci - table) as u32);
        info.u16(ci, 0x1F00).u32(ci + 4, sample_at as u32);
        if let Some(c) = coefs {
            info.u16(ci + 8, 0x0300).u32(ci + 12, 0x14);
            for (k, v) in c[i].iter().enumerate() {
                info.u16(ci + 0x14 + 2 * k, *v as u16);
            }
        }
        sample_at += d.len().next_multiple_of(0x20);
    }
    let mut body = Vec::new();
    for d in data {
        body.extend_from_slice(d);
        body.resize(body.len().next_multiple_of(0x20), 0);
    }
    let data_len = (8 + body.len()).next_multiple_of(0x20);
    let mut data_block = Le::new(data_len);
    data_block.bytes(0, b"DATA").u32(4, data_len as u32).bytes(8, &body);
    let head = 0x40;
    let total = head + info_len + data_len;
    let mut h = Le::new(head);
    h.bytes(0, b"CWAV").u16(4, 0xFEFF).u16(6, head as u16).u32(8, 0x0102_0000).u32(0xC, total as u32).u16(0x10, 2);
    h.u16(0x14, 0x7000).u32(0x18, head as u32).u32(0x1C, info_len as u32);
    h.u16(0x20, 0x7001).u32(0x24, (head + info_len) as u32).u32(0x28, data_len as u32);
    let mut out = h.0;
    out.extend_from_slice(&info.0);
    out.extend_from_slice(&data_block.0);
    out
}

/// One SMDH title slot.
#[derive(Debug, Clone, Copy)]
pub struct SmdhSlot<'a> {
    /// Language slot (0 Japanese, 1 English, ...).
    pub slot: usize,
    /// Short title.
    pub short: &'a str,
    /// Long title.
    pub long: &'a str,
    /// Publisher.
    pub publisher: &'a str,
}

/// An SMDH with `slots`, the large icon's storage-order pixel `i` = `large(i)` and the small one's = `small(i)`
/// (RGB565), region lockout `region` and `ratings` (slot, byte).
pub fn smdh_with(
    slots: &[SmdhSlot<'_>],
    large: impl Fn(usize) -> u16,
    small: impl Fn(usize) -> u16,
    region: u32,
    ratings: &[(usize, u8)],
) -> Vec<u8> {
    let mut b = Le::new(0x36C0);
    b.bytes(0, b"SMDH");
    for s in slots {
        let at = 8 + s.slot * 0x200;
        for (field, off) in [(s.short, 0), (s.long, 0x80), (s.publisher, 0x180)] {
            for (i, unit) in field.encode_utf16().enumerate() {
                b.u16(at + off + i * 2, unit);
            }
        }
    }
    for (slot, v) in ratings {
        b.bytes(0x2008 + slot, &[*v]);
    }
    b.u32(0x2018, region);
    for i in 0..24 * 24 {
        b.u16(0x2040 + i * 2, small(i));
    }
    for i in 0..48 * 48 {
        b.u16(0x24C0 + i * 2, large(i));
    }
    b.0
}

/// An SMDH with an English title and a large icon whose every pixel is `rgb565` (the original fixture: region field
/// left 0, no ratings).
pub fn smdh(short: &str, long: &str, publisher: &str, rgb565: u16) -> Vec<u8> {
    smdh_with(&[SmdhSlot { slot: 1, short, long, publisher }], |_| rgb565, |_| 0, 0, &[])
}

/// The original metadata test's 3DS file: an NCSD whose NCCH carries `title_id` and `product_code`, around an
/// ExeFS of `files`.
pub fn three_ds(files: &[(&str, &[u8])], title_id: u64, product_code: &str) -> Vec<u8> {
    ncsd(&with_product_code(with_program_id(plain_ncch(&exefs(files)), title_id), product_code))
}

/// The original metadata test's SMDH: "Short" / "Long Name" / "Pub", red icon, `region` lockout, `ratings`.
pub fn reference_smdh(region: u32, ratings: &[(usize, u8)]) -> Vec<u8> {
    let mut b = smdh("Short", "Long Name", "Pub", 0xF800);
    for (slot, v) in ratings {
        b[0x2008 + slot] = *v;
    }
    b[0x2018..0x201C].copy_from_slice(&region.to_le_bytes());
    b
}

/// A richer sample for binding and cross-reader checks: several languages (an ideographic space, a multi-line long
/// title, a slot past the known languages), patterned icons so tile order matters, a region lockout, ratings, ids
/// and a banner.
pub fn sample_3ds() -> Vec<u8> {
    let slots = [
        SmdhSlot { slot: 0, short: "\u{3000}サンプル", long: "サンプル\u{3000}ゲーム", publisher: "見本" },
        SmdhSlot { slot: 1, short: "Sample", long: "Sample Game\nSubtitle", publisher: "Example Pub" },
        SmdhSlot { slot: 2, short: "Exemple", long: "", publisher: "" },
        SmdhSlot { slot: 13, short: "Unused slot", long: "", publisher: "" },
    ];
    let icon = smdh_with(
        &slots,
        |i| (i as u16).wrapping_mul(37) ^ 0x5A5A,
        |i| (i as u16).wrapping_mul(101),
        0b0000_0110,
        &[(0, 0x80 | 12), (1, 0x80 | 10), (4, 0x80 | 0x20), (7, 0x80 | 0x40)],
    );
    let banner = cbmd(&[0xC6; 32], &[(1, &[0x11; 16])]);
    three_ds(&[(".code", &[0u8; 100]), ("banner", &banner), ("icon", &icon)], 0x0004_0000_0012_3400, "CTR-P-SMPL")
}

/// Every committed sample file (name, bytes): what `write-fixtures` writes into `fixtures/` and the bindings' tests read.
pub fn samples() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("sample.3ds", sample_3ds()),
        ("sample.nds", nds::sample_nds()),
        ("sample-nds.zip", nds::zip("sample.nds", &nds::sample_nds(), true)),
        ("sample-gc.rvz", gc::sample_rvz()),
        ("sample-wii.rvz", wii::sample_rvz()),
        ("sample.wad", wii::sample_wad()),
        ("sample.wua", wiiu::sample_wua()),
        ("ids/sample-ps2.iso", ids::sample_ps2_iso()),
        ("ids/sample-psp.iso", ids::sample_psp_iso()),
        ("ids/sample.bin", ids::raw2352(&ids::sample_ps2_iso(), 2)),
        ("ids/sample.cue", ids::sample_cue().into_bytes()),
        ("ids/sample.pbp", ids::sample_pbp()),
        ("ids/sample.chd", ids::sample_chd()),
        ("ids/sample.nsp", ids::sample_nsp()),
        ("ids/sample-rom.zip", ids::sample_rom_zip()),
        ("ids/sample.sfc", ids::sample_rom(4096)),
        ("ids/sample.nes", ids::sample_nes()),
        ("ids/sample.xbe", ids::sample_xbe()),
        ("ids/sample-gc.iso", ids::sample_gc_iso()),
        ("carts/sample.gb", carts::gb::Spec::default().build()),
        ("carts/sample.gbc", carts::gb::Spec { cgb: 0x80, manufacturer: Some("SYNT"), ..Default::default() }.build()),
        ("carts/sample.gba", carts::gba::Spec::default().build()),
        ("carts/sample.z64", carts::n64::Spec::default().build()),
        ("carts/sample.sfc", carts::snes::Spec::new(carts::snes::Layout::Lo).build()),
        ("carts/sample.nes", carts::nes::Spec { prg: 2, chr: 1, mapper: 4, ..Default::default() }.build()),
        ("carts/sample.md", carts::md::Spec::default().build()),
        ("carts/sample.sms", carts::sms::Spec::default().build()),
        ("carts/sample.gg", carts::sms::Spec { region: 6, ..Default::default() }.build()),
        ("carts/sample-gb.zip", zip::zip(&[("sample.gb", &carts::gb::Spec::default().build(), true)])),
    ]
}
