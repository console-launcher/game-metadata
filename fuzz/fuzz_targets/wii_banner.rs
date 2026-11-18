// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Wii channel banner on arbitrary bytes. Byte 0 picks the scale (the frame is at most about 64 pixels a side), byte 1 the
//! frame, the rest is a `banner.bin` archive (and, separately, an `opening.bnr` or RVZ for the file readers). Never panics
//! (hostile layouts, animations, textures); a banner that opens draws the same frame whatever the output buffer held before, a
//! short buffer is refused, and the counts agree with the size.
#![no_main]

use gamemeta_core::wii::{self, banner::WiiBanner};
use gamemeta_core::{ByteSource, SliceSource};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let [s, f, rest @ ..] = data else { return };
    if let Some(banner) = WiiBanner::open(rest) {
        let (lw, lh) = banner.layout_size();
        assert!(lw > 0.0 && lh > 0.0 && lw <= 4096.0 && lh <= 4096.0);
        // About 1 to 64 pixels on the longest side.
        let scale = (f32::from(s % 64) + 1.0) / lw.max(lh);
        // A denormal layout size overflows the scale; the library refuses it (checked below), nothing more to draw.
        if !scale.is_finite() {
            assert!(banner.pixel_size(scale).is_none() && banner.render_into(0.0, scale, &mut []).is_none());
            return;
        }
        let frame = f32::from(*f) * 3.7;
        let (w, h) = banner.pixel_size(scale).expect("a small frame");
        let len = w as usize * h as usize * 4;
        let (mut a, mut b) = (vec![0u8; len], vec![0xA5u8; len]);
        assert_eq!(banner.render_into(frame, scale, &mut a), Some((w, h)));
        assert_eq!(banner.render_into(frame, scale, &mut b), Some((w, h)));
        assert!(a == b, "the frame depends on the buffer's old contents");
        assert!(banner.render_into(frame, scale, &mut a[1..]).is_none());
        let image = banner.poster(scale).expect("a poster");
        assert_eq!(image.pixels.len(), len);
        assert!(banner.skipped().iter().all(|(_, n)| *n > 0));
        assert!(banner.pixel_size(0.0).is_none() && banner.pixel_size(f32::NAN).is_none());
    }
    // The file readers: an RVZ, an opening.bnr or a banner.bin in one.
    let mut src = SliceSource::new(data);
    let _ = wii::open_channel_banner(&mut src);
    assert_eq!(src.size(), data.len() as u64);
    let _ = wii::banner::banner_bin(data);
});
