// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Disc and container images for the id readers, built from made-up ids. They mirror the original suite's
//! `SyntheticImages` (and the `PspParamSfoTest` / `DiscIdTest` builders) so both suites assert on the same bytes. No real
//! game bytes, ids or hashes.

use crate::Le;
use crate::zip::crc32;

fn ascii(s: &str) -> Vec<u8> {
    s.bytes().collect()
}

const GC_MAGIC: [u8; 4] = [0xC2, 0x33, 0x9F, 0x3D];
const WII_MAGIC: [u8; 4] = [0x5D, 0x1C, 0x9E, 0xA3];

/// A GameCube disc header (0x440 bytes) with `id` at 0 and the GameCube magic at 0x1C. `id` is written as raw bytes
/// (Latin-1 for `char`s up to U+00FF), so a test can plant a NUL or a lower-case letter.
pub fn game_cube(id: &str) -> Vec<u8> {
    let mut b = vec![0u8; 0x440];
    let id: Vec<u8> = id.chars().map(|c| c as u8).collect();
    b[..id.len()].copy_from_slice(&id);
    b[0x1C..0x20].copy_from_slice(&GC_MAGIC);
    b
}

/// A Wii disc header (0x440 bytes) with `id` at 0 and the Wii magic at 0x18.
pub fn wii(id: &str) -> Vec<u8> {
    let mut b = vec![0u8; 0x440];
    let id = ascii(id);
    b[..id.len()].copy_from_slice(&id);
    b[0x18..0x1C].copy_from_slice(&WII_MAGIC);
    b
}

/// A WBFS file: the `WBFS` magic, then the disc header copy at 0x200.
pub fn wbfs(disc_header: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 0x400];
    b[..4].copy_from_slice(b"WBFS");
    let n = disc_header.len().min(0x100);
    b[0x200..0x200 + n].copy_from_slice(&disc_header[..n]);
    b
}

/// An RVZ/WIA file: the magic, 0x48 bytes of file header, then the disc struct whose `dhead` starts at 0x10.
pub fn rvz(disc_header: &[u8], magic: &[u8; 4]) -> Vec<u8> {
    let mut b = vec![0u8; 0x200];
    b[..4].copy_from_slice(magic);
    let n = disc_header.len().min(0x80);
    b[0x58..0x58 + n].copy_from_slice(&disc_header[..n]);
    b
}

/// A CISO file: the magic, a block map whose first block is present, the disc at 0x8000.
pub fn ciso(disc_header: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 0x8000 + disc_header.len()];
    b[..4].copy_from_slice(b"CISO");
    b[6] = 0x20; // block size 0x200000, little endian
    b[8] = 1;
    b[0x8000..].copy_from_slice(disc_header);
    b
}

fn record(id: &str, extent: u32, size: u32, directory: bool) -> Vec<u8> {
    let name = if id == "." { vec![0] } else { ascii(id) };
    let len = 33 + name.len() + usize::from(name.len().is_multiple_of(2));
    let mut b = Le::new(len);
    b.0[0] = len as u8;
    b.u32(2, extent).u32(10, size);
    b.0[25] = if directory { 2 } else { 0 };
    b.0[32] = name.len() as u8;
    b.bytes(33, &name);
    b.0
}

/// An ISO 9660 image (2048-byte sectors) holding `files` by path (`SYSTEM.CNF;1`, `PSP_GAME/PARAM.SFO;1`; one folder
/// level): 16 system sectors, the volume descriptor, its terminator, the root directory, one sector per folder, then the data.
pub fn iso9660(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut folders: Vec<&str> = Vec::new();
    for (path, _) in files {
        if let Some((folder, _)) = path.split_once('/') {
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
    }
    let mut next = 19 + folders.len();
    let mut at = Vec::new();
    for (_, data) in files {
        at.push(next);
        next += data.len().div_ceil(2048);
    }
    let mut image = vec![0u8; next * 2048];
    let pvd = 16 * 2048;
    image[pvd] = 1;
    image[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
    image[pvd + 6] = 1;
    let root = record(".", 18, 2048, true);
    image[pvd + 156..pvd + 156 + root.len()].copy_from_slice(&root);
    image[17 * 2048] = 255;
    image[17 * 2048 + 1..17 * 2048 + 6].copy_from_slice(b"CD001");
    let mut write_dir = |sector: usize, entries: Vec<Vec<u8>>| {
        let mut p = sector * 2048;
        for e in entries {
            image[p..p + e.len()].copy_from_slice(&e);
            p += e.len();
        }
    };
    let mut root_entries = Vec::new();
    for (i, (path, data)) in files.iter().enumerate() {
        if !path.contains('/') {
            root_entries.push(record(path, at[i] as u32, data.len() as u32, false));
        }
    }
    for (i, folder) in folders.iter().enumerate() {
        root_entries.push(record(folder, 19 + i as u32, 2048, true));
    }
    write_dir(18, root_entries);
    for (i, folder) in folders.iter().enumerate() {
        let entries = files
            .iter()
            .enumerate()
            .filter_map(|(j, (path, data))| {
                let (f, name) = path.split_once('/')?;
                (f == *folder).then(|| record(name, at[j] as u32, data.len() as u32, false))
            })
            .collect();
        write_dir(19 + i, entries);
    }
    for (i, (_, data)) in files.iter().enumerate() {
        image[at[i] * 2048..at[i] * 2048 + data.len()].copy_from_slice(data);
    }
    image
}

/// `iso` (2048-byte sectors) as raw 2352-byte sectors, the way a `.bin` stores a data track: `mode` 1 (16 bytes before the
/// data) or 2 (24).
pub fn raw2352(iso: &[u8], mode: u8) -> Vec<u8> {
    let head = if mode == 1 { 16 } else { 24 };
    let mut out = Vec::new();
    for sector in iso.chunks_exact(2048) {
        let mut s = vec![0u8; 2352];
        s[1..=10].fill(0xFF);
        s[15] = mode;
        s[head..head + 2048].copy_from_slice(sector);
        out.extend(s);
    }
    out
}

/// A `SYSTEM.CNF` with the given boot line.
pub fn system_cnf(boot: &str) -> Vec<u8> {
    ascii(&format!("{boot}\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFF00\r\n"))
}

/// A PARAM.SFO with `entries` (UTF-8 strings), in the order given.
pub fn sfo(entries: &[(&str, &str)]) -> Vec<u8> {
    let (mut keys, mut data, mut index) = (Vec::new(), Vec::new(), Vec::new());
    for (key, value) in entries {
        let mut v = value.as_bytes().to_vec();
        v.push(0);
        index.extend((keys.len() as u16).to_le_bytes());
        index.extend(0x0204u16.to_le_bytes());
        index.extend((v.len() as u32).to_le_bytes());
        index.extend((v.len() as u32).to_le_bytes());
        index.extend((data.len() as u32).to_le_bytes());
        keys.extend(key.as_bytes());
        keys.push(0);
        data.extend(v);
    }
    let key_start = 20 + index.len();
    let data_start = key_start + keys.len();
    let mut out = vec![0, b'P', b'S', b'F'];
    for v in [0x101, key_start as u32, data_start as u32, entries.len() as u32] {
        out.extend(v.to_le_bytes());
    }
    out.extend(index);
    out.extend(keys);
    out.extend(data);
    out
}

/// An `EBOOT.PBP` header (40 bytes), the SFO, then `icon_size` bytes standing for the icon.
pub fn pbp(sfo: &[u8], icon_size: usize) -> Vec<u8> {
    let sfo_offset = 40u32;
    let icon_offset = sfo_offset + sfo.len() as u32;
    let mut h = Le::new(40);
    h.bytes(0, &[0, b'P', b'B', b'P']).u32(4, 0x10000).u32(8, sfo_offset).u32(12, icon_offset);
    for i in 0..6 {
        h.u32(16 + 4 * i, icon_offset + icon_size as u32);
    }
    let mut out = h.0;
    out.extend(sfo);
    out.extend(vec![0u8; icon_size]);
    out
}

/// The PSP reference suite's small ISO: volume descriptor at sector 16, root at 17, `PSP_GAME` at 18, the SFO (named
/// `sfo_name`) at 19.
pub fn psp_iso(sfo: &[u8], sfo_name: &str) -> Vec<u8> {
    let mut image = vec![0u8; 2048 * 20 + sfo.len()];
    let mut pvd = Le::new(2048);
    pvd.bytes(0, &[1]).bytes(1, b"CD001").u16(128, 2048);
    pvd.bytes(156, &record(".", 17, 2048, true)[..34]);
    image[16 * 2048..17 * 2048].copy_from_slice(&pvd.0);
    let mut put =
        |sector: usize, bytes: &[u8]| image[sector * 2048..sector * 2048 + bytes.len()].copy_from_slice(bytes);
    put(17, &record("PSP_GAME", 18, 2048, true));
    put(18, &record(sfo_name, 19, sfo.len() as u32, false));
    put(19, sfo);
    image
}

/// A CHD v5 header (124 bytes) with `raw_sha1` (hex) at 0x40.
pub fn chd(raw_sha1: &str, version: u32) -> Vec<u8> {
    let mut b = vec![0u8; 124];
    b[..8].copy_from_slice(b"MComprHD");
    b[8..12].copy_from_slice(&124u32.to_be_bytes());
    b[12..16].copy_from_slice(&version.to_be_bytes());
    for (i, pair) in raw_sha1.as_bytes().chunks(2).enumerate() {
        b[0x40 + i] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
    }
    b
}

/// A stored zip of `entries` (name, bytes); a name ending in `/` is a folder.
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let list: Vec<(&str, &[u8], bool)> = entries.iter().map(|(n, b)| (*n, *b, false)).collect();
    crate::zip::zip(&list)
}

/// CRC-32 of `bytes` as eight lower-case hex digits.
pub fn crc_hex(bytes: &[u8]) -> String {
    format!("{:08x}", crc32(bytes))
}

/// A PFS0 (NSP) holding empty files called `names`.
pub fn pfs0(names: &[&str]) -> Vec<u8> {
    let mut table = Vec::new();
    let mut offsets = Vec::new();
    for n in names {
        offsets.push(table.len() as u32);
        table.extend(n.as_bytes());
        table.push(0);
    }
    let mut head = Le::new(16 + 24 * names.len());
    head.bytes(0, b"PFS0").u32(4, names.len() as u32).u32(8, table.len() as u32);
    for (i, off) in offsets.iter().enumerate() {
        head.u32(16 + 24 * i + 16, *off);
    }
    let mut out = head.0;
    out.extend(table);
    out
}

/// A Wii WAD with the plain header, a ticket and a title metadata block holding `title_id` at 0x18C.
pub fn wad(title_id: u64) -> Vec<u8> {
    let mut b = vec![0u8; 0x40 + 0xA00 + 0x2C0 + 0x240];
    b[..4].copy_from_slice(&0x20u32.to_be_bytes());
    b[4] = b'I';
    b[5] = b's';
    b[8..12].copy_from_slice(&0xA00u32.to_be_bytes());
    b[0x10..0x14].copy_from_slice(&0x2A4u32.to_be_bytes());
    b[0x14..0x18].copy_from_slice(&0x208u32.to_be_bytes());
    let tmd = 0x40 + 0xA00 + 0x2C0;
    b[tmd + 0x18C..tmd + 0x194].copy_from_slice(&title_id.to_be_bytes());
    b
}

/// A bare NCCH header (no ExeFS) whose program id is `title_id`.
pub fn ncch(title_id: u64) -> Vec<u8> {
    let mut b = vec![0u8; 0x200];
    b[0x100..0x104].copy_from_slice(b"NCCH");
    b[0x118..0x120].copy_from_slice(&title_id.to_le_bytes());
    b
}

/// A DS header with `game_code` and no banner.
pub fn ds_header(game_code: &str) -> Vec<u8> {
    let mut b = vec![0u8; 0x400];
    b[0x0C..0x0C + game_code.len()].copy_from_slice(game_code.as_bytes());
    b[0x10..0x12].copy_from_slice(b"01");
    b
}

/// A Wii U `meta.xml` naming `title_id`.
pub fn wiiu_meta(title_id: &str) -> Vec<u8> {
    ascii(&format!("<menu><title_id type=\"hexBinary\" length=\"8\">{title_id}</title_id></menu>"))
}

/// A GameCube or Wii header the way the frontend reference suite builds it: 2048 bytes, the id then the magic, big endian.
pub fn disc_header_2048(id: &str, wii: bool) -> Vec<u8> {
    let mut b = vec![0u8; 2048];
    b[..id.len()].copy_from_slice(id.as_bytes());
    let (at, magic) = if wii { (0x18, WII_MAGIC) } else { (0x1C, GC_MAGIC) };
    b[at..at + 4].copy_from_slice(&magic);
    b
}

/// RVZ/WIA as the frontend reference suite builds it: `magic` + 1, header 1, header 2, then the plain 0x80-byte disc header.
pub fn compressed_2048(magic: &str, disc: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 2048];
    b[..magic.len()].copy_from_slice(magic.as_bytes());
    b[3] = 1;
    b[0x58..0xD8].copy_from_slice(&disc[..0x80]);
    b
}

/// An Xbox `default.xbe` header (2048 bytes): the base address, the certificate address, title id and region.
pub fn xbe(title_id: u32, region: u32, base: u32, cert_at: usize) -> Vec<u8> {
    let mut b = Le::new(2048);
    b.bytes(0, b"XBEH").u32(0x104, base).u32(0x118, base.wrapping_add(cert_at as u32));
    if cert_at + 0xA4 <= 2048 {
        b.u32(cert_at + 8, title_id).u32(cert_at + 0xA0, region);
    }
    b.0
}

/// An iNES header: `NES\x1A`, the PRG and CHR sizes in 16 KiB / 8 KiB units, `flags6`.
pub fn ines_header(prg_units: u8, chr_units: u8, flags6: u8) -> Vec<u8> {
    let mut b = vec![0u8; 16];
    b[..4].copy_from_slice(&[b'N', b'E', b'S', 0x1A]);
    b[4] = prg_units;
    b[5] = chr_units;
    b[6] = flags6;
    b
}

/// A PlayStation 2 disc with a made-up serial, as a small ISO.
pub fn sample_ps2_iso() -> Vec<u8> {
    iso9660(&[("SYSTEM.CNF;1", &system_cnf("BOOT2 = cdrom0:\\SLUS_000.03;1")), ("README.TXT;1", b"synthetic")])
}

/// A PSP disc with a made-up id and title, as a small ISO.
pub fn sample_psp_iso() -> Vec<u8> {
    iso9660(&[("PSP_GAME/PARAM.SFO;1", &sfo(&[("DISC_ID", "ULUS00001"), ("TITLE", "Sample Portable")]))])
}

/// A cue sheet whose first (data) track is `sample.bin`.
pub fn sample_cue() -> String {
    "FILE \"sample.bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n".into()
}

/// An `EBOOT.PBP` carrying the sample PSP `PARAM.SFO`.
pub fn sample_pbp() -> Vec<u8> {
    pbp(&sfo(&[("DISC_ID", "ULUS00001"), ("TITLE", "Sample Portable")]), 16)
}

/// A bare GameCube header file with a made-up id.
pub fn sample_gc_iso() -> Vec<u8> {
    game_cube("GSMP01")
}

/// A CHD v5 header with a made-up raw SHA-1.
pub fn sample_chd() -> Vec<u8> {
    chd("0123456789abcdef0123456789abcdef01234567", 5)
}

/// An NSP naming a made-up title.
pub fn sample_nsp() -> Vec<u8> {
    pfs0(&["0100000000001000000000000000000a.tik", "sample.nca"])
}

/// A zip holding one made-up ROM.
pub fn sample_rom_zip() -> Vec<u8> {
    zip(&[("game.sfc", &sample_rom(4096))])
}

/// The bytes of the sample ROM (`len` bytes of a repeating pattern).
pub fn sample_rom(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31) as u8).collect()
}

/// A tiny iNES file: header, 32 KiB of PRG, 8 KiB of CHR.
pub fn sample_nes() -> Vec<u8> {
    let mut b = ines_header(2, 1, 0);
    b.extend((0..16384 * 2).map(|i| (i % 251) as u8));
    b.extend((0..8192).map(|i| (i % 241) as u8));
    b
}

/// An Xbox `default.xbe` header with a made-up title id.
pub fn sample_xbe() -> Vec<u8> {
    xbe(0x4D53_0004, 1, 0x10000, 0x178)
}
