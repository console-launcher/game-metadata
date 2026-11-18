// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The CBMD container of an ExeFS `banner`.

use crate::lz11;
use crate::model::BannerScene;
use crate::reader::{FormatError, Reader};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Language slots after the common file (3dbrew CBMD): 0 EUR English .. 7 EUR Russian, 8 Japanese, 9 USA English ..
/// 12 USA Portuguese, then newer ones up to the jingle offset.
const SLOTS: usize = 30;
const MAGIC: u32 = 0x444D_4243; // "CBMD"
const MAX_CGFX: usize = 8 << 20;

/// A common CGFX and the per-language ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cbmd {
    /// The common CGFX (decompressed), when the container has one.
    pub common: Option<Vec<u8>>,
    /// Language slot to its CGFX (decompressed).
    pub regional: BTreeMap<i32, Vec<u8>>,
    /// The jingle (a BCWAV file, see [`crate::bcwav`]), when the container has one.
    pub jingle: Option<Vec<u8>>,
}

impl Cbmd {
    /// Slot of EUR English (0 EUR English .. 7 EUR Russian, 8 Japanese, 9 USA English .. 12 USA Portuguese).
    pub const EUR_ENGLISH: i32 = 0;
    /// Slot of USA English.
    pub const USA_ENGLISH: i32 = 9;

    /// Splits a CBMD; `None` when it is not one. A language file that does not decompress also gives `None`.
    pub fn parse(banner: &[u8]) -> Option<Cbmd> {
        let r = Reader(banner);
        if banner.len() < 0x88 || r.u32(0).ok()? != MAGIC {
            return None;
        }
        let jingle =
            r.i32(0x84).ok().filter(|&v| v >= 0x88 && v as usize <= banner.len()).map_or(banner.len(), |v| v as usize);
        let offsets: Vec<i32> = (0..=SLOTS as i64).map(|i| r.i32(8 + 4 * i)).collect::<Result<_, _>>().ok()?;
        // A file runs to the next offset (or the jingle, or the end), decompressed when LZ11.
        let cgfx_at = |i: usize| -> Result<Option<Vec<u8>>, FormatError> {
            let at = offsets[i];
            if at < 0x88 || at as usize >= banner.len() {
                return Ok(None);
            }
            let at = at as usize;
            let end = offsets
                .iter()
                .filter(|&&o| o > at as i32)
                .map(|&o| o as usize)
                .chain([jingle, banner.len()])
                .filter(|&e| e > at)
                .min()
                .unwrap_or(banner.len());
            let raw = &banner[at..end.min(banner.len())];
            if lz11::is_compressed(raw, 0) {
                lz11::decompress(raw, 0, MAX_CGFX).map(Some)
            } else {
                Ok((raw.len() >= 0x14).then(|| raw.to_vec()))
            }
        };
        let jingle =
            (jingle < banner.len() && banner[jingle..].starts_with(b"CWAV")).then(|| banner[jingle..].to_vec());
        let common = cgfx_at(0).ok()?;
        let mut regional = BTreeMap::new();
        for i in 0..SLOTS {
            if let Some(cgfx) = cgfx_at(i + 1).ok()? {
                regional.insert(i as i32, cgfx);
            }
        }
        Some(Cbmd { common, regional, jingle })
    }

    /// The language CGFX the scene takes its localized textures from: USA English, else EUR English, else none.
    pub fn localized(&self) -> Option<&[u8]> {
        self.regional.get(&Self::USA_ENGLISH).or_else(|| self.regional.get(&Self::EUR_ENGLISH)).map(Vec::as_slice)
    }

    /// The banner scene: the common CGFX with the [`localized`](Self::localized) textures over its placeholders.
    pub fn scene(&self) -> Option<BannerScene> {
        crate::cgfx::parse_with_localized(self.common.as_deref()?, self.localized())
    }
}
