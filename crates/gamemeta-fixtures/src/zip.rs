// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A minimal zip writer for synthetic files: stored or deflated entries, a central directory, no zip64.

/// CRC-32 (IEEE), bitwise: fixtures are small.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// A zip of `entries` (name, content, deflate?), in order. Deflated entries are written the way Java's
/// `ZipOutputStream` does: flag bit 3 set, sizes zero in the local header, a data descriptor after the data.
pub fn zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, content, deflate) in entries {
        let crc = crc32(content);
        let data = if *deflate { miniz_oxide::deflate::compress_to_vec(content, 6) } else { content.to_vec() };
        let (method, flags) = if *deflate { (8u16, 8u16) } else { (0, 0) };
        let local_at = out.len() as u32;
        out.extend(0x0403_4B50u32.to_le_bytes());
        out.extend(20u16.to_le_bytes());
        out.extend(flags.to_le_bytes());
        out.extend(method.to_le_bytes());
        out.extend([0u8; 4]); // time, date
        let (lcrc, lcomp, lsize) = if *deflate { (0, 0, 0) } else { (crc, data.len() as u32, content.len() as u32) };
        out.extend(lcrc.to_le_bytes());
        out.extend(lcomp.to_le_bytes());
        out.extend(lsize.to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(&data);
        if *deflate {
            out.extend(0x0807_4B50u32.to_le_bytes());
            out.extend(crc.to_le_bytes());
            out.extend((data.len() as u32).to_le_bytes());
            out.extend((content.len() as u32).to_le_bytes());
        }
        central.extend(0x0201_4B50u32.to_le_bytes());
        central.extend(20u16.to_le_bytes());
        central.extend(20u16.to_le_bytes());
        central.extend(flags.to_le_bytes());
        central.extend(method.to_le_bytes());
        central.extend([0u8; 4]);
        central.extend(crc.to_le_bytes());
        central.extend((data.len() as u32).to_le_bytes());
        central.extend((content.len() as u32).to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0u8; 12]); // extra, comment, disk, internal attrs, external attrs (4 bytes)
        central.extend(local_at.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let cd_at = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4B50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(cd_at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}
