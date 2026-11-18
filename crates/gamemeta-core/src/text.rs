// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Text rules matching the original reader (Kotlin/JVM) exactly, so titles compare byte for byte across the two.

use alloc::string::String;
use alloc::vec::Vec;

/// Kotlin's `Char.isWhitespace()` on the JVM: `Character.isWhitespace || Character.isSpaceChar`. Differs from Rust's
/// `char::is_whitespace` (Unicode `White_Space`) at U+001C..U+001F (whitespace here) and U+0085 (not here).
pub(crate) fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{09}'..='\u{0D}'
            | '\u{1C}'..='\u{20}'
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

pub(crate) fn trim(s: &str) -> &str {
    s.trim_matches(is_whitespace)
}

pub(crate) fn is_blank(s: &str) -> bool {
    s.chars().all(is_whitespace)
}

/// Kotlin's `lines()`: split on CRLF, LF or CR.
pub(crate) fn lines(s: &str) -> impl Iterator<Item = &str> {
    let mut rest = Some(s);
    core::iter::from_fn(move || {
        let s = rest?;
        match s.find(['\r', '\n']) {
            None => {
                rest = None;
                Some(s)
            }
            Some(i) => {
                let skip = if s[i..].starts_with("\r\n") { 2 } else { 1 };
                rest = Some(&s[i + skip..]);
                Some(&s[..i])
            }
        }
    })
}

/// The lines of `s`, each trimmed, empty ones dropped, joined with one space.
pub(crate) fn joined_lines(s: &str) -> String {
    let parts: Vec<&str> = lines(s).map(trim).filter(|l| !l.is_empty()).collect();
    parts.join(" ")
}

/// UTF-16LE up to the first NUL unit in `bytes` (an even-length window), unpaired surrogates as U+FFFD.
pub(crate) fn utf16le_until_nul(bytes: &[u8]) -> String {
    let units = bytes.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).take_while(|&u| u != 0);
    char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_matches_the_jvm_set_not_unicode_white_space() {
        assert!(is_whitespace('\u{3000}'));
        assert!(is_whitespace('\u{1F}'));
        assert!(is_whitespace('\u{A0}'));
        assert!(!is_whitespace('\u{85}'));
        assert!(!is_whitespace('a'));
        assert_eq!(trim("\u{3000} Title \u{1C}"), "Title");
    }

    #[test]
    fn lines_split_on_crlf_lf_and_cr() {
        let v: Vec<&str> = lines("a\r\nb\nc\rd").collect();
        assert_eq!(v, ["a", "b", "c", "d"]);
        let v: Vec<&str> = lines("x\n").collect();
        assert_eq!(v, ["x", ""]);
        assert_eq!(joined_lines(" Long \n\n Name \r"), "Long Name");
    }

    #[test]
    fn utf16_stops_at_nul_and_replaces_lone_surrogates() {
        assert_eq!(utf16le_until_nul(&[b'H', 0, b'i', 0, 0, 0, b'X', 0]), "Hi");
        assert_eq!(utf16le_until_nul(&[0x00, 0xD8, b'a', 0]), "\u{FFFD}a");
    }
}
