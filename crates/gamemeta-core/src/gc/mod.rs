// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Nintendo GameCube discs: ISO/GCM, CISO, GCZ and RVZ images, down to the disc header and `opening.bnr`.
//!
//! Format facts: YAGCD and the public RVZ format notes (see `NOTICE.md`).

pub(crate) mod bnr;
pub(crate) mod containers;
pub(crate) mod disc;
pub(crate) mod rvz;

use crate::source::fetch_opt;
use crate::{ByteSource, GameIds, GameInfo, ImageKind, Platform, ReadError};

pub(crate) const EXTENSIONS: [&str; 7] = ["iso", "gcm", "ciso", "gcz", "rvz", "wia", "tgc"];
const HEADER: u64 = 0x440;
const GC_MAGIC: [u8; 4] = [0xC2, 0x33, 0x9F, 0x3D];

/// Whether a file of this name could be a GameCube disc this reader opens; no file access.
pub fn is_candidate(file_name: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(_, e)| EXTENSIONS.iter().any(|w| e.eq_ignore_ascii_case(w)))
}

/// A GameCube disc image's ids, region, titles per language and banner. `None` for a file that is not a GameCube
/// disc (a Wii disc included) or cannot be read.
pub fn read_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo> {
    read(source).ok().flatten()
}

fn read<S: ByteSource + ?Sized>(source: &mut S) -> Result<Option<GameInfo>, ReadError> {
    let Some(mut disc) = containers::open(source)? else { return Ok(None) };
    let Some(header) = fetch_opt(&mut *disc, 0, HEADER)? else { return Ok(None) };
    if header[0x1C..0x20] != GC_MAGIC {
        return Ok(None);
    }
    let game_id = disc::game_id(&header);
    let region = game_id.map(|id| id[3]);
    let charset = bnr::Charset::for_region(region);
    let parsed = disc::read_root_file(&mut *disc, &header, "opening.bnr", bnr::MAX_SIZE, 0)?
        .and_then(|b| bnr::parse(&b, charset, region == Some(b'J')));
    let mut info = GameInfo::new(Platform::GameCube);
    info.ids = GameIds {
        game_id: game_id.map(|id| id.iter().map(|&c| char::from(c)).collect()),
        maker_code: game_id.map(|id| id[4..].iter().map(|&c| char::from(c)).collect()),
        ..GameIds::default()
    };
    info.regions = disc::regions(region);
    let header_title = bnr::text(&header, 0x20, 0x40, charset);
    info.header_title = Some(header_title).filter(|t| !crate::text::is_blank(t));
    if let Some(parsed) = parsed {
        info.titles = parsed.titles;
        if parsed.image.is_some() {
            info.images.push(ImageKind::Banner);
        }
        info.icon = parsed.image;
    }
    Ok(Some(info))
}
