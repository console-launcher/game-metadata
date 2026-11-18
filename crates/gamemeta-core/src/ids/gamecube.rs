// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The 6-character GameCube / Wii game id of a disc image, from the header alone.

use alloc::string::String;

use crate::ByteSource;
use crate::wii;

const GC_MAGIC: [u8; 4] = [0xC2, 0x33, 0x9F, 0x3D];
const WII_MAGIC: [u8; 4] = [0x5D, 0x1C, 0x9E, 0xA3];

/// The disc type the 0x40-byte header names: the GameCube magic at 0x1C or the Wii magic at 0x18.
pub(crate) fn has_disc_magic(head: &[u8]) -> bool {
    head.get(0x1C..0x20) == Some(&GC_MAGIC[..]) || head.get(0x18..0x1C) == Some(&WII_MAGIC[..])
}

/// The id of a plain, CISO, GCZ, RVZ, WIA or WBFS image; `None` for any other file. Shares the container logic of the
/// Wii reader ([`wii::disc_header`]); a GCZ gives its id here, which the original id reader did not.
pub(crate) fn game_id<S: ByteSource + ?Sized>(source: &mut S) -> Option<String> {
    let head = wii::disc_header(source, 0x40).ok()??;
    if !has_disc_magic(&head) {
        return None;
    }
    let id = head.get(..6)?;
    id.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()).then(|| id.iter().map(|&c| char::from(c)).collect())
}
