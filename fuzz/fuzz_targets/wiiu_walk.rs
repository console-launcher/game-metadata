// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Wii U readers on arbitrary bytes: a `.wua` (ZArchive footer, tables, zstd blocks, tree walk), the `meta.xml`
//! scanner and the TGA decoder (the input is also served as a title's `meta.xml` and `iconTex.tga`). Never panics;
//! what comes back is well formed.
#![no_main]

use gamemeta_core::wiiu::{self, TitleFiles};
use gamemeta_core::{ImageKind, Platform, RgbaImage, SliceSource};
use libfuzzer_sys::fuzz_target;

/// A title whose `meta.xml` and icon are both the fuzz input.
struct Title<'a>(&'a [u8]);

impl TitleFiles for Title<'_> {
    fn size(&mut self, path: &str) -> Option<u64> {
        matches!(path, "meta/meta.xml" | "meta/iconTex.tga").then_some(self.0.len() as u64)
    }
    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        (self.0.len() <= max && self.size(path).is_some()).then(|| self.0.to_vec())
    }
}

fn well_formed(img: &RgbaImage) {
    assert!(img.width >= 1 && img.height >= 1 && img.width <= 4096 && img.height <= 4096);
    assert_eq!(img.pixels.len(), img.width as usize * img.height as usize * 4);
}

fn check(info: &gamemeta_core::GameInfo) {
    assert_eq!(info.platform, Platform::WiiU);
    assert!(info.titles.len() <= 12);
    assert!(info.titles.windows(2).all(|w| w[0].0 < w[1].0));
    assert_eq!(info.images.contains(&ImageKind::Icon), info.icon.is_some());
    if let Some(img) = &info.icon {
        well_formed(img);
    }
}

fuzz_target!(|data: &[u8]| {
    if let Some(info) = wiiu::read_wua_info(&mut SliceSource::new(data)) {
        check(&info);
    }
    let mut src = SliceSource::new(data);
    if let Some(mut files) = wiiu::open_wua(&mut src) {
        for kind in [ImageKind::Icon, ImageKind::TvSplash, ImageKind::GamePadSplash, ImageKind::BootLogo] {
            if let Some(img) = wiiu::image(&mut files, kind) {
                well_formed(&img);
            }
        }
    }
    if let Some(info) = wiiu::read_title(&mut Title(data)) {
        check(&info);
    }
    if let Some(img) = wiiu::decode_tga(data) {
        well_formed(&img);
    }
});
