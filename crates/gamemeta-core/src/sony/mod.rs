// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sony: PSP games (an `EBOOT.PBP` or a PSP ISO), PS3 titles (a disc folder or a game folder) and Vita titles (a folder or a
//! `.vpk`), read from their `PARAM.SFO` and `ICON0.PNG`. Nothing here decrypts anything: an encrypted PS3 ISO or an NPS package is
//! not a file this module opens.
//!
//! - A PSP game gives its title and disc id, `ICON0.PNG` as the icon, `PIC1.PNG` on demand ([`psp_image`]) and `ICON1.PMF` as the
//!   undecoded movie ([`psp_movie`]); its size is [`GameInfo::boot_movie`].
//! - PS3 and Vita titles come through [`TitleFiles`], the callback the Wii U reader uses: the caller opens the files of the
//!   title's folder. A `.vpk` is a zip, read in place.

use alloc::string::String;
use alloc::vec::Vec;

use crate::ids::iso9660::{self, Layout};
use crate::ids::psp as sfo;
use crate::source::fetch_opt;
use crate::wiiu::TitleFiles;
use crate::zip::{self, Entry};
use crate::{ByteSource, GameInfo, ImageKind, Language, Platform, Region, RgbaImage, Title, png};

/// The biggest picture file read (`PIC1.PNG` is about 480 x 272).
const MAX_PICTURE: usize = 8 << 20;
/// The biggest `ICON1.PMF` returned.
const MAX_MOVIE: usize = 32 << 20;
const MAX_SFO: usize = 64 * 1024;
const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G'];

/// What kind of movie [`psp_movie`] holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieKind {
    /// A PSMF (`ICON1.PMF`), the animated icon.
    Pmf,
}

/// A movie file kept as the file stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movie {
    /// The format.
    pub kind: MovieKind,
    /// The file's bytes.
    pub bytes: Vec<u8>,
}

fn has_extension(file_name: &str, want: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case(want))
}

/// Whether a file of this name is a PSP `EBOOT.PBP`.
pub fn is_pbp(file_name: &str) -> bool {
    has_extension(file_name, "pbp")
}

/// Whether a file of this name is a Vita `.vpk`.
pub fn is_vpk(file_name: &str) -> bool {
    has_extension(file_name, "vpk")
}

// ---- PSP

/// The files of a PSP game this module reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Sfo,
    Icon0,
    Icon1,
    Pic1,
}

impl Part {
    /// Where a PBP header keeps the offset (it holds eight: SFO, ICON0, ICON1, PIC0, PIC1, SND0, DATA.PSP, DATA.PSAR).
    fn pbp_slot(self) -> usize {
        match self {
            Part::Sfo => 0,
            Part::Icon0 => 1,
            Part::Icon1 => 2,
            Part::Pic1 => 4,
        }
    }

    fn iso_path(self) -> &'static str {
        match self {
            Part::Sfo => "PSP_GAME/PARAM.SFO",
            Part::Icon0 => "PSP_GAME/ICON0.PNG",
            Part::Icon1 => "PSP_GAME/ICON1.PMF",
            Part::Pic1 => "PSP_GAME/PIC1.PNG",
        }
    }
}

/// A PSP game's container: a PBP's offset table or an ISO's directory tree.
enum Psp {
    Pbp { offsets: [u64; 8], size: u64 },
    Iso(Layout),
}

impl Psp {
    fn open<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<Psp> {
        if is_pbp(file_name) {
            let head = fetch_opt(source, 0, 40).ok()??;
            if head[..4] != [0, b'P', b'B', b'P'] {
                return None;
            }
            let mut offsets = [0u64; 8];
            for (i, o) in offsets.iter_mut().enumerate() {
                *o = u64::from(u32::from_le_bytes(head[8 + 4 * i..12 + 4 * i].try_into().ok()?));
            }
            Some(Psp::Pbp { offsets, size: source.size() })
        } else if has_extension(file_name, "iso") {
            Some(Psp::Iso(iso9660::layout_of(source)))
        } else {
            None
        }
    }

    /// Where `part` is and how long: `None` when the container has none.
    fn find<S: ByteSource + ?Sized>(&self, source: &mut S, part: Part) -> Option<(u64, u64)> {
        match self {
            Psp::Pbp { offsets, size } => {
                // A file runs to the next offset in the header; the same offset twice is an empty slot.
                let at = offsets[part.pbp_slot()];
                let end = offsets[part.pbp_slot() + 1..].iter().copied().find(|&o| o > at).unwrap_or(*size).min(*size);
                (at >= 40 && end > at).then_some((at, end - at))
            }
            Psp::Iso(layout) => iso9660::locate(source, *layout, part.iso_path()).map(|f| (0, f.size)),
        }
    }

    /// The first `max` bytes of `part` (all of it when it is shorter); `None` when missing or over `limit` bytes whole.
    fn read<S: ByteSource + ?Sized>(&self, source: &mut S, part: Part, max: usize, limit: usize) -> Option<Vec<u8>> {
        let (at, len) = self.find(source, part)?;
        if len > limit as u64 {
            return None;
        }
        let len = len.min(max as u64);
        match self {
            Psp::Pbp { .. } => fetch_opt(source, at, len).ok()?,
            Psp::Iso(layout) => iso9660::read_file(source, *layout, part.iso_path(), len),
        }
    }
}

/// The region of a PSP disc id (`ULUS10041`) or a PS3 title id (`BLUS30001`): the third letter, U for the US.
fn psp_region(disc_id: &str) -> Option<Region> {
    match disc_id.as_bytes().get(2)? {
        b'U' => Some(Region::NorthAmerica),
        b'E' => Some(Region::Europe),
        b'J' => Some(Region::Japan),
        b'K' => Some(Region::Korea),
        _ => None,
    }
}

fn single_title(name: &str, short: Option<&str>) -> Title {
    Title { name: name.into(), short_name: short.unwrap_or(name).into(), publisher: None }
}

/// A PSP game's info from an `EBOOT.PBP` or a PSP ISO (told by `file_name`'s extension). `None` when it is neither or has no
/// readable `PARAM.SFO`.
pub fn read_psp_info<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<GameInfo> {
    let psp = Psp::open(source, file_name)?;
    let params = sfo::parse(&psp.read(source, Part::Sfo, MAX_SFO, MAX_SFO)?)?;
    let mut info = GameInfo::new(Platform::Psp);
    if let Some(title) = &params.title {
        info.titles.push((Language::English, single_title(title, None)));
    }
    info.regions = params.disc_id.as_deref().and_then(psp_region).into_iter().collect();
    info.ids.product_code = params.disc_id;
    info.icon = psp.read(source, Part::Icon0, MAX_PICTURE, MAX_PICTURE).and_then(|b| png::decode(&b));
    if info.icon.is_some() {
        info.images.push(ImageKind::Icon);
    }
    if psp.read(source, Part::Pic1, 8, MAX_PICTURE).is_some_and(|b| b.starts_with(PNG_SIGNATURE)) {
        info.images.push(ImageKind::Background);
    }
    if psp.read(source, Part::Icon1, 4, MAX_MOVIE).is_some_and(|b| b == b"PSMF") {
        info.boot_movie = psp.find(source, Part::Icon1).map(|(_, len)| len);
    }
    Some(info)
}

/// A PSP game's picture: [`ImageKind::Icon`] (`ICON0.PNG`) or [`ImageKind::Background`] (`PIC1.PNG`); `None` when the file is
/// missing or not a PNG this reader decodes (and for any other kind).
pub fn psp_image<S: ByteSource + ?Sized>(source: &mut S, file_name: &str, kind: ImageKind) -> Option<RgbaImage> {
    let part = match kind {
        ImageKind::Icon => Part::Icon0,
        ImageKind::Background => Part::Pic1,
        _ => return None,
    };
    let psp = Psp::open(source, file_name)?;
    png::decode(&psp.read(source, part, MAX_PICTURE, MAX_PICTURE)?)
}

/// A PSP game's `ICON1.PMF` as the file stores it; `None` when there is none or it is not a PSMF.
pub fn psp_movie<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<Movie> {
    let psp = Psp::open(source, file_name)?;
    let bytes = psp.read(source, Part::Icon1, MAX_MOVIE, MAX_MOVIE)?;
    bytes.starts_with(b"PSMF").then_some(Movie { kind: MovieKind::Pmf, bytes })
}

// ---- PS3 and Vita

/// `TITLE_00` to `TITLE_11` in a PS3 `PARAM.SFO`, as the languages they name.
const PS3_LANGUAGES: [Language; 12] = [
    Language::Japanese,
    Language::English,
    Language::French,
    Language::Spanish,
    Language::German,
    Language::Italian,
    Language::Dutch,
    Language::Portuguese,
    Language::Russian,
    Language::Korean,
    Language::ChineseTraditional,
    Language::ChineseSimplified,
];

fn value<'a>(entries: &'a [(String, String)], key: &str) -> Option<&'a str> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// The first of `paths` that holds a readable SFO, and its entries.
fn first_sfo<F: TitleFiles + ?Sized>(
    files: &mut F,
    paths: &[&'static str],
) -> Option<(&'static str, Vec<(String, String)>)> {
    paths.iter().find_map(|&path| Some((path, sfo::strings(&files.read(path, MAX_SFO)?)?)))
}

/// A PS3 title from its files: `PS3_GAME/PARAM.SFO` (a disc folder) or `PARAM.SFO` (a game folder, installed or from a package),
/// with `ICON0.PNG` beside it. The title id (`BLUS30001`) gives the region; `TITLE_01` and the other language titles are read.
/// `None` when there is no readable SFO.
pub fn read_ps3_title<F: TitleFiles + ?Sized>(files: &mut F) -> Option<GameInfo> {
    let (path, entries) = first_sfo(files, &["PS3_GAME/PARAM.SFO", "PARAM.SFO"])?;
    let mut info = GameInfo::new(Platform::Ps3);
    let default = value(&entries, "TITLE");
    for (slot, language) in PS3_LANGUAGES.iter().enumerate() {
        let name =
            value(&entries, &alloc::format!("TITLE_{slot:02}")).or(default.filter(|_| *language == Language::English));
        if let Some(name) = name {
            info.titles.push((*language, single_title(name, None)));
        }
    }
    info.titles.sort_by_key(|(l, _)| *l);
    let id = value(&entries, "TITLE_ID").map(String::from);
    info.regions = id.as_deref().and_then(psp_region).into_iter().collect();
    info.ids.title_id = id;
    let folder = &path[..path.len() - "PARAM.SFO".len()];
    info.icon = files.read(&alloc::format!("{folder}ICON0.PNG"), MAX_PICTURE).and_then(|b| png::decode(&b));
    if info.icon.is_some() {
        info.images.push(ImageKind::Icon);
    }
    (!info.titles.is_empty() || info.ids.title_id.is_some()).then_some(info)
}

/// A Vita title's region from the fourth letter of its title id (`PCSE00001`).
fn vita_region(id: &str) -> Option<Region> {
    match id.as_bytes().get(3)? {
        b'A' | b'E' => Some(Region::NorthAmerica),
        b'B' | b'F' => Some(Region::Europe),
        b'C' | b'G' => Some(Region::Japan),
        _ => None,
    }
}

/// A Vita title from its files: `sce_sys/param.sfo` and `sce_sys/icon0.png`. `TITLE` is the name and `STITLE` the short one.
/// `None` when there is no readable SFO.
pub fn read_vita_title<F: TitleFiles + ?Sized>(files: &mut F) -> Option<GameInfo> {
    let (_, entries) = first_sfo(files, &["sce_sys/param.sfo", "sce_sys/PARAM.SFO"])?;
    let mut info = GameInfo::new(Platform::Vita);
    let name = value(&entries, "TITLE").or(value(&entries, "STITLE"));
    if let Some(name) = name {
        info.titles.push((Language::English, single_title(name, value(&entries, "STITLE"))));
    }
    let id = value(&entries, "TITLE_ID").map(String::from);
    info.regions = id.as_deref().and_then(vita_region).into_iter().collect();
    info.ids.title_id = id;
    let icon = files.read("sce_sys/icon0.png", MAX_PICTURE).or_else(|| files.read("sce_sys/ICON0.PNG", MAX_PICTURE));
    info.icon = icon.and_then(|b| png::decode(&b));
    if info.icon.is_some() {
        info.images.push(ImageKind::Icon);
    }
    (!info.titles.is_empty() || info.ids.title_id.is_some()).then_some(info)
}

/// The files of a zip, by path (case-insensitive), as [`TitleFiles`].
struct ZipFiles<'a, S: ?Sized> {
    source: &'a mut S,
    entries: Vec<Entry>,
}

impl<S: ByteSource + ?Sized> ZipFiles<'_, S> {
    fn entry(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| !e.is_dir() && e.name.eq_ignore_ascii_case(path))
    }
}

impl<S: ByteSource + ?Sized> TitleFiles for ZipFiles<'_, S> {
    fn size(&mut self, path: &str) -> Option<u64> {
        self.entry(path).map(|e| e.size)
    }

    fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>> {
        let entry = self.entry(path)?.clone();
        if entry.size > max as u64 {
            return None;
        }
        let mut out = Vec::new();
        zip::stream(self.source, &entry, |chunk| {
            out.extend_from_slice(chunk);
            if out.len() > max { crate::deflate::Flow::Stop } else { crate::deflate::Flow::Continue }
        })
        .ok()?;
        (out.len() as u64 == entry.size).then_some(out)
    }
}

/// How many central directory entries of a `.vpk` are looked through for `sce_sys`.
const VPK_ENTRIES: usize = 100_000;

/// A Vita title from a `.vpk`: its `sce_sys/param.sfo` and `icon0.png`, read in place. `None` when the file is not a zip or has no
/// SFO (a very large package whose directory outgrows what is read may not show its `sce_sys`).
pub fn read_vpk_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo> {
    let entries = zip::entries(source, VPK_ENTRIES).ok()??;
    read_vita_title(&mut ZipFiles { source, entries })
}
