// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Nintendo Wii U titles: the `meta/` folder of an unpacked title (found from its `.rpx`) or of one title in a
//! `.wua`. Titles per language, ids, region, the icon and the boot media sizes come from `meta/meta.xml` and the TGA
//! pictures; the large splashes are decoded on demand with [`image`]. `.wud` and `.wux` are encrypted and give
//! nothing: no key is ever used.
//!
//! The core does no I/O: a title is a [`TitleFiles`], which the caller backs with a folder, a `.wua`
//! ([`open_wua`]) or an opener callback ([`OpenerFiles`]).
//!
//! Format facts: the ZArchive format description (see `NOTICE.md`).

mod meta_xml;
mod tga;
mod zarchive;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::source::fetch_opt;
use crate::{ByteSource, GameInfo, ImageKind, Language, Platform, Region, RgbaImage, Title};

const MAX_META: usize = 1 << 20;
const MAX_IMAGE: usize = 16 << 20;
const ALL_REGIONS: u64 = 0x7FFF_FFFF;
const KNOWN: u64 = 0x77;
/// `meta.xml` language suffixes, in [`Language`] slot order.
const SUFFIXES: [&str; 12] = ["ja", "en", "fr", "de", "it", "es", "zhs", "ko", "nl", "pt", "ru", "zht"];
const IMAGE_FILES: [(ImageKind, &str); 4] = [
    (ImageKind::Icon, "meta/iconTex.tga"),
    (ImageKind::TvSplash, "meta/bootTvTex.tga"),
    (ImageKind::GamePadSplash, "meta/bootDrcTex.tga"),
    (ImageKind::BootLogo, "meta/bootLogoTex.tga"),
];

/// The files of one Wii U title, by path from the title's root (`meta/meta.xml`, `meta/iconTex.tga`).
pub trait TitleFiles {
    /// The size of the file at `path`, or `None` when there is none.
    fn size(&mut self, path: &str) -> Option<u64>;
    /// The file's bytes; `None` when it is missing, unreadable or over `max` bytes.
    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>>;
}

/// A title seen from its `.rpx`, which sits in the title's `code/` folder: `open` opens a file by its path relative to
/// the `.rpx`'s folder (the caller resolves `../meta/meta.xml` against the game file), and the title path `meta/meta.xml`
/// is asked for as `../meta/meta.xml`. `open` returns `None` for a file that is not there.
pub struct OpenerFiles<O>(pub O);

impl<O> core::fmt::Debug for OpenerFiles<O> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OpenerFiles")
    }
}

impl<O: FnMut(&str) -> Option<Box<dyn ByteSource>>> TitleFiles for OpenerFiles<O> {
    fn size(&mut self, path: &str) -> Option<u64> {
        let mut name = String::from("../");
        name.push_str(path);
        Some((self.0)(&name)?.size())
    }

    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        let mut name = String::from("../");
        name.push_str(path);
        let mut source = (self.0)(&name)?;
        read_all(source.as_mut(), max)
    }
}

/// All of `source`, or `None` when it is over `max` bytes or unreadable.
pub(crate) fn read_all<S: ByteSource + ?Sized>(source: &mut S, max: usize) -> Option<Vec<u8>> {
    let size = source.size();
    if size > max as u64 {
        return None;
    }
    fetch_opt(source, 0, size).ok()?
}

/// Whether a file of this name could be a Wii U title this reader opens (`.rpx`, through its folder, or `.wua`); no
/// file access.
pub fn is_candidate(file_name: &str) -> bool {
    is_rpx(file_name) || is_wua(file_name)
}

/// The extensions of a Wii U title file.
pub(crate) const EXTENSIONS: [&str; 2] = ["rpx", "wua"];

/// Whether the name is a `.rpx`.
pub fn is_rpx(file_name: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case("rpx"))
}

/// Whether the name is a `.wua`.
pub fn is_wua(file_name: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case("wua"))
}

/// The base title of a `.wua` (else the first title with a `meta/meta.xml`) as [`TitleFiles`].
pub struct WuaFiles<'a, S: ?Sized> {
    archive: zarchive::Archive<'a, S>,
    folder: zarchive::Entry,
}

impl<S: ?Sized> core::fmt::Debug for WuaFiles<'_, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WuaFiles").field("folder", &self.folder.name).finish_non_exhaustive()
    }
}

impl<S: ByteSource + ?Sized> WuaFiles<'_, S> {
    fn file(&self, path: &str) -> Option<zarchive::Entry> {
        self.archive.find(Some(&self.folder), path).filter(|e| e.is_file)
    }
}

impl<S: ByteSource + ?Sized> TitleFiles for WuaFiles<'_, S> {
    fn size(&mut self, path: &str) -> Option<u64> {
        self.file(path).map(|e| e.size_or_count)
    }

    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        let entry = self.file(path)?;
        self.archive.read(&entry, max)
    }
}

/// Opens a `.wua`; `None` when it is not a readable ZArchive or holds no title with a `meta/meta.xml`.
pub fn open_wua<S: ByteSource + ?Sized>(source: &mut S) -> Option<WuaFiles<'_, S>> {
    let archive = zarchive::Archive::open(source)?;
    let folder = archive.pick_title()?;
    Some(WuaFiles { archive, folder })
}

/// A `.wua`'s base title.
pub fn read_wua_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo> {
    read_title(&mut open_wua(source)?)
}

/// A TGA (type 2 or 10, 24 or 32 bit) decoded; `None` for anything else.
pub fn decode_tga(bytes: &[u8]) -> Option<RgbaImage> {
    tga::decode(bytes)
}

fn path_of(kind: ImageKind) -> Option<&'static str> {
    IMAGE_FILES.iter().find(|(k, _)| *k == kind).map(|(_, p)| *p)
}

/// One picture of the title decoded: [`ImageKind::Icon`], `TvSplash`, `GamePadSplash` or `BootLogo`; `None` when the
/// file is missing or not a readable TGA (and for the kinds a Wii U title does not have).
pub fn image<F: TitleFiles + ?Sized>(files: &mut F, kind: ImageKind) -> Option<RgbaImage> {
    tga::decode(&files.read(path_of(kind)?, MAX_IMAGE)?)
}

/// A title's id (16 upper-case hex digits) from `meta/meta.xml` alone, without the pictures; `None` when there is no readable
/// `meta.xml` or it names none.
pub fn title_id<F: TitleFiles + ?Sized>(files: &mut F) -> Option<String> {
    let bytes = files.read("meta/meta.xml", MAX_META)?;
    let xml = String::from_utf8_lossy(&bytes);
    let tags = meta_xml::parse(&xml);
    let id = tags.iter().find(|(n, _)| *n == "title_id")?.1.to_uppercase();
    (!id.is_empty()).then_some(id)
}

/// A title's info from its files; `None` when there is no readable `meta.xml` or it names nothing.
pub fn read_title<F: TitleFiles + ?Sized>(files: &mut F) -> Option<GameInfo> {
    let bytes = files.read("meta/meta.xml", MAX_META)?;
    let xml = String::from_utf8_lossy(&bytes);
    let tags = meta_xml::parse(&xml);
    let tag = |name: &str| tags.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_str());
    let mut info = GameInfo::new(Platform::WiiU);
    for (slot, suffix) in SUFFIXES.iter().enumerate() {
        let long = tag(&alloc::format!("longname_{suffix}"));
        let short = tag(&alloc::format!("shortname_{suffix}"));
        let Some(name) = long.or(short) else { continue };
        let publisher = tag(&alloc::format!("publisher_{suffix}")).map(String::from);
        let language = Language::from_slot(slot)?;
        info.titles.push((language, Title { name: name.into(), short_name: short.unwrap_or(name).into(), publisher }));
    }
    info.ids.title_id = tag("title_id").map(str::to_uppercase);
    info.ids.product_code = tag("product_code").map(str::to_uppercase);
    if info.titles.is_empty() && info.ids.title_id.is_none() && info.ids.product_code.is_none() {
        return None;
    }
    let bits = tag("region").and_then(|r| u64::from_str_radix(r, 16).ok());
    if let Some(bits) = bits.filter(|b| *b != ALL_REGIONS) {
        const MASKS: [(u64, Region); 6] = [
            (0x01, Region::Japan),
            (0x02, Region::NorthAmerica),
            (0x04, Region::Europe),
            (0x10, Region::China),
            (0x20, Region::Korea),
            (0x40, Region::Taiwan),
        ];
        info.regions = MASKS.iter().filter(|(m, _)| bits & m != 0).map(|(_, r)| *r).collect();
    }
    info.region_free = bits.is_some_and(|b| b == ALL_REGIONS || b & KNOWN == KNOWN);
    info.icon = image(files, ImageKind::Icon);
    for (kind, path) in IMAGE_FILES {
        let present = if kind == ImageKind::Icon { info.icon.is_some() } else { files.size(path).is_some() };
        if present {
            info.images.push(kind);
        }
    }
    info.boot_movie = files.size("meta/bootMovie.h264");
    info.boot_sound = files.size("meta/bootSound.btsnd");
    Some(info)
}
