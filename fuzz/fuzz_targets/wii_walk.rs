// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Wii readers on arbitrary bytes: discs (plain, WBFS, WIA, RVZ with its partition walk), WADs and the
//! `opening.bnr` (IMET, U8, LZ77, IMD5, TPL) parser. Never panics; what comes back is well formed.
#![no_main]

use gamemeta_core::{ByteSource, ImageKind, Platform, SliceSource, wii};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut src = SliceSource::new(data);
    for info in [wii::read_info(&mut src), wii::read_wad_info(&mut src)].into_iter().flatten() {
        assert_eq!(info.platform, Platform::Wii);
        assert!(info.titles.len() <= 10);
        assert!(info.titles.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(info.images.contains(&ImageKind::Logo), info.icon.is_some());
        if let Some(img) = &info.icon {
            assert_eq!(img.pixels.len(), (img.width * img.height * 4) as usize);
        }
    }
    if let Some(bnr) = wii::parse_opening_bnr(data) {
        assert!(bnr.titles.len() <= 10);
        if let Some(img) = &bnr.logo {
            assert!(img.width >= 64 && img.width > img.height && img.width <= 4096 && img.height <= 4096);
            assert_eq!(img.pixels.len(), (img.width * img.height * 4) as usize);
        }
    }
    assert_eq!(src.size(), data.len() as u64);
});
