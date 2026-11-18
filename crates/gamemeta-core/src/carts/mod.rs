// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cartridge header readers: Game Boy / Color, Game Boy Advance, Nintendo 64, Super Nintendo, NES, Genesis / Mega
//! Drive (and 32X), Master System / Game Gear.
//!
//! One module per family, each with a typed `Header` and a pure `parse` over header bytes (no I/O, so it can be fuzzed
//! and tested on slices). [`read_info`] fetches the few bytes a family needs from a [`ByteSource`], never the whole
//! file; [`checksum_matches`] is the separate, opt-in call that streams a whole ROM to check a global checksum.
//!
//! Facts only, from the public format documents listed in `NOTICE.md`. A file whose header does not check out is
//! `None`: a title is never guessed.

use alloc::string::String;
use alloc::vec::Vec;

use crate::deflate::Flow;
use crate::source::fetch;
use crate::{ByteSource, GameInfo, ReadError, zip};

pub mod gb;
pub mod gba;
pub mod md;
pub mod n64;
pub mod nes;
pub mod sms;
pub mod snes;

/// A cartridge family: which header layout a file's extension asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Family {
    /// `.gb .gbc .sgb`
    GameBoy,
    /// `.gba .agb`
    Gba,
    /// `.z64 .v64 .n64`
    N64,
    /// `.sfc .smc .swc .fig`
    Snes,
    /// `.nes`
    Nes,
    /// `.md .gen .bin .smd .32x`
    MegaDrive,
    /// `.sms .gg`
    MasterSystem,
}

impl Family {
    /// Every family.
    pub const ALL: [Family; 7] =
        [Family::GameBoy, Family::Gba, Family::N64, Family::Snes, Family::Nes, Family::MegaDrive, Family::MasterSystem];

    /// The family's name for a list of what is supported.
    pub fn name(self) -> &'static str {
        match self {
            Family::GameBoy => "Game Boy, Game Boy Color",
            Family::Gba => "Game Boy Advance",
            Family::N64 => "Nintendo 64",
            Family::Snes => "Super Nintendo",
            Family::Nes => "NES",
            Family::MegaDrive => "Sega Genesis, 32X",
            Family::MasterSystem => "Master System, Game Gear",
        }
    }

    /// The extensions (lower case, no dot) that select this family.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Family::GameBoy => &["gb", "gbc", "sgb"],
            Family::Gba => &["gba", "agb"],
            Family::N64 => &["z64", "v64", "n64"],
            Family::Snes => &["sfc", "smc", "swc", "fig"],
            Family::Nes => &["nes"],
            Family::MegaDrive => &["md", "gen", "bin", "smd", "32x"],
            Family::MasterSystem => &["sms", "gg"],
        }
    }

    /// The family an extension (without the dot, any case) belongs to.
    pub fn for_extension(ext: &str) -> Option<Family> {
        Family::ALL.into_iter().find(|f| f.extensions().iter().any(|e| ext.eq_ignore_ascii_case(e)))
    }

    /// The family a file name's extension belongs to.
    pub fn for_file(file_name: &str) -> Option<Family> {
        Family::for_extension(file_name.rsplit_once('.').map(|(_, e)| e)?)
    }

    /// How many leading bytes of the file the reader may look at (a zip entry is decompressed this far only).
    fn prefix_len(self) -> u64 {
        match self {
            Family::GameBoy => 0x150,
            Family::Gba => 0xC0,
            Family::N64 => 0x40,
            Family::Snes => snes::MAX_PREFIX,
            Family::Nes => 16,
            Family::MegaDrive => md::MAX_PREFIX,
            Family::MasterSystem => 0x8000,
        }
    }
}

/// A typed header field value, for [`CartHeader::fields`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// Text.
    Text(String),
    /// A number (codes, sizes, checksums).
    Number(i64),
    /// A flag.
    Flag(bool),
    /// A field the header does not hold or that did not decode.
    Missing,
}

pub(crate) fn text(s: &str) -> Field {
    Field::Text(s.into())
}
pub(crate) fn opt_text(s: &Option<String>) -> Field {
    s.as_deref().map_or(Field::Missing, text)
}
pub(crate) fn num(n: impl Into<i64>) -> Field {
    Field::Number(n.into())
}
pub(crate) fn opt_num<N: Into<i64> + Copy>(n: Option<N>) -> Field {
    n.map_or(Field::Missing, num)
}
pub(crate) fn size_field(n: Option<u64>) -> Field {
    n.and_then(|n| i64::try_from(n).ok()).map_or(Field::Missing, Field::Number)
}

/// The header of one cartridge file, typed by family.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CartHeader {
    /// Game Boy / Game Boy Color.
    GameBoy(gb::Header),
    /// Game Boy Advance.
    Gba(gba::Header),
    /// Nintendo 64.
    N64(n64::Header),
    /// Super Nintendo.
    Snes(snes::Header),
    /// NES.
    Nes(nes::Header),
    /// Genesis / Mega Drive / 32X.
    MegaDrive(md::Header),
    /// Master System / Game Gear.
    MasterSystem(sms::Header),
}

impl CartHeader {
    /// A short stable name for the family (`"gb"`, `"gba"`, `"n64"`, `"snes"`, `"nes"`, `"md"`, `"sms"`).
    pub fn family(&self) -> &'static str {
        match self {
            CartHeader::GameBoy(_) => "gb",
            CartHeader::Gba(_) => "gba",
            CartHeader::N64(_) => "n64",
            CartHeader::Snes(_) => "snes",
            CartHeader::Nes(_) => "nes",
            CartHeader::MegaDrive(_) => "md",
            CartHeader::MasterSystem(_) => "sms",
        }
    }

    /// Every decoded field, in a fixed order, by name.
    pub fn fields(&self) -> Vec<(&'static str, Field)> {
        match self {
            CartHeader::GameBoy(h) => h.fields(),
            CartHeader::Gba(h) => h.fields(),
            CartHeader::N64(h) => h.fields(),
            CartHeader::Snes(h) => h.fields(),
            CartHeader::Nes(h) => h.fields(),
            CartHeader::MegaDrive(h) => h.fields(),
            CartHeader::MasterSystem(h) => h.fields(),
        }
    }

    /// The fields as one JSON object: `{"family":"gb","title":"...",...}`; a missing field is `null`.
    pub fn to_json(&self) -> String {
        use core::fmt::Write;
        let mut out = String::from("{\"family\":");
        json_string(&mut out, self.family());
        for (name, value) in self.fields() {
            let _ = write!(out, ",\"{name}\":");
            match value {
                Field::Text(t) => json_string(&mut out, &t),
                Field::Number(n) => {
                    let _ = write!(out, "{n}");
                }
                Field::Flag(b) => out.push_str(if b { "true" } else { "false" }),
                Field::Missing => out.push_str("null"),
            }
        }
        out.push('}');
        out
    }
}

fn json_string(out: &mut String, s: &str) {
    use core::fmt::Write;
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Printable ASCII up to the first NUL, other bytes dropped, trimmed: the text of a fixed-width header field.
pub(crate) fn ascii(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let s: String = bytes[..end].iter().filter(|b| (0x20..0x7F).contains(*b)).map(|&b| char::from(b)).collect();
    s.trim().into()
}

/// Shift_JIS text up to the first NUL (a Mega Drive domestic title, an N64 title), runs of spaces squeezed to one and
/// trimmed. Control characters are dropped.
pub(crate) fn sjis(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes[..end]);
    squeeze(&decoded)
}

/// Control characters dropped, runs of white space (ASCII, ideographic, any Unicode space) squeezed to one space, trimmed.
pub(crate) fn squeeze(s: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for c in s.chars().filter(|c| !c.is_control()) {
        if c.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// Up to `len` bytes at `at`; fewer at the end of the source, `None` when `at` is at or past it or a read failed.
pub(crate) fn head<S: ByteSource + ?Sized>(source: &mut S, at: u64, len: u64) -> Option<Vec<u8>> {
    let size = source.size();
    if at >= size {
        return None;
    }
    fetch(source, at, len.min(size - at)).ok()
}

fn extension(file_name: &str) -> &str {
    file_name.rsplit_once('.').map_or("", |(_, e)| e)
}

/// The Mega Drive header of `source`: a Super Magic Drive file's first block de-interleaved, else the plain image's.
fn md_header<S: ByteSource + ?Sized>(source: &mut S) -> Option<md::Header> {
    let size = source.size();
    let first = head(source, 0, md::HEADER_END as u64)?;
    if md::is_smd(&first, size) && size >= md::MAX_PREFIX {
        let block = fetch(source, md::SMD_HEADER, md::SMD_BLOCK).ok()?;
        return md::parse(&md::deinterleave(&block), md::Layout::Smd);
    }
    md::parse(&first, md::Layout::Plain)
}

/// The Master System header of `source`: the first of the three offsets that holds `TMR SEGA`.
fn sms_header<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<sms::Header> {
    let size = source.size();
    let gg = extension(file_name).eq_ignore_ascii_case("gg");
    sms::OFFSETS.iter().filter(|&&at| at + sms::HEADER_LEN as u64 <= size).find_map(|&at| {
        let bytes = fetch(source, at, sms::HEADER_LEN as u64).ok()?;
        sms::parse(&bytes, at, gg)
    })
}

/// The header of the cartridge `source` holds, read as `family`, and what it says about the game. `file_name` only
/// decides Master System versus Game Gear when the header does not. Reads the few header bytes the family needs, never
/// the whole file. `None` when the header does not check out.
pub fn read_info<S: ByteSource + ?Sized>(source: &mut S, family: Family, file_name: &str) -> Option<GameInfo> {
    Some(match family {
        Family::GameBoy => gb::parse(&head(source, 0, gb::HEADER_END as u64)?)?.info(),
        Family::Gba => gba::parse(&head(source, 0, gba::HEADER_END as u64)?)?.info(),
        Family::N64 => n64::parse(&head(source, 0, n64::HEADER_END as u64)?)?.info(),
        Family::Nes => nes::parse(&head(source, 0, nes::HEADER_END as u64)?)?.info(),
        Family::Snes => {
            let size = source.size();
            let copier = snes::copier_header(size);
            let found = snes::best(snes::windows(size).into_iter().filter_map(|(layout, start)| {
                let window = fetch(source, start, snes::WINDOW as u64).ok()?;
                snes::parse_window(&window, layout, copier)
            }));
            found?.info()
        }
        Family::MegaDrive => md_header(source)?.info(),
        Family::MasterSystem => sms_header(source, file_name)?.info(),
    })
}

/// A zip entry's leading bytes standing in for the whole file: the declared size, the bytes decompressed so far.
struct Prefix {
    data: Vec<u8>,
    size: u64,
}

impl ByteSource for Prefix {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError> {
        let start = usize::try_from(offset).map_err(|_| ReadError::OutOfRange)?;
        let end = start.checked_add(buf.len()).ok_or(ReadError::OutOfRange)?;
        buf.copy_from_slice(self.data.get(start..end).ok_or(ReadError::OutOfRange)?);
        Ok(())
    }
}

const MAX_ZIP_ENTRIES: usize = 256;

/// The same for a zip holding a cartridge ROM: the first entry with a cartridge extension is decompressed as far as
/// its family's header reaches and read.
pub fn read_zip_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo> {
    let entries = zip::entries(source, MAX_ZIP_ENTRIES).ok()??;
    let (entry, family) =
        entries.iter().filter(|e| !e.is_dir()).find_map(|e| Family::for_file(&e.name).map(|f| (e, f)))?;
    let want = family.prefix_len().min(entry.size);
    let mut data = Vec::new();
    zip::stream(source, entry, |chunk| {
        let room = usize::try_from(want).unwrap_or(usize::MAX).saturating_sub(data.len());
        data.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if data.len() as u64 >= want { Flow::Stop } else { Flow::Continue }
    })
    .ok()?;
    read_info(&mut Prefix { data, size: entry.size }, family, &entry.name)
}

const CHUNK: u64 = 64 * 1024;

/// Calls `each(offset, bytes)` for consecutive pieces of the first `len` bytes of `source`, in order.
fn stream<S: ByteSource + ?Sized>(
    source: &mut S,
    mut at: u64,
    len: u64,
    mut each: impl FnMut(u64, &[u8]),
) -> Option<()> {
    let end = at.checked_add(len)?;
    let start = at;
    while at < end {
        let chunk = fetch(source, at, (end - at).min(CHUNK)).ok()?;
        each(at - start, &chunk);
        at += chunk.len() as u64;
    }
    Some(())
}

fn gb_checksum<S: ByteSource + ?Sized>(source: &mut S) -> Option<bool> {
    let header = gb::parse(&head(source, 0, gb::HEADER_END as u64)?)?;
    let mut sum = 0u16;
    stream(source, 0, source.size(), |at, bytes| {
        for (i, &b) in bytes.iter().enumerate() {
            let pos = at + i as u64;
            if pos != 0x14E && pos != 0x14F {
                sum = sum.wrapping_add(u16::from(b));
            }
        }
    })?;
    Some(sum == header.global_checksum)
}

fn md_checksum<S: ByteSource + ?Sized>(source: &mut S) -> Option<bool> {
    let header = md_header(source)?;
    let smd = header.layout == md::Layout::Smd;
    let data_len = source.size() - if smd { md::SMD_HEADER } else { 0 };
    let end = (u64::from(header.rom_end) + 1).min(data_len);
    if end <= md::HEADER_END as u64 {
        return None;
    }
    let mut sum = 0u16;
    let mut add = |logical: u64, bytes: &[u8]| {
        for (i, pair) in bytes.chunks_exact(2).enumerate() {
            let pos = logical + 2 * i as u64;
            if pos >= md::HEADER_END as u64 && pos + 2 <= end {
                sum = sum.wrapping_add(u16::from_be_bytes([pair[0], pair[1]]));
            }
        }
    };
    if smd {
        let mut block_at = 0;
        while block_at < end {
            let block = fetch(source, md::SMD_HEADER + block_at, md::SMD_BLOCK.min(data_len - block_at)).ok()?;
            add(block_at, &md::deinterleave(&block));
            block_at += md::SMD_BLOCK;
        }
    } else {
        stream(source, 0, end, |at, bytes| add(at, bytes))?;
    }
    Some(sum == header.checksum)
}

fn sms_checksum<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<bool> {
    let header = sms_header(source, file_name)?;
    let end = header.rom_size?.min(source.size());
    let mut sum = 0u16;
    stream(source, 0, end, |at, bytes| {
        for (i, &b) in bytes.iter().enumerate() {
            let pos = at + i as u64;
            if pos < header.offset || pos >= header.offset + sms::HEADER_LEN as u64 {
                sum = sum.wrapping_add(u16::from(b));
            }
        }
    })?;
    Some(sum == header.checksum)
}

/// Streams the whole ROM and checks the global checksum its header holds: Game Boy (the sum of every byte but the two
/// checksum bytes), Mega Drive (the word sum from 0x200 to the header's ROM end, through an SMD interleave too), Master
/// System / Game Gear (the byte sum around the header, over the size the header names). `None` for a family with no such
/// check here (GBA, N64, NES, and SNES, whose complement pair is [`snes::Header::checksum_valid`]), a header that does not
/// read, or a source that fails. Not for zips.
pub fn checksum_matches<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<bool> {
    match Family::for_file(file_name)? {
        Family::GameBoy => gb_checksum(source),
        Family::MegaDrive => md_checksum(source),
        Family::MasterSystem => sms_checksum(source, file_name),
        Family::Gba | Family::N64 | Family::Snes | Family::Nes => None,
    }
}
