// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A WAD's (installable channel, WiiWare, Virtual Console) title id and version from its plain header and title
//! metadata (TMD). The contents are encrypted and are never read: no banner, no title.

use alloc::string::String;

use crate::le::be32_at;
use crate::source::{fetch, fetch_opt};
use crate::{ByteSource, ReadError};

const TMD_TITLE_ID: u64 = 0x18C;
const TMD_VERSION: u64 = 0x1DC;

pub(crate) struct Info {
    pub title_id: String,
    pub game_id: Option<String>,
    #[allow(dead_code)] // the title version: parsed as the original implementation does, not part of the API yet
    pub version: u16,
}

fn align64(v: u64) -> u64 {
    (v + 0x3F) & !0x3F
}

pub(crate) fn read<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<Info>, ReadError> {
    let Some(head) = fetch_opt(source, 0, 0x20)? else { return Ok(None) };
    let word = |at| u64::from(be32_at(&head, at).unwrap_or(0));
    let (header_size, certs, ticket, tmd) = (word(0), word(8), word(0x10), word(0x14));
    if !matches!(&head[4..6], b"Is" | b"ib" | b"Bk") {
        return Ok(None);
    }
    // The installable layout: a 0x20 byte header whose sizes place the TMD. Any other (the `Bk` backup header is 0x70 bytes,
    // with other fields) is searched for its TMD.
    if header_size != 0x20 {
        return scan(source);
    }
    if certs > 0x10000 || !(0x2A4..=0x10000).contains(&ticket) || !(0x1E0..=0x10000).contains(&tmd) {
        return Ok(None);
    }
    let tmd_at = align64(header_size) + align64(certs) + align64(ticket);
    if tmd_at + TMD_VERSION + 2 > source.size() {
        return Ok(None);
    }
    let id = fetch(source, tmd_at + TMD_TITLE_ID, 8)?;
    if id.iter().all(|&b| b == 0) {
        return Ok(None); // a zeroed block is not a TMD
    }
    let version = fetch(source, tmd_at + TMD_VERSION, 2)?;
    Ok(Some(info(&id, &version)))
}

fn info(id: &[u8], version: &[u8]) -> Info {
    let game_id = Some(&id[4..8])
        .filter(|s| s.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        .map(|s| s.iter().map(|&c| char::from(c)).collect());
    let title_id = id.iter().map(|b| alloc::format!("{b:02X}")).collect();
    Info { title_id, game_id, version: u16::from_be_bytes([version[0], version[1]]) }
}

/// How far into the file [`scan`] looks for a TMD.
const SCAN: u64 = 0x4000;

/// A TMD found by its own shape: a signature type (RSA-2048 or RSA-4096), then the issuer `Root-...-CP...` that follows the
/// signature, on a 64 byte boundary. The title id and version sit at the usual offsets from there.
fn scan<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<Info>, ReadError> {
    let Some(bytes) = fetch_opt(source, 0, SCAN.min(source.size()))? else { return Ok(None) };
    for at in (0x40..bytes.len()).step_by(0x40) {
        // The signature (0x100 or 0x200 bytes) and its padding to 0x40 come first, then the TMD body.
        let body = match be32_at(&bytes, at) {
            Some(0x0001_0001) => 0x140,
            Some(0x0001_0000) => 0x240,
            _ => continue,
        };
        let Some(issuer) = bytes.get(at + body..at + body + 0x40) else { continue };
        let name = &issuer[..issuer.iter().position(|&b| b == 0).unwrap_or(issuer.len())];
        let is_cp = name.starts_with(b"Root-") && name.windows(3).any(|w| w == b"-CP");
        let (Some(id), Some(version)) =
            (bytes.get(at + body + 0x4C..at + body + 0x54), bytes.get(at + body + 0x9C..at + body + 0x9E))
        else {
            continue;
        };
        if is_cp && id.iter().any(|&b| b != 0) {
            return Ok(Some(info(id, version)));
        }
    }
    Ok(None)
}
