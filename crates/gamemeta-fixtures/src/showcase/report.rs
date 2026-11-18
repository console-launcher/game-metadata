// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reads a demo file the way an app does and prints what came out, one stable text per file: the golden files and the
//! contact sheet are built from this.

use super::hash::{hex, sha256};
use gamemeta_core::{ByteSource, GameInfo, ReadError, RgbaImage, SliceSource, ids::Ids, wiiu};

/// What the readers say about one demo file.
#[derive(Debug)]
pub struct Read {
    /// The metadata reader's answer.
    pub info: Option<GameInfo>,
    /// The ids reader's answer (with the RetroAchievements hash filled in where there is one).
    pub ids: Option<Ids>,
    /// The streaming checksum check, for the formats that have one.
    pub checksum: Option<bool>,
}

/// An owned copy of a file, for the opener (which hands out sources that outlive the call).
struct Owned(Vec<u8>);

impl ByteSource for Owned {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        SliceSource::new(&self.0).read_at(offset, buf)
    }
}

fn find<'a>(files: &'a [(String, Vec<u8>)], path: &str) -> Option<&'a [u8]> {
    files.iter().find(|(p, _)| p == path).map(|(_, b)| b.as_slice())
}

/// `path` joined to the folder `dir` with `..` resolved, the way a file system would.
fn resolve(dir: &str, path: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for p in path.split('/') {
        match p {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Reads `name` (a path in `files`) with every reader that applies; a `.rpx` reads its title folder.
pub fn read(files: &[(String, Vec<u8>)], name: &str) -> Read {
    let bytes = find(files, name).unwrap_or_else(|| panic!("{name} is not in the demo set"));
    let dir = name.rsplit_once('/').map_or("", |(d, _)| d).to_string();
    let opener = |path: &str| -> Option<Box<dyn ByteSource>> {
        let found = find(files, &resolve(&dir, path))?;
        Some(Box::new(Owned(found.to_vec())))
    };
    let mut source = SliceSource::new(bytes);
    let info = if wiiu::is_rpx(name) {
        wiiu::read_title(&mut wiiu::OpenerFiles(opener))
    } else {
        gamemeta_core::read(&mut SliceSource::new(bytes), name)
    };
    let mut ids = gamemeta_core::ids::read_with(&mut source, name, &mut { opener });
    if let Some(hash) = gamemeta_core::ids::ra_hash(&mut source, name) {
        ids.get_or_insert_with(Default::default).ra_md5 = Some(hash);
    }
    let checksum = gamemeta_core::carts::checksum_matches(&mut SliceSource::new(bytes), name);
    Read { info, ids, checksum }
}

/// `image`'s size and the SHA-256 of its RGBA bytes, as one short text.
pub fn image_line(image: &RgbaImage) -> String {
    format!("{}x{} sha256={}", image.width, image.height, hex(&sha256(&image.pixels)))
}

/// The golden text of one demo file: both readers' JSON, then every image they extracted by size and hash.
pub fn golden(files: &[(String, Vec<u8>)], name: &str) -> String {
    let r = read(files, name);
    let mut out = format!(
        "file: {name}\ninfo: {}\nids: {}\n",
        gamemeta_json::info(r.info.as_ref()),
        gamemeta_json::ids(r.ids.as_ref())
    );
    if let Some(i) = &r.info {
        for (label, image) in [("icon", &i.icon), ("smallIcon", &i.small_icon)] {
            if let Some(im) = image {
                out += &format!("{label}: {}\n", image_line(im));
            }
        }
        for (n, step) in i.animated_icon.iter().enumerate() {
            out += &format!("frame {n} ({} ticks): {}\n", step.duration_frames, image_line(&step.frame));
        }
    }
    if let Some(c) = r.checksum {
        out += &format!("checksum: {c}\n");
    }
    out
}

/// Every image the readers extract from the demo files, for the contact sheet and the page: (file, label, image). A file
/// contributes its icon, small icon, animation frames and, for a Wii U title, its boot images. Folders' files are read
/// through their \`.rpx\`.
pub fn images(files: &[(String, Vec<u8>)]) -> Vec<(String, String, RgbaImage)> {
    let mut out = Vec::new();
    for (name, _) in files {
        if !super::is_demo_entry(name) {
            continue;
        }
        if name == "tux-wii.rvz" {
            let bytes = find(files, name).expect("listed");
            let banner = gamemeta_core::wii::open_channel_banner(&mut SliceSource::new(bytes)).ok().flatten();
            for frame in [0.0, 30.0, 60.0, 90.0] {
                if let Some(image) = banner.as_ref().and_then(|b| b.render(frame, 0.4)) {
                    out.push((name.clone(), format!("banner frame {frame}"), image));
                }
            }
        }
        let Some(info) = read(files, name).info else { continue };
        let mut push = |label: &str, image: Option<RgbaImage>| {
            if let Some(i) = image {
                out.push((name.clone(), label.to_string(), i));
            }
        };
        push("icon", info.icon.clone());
        push("small icon", info.small_icon.clone());
        for (n, step) in info.animated_icon.iter().enumerate() {
            push(&format!("frame {n}"), Some(step.frame.clone()));
        }
    }
    out
}

/// A contact sheet of \`images\`: each shown at \`scale\`x (nearest neighbour) on a checkerboard so transparency shows, left
/// to right, wrapping at \`columns\` cells; every cell is as big as the biggest image.
pub fn contact_sheet(images: &[(String, String, RgbaImage)], columns: usize, scale: u32) -> RgbaImage {
    let cell_w = images.iter().map(|i| i.2.width).max().unwrap_or(1) * scale + 8;
    let cell_h = images.iter().map(|i| i.2.height).max().unwrap_or(1) * scale + 8;
    let rows = images.len().div_ceil(columns).max(1) as u32;
    let (w, h) = (cell_w * columns as u32, cell_h * rows);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let shade = if ((x / 8) + (y / 8)) % 2 == 0 { 0xC8 } else { 0xE4 };
            let i = ((y * w + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&[shade, shade, shade, 255]);
        }
    }
    for (n, (_, _, image)) in images.iter().enumerate() {
        let (cx, cy) = ((n % columns) as u32 * cell_w + 4, (n / columns) as u32 * cell_h + 4);
        for y in 0..image.height * scale {
            for x in 0..image.width * scale {
                let s = (((y / scale) * image.width + x / scale) * 4) as usize;
                let a = u32::from(image.pixels[s + 3]);
                let d = (((cy + y) * w + cx + x) * 4) as usize;
                for c in 0..3 {
                    pixels[d + c] =
                        ((u32::from(image.pixels[s + c]) * a + u32::from(pixels[d + c]) * (255 - a) + 127) / 255) as u8;
                }
            }
        }
    }
    RgbaImage { width: w, height: h, pixels }
}
