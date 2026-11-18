// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The leaf `<tag ...>text</tag>` pairs (text with no child tags) of a Wii U `meta/meta.xml`. The file is machine
//! generated and flat, so this is a small scanner, not an XML parser: no DTDs or external entities are looked at.
//! The first value of a tag wins; empty tags are dropped; text has its CDATA and entities decoded and its lines
//! joined by single spaces. A work budget keeps hostile input (thousands of unclosed tags) linear.

use alloc::string::String;
use alloc::vec::Vec;

use crate::text;

const CDATA_OPEN: &str = "<![CDATA[";
const CDATA_CLOSE: &str = "]]>";

/// Java's `\s`.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

fn find(hay: &str, from: usize, needle: &str, budget: &mut usize) -> Option<usize> {
    let rest = hay.get(from..)?;
    let at = rest.find(needle);
    *budget = budget.checked_sub(at.unwrap_or(rest.len()).max(1))?;
    at.map(|a| from + a)
}

struct Pair<'a> {
    name: &'a str,
    raw: &'a str,
    end: usize,
}

/// The pair starting at `start` (a `<`); `Ok(None)` when none starts there, `Err` when the budget ran out.
fn pair<'a>(xml: &'a str, start: usize, budget: &mut usize) -> Result<Option<Pair<'a>>, ()> {
    let b = xml.as_bytes();
    let name_start = start + 1;
    let mut p = name_start;
    while b.get(p).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
        p += 1;
    }
    if p == name_start {
        return Ok(None);
    }
    let name = &xml[name_start..p];
    if b.get(p).copied().is_some_and(is_space) {
        match find(xml, p, ">", budget) {
            Some(gt) => p = gt,
            None if *budget == 0 => return Err(()),
            None => return Ok(None),
        }
    }
    if b.get(p) != Some(&b'>') {
        return Ok(None);
    }
    let content = p + 1;
    let mut q = content;
    loop {
        let Some(lt) = find(xml, q, "<", budget) else {
            return if *budget == 0 { Err(()) } else { Ok(None) };
        };
        if xml[lt..].starts_with(CDATA_OPEN) {
            match find(xml, lt + CDATA_OPEN.len(), CDATA_CLOSE, budget) {
                Some(close) => q = close + CDATA_CLOSE.len(),
                None if *budget == 0 => return Err(()),
                None => return Ok(None),
            }
            continue;
        }
        let Some(after) = xml[lt..].strip_prefix("</").and_then(|s| s.strip_prefix(name)) else {
            return Ok(None);
        };
        let spaces = after.bytes().take_while(|c| is_space(*c)).count();
        if after.as_bytes().get(spaces) != Some(&b'>') {
            return Ok(None);
        }
        let end = lt + 2 + name.len() + spaces + 1;
        return Ok(Some(Pair { name, raw: &xml[content..lt], end }));
    }
}

/// Every leaf tag's decoded text, first occurrence of a name only, empty values dropped.
pub(crate) fn parse(xml: &str) -> Vec<(&str, String)> {
    let mut out: Vec<(&str, String)> = Vec::new();
    let mut budget = xml.len().saturating_mul(64).max(4096);
    let mut i = 0;
    while let Some(rel) = xml.get(i..).and_then(|s| s.find('<')) {
        let start = i + rel;
        match pair(xml, start, &mut budget) {
            Err(()) => break,
            Ok(None) => i = start + 1,
            Ok(Some(p)) => {
                i = p.end;
                let text = decode(p.raw);
                if !text.is_empty() && !out.iter().any(|(n, _)| *n == p.name) {
                    out.push((p.name, text));
                }
            }
        }
    }
    out
}

fn decode(raw: &str) -> String {
    // CDATA content is literal: escape it so the entity pass leaves it alone, as the original implementation does.
    let mut escaped = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(open) = rest.find(CDATA_OPEN) {
        let body = &rest[open + CDATA_OPEN.len()..];
        let Some(close) = body.find(CDATA_CLOSE) else { break };
        escaped.push_str(&rest[..open]);
        for c in body[..close].chars() {
            match c {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                c => escaped.push(c),
            }
        }
        rest = &body[close + CDATA_CLOSE.len()..];
    }
    escaped.push_str(rest);
    text::joined_lines(&entities(&escaped))
}

/// The entity at the start of `s` (after its `&`): the character it stands for (`None` = drops out) and its length
/// including the `;`; `None` when it is not an entity.
fn entity(s: &str) -> Option<(Option<char>, usize)> {
    for (name, c) in [("amp;", '&'), ("lt;", '<'), ("gt;", '>'), ("quot;", '"'), ("apos;", '\'')] {
        if s.starts_with(name) {
            return Some((Some(c), name.len()));
        }
    }
    let (digits, radix, skip) = match s.strip_prefix("#x") {
        Some(h) => (h, 16, 2),
        None => (s.strip_prefix('#')?, 10, 1),
    };
    let n = digits.bytes().take_while(|c| (*c as char).is_digit(radix)).count();
    if n == 0 || digits.as_bytes().get(n) != Some(&b';') {
        return None;
    }
    let code = i32::from_str_radix(&digits[..n], radix).ok().filter(|c| (0..=0x10FFFF).contains(c));
    // An out of range number drops out; a lone surrogate (valid for Java) becomes U+FFFD.
    let ch = code.map(|c| char::from_u32(c as u32).unwrap_or(char::REPLACEMENT_CHARACTER));
    Some((ch, skip + n + 1))
}

fn entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match entity(after) {
            Some((c, len)) => {
                out.extend(c);
                rest = &after[len..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
