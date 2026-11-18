// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! PlayStation 1 and 2 serials (`SLUS00001`) from `SYSTEM.CNF`, in a disc image or in the first data track of a cue sheet.

use alloc::string::String;

use super::iso9660::{self, Layout};
use super::{Opener, get, latin1};
use crate::ByteSource;
use crate::text;

const CNF_LIMIT: u64 = 64 * 1024;

/// The serial of the disc image in `source`, the layout told by its first bytes.
pub(crate) fn serial<S: ByteSource + ?Sized>(source: &mut S) -> Option<String> {
    let layout = iso9660::layout_of(source);
    serial_in(source, layout)
}

fn serial_in<S: ByteSource + ?Sized>(source: &mut S, layout: Layout) -> Option<String> {
    let text = latin1(&iso9660::read_file(source, layout, "SYSTEM.CNF", CNF_LIMIT)?);
    let boot = text::lines(&text).map(text::trim).find(|l| starts_with_ignore_case(l, "BOOT"))?;
    // `BOOT2 = cdrom0:\SLES_000.02;1`: what follows the `=`, after the last `\` and then the last `:`.
    let value = boot.split_once('=').map_or("", |(_, v)| v);
    let value = value.rsplit_once('\\').map_or(value, |(_, v)| v);
    let value = value.rsplit_once(':').map_or(value, |(_, v)| v);
    find_serial(text::trim(value))
}

fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.is_char_boundary(prefix.len()) && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// The first `LLLL[_-]DDD[.]DD` in `text` (four letters, three digits, two digits), joined and upper-cased.
fn find_serial(text: &str) -> Option<String> {
    let b = text.as_bytes();
    for start in 0..b.len() {
        let mut at = start;
        let letters = b.get(at..at + 4)?;
        if !letters.iter().all(u8::is_ascii_alphabetic) {
            continue;
        }
        at += 4;
        if matches!(b.get(at), Some(b'_' | b'-')) {
            at += 1;
        }
        let Some(three) = b.get(at..at + 3).filter(|d| d.iter().all(u8::is_ascii_digit)) else { continue };
        at += 3;
        if b.get(at) == Some(&b'.') {
            at += 1;
        }
        let Some(two) = b.get(at..at + 2).filter(|d| d.iter().all(u8::is_ascii_digit)) else { continue };
        let mut out = String::new();
        for part in [letters, three, two] {
            out.extend(part.iter().map(|&c| char::from(c.to_ascii_uppercase())));
        }
        return Some(out);
    }
    None
}

/// The serial of the first data track of a cue sheet, read from the track file `opener` opens by the name the sheet gives.
pub(crate) fn serial_from_cue<S: ByteSource + ?Sized>(cue: &mut S, opener: &mut dyn Opener) -> Option<String> {
    let size = cue.size().min(CNF_LIMIT);
    let text = latin1(&get(cue, 0, size)?);
    let mut file: Option<String> = None;
    for line in text::lines(&text).map(text::trim) {
        let upper = line.to_uppercase();
        if upper.starts_with("FILE") {
            file = Some(quoted(line).map_or_else(
                || {
                    let after = line.split_once(' ').map_or("", |(_, r)| r);
                    text::trim(after.rsplit_once(' ').map_or(after, |(head, _)| head)).into()
                },
                String::from,
            ));
        } else if upper.starts_with("TRACK") {
            let mode = upper.split([' ', '\t', '\n', '\u{B}', '\u{C}', '\r']).filter(|t| !t.is_empty()).nth(2);
            let Some(mode) = mode else { continue };
            if mode == "AUDIO" {
                continue;
            }
            let mut track = opener.open(file.as_deref()?)?;
            let layout = match mode {
                "MODE1/2352" => Layout::Raw { head: 16 },
                "MODE2/2352" => Layout::Raw { head: 24 },
                _ => Layout::Plain,
            };
            return serial_in(&mut *track, layout);
        }
    }
    None
}

/// The text between the first pair of double quotes of `line`.
fn quoted(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once('"')?;
    Some(rest.split_once('"')?.0)
}
