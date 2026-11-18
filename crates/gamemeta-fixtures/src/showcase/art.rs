// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Art preparation, all integer math so the same frames give the same bytes everywhere: the Tux frames (rendered by
//! `fixtures/showcase/supertuxkart/formats/source/render_tux.py` from SuperTuxKart's mesh and textures) are cropped,
//! scaled, composited, quantized and packed into each platform's pixel format.
use gamemeta_core::{RgbaImage, png};

const FRAME_BYTES: [&[u8]; 8] = [
    include_bytes!("../../../../fixtures/showcase/supertuxkart/formats/source/frames/tux-0.png"),
    include_bytes!("../../../../fixtures/showcase/supertuxkart/formats/source/frames/tux-1.png"),
    include_bytes!("../../../../fixtures/showcase/supertuxkart/formats/source/frames/tux-2.png"),
    include_bytes!("../../../../fixtures/showcase/supertuxkart/formats/source/frames/tux-3.png"),
    include_bytes!("../../../../fixtures/showcase/supertuxkart/formats/source/frames/tux-4.png"),
    include_bytes!("../../../../fixtures/showcase/supertuxkart/formats/source/frames/tux-5.png"),
    include_bytes!("../../../../fixtures/showcase/supertuxkart/formats/source/frames/tux-6.png"),
    include_bytes!("../../../../fixtures/showcase/supertuxkart/formats/source/frames/tux-7.png"),
];

/// Number of turntable frames.
pub const FRAMES: usize = 8;

/// Turntable frame `n` (0..8): the kart turned `n / 8` of a turn, 256x256, transparent background.
pub fn frame(n: usize) -> RgbaImage {
    png::decode(FRAME_BYTES[n % FRAMES]).expect("the committed frames are valid PNGs")
}

/// `image` as a PNG file.
pub fn png_bytes(image: &RgbaImage) -> Vec<u8> {
    png::encode(image).expect("the pixel count matches the size")
}

fn at(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width + x) * 4) as usize;
    [image.pixels[i], image.pixels[i + 1], image.pixels[i + 2], image.pixels[i + 3]]
}

/// A new image of `width` x `height` filled with `pixel(x, y)`.
pub fn from_fn(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.extend(pixel(x, y));
        }
    }
    RgbaImage { width, height, pixels }
}

/// The smallest square around the visible pixels (alpha 8 and up) plus `margin` pixels each side, transparent where the
/// square is bigger than the art.
pub fn crop_square(image: &RgbaImage, margin: u32) -> RgbaImage {
    let (mut x0, mut y0, mut x1, mut y1) = (image.width, image.height, 0, 0);
    for y in 0..image.height {
        for x in 0..image.width {
            if at(image, x, y)[3] >= 8 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    let (w, h) = (x1 + 1 - x0, y1 + 1 - y0);
    let side = w.max(h) + 2 * margin;
    let (ox, oy) = (x0 as i64 - ((side - w) / 2) as i64, y0 as i64 - ((side - h) / 2) as i64);
    from_fn(side, side, |x, y| {
        let (sx, sy) = (x as i64 + ox, y as i64 + oy);
        if sx < 0 || sy < 0 || sx >= image.width as i64 || sy >= image.height as i64 {
            [0; 4]
        } else {
            at(image, sx as u32, sy as u32)
        }
    })
}

/// `image` scaled to `width` x `height` by averaging the source pixels each destination pixel covers (colours weighted by
/// alpha, so edges keep their colour).
pub fn resize(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    from_fn(width, height, |x, y| {
        let (sx0, sx1) =
            (x * image.width / width, ((x + 1) * image.width).div_ceil(width).max(x * image.width / width + 1));
        let (sy0, sy1) =
            (y * image.height / height, ((y + 1) * image.height).div_ceil(height).max(y * image.height / height + 1));
        let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
        for sy in sy0..sy1.min(image.height) {
            for sx in sx0..sx1.min(image.width) {
                let p = at(image, sx, sy);
                let w = u64::from(p[3]);
                r += u64::from(p[0]) * w;
                g += u64::from(p[1]) * w;
                b += u64::from(p[2]) * w;
                a += w;
                n += 1;
            }
        }
        if a == 0 {
            return [0; 4];
        }
        [((r + a / 2) / a) as u8, ((g + a / 2) / a) as u8, ((b + a / 2) / a) as u8, ((a + n / 2) / n) as u8]
    })
}

/// `top` over `bottom` (same size), straight alpha.
pub fn over(top: &RgbaImage, bottom: &RgbaImage) -> RgbaImage {
    from_fn(top.width, top.height, |x, y| {
        let (t, b) = (at(top, x, y), at(bottom, x, y));
        let (ta, ba) = (u32::from(t[3]), u32::from(b[3]));
        let out_a = ta * 255 + ba * (255 - ta);
        if out_a == 0 {
            return [0; 4];
        }
        let mix =
            |tc: u8, bc: u8| ((u32::from(tc) * ta * 255 + u32::from(bc) * ba * (255 - ta) + out_a / 2) / out_a) as u8;
        [mix(t[0], b[0]), mix(t[1], b[1]), mix(t[2], b[2]), ((out_a + 127) / 255) as u8]
    })
}

/// A vertical gradient from `top` to `bottom`, opaque.
pub fn gradient(width: u32, height: u32, top: [u8; 3], bottom: [u8; 3]) -> RgbaImage {
    from_fn(width, height, |_, y| {
        let t = y * 255 / height.saturating_sub(1).max(1);
        let mix = |a: u8, b: u8| ((u32::from(a) * (255 - t) + u32::from(b) * t + 127) / 255) as u8;
        [mix(top[0], bottom[0]), mix(top[1], bottom[1]), mix(top[2], bottom[2]), 255]
    })
}

/// `src` pasted onto `dst` with its top-left at (`x`, `y`) (negative or past the edge is clipped), blended over what is there.
pub fn paste(dst: &RgbaImage, src: &RgbaImage, x: i64, y: i64) -> RgbaImage {
    let mut layer = from_fn(dst.width, dst.height, |_, _| [0; 4]);
    for sy in 0..src.height {
        for sx in 0..src.width {
            let (dx, dy) = (x + sx as i64, y + sy as i64);
            if dx >= 0 && dy >= 0 && dx < dst.width as i64 && dy < dst.height as i64 {
                let i = ((dy as u32 * dst.width + dx as u32) * 4) as usize;
                layer.pixels[i..i + 4].copy_from_slice(&at(src, sx, sy));
            }
        }
    }
    over(&layer, dst)
}

/// The kart in frame `n`, cropped square with a margin of one sixty-fourth, scaled to `side` pixels.
pub fn icon(n: usize, side: u32) -> RgbaImage {
    let f = frame(n);
    let cropped = crop_square(&f, f.width / 64);
    resize(&cropped, side, side)
}

/// `image` on an opaque background of `bg`.
pub fn flatten(image: &RgbaImage, bg: [u8; 3]) -> RgbaImage {
    over(image, &from_fn(image.width, image.height, |_, _| [bg[0], bg[1], bg[2], 255]))
}

/// The sky colour the opaque icons sit on.
pub const SKY: [u8; 3] = [0x5B, 0x9B, 0xD5];
/// The darker end of the sky gradient.
pub const SKY_DEEP: [u8; 3] = [0x1F, 0x4E, 0x8C];

// ------------------------------------------------------------------------------------------------ pixel formats

/// RGB565 (3DS icons).
pub fn rgb565(p: [u8; 4]) -> u16 {
    (u16::from(p[0] >> 3) << 11) | (u16::from(p[1] >> 2) << 5) | u16::from(p[2] >> 3)
}

/// Opaque RGB555 with the top bit set, or alpha (3 bits) and RGB444 (GameCube / Wii RGB5A3).
pub fn rgb5a3(p: [u8; 4]) -> u16 {
    if p[3] >= 0xE0 {
        0x8000 | (u16::from(p[0] >> 3) << 10) | (u16::from(p[1] >> 3) << 5) | u16::from(p[2] >> 3)
    } else {
        (u16::from(p[3] >> 5) << 12) | (u16::from(p[0] >> 4) << 8) | (u16::from(p[1] >> 4) << 4) | u16::from(p[2] >> 4)
    }
}

/// `0xAARRGGBB`.
pub fn argb(p: [u8; 4]) -> u32 {
    u32::from_be_bytes([p[3], p[0], p[1], p[2]])
}

/// BGRA bytes, as a TGA stores a pixel.
pub fn bgra(p: [u8; 4]) -> [u8; 4] {
    [p[2], p[1], p[0], p[3]]
}

/// The (x, y) in an 8x8 tile of the `i`th pixel in 3DS storage order (Morton: x from the even bits, y from the odd).
pub fn morton(i: usize) -> (usize, usize) {
    let (mut x, mut y) = (0, 0);
    for bit in 0..3 {
        x |= ((i >> (2 * bit)) & 1) << bit;
        y |= ((i >> (2 * bit + 1)) & 1) << bit;
    }
    (x, y)
}

/// The `i`th pixel in storage order of a `side` x `side` 3DS icon: 8x8 tiles in rows, Morton order inside each.
pub fn tiled_pixel(image: &RgbaImage, i: usize) -> [u8; 4] {
    let side = image.width as usize;
    let tile = i / 64;
    let (mx, my) = morton(i % 64);
    let (tx, ty) = (tile % (side / 8), tile / (side / 8));
    at(image, (tx * 8 + mx) as u32, (ty * 8 + my) as u32)
}

// ------------------------------------------------------------------------------------------------ quantizing

/// An image reduced to `colours` palette entries (entry 0 is transparent): the palette as 5-bit-per-channel RGB and one
/// palette index per pixel, top-down rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indexed {
    /// Palette entries (entry 0 unused: transparent).
    pub palette: Vec<[u8; 3]>,
    /// Width and height in pixels.
    pub size: (usize, usize),
    /// One index per pixel.
    pub indices: Vec<u8>,
}

impl Indexed {
    /// The index at (x, y).
    pub fn index(&self, x: usize, y: usize) -> usize {
        usize::from(self.indices[y * self.size.0 + x])
    }

    /// The palette as BGR555 little-endian words (the DS format), 2 bytes per entry.
    pub fn bgr555(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for c in &self.palette {
            let v = u16::from(c[0]) | (u16::from(c[1]) << 5) | (u16::from(c[2]) << 10);
            out.extend(v.to_le_bytes());
        }
        out
    }
}

/// Median-cut `image` to at most `colours` entries, entry 0 reserved for pixels with alpha under 128. Colours are first cut
/// to 5 bits per channel; ties sort by colour, so the result is the same on every machine.
pub fn quantize(image: &RgbaImage, colours: usize) -> Indexed {
    let (w, h) = (image.width as usize, image.height as usize);
    let px: Vec<Option<[u8; 3]>> = (0..w * h)
        .map(|i| {
            let p = [image.pixels[i * 4], image.pixels[i * 4 + 1], image.pixels[i * 4 + 2], image.pixels[i * 4 + 3]];
            (p[3] >= 128).then(|| [p[0] >> 3, p[1] >> 3, p[2] >> 3])
        })
        .collect();
    let mut boxes: Vec<Vec<[u8; 3]>> = vec![px.iter().flatten().copied().collect()];
    while boxes.len() < colours - 1 {
        // Split the box with the widest channel range (the most pixels on a tie, the first on a further tie).
        let range = |b: &Vec<[u8; 3]>| {
            (0..3)
                .map(|c| b.iter().map(|p| p[c]).max().unwrap_or(0) - b.iter().map(|p| p[c]).min().unwrap_or(0))
                .max()
                .unwrap_or(0)
        };
        let Some((i, _)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1 && range(b) > 0)
            .max_by_key(|(i, b)| (range(b), b.len(), usize::MAX - i))
        else {
            break;
        };
        let mut b = boxes.swap_remove(i);
        let channel = (0..3)
            .max_by_key(|&c| (b.iter().map(|p| p[c]).max().unwrap() - b.iter().map(|p| p[c]).min().unwrap(), 2 - c))
            .unwrap();
        b.sort_by_key(|p| (p[channel], p[0], p[1], p[2]));
        let upper = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(upper);
        boxes.sort();
    }
    let mut palette = vec![[0u8; 3]];
    for b in &boxes {
        let n = b.len().max(1) as u32;
        let mean = |c: usize| ((b.iter().map(|p| u32::from(p[c])).sum::<u32>() + n / 2) / n) as u8;
        palette.push([mean(0), mean(1), mean(2)]);
    }
    let indices = px
        .iter()
        .map(|p| match p {
            None => 0,
            Some(c) => {
                let dist = |q: &[u8; 3]| (0..3).map(|k| (i32::from(c[k]) - i32::from(q[k])).pow(2)).sum::<i32>();
                (1..palette.len()).min_by_key(|&i| (dist(&palette[i]), i)).unwrap_or(0) as u8
            }
        })
        .collect();
    Indexed { palette, size: (w, h), indices }
}

/// The image `indexed` stands for, back in RGBA (5-bit channels spread to 8 bits by bit replication, as the readers do).
pub fn expand(indexed: &Indexed) -> RgbaImage {
    let spread = |v: u8| (v << 3) | (v >> 2);
    from_fn(indexed.size.0 as u32, indexed.size.1 as u32, |x, y| {
        let i = indexed.index(x as usize, y as usize);
        if i == 0 {
            [0; 4]
        } else {
            let c = indexed.palette[i];
            [spread(c[0]), spread(c[1]), spread(c[2]), 255]
        }
    })
}
