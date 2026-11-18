// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! bzip2 decoding (WIA groups): own decoder, no dependency.

use alloc::vec::Vec;

/// Decodes one bzip2 stream into at most `max` bytes; `None` for corrupt data (a block or stream CRC that does not match),
/// randomised blocks (obsolete) or output over `max`.
pub(crate) fn decompress(input: &[u8], max: usize) -> Option<Vec<u8>> {
    let mut bits = Bits { data: input, pos: 0 };
    if bits.get(24)? != 0x42_5A_68 {
        return None; // "BZh"
    }
    let level = bits.get(8)?;
    if !(u32::from(b'1')..=u32::from(b'9')).contains(&level) {
        return None;
    }
    let block_max = (level - u32::from(b'0')) as usize * 100_000;
    let mut out = Vec::new();
    let mut combined = 0u32;
    loop {
        let magic = (u64::from(bits.get(24)?) << 24) | u64::from(bits.get(24)?);
        if magic == 0x1772_4538_5090 {
            return (bits.get(32)? == combined).then_some(out);
        }
        if magic != 0x3141_5926_5359 {
            return None;
        }
        let expected = bits.get(32)?;
        let start = out.len();
        block(&mut bits, block_max, &mut out, max)?;
        let crc = crc(&out[start..]);
        if crc != expected {
            return None;
        }
        combined = combined.rotate_left(1) ^ crc;
    }
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    /// The next `n` (at most 32) bits, most significant first; `None` past the end.
    fn get(&mut self, n: u32) -> Option<u32> {
        let end = self.pos.checked_add(n as usize)?;
        if end > self.data.len() * 8 {
            return None;
        }
        let mut v = 0u64;
        for i in self.pos..end {
            v = (v << 1) | u64::from((self.data[i / 8] >> (7 - i % 8)) & 1);
        }
        self.pos = end;
        Some(v as u32)
    }
}

/// The bzip2 CRC: CRC-32 with polynomial 0x04C11DB7, most significant bit first.
fn crc(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04C1_1DB7 } else { crc << 1 };
        }
    }
    !crc
}

const MAX_LEN: usize = 20;
const GROUP: usize = 50;

/// A canonical Huffman table in the form the reference decoder uses.
struct Table {
    limit: [i32; MAX_LEN + 2],
    base: [i32; MAX_LEN + 2],
    perm: Vec<u16>,
    min: usize,
}

impl Table {
    fn new(lengths: &[u8]) -> Self {
        let min = usize::from(*lengths.iter().min().unwrap_or(&1));
        let max = usize::from(*lengths.iter().max().unwrap_or(&1));
        let mut perm = Vec::with_capacity(lengths.len());
        for len in min..=max {
            for (symbol, &l) in lengths.iter().enumerate() {
                if usize::from(l) == len {
                    perm.push(symbol as u16);
                }
            }
        }
        let mut count = [0i32; MAX_LEN + 2];
        for &l in lengths {
            count[usize::from(l)] += 1;
        }
        let (mut limit, mut base) = ([-1i32; MAX_LEN + 2], [0i32; MAX_LEN + 2]);
        let (mut code, mut index) = (0i32, 0i32);
        for len in min..=max {
            base[len] = index - code;
            code += count[len];
            index += count[len];
            limit[len] = code - 1;
            code <<= 1;
        }
        Table { limit, base, perm, min }
    }

    fn symbol(&self, bits: &mut Bits) -> Option<usize> {
        let mut len = self.min;
        let mut code = bits.get(len as u32)? as i32;
        loop {
            if len > MAX_LEN {
                return None;
            }
            if code <= self.limit[len] {
                return self.perm.get(usize::try_from(code + self.base[len]).ok()?).map(|&s| usize::from(s));
            }
            code = (code << 1) | bits.get(1)? as i32;
            len += 1;
        }
    }
}

/// One block: its symbol tables, the Huffman and move-to-front decoding, the inverse BWT and the first run-length stage,
/// appended to `out` (which may not grow past `max`).
fn block(bits: &mut Bits, block_max: usize, out: &mut Vec<u8>, max: usize) -> Option<()> {
    if bits.get(1)? != 0 {
        return None; // randomised
    }
    let orig_ptr = bits.get(24)? as usize;
    // The bytes the block uses: 16 groups of 16, each present only if its bit is set.
    let used_groups = bits.get(16)?;
    let mut seq_to_unseq = Vec::new();
    for g in 0..16 {
        if used_groups & (0x8000 >> g) != 0 {
            let used = bits.get(16)?;
            for i in 0..16 {
                if used & (0x8000 >> i) != 0 {
                    seq_to_unseq.push((g * 16 + i) as u8);
                }
            }
        }
    }
    if seq_to_unseq.is_empty() {
        return None;
    }
    let alpha = seq_to_unseq.len() + 2;
    let groups = bits.get(3)? as usize;
    if !(2..=6).contains(&groups) {
        return None;
    }
    let selector_count = bits.get(15)? as usize;
    if selector_count == 0 {
        return None;
    }
    let mut order: Vec<u8> = (0..groups as u8).collect();
    let mut selectors = Vec::with_capacity(selector_count);
    for _ in 0..selector_count {
        let mut j = 0usize;
        while bits.get(1)? == 1 {
            j += 1;
            if j >= groups {
                return None;
            }
        }
        let v = order.remove(j);
        order.insert(0, v);
        selectors.push(v);
    }
    let mut tables = Vec::with_capacity(groups);
    for _ in 0..groups {
        let mut len = bits.get(5)? as i32;
        let mut lengths = Vec::with_capacity(alpha);
        for _ in 0..alpha {
            loop {
                if !(1..=MAX_LEN as i32).contains(&len) {
                    return None;
                }
                if bits.get(1)? == 0 {
                    break;
                }
                len += if bits.get(1)? == 0 { 1 } else { -1 };
            }
            lengths.push(len as u8);
        }
        tables.push(Table::new(&lengths));
    }
    // Huffman symbols, 50 to a selector, into the move-to-front list's bytes.
    let eob = alpha - 1;
    let mut mtf: Vec<u8> = (0..=255u8).take(seq_to_unseq.len()).collect();
    let mut tt: Vec<u32> = Vec::new();
    let mut counts = [0usize; 256];
    let (mut run, mut weight) = (0usize, 1usize);
    let (mut selector, mut left) = (0usize, 0usize);
    let mut table = &tables[0];
    loop {
        if left == 0 {
            table = &tables[usize::from(*selectors.get(selector)?)];
            selector += 1;
            left = GROUP;
        }
        left -= 1;
        let symbol = table.symbol(bits)?;
        if symbol <= 1 {
            run += weight << symbol;
            weight <<= 1;
            if run > block_max {
                return None;
            }
            continue;
        }
        if run > 0 {
            let byte = seq_to_unseq[usize::from(mtf[0])];
            if tt.len() + run > block_max {
                return None;
            }
            counts[usize::from(byte)] += run;
            tt.resize(tt.len() + run, u32::from(byte));
            (run, weight) = (0, 1);
        }
        if symbol == eob {
            break;
        }
        let index = symbol - 1;
        if index >= mtf.len() || tt.len() >= block_max {
            return None;
        }
        let v = mtf.remove(index);
        mtf.insert(0, v);
        let byte = seq_to_unseq[usize::from(v)];
        counts[usize::from(byte)] += 1;
        tt.push(u32::from(byte));
    }
    let n = tt.len();
    if n == 0 || orig_ptr >= n {
        return None;
    }
    // Inverse BWT: link every position to the next in the original order.
    let mut next = [0usize; 256];
    let mut sum = 0;
    for (b, &c) in counts.iter().enumerate() {
        next[b] = sum;
        sum += c;
    }
    for i in 0..n {
        let b = (tt[i] & 0xFF) as usize;
        tt[next[b]] |= (i as u32) << 8;
        next[b] += 1;
    }
    // Walk it, undoing the first run-length stage (four equal bytes, then a count of further copies).
    let mut pos = tt[orig_ptr] >> 8;
    let (mut previous, mut same) = (None::<u8>, 0u32);
    let mut i = 0;
    while i < n {
        let entry = *tt.get(pos as usize)?;
        pos = entry >> 8;
        let byte = (entry & 0xFF) as u8;
        i += 1;
        if same == 4 {
            let extra = usize::from(byte);
            if out.len() + extra > max {
                return None;
            }
            out.resize(out.len() + extra, previous.unwrap_or(0));
            (previous, same) = (None, 0);
            continue;
        }
        same = if previous == Some(byte) { same + 1 } else { 1 };
        previous = Some(byte);
        if out.len() >= max {
            return None;
        }
        out.push(byte);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamemeta_fixtures::gc::compress;

    fn pseudo(len: usize) -> Vec<u8> {
        let mut x = 0x2545_F491_4F6C_DD1Du64;
        (0..len)
            .map(|i| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                if i % 5000 < 2500 { (x >> 56) as u8 } else { b"abcabcabd"[i % 9] }
            })
            .collect()
    }

    #[test]
    fn it_decodes_what_libbz2_encodes() {
        for data in [
            b"hello hello hello".to_vec(),
            vec![0u8; 5000],
            vec![7u8; 1],
            (0..=255u8).cycle().take(70_000).collect(),
            pseudo(350_000), // several 100k blocks
        ] {
            let packed = compress(2, &data);
            assert_eq!(decompress(&packed, data.len()).as_deref(), Some(&data[..]), "{} bytes", data.len());
        }
    }

    #[test]
    fn it_refuses_corrupt_truncated_or_oversized_data() {
        let data = pseudo(20_000);
        let packed = compress(2, &data);
        assert!(decompress(&packed, data.len() - 1).is_none(), "over max");
        assert!(decompress(&packed[..packed.len() / 2], data.len()).is_none(), "truncated");
        let mut bad = packed.clone();
        let mid = bad.len() / 2;
        bad[mid] ^= 0x55;
        assert!(decompress(&bad, data.len()).is_none(), "bit flip");
        assert!(decompress(&[], 10).is_none());
        assert!(decompress(b"BZh9garbage", 10).is_none());
    }
}
