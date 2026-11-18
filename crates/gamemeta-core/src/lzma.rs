// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Raw LZMA and LZMA2 decoding (WIA groups), on `lzma-rust2`.

use alloc::vec::Vec;

/// Which stream `compr_data` describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Lzma,
    Lzma2,
}

/// One decoder call: input consumed, output produced, whether the stream ended.
type Step = (usize, usize, bool);
type Process<'a> = dyn FnMut(&[u8], &mut [u8], lzma_rust2::Action) -> Option<Step> + 'a;

/// Decodes a raw stream (no header) described by `props` (the WIA `compr_data`), at most `max` bytes.
///
/// An LZMA stream in a WIA has no end marker, so the decoder cannot finish without the output size: pass `Some(size)` for the
/// exact size, or `None` to get a prefix (everything but the last few kilobytes, which the decoder holds back until it knows
/// the end) that is enough to read a size out of. An LZMA2 stream ends itself, so `size` is ignored and the output is whole.
/// `None` for bad props, damaged data or no output.
pub(crate) fn decompress(kind: Kind, props: &[u8], input: &[u8], size: Option<usize>, max: usize) -> Option<Vec<u8>> {
    use lzma_rust2::{Action, Status};
    let limit = size.map_or(max, |s| s.min(max));
    if input.is_empty() || limit == 0 || size.is_some_and(|s| s > max) {
        return None;
    }
    // Output never passes `limit`, so a dictionary of that size is as good as the stream's own (and is what gets allocated).
    let dict = |size: u32| size.min(u32::try_from(limit).unwrap_or(u32::MAX)).max(4096);
    let mut lzma;
    let mut lzma2;
    let process: &mut Process = match kind {
        Kind::Lzma => {
            let (&byte, dict_size) = props.split_first().filter(|(_, rest)| rest.len() >= 4)?;
            let dict_size = u32::from_le_bytes(dict_size[..4].try_into().ok()?);
            lzma = lzma_rust2::LzmaStream::new_with_props(limit as u64, byte, dict(dict_size), None).ok()?;
            &mut |i, o, a| {
                let r = lzma.process(i, o, a).ok()?;
                Some((r.bytes_consumed, r.bytes_produced, r.status == Status::StreamEnd))
            }
        }
        Kind::Lzma2 => {
            let &byte = props.first().filter(|&&b| b <= 40)?;
            let dict_size = if byte == 40 { u32::MAX } else { (2 | u32::from(byte & 1)) << (byte / 2 + 11) };
            lzma2 = lzma_rust2::Lzma2Stream::new(dict(dict_size));
            &mut |i, o, a| {
                let r = lzma2.process(i, o, a).ok()?;
                Some((r.bytes_consumed, r.bytes_produced, r.status == Status::StreamEnd))
            }
        }
    };
    // Finish when the end is known (the size, or LZMA2's own); Run for a prefix, where running out of input is the end.
    let action = if size.is_some() || kind == Kind::Lzma2 { Action::Finish } else { Action::Run };
    let mut out = Vec::new();
    let (mut buf, mut used) = ([0u8; 4096], 0usize);
    while out.len() < limit {
        let want = buf.len().min(limit - out.len());
        let (consumed, produced, end) = process(&input[used..], &mut buf[..want], action)?;
        used += consumed;
        out.extend_from_slice(&buf[..produced]);
        if end || (consumed == 0 && produced == 0) {
            break;
        }
    }
    let whole = size.is_none_or(|s| out.len() == s);
    (!out.is_empty() && whole).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamemeta_fixtures::gc::{compr_data, compress};

    fn data() -> Vec<u8> {
        (0..40_000u32).map(|i| if i % 3000 < 1500 { (i * 7 % 251) as u8 } else { b"zyx"[(i % 3) as usize] }).collect()
    }

    #[test]
    fn it_decodes_lzma_with_its_size_and_lzma2_without() {
        let d = data();
        let lzma = decompress(Kind::Lzma, &compr_data(3), &compress(3, &d), Some(d.len()), 1 << 20).unwrap();
        assert_eq!(lzma, d);
        let lzma2 = decompress(Kind::Lzma2, &compr_data(4), &compress(4, &d), None, 1 << 20).unwrap();
        assert_eq!(lzma2, d);
    }

    #[test]
    fn an_lzma_prefix_is_most_of_the_data_when_the_size_is_not_known() {
        let d = data();
        let out = decompress(Kind::Lzma, &compr_data(3), &compress(3, &d), None, 1 << 20).unwrap();
        assert!(d.starts_with(&out) && out.len() > d.len() / 2, "{} of {}", out.len(), d.len());
    }

    #[test]
    fn it_refuses_a_wrong_size_bad_props_empty_input_or_a_size_over_max() {
        let d = data();
        let packed = compress(3, &d);
        let props = compr_data(3);
        assert!(decompress(Kind::Lzma, &props, &packed, Some(d.len() + 100), 1 << 20).is_none(), "size past the data");
        assert!(decompress(Kind::Lzma, &props, &packed, Some(d.len()), 1000).is_none(), "size over max");
        assert!(decompress(Kind::Lzma, &[0xFF; 5], &packed, Some(100), 1000).is_none());
        assert!(decompress(Kind::Lzma, &props, &[], Some(100), 1000).is_none());
        assert!(decompress(Kind::Lzma2, &[], &compress(4, &d), None, 1000).is_none());
        assert!(decompress(Kind::Lzma2, &[63], &compress(4, &d), None, 1000).is_none());
        assert!(decompress(Kind::Lzma2, &compr_data(4), &compress(4, &d)[..100], None, 1 << 20).is_none(), "cut");
    }
}
