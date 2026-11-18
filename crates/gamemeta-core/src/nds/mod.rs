// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Nintendo DS and DSi: the header and banner block of a `.nds`/`.dsi`/`.srl`/`.ids` ROM, or of the first ROM in a zip.

use alloc::string::String;
use alloc::vec::Vec;

use crate::deflate::{Failure, Flow};
use crate::le::{u16_at, u32_at};
use crate::source::fetch;
use crate::text::{is_blank, lines, trim, utf16le_until_nul};
use crate::{ByteSource, GameIds, GameInfo, IconStep, Language, Platform, ReadError, Region, RgbaImage, Title};

/// Size of the DS header the readers look at.
pub const HEADER_SIZE: usize = 0x200;
/// The largest banner block (version 0x0103, up to the end of the animation sequence).
pub const MAX_BANNER: usize = 0x23C0;
/// A zip entry is read forward only; a banner further in than this (decompressed) is not tried.
pub const MAX_ZIP_SKIP: u64 = 64 << 20;

/// What a DS banner block holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    /// The banner version (1, 2, 3, or 0x0103 for DSi).
    pub version: u16,
    /// Titles, language order JP, EN, FR, DE, IT, ES, then Chinese and Korean where the version has them.
    pub titles: Vec<String>,
    /// The 32x32 icon.
    pub icon: RgbaImage,
    /// A DSi animated icon's steps; empty for a still icon.
    pub animation: Vec<IconStep>,
}

impl Banner {
    /// More than one animation step.
    pub fn animated(&self) -> bool {
        self.animation.len() > 1
    }

    /// The raw text of the English slot if it is not blank, else the first non-blank slot's.
    fn preferred(&self) -> &str {
        self.titles
            .get(EN_SLOT)
            .filter(|t| !is_blank(t))
            .or_else(|| self.titles.iter().find(|t| !is_blank(t)))
            .map_or("", |t| t.as_str())
    }

    /// The English title (second slot), else the first non-empty one; lines joined with a space.
    pub fn title(&self) -> String {
        self.title_lines().join(" ")
    }

    /// The title's lines (name, subtitle, publisher), trimmed, blank ones dropped.
    pub fn title_lines(&self) -> Vec<String> {
        lines(self.preferred()).map(trim).filter(|l| !l.is_empty()).map(String::from).collect()
    }

    /// The name a card shows: the title's lines without the last one (the publisher) when there are two or more.
    pub fn display_name(&self) -> String {
        let l = self.title_lines();
        let keep = if l.len() >= 2 { l.len() - 1 } else { l.len() };
        l[..keep].join(" ")
    }
}

const EN_SLOT: usize = 1;
const ICON_SIZE: usize = 32;
const BANNER_OFFSET_AT: usize = 0x68;
const ICON_AT: usize = 0x20;
const PALETTE_AT: usize = 0x220;
const TITLES_AT: usize = 0x240;
const TITLE_BYTES: usize = 0x100;
const ANIM_BITMAPS_AT: usize = 0x1240;
const ANIM_PALETTES_AT: usize = 0x2240;
const ANIM_SEQUENCE_AT: usize = 0x2340;
const SEQUENCE_TOKENS: usize = 64;
const BITMAP_BYTES: usize = 0x200;
const PALETTE_BYTES: usize = 0x20;
const MAX_ZIP_ENTRIES: usize = 64;

/// The outcome of reading a banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    /// `raw` is the banner block as read (at most [`MAX_BANNER`] bytes).
    Found {
        /// The parsed banner.
        banner: Banner,
        /// The block as stored.
        raw: Vec<u8>,
    },
    /// No banner, and why.
    NoBanner(&'static str),
}

/// CRC-16 as the firmware computes it: modbus, initial value 0xFFFF, over `from..min(to, len)`.
pub fn crc16(bytes: &[u8], from: usize, to: usize) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in bytes.get(from..to.min(bytes.len())).unwrap_or(&[]) {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

fn extension(file_name: &str) -> &str {
    file_name.rsplit_once('.').map_or("", |(_, e)| e)
}

/// The extensions of a DS ROM.
pub(crate) const ROM_EXTENSIONS: [&str; 4] = ["nds", "dsi", "srl", "ids"];

/// Whether `file_name` is a DS ROM (`nds`, `dsi`, `srl`, `ids`), case-insensitively.
pub fn is_rom(file_name: &str) -> bool {
    ROM_EXTENSIONS.iter().any(|e| extension(file_name).eq_ignore_ascii_case(e))
}

/// Whether `file_name` is a zip.
pub fn is_zip(file_name: &str) -> bool {
    extension(file_name).eq_ignore_ascii_case("zip")
}

/// A ROM or a zip.
pub fn is_candidate(file_name: &str) -> bool {
    is_rom(file_name) || is_zip(file_name)
}

/// A `.7z`: no 7z reader ships, so it has no banner.
pub fn is_unsupported_archive(file_name: &str) -> bool {
    extension(file_name).eq_ignore_ascii_case("7z")
}

/// The extent a version's banner needs.
fn required_length(version: u16) -> Option<usize> {
    match version {
        0x0001 => Some(0x840),
        0x0002 => Some(0x940),
        0x0003 => Some(0xA40),
        0x0103 => Some(MAX_BANNER),
        _ => None,
    }
}

fn banner_offset(header: &[u8], file_size: u64) -> Option<u64> {
    let offset = u64::from(u32_at(header, BANNER_OFFSET_AT)?);
    (offset >= HEADER_SIZE as u64 && offset < file_size).then_some(offset)
}

/// Reads the banner block of a ROM: the header's banner offset, then up to [`MAX_BANNER`] bytes.
pub fn read_banner<S: ByteSource + ?Sized>(source: &mut S) -> Read {
    match read_rom(source) {
        Ok((_, read)) => read,
        Err(ReadError::OutOfRange) => Read::NoBanner("short file"),
        Err(ReadError::Io) => Read::NoBanner("unreadable"),
    }
}

/// The header and the banner of a ROM. An unreadable banner is a [`Read::NoBanner`], not an error; only the header
/// read can fail.
fn read_rom<S: ByteSource + ?Sized>(source: &mut S) -> Result<(Vec<u8>, Read), ReadError> {
    let header = fetch(source, 0, HEADER_SIZE as u64)?;
    let size = source.size();
    let Some(offset) = banner_offset(&header, size) else { return Ok((header, Read::NoBanner("no banner offset"))) };
    let length = (MAX_BANNER as u64).min(size - offset);
    let read = match fetch(source, offset, length) {
        Ok(bytes) => parse_banner(&bytes),
        Err(ReadError::OutOfRange) => Read::NoBanner("banner outside file"),
        Err(ReadError::Io) => Read::NoBanner("unreadable"),
    };
    Ok((header, read))
}

/// Parses a banner block (`bytes` is the block as far as the file goes, at most [`MAX_BANNER`]).
pub fn parse_banner(bytes: &[u8]) -> Read {
    if bytes.len() < 0x242 {
        return Read::NoBanner("short banner");
    }
    let Some(version) = u16_at(bytes, 0) else { return Read::NoBanner("short banner") };
    let Some(need) = required_length(version) else { return Read::NoBanner("unknown banner version") };
    if bytes.len() < need {
        return Read::NoBanner("short banner");
    }
    // The firmware checks these CRCs; a block that fails the base one is garbage (an encrypted or damaged file).
    if Some(crc16(bytes, ICON_AT, 0x840)) != u16_at(bytes, 2) {
        return Read::NoBanner("bad banner checksum");
    }
    let crc_ok = |at: usize, end: usize| Some(crc16(bytes, ICON_AT, end)) == u16_at(bytes, at);
    let title_count = if version >= 0x0003 && crc_ok(6, 0xA40) {
        8
    } else if version >= 0x0002 && crc_ok(4, 0x940) {
        7
    } else {
        6
    };
    let titles = (0..title_count).map(|i| decode_title(bytes, TITLES_AT + i * TITLE_BYTES)).collect();
    let icon = decode_icon(bytes, ICON_AT, &read_palette(bytes, PALETTE_AT));
    let animated = version == 0x0103 && u16_at(bytes, 8) == Some(crc16(bytes, ANIM_BITMAPS_AT, MAX_BANNER));
    let animation = if animated { decode_animation(bytes) } else { Vec::new() };
    Read::Found { banner: Banner { version, titles, icon, animation }, raw: bytes[..need].to_vec() }
}

fn decode_animation(bytes: &[u8]) -> Vec<IconStep> {
    let palettes: Vec<[u32; 16]> = (0..8).map(|i| read_palette(bytes, ANIM_PALETTES_AT + i * PALETTE_BYTES)).collect();
    let mut steps = Vec::new();
    for i in 0..SEQUENCE_TOKENS {
        let token = u16_at(bytes, ANIM_SEQUENCE_AT + i * 2).unwrap_or(0);
        let duration = u32::from(token & 0xFF);
        // 0 ends the sequence; a still icon is stored as one token and a zero-duration one.
        if duration == 0 {
            break;
        }
        let bitmap = usize::from((token >> 8) & 7);
        let pal = usize::from((token >> 11) & 7);
        let image = decode_icon(bytes, ANIM_BITMAPS_AT + bitmap * BITMAP_BYTES, &palettes[pal]);
        steps
            .push(IconStep { frame: flip(image, token & 0x4000 != 0, token & 0x8000 != 0), duration_frames: duration });
    }
    steps
}

fn flip(image: RgbaImage, horizontal: bool, vertical: bool) -> RgbaImage {
    if !horizontal && !vertical {
        return image;
    }
    let mut out = alloc::vec![0u8; image.pixels.len()];
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let sx = if horizontal { ICON_SIZE - 1 - x } else { x };
            let sy = if vertical { ICON_SIZE - 1 - y } else { y };
            let (s, o) = ((sy * ICON_SIZE + sx) * 4, (y * ICON_SIZE + x) * 4);
            out[o..o + 4].copy_from_slice(&image.pixels[s..s + 4]);
        }
    }
    RgbaImage { width: image.width, height: image.height, pixels: out }
}

/// 16 palette entries as 0xAARRGGBB; entry 0 is transparent.
fn read_palette(bytes: &[u8], at: usize) -> [u32; 16] {
    let expand5 = |v: u32| (v << 3) | (v >> 2);
    let mut p = [0u32; 16];
    for (i, slot) in p.iter_mut().enumerate().skip(1) {
        let c = u32::from(u16_at(bytes, at + i * 2).unwrap_or(0));
        *slot = 0xFF00_0000 | (expand5(c & 31) << 16) | (expand5((c >> 5) & 31) << 8) | expand5((c >> 10) & 31);
    }
    p
}

/// 4x4 tiles of 8x8 pixels, row-major, 4 bits a pixel, the low nibble first (left).
fn decode_icon(bytes: &[u8], at: usize, palette: &[u32; 16]) -> RgbaImage {
    let mut out = alloc::vec![0u8; ICON_SIZE * ICON_SIZE * 4];
    for ty in 0..4 {
        for tx in 0..4 {
            for row in 0..8 {
                for col in 0..8 {
                    let byte = bytes.get(at + (ty * 4 + tx) * 32 + row * 4 + col / 2).copied().unwrap_or(0);
                    let index = if col % 2 == 0 { byte & 15 } else { byte >> 4 };
                    let argb = palette[usize::from(index)];
                    let o = ((ty * 8 + row) * ICON_SIZE + tx * 8 + col) * 4;
                    out[o..o + 4].copy_from_slice(&[
                        (argb >> 16) as u8,
                        (argb >> 8) as u8,
                        argb as u8,
                        (argb >> 24) as u8,
                    ]);
                }
            }
        }
    }
    RgbaImage { width: ICON_SIZE as u32, height: ICON_SIZE as u32, pixels: out }
}

fn decode_title(bytes: &[u8], at: usize) -> String {
    let window = bytes.get(at..at + TITLE_BYTES).unwrap_or(&[]);
    String::from(trim(&utf16le_until_nul(window)))
}

/// What the stream walk of a zip entry found.
enum Walk {
    /// Fewer than [`HEADER_SIZE`] bytes in the entry.
    ShortHeader,
    Rom {
        header: Vec<u8>,
        banner: Read,
    },
}

/// A stored entry is addressable: read its header, then the banner where the header says, nothing between.
fn walk_stored<S: ByteSource + ?Sized>(source: &mut S, entry: &crate::zip::Entry) -> Result<Walk, Failure> {
    if entry.size < HEADER_SIZE as u64 {
        return Ok(Walk::ShortHeader);
    }
    let mut header = alloc::vec![0u8; HEADER_SIZE];
    crate::zip::read_stored(source, entry, 0, &mut header)?;
    let banner = match banner_offset(&header, u64::MAX) {
        None => Read::NoBanner("no banner offset"),
        Some(at) if at > entry.size => Read::NoBanner("banner too far in"),
        Some(at) => {
            let len = (MAX_BANNER as u64).min(entry.size - at) as usize;
            let mut block = alloc::vec![0u8; len];
            crate::zip::read_stored(source, entry, at, &mut block)?;
            parse_banner(&block)
        }
    };
    Ok(Walk::Rom { header, banner })
}

/// Walks the decompressed bytes of the first ROM entry: the header, then forward to the banner (never holding more
/// than the header and the banner).
fn walk_entry<S: ByteSource + ?Sized>(source: &mut S, entry: &crate::zip::Entry) -> Result<Walk, Failure> {
    let mut header: Vec<u8> = Vec::with_capacity(HEADER_SIZE);
    let mut banner_at: Option<u64> = None;
    let mut block: Vec<u8> = Vec::new();
    let mut verdict: Option<Read> = None;
    let mut pos = 0u64;
    if entry.is_stored() {
        return walk_stored(source, entry);
    }
    crate::zip::stream(source, entry, |chunk| {
        let mut chunk = chunk;
        if header.len() < HEADER_SIZE {
            let take = (HEADER_SIZE - header.len()).min(chunk.len());
            header.extend_from_slice(&chunk[..take]);
            pos += take as u64;
            chunk = &chunk[take..];
            if header.len() < HEADER_SIZE {
                return Flow::Continue;
            }
            match banner_offset(&header, u64::MAX) {
                None => {
                    verdict = Some(Read::NoBanner("no banner offset"));
                    return Flow::Stop;
                }
                Some(at) if at - HEADER_SIZE as u64 > MAX_ZIP_SKIP => {
                    verdict = Some(Read::NoBanner("banner too far in"));
                    return Flow::Stop;
                }
                Some(at) => banner_at = Some(at),
            }
        }
        let Some(at) = banner_at else { return Flow::Continue };
        let skip = at.saturating_sub(pos).min(chunk.len() as u64) as usize;
        pos += skip as u64;
        chunk = &chunk[skip..];
        if pos < at {
            return Flow::Continue;
        }
        let take = (MAX_BANNER - block.len()).min(chunk.len());
        block.extend_from_slice(&chunk[..take]);
        pos += take as u64;
        if block.len() == MAX_BANNER { Flow::Stop } else { Flow::Continue }
    })?;
    if header.len() < HEADER_SIZE {
        return Ok(Walk::ShortHeader);
    }
    let banner = match verdict {
        Some(v) => v,
        None if pos < banner_at.unwrap_or(0) => Read::NoBanner("banner too far in"),
        None => parse_banner(&block),
    };
    Ok(Walk::Rom { header, banner })
}

/// A DS ROM inside a zip, as the zip's directory describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipRom {
    /// The entry's path inside the zip.
    pub name: String,
    /// CRC-32 of the decompressed ROM.
    pub crc: u32,
    /// Decompressed size in bytes.
    pub size: u64,
}

/// Every DS ROM in a zip (the first [`MAX_ZIP_ENTRIES`] entries), in directory order. Empty for anything that is not a zip.
pub fn zip_roms<S: ByteSource + ?Sized>(source: &mut S) -> Vec<ZipRom> {
    let entries = crate::zip::entries(source, MAX_ZIP_ENTRIES).ok().flatten().unwrap_or_default();
    entries
        .into_iter()
        .filter(|e| !e.is_dir() && is_rom(&e.name))
        .map(|e| ZipRom { name: e.name.clone(), crc: e.crc, size: e.size })
        .collect()
}

/// The DS ROM entry of a zip called `name` (the first ROM when `None`) and what streaming it gave. `Ok(None)`: no such
/// ROM (or no zip at all).
fn read_zip<S: ByteSource + ?Sized>(source: &mut S, name: Option<&str>) -> Result<Option<Walk>, Failure> {
    let entries = crate::zip::entries(source, MAX_ZIP_ENTRIES).map_err(|_| Failure::Source)?;
    let Some(entry) =
        entries.into_iter().flatten().find(|e| !e.is_dir() && is_rom(&e.name) && name.is_none_or(|n| e.name == n))
    else {
        return Ok(None);
    };
    walk_entry(source, &entry).map(Some)
}

/// Reads the banner of the first DS ROM in a zip, streamed: its header and banner are read and nothing else is kept.
pub fn read_zip_banner<S: ByteSource + ?Sized>(source: &mut S) -> Read {
    match read_zip(source, None) {
        Ok(None) => Read::NoBanner("no DS file in the zip"),
        Ok(Some(Walk::ShortHeader)) => Read::NoBanner("short file"),
        Ok(Some(Walk::Rom { banner, .. })) => banner,
        Err(Failure::Source) => Read::NoBanner("unreadable zip"),
        Err(Failure::Corrupt) => Read::NoBanner("damaged zip"),
    }
}

/// A 4-character (or 2) ASCII code: letters and digits only, else `None`.
fn ascii_code(b: &[u8], at: usize, len: usize) -> Option<String> {
    let s = b.get(at..at + len)?;
    s.iter().all(u8::is_ascii_alphanumeric).then(|| s.iter().map(|&c| c as char).collect())
}

/// Titles, icon, ids and region of a DS ROM. `None` when the header cannot be read, or there is neither a banner nor
/// a game code.
pub fn read_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo> {
    let (header, read) = read_rom(source).ok()?;
    info(&header, read)
}

/// The same for the first DS ROM in a zip.
pub fn read_zip_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo> {
    read_zip_info_of_entry(source, None)
}

/// The same for the ROM called `name` inside the zip (one of [`zip_roms`]).
pub fn read_zip_info_of<S: ByteSource + ?Sized>(source: &mut S, name: &str) -> Option<GameInfo> {
    read_zip_info_of_entry(source, Some(name))
}

fn read_zip_info_of_entry<S: ByteSource + ?Sized>(source: &mut S, name: Option<&str>) -> Option<GameInfo> {
    match read_zip(source, name) {
        Ok(Some(Walk::Rom { header, banner })) => info(&header, banner),
        _ => None,
    }
}

fn info(header: &[u8], read: Read) -> Option<GameInfo> {
    let (banner, raw_len) = match read {
        Read::Found { banner, raw } => (Some(banner), raw.len() as u64),
        Read::NoBanner(_) => (None, 0),
    };
    let game_code = ascii_code(header, 0x0C, 4);
    if banner.is_none() && game_code.is_none() {
        return None;
    }
    let region = header.get(0x1D).copied().unwrap_or(0);
    let mut info = GameInfo::new(if header.get(0x12) == Some(&3) { Platform::Dsi } else { Platform::Ds });
    info.ids = GameIds { game_id: game_code, maker_code: ascii_code(header, 0x10, 2), ..GameIds::default() };
    info.regions = if region & 0x80 != 0 {
        alloc::vec![Region::China]
    } else if region & 0x40 != 0 {
        alloc::vec![Region::Korea]
    } else {
        Vec::new()
    };
    if let Some(b) = banner {
        info.banner = Some(crate::Extent { offset: u64::from(u32_at(header, BANNER_OFFSET_AT)?), size: raw_len });
        for (i, raw) in b.titles.iter().enumerate() {
            let l: Vec<&str> = lines(raw).map(trim).filter(|l| !l.is_empty()).collect();
            let (Some(first), Some(language)) = (l.first(), Language::from_slot(i)) else { continue };
            let publisher = (l.len() >= 2).then(|| String::from(l[l.len() - 1]));
            let name_lines = if l.len() >= 2 { &l[..l.len() - 1] } else { &l[..] };
            info.titles
                .push((language, Title { name: name_lines.join(" "), short_name: String::from(*first), publisher }));
        }
        let animated = b.animated();
        info.icon = Some(b.icon);
        if animated {
            info.animated_icon = b.animation;
        }
    }
    Some(info)
}
