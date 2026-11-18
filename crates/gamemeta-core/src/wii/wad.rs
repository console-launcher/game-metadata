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
    if header_size != 0x20 || !matches!(&head[4..6], b"Is" | b"ib" | b"Bk") {
        return Ok(None);
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
    let game_id = Some(&id[4..8])
        .filter(|s| s.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        .map(|s| s.iter().map(|&c| char::from(c)).collect());
    let title_id = id.iter().map(|b| alloc::format!("{b:02X}")).collect();
    Ok(Some(Info { title_id, game_id, version: u16::from_be_bytes([version[0], version[1]]) }))
}
