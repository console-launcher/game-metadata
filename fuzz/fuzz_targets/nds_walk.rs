// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The DS ROM, banner and zip readers on arbitrary bytes: never panics, extents stay consistent, images well formed.
#![no_main]

use gamemeta_core::nds::{self, Read};
use gamemeta_core::{ByteSource, SliceSource};
use libfuzzer_sys::fuzz_target;

fn check_read(read: &Read) {
    if let Read::Found { banner, raw } = read {
        assert!(raw.len() <= nds::MAX_BANNER);
        assert!(matches!(banner.titles.len(), 6..=8));
        for img in std::iter::once(&banner.icon).chain(banner.animation.iter().map(|s| &s.frame)) {
            assert_eq!((img.width, img.height), (32, 32));
            assert_eq!(img.pixels.len(), 32 * 32 * 4);
        }
        assert!(banner.animation.len() <= 64);
        assert!(banner.animation.iter().all(|s| s.duration_frames > 0));
    }
}

fuzz_target!(|data: &[u8]| {
    let size = data.len() as u64;
    let mut src = SliceSource::new(data);
    check_read(&nds::parse_banner(data));
    check_read(&nds::read_banner(&mut src));
    check_read(&nds::read_zip_banner(&mut src));
    for info in [nds::read_info(&mut src), nds::read_zip_info(&mut src)].into_iter().flatten() {
        assert!(info.titles.len() <= 8);
        for img in [&info.icon].into_iter().flatten().chain(info.animated_icon.iter().map(|s| &s.frame)) {
            assert_eq!(img.pixels.len(), (img.width * img.height * 4) as usize);
        }
        assert!(info.ids.game_id.as_ref().is_none_or(|g| g.len() == 4));
    }
    assert_eq!(src.size(), size);
});
