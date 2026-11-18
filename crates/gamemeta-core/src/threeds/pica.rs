// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! PICA200 texture layout helpers.

/// The (x, y) inside an 8x8 tile of the `index`th pixel in storage order (Morton / Z-order: x from the even bits of
/// the index, y from the odd bits).
pub fn morton_xy(index: usize) -> (usize, usize) {
    let bit = |b: usize| (index >> b) & 1;
    (bit(0) | bit(2) << 1 | bit(4) << 2, bit(1) | bit(3) << 1 | bit(5) << 2)
}

#[cfg(test)]
mod tests {
    use super::morton_xy;

    /// The first rows of the original implementation swizzle table (pixel index -> y * 8 + x).
    #[test]
    fn matches_the_swizzle_table() {
        let table = [
            0, 1, 8, 9, 2, 3, 10, 11, 16, 17, 24, 25, 18, 19, 26, 27, 4, 5, 12, 13, 6, 7, 14, 15, 20, 21, 28, 29, 22,
            23, 30, 31,
        ];
        for (i, want) in table.iter().enumerate() {
            let (x, y) = morton_xy(i);
            assert_eq!(y * 8 + x, *want, "index {i}");
        }
        assert_eq!(morton_xy(32), (0, 4));
        assert_eq!(morton_xy(63), (7, 7));
    }
}
