// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The GameCube disc readers (plain, CISO, GCZ, RVZ) on arbitrary bytes: never panics, whatever the header claims, and
//! what comes back is well formed.
#![no_main]

use gamemeta_core::{ByteSource, ImageKind, Platform, SliceSource, gc};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut src = SliceSource::new(data);
    if let Some(info) = gc::read_info(&mut src) {
        assert_eq!(info.platform, Platform::GameCube);
        assert!(info.titles.len() <= 6);
        assert!(info.titles.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(info.ids.game_id.as_ref().is_none_or(|g| g.len() == 6));
        assert_eq!(info.images.contains(&ImageKind::Banner), info.icon.is_some());
        if let Some(img) = &info.icon {
            assert_eq!((img.width, img.height), (96, 32));
            assert_eq!(img.pixels.len(), 96 * 32 * 4);
        }
    }
    assert_eq!(src.size(), data.len() as u64);
});
