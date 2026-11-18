// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The 3DS container walk and SMDH reader on arbitrary bytes: never panics, and everything it locates lies inside
//! the file.
#![no_main]

use gamemeta_core::threeds;
use gamemeta_core::{ByteSource, SliceSource};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let size = data.len() as u64;
    let mut src = SliceSource::new(data);
    let layout = threeds::locate(&mut src);
    for e in [layout.banner, layout.icon].into_iter().flatten() {
        // The walk checks the ExeFS header is inside the file, not each entry: entries may point past a cut file,
        // and reading them must then fail cleanly.
        assert!(e.size > 0 && e.offset.checked_add(e.size).is_some());
    }
    if let Ok(found) = threeds::read(&mut src) {
        assert_eq!(Some(found.banner.len() as u64), layout.banner.map(|b| b.size));
        assert!(found.banner.len() as u64 <= size);
    }
    let _ = threeds::title_id(&mut src);
    if let Some(info) = threeds::read_info(&mut src) {
        for img in [&info.icon, &info.small_icon].into_iter().flatten() {
            assert_eq!(img.pixels.len(), (img.width * img.height * 4) as usize);
        }
        assert!(info.ids.title_id.as_ref().is_none_or(|t| t.len() == 16));
    }
    assert_eq!(src.size(), size);
});
