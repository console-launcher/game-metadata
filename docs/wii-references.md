# Wii channel banners: checking them against the emulator

The Wii banner renderer (`gamemeta_core::wii::banner`) is written from the NW4R layout format and can be checked against what the
emulator's own Wii Menu draws. This page is how to do that check, what it found, and how to improve the renderer from it. The
tooling is `scripts/dolphin/` (see its README); the test is `crates/gamemeta-core/tests/wii_references.rs`.

**There is an optional private reference set, and it is not part of this repository.** Nothing in the repository holds banner
archives, emulator frames or a NAND, and nothing in it says where such a set is kept. If you have one (your own captures, or one
you were given access to), point the tests at it with `GM_WII_REFS=<dir>`; without it every reference test prints "skipped" and
passes. That is the normal state for a contributor. Never commit, upload or print a NAND, a key file, a disc, a banner archive or a
clip.

## Which surface is checked

The Wii Menu's Disc Channel tile is drawn from the channel's **`icon.bin`** (`meta/icon.bin` in `opening.bnr`), not from
`banner.bin`: the same NW4R archive format, a layout of its own (608 x 456), usually one `icon.brlan`. `banner.bin` is what the
menu shows when a channel is opened (the pointer on the tile), which the capture automation cannot do: the emulated pointer never
moved. So `icon.bin` is the surface compared against frames; `banner.bin` is covered by the structure checks (archives parse and
draw) and by the shared code. `gamemeta wii-banner <disc.rvz> --icon` draws the icon.

## Capture (bring your own files)

`scripts/dolphin/capture <disc.rvz> <slug>` boots the Wii Menu of a NAND you imported into the emulator (own Xvfb, OpenGL on Mesa,
no audio), inserts the disc, dumps the emulator's frames (frame-exact lossless PNG; a slow software renderer only costs wall time),
and keeps the tile crop of each under `$WII_REFS/out/<slug>/frames/` with a `meta.json` (emulator version from `flatpak info`).
`scripts/dolphin/align.py <slug> --write` then finds, for three captured frames, which icon frame each one shows (smallest mean
difference in the tile's window) and writes `samples.json`.

Fixed geometry (fitted once, then held): the tile shows a window of the icon layout centred on the layout centre, 172 x 82 tile
pixels at 0.84 layout units per tile pixel (render at scale 1/0.84 = 1.19), at (28, 22)-(200, 104) in the 235 x 130 tile crop. The
Menu's own rounded frame and shading sit outside that window and are not compared.

## The reference set's layout

```
manifest.json                     {"files": {path: {size, sha256}}, "titles": {slug: {title, files}}}
banners/<slug>/banner.bin         the channel banner archive (still wrapped)
banners/<slug>/icon.bin           the channel icon archive
refs/<slug>/meta.json             the emulator version, the capture, the samples (tile frame -> icon frame), scale, window
refs/<slug>/f<N>.png              the emulator's tile crop at capture frame N
```

Archives are read straight from a disc's decrypted partition (an RVZ stores it so) by `gamemeta wii-banner <disc> --bin out.bin
[--icon]`; nothing here decrypts anything.

To keep a set in a store you control (a cloud bucket), `scripts/dolphin/ref_library.py` has `stage` (build the set from your capture
root), `publish` and `fetch`; the store comes from `GM_WII_REFS_BUCKET` (a `gs://` URL, optionally `GM_WII_REFS_PROJECT`) or
`bucket = "..."` in `~/.config/gamemeta/refs.toml`, never from the repository, and unset means no store. `scripts/dolphin/fetch-refs
[--env]` syncs it into `~/.cache/gamemeta-wii-refs` (or `$GM_WII_REFS_CACHE`) by manifest hash: a warm cache downloads nothing, a
changed entry downloads only that file, a corrupt download is refused. With no store or no access it prints one `skipped` line and
exits 0.

## The test

`GM_WII_REFS=<dir> cargo test -p gamemeta-core --test wii_references -- --nocapture`:

- every entry's icon is drawn at its sample frames and compared with the reference tile in the window: **mean absolute difference per
  channel under 20** (0..255) is a match; the matching entries measure 13 to 16;
- an entry that does not match yet is in `KNOWN_MISMATCH` with a ceiling (its measured mean plus a little) and the reason. Entries are
  named by an opaque id (the first 8 hex digits of the MD5 of the entry's name). The test is green today and red on a regression; it
  is also red when a listed entry starts matching ("remove it from the list");
- every archive in the set opens and draws a non-blank poster.

## What real banners fixed (2026-11-17)

Found by decoding shipped channel banners and comparing with the emulator; each fix has a test that was red first
(`tests/wii_banner.rs`):

1. `mat1` flag word (offset 0x3C): the counts are the **low** nibbles: texture maps bits 0-3, texture SRTs 4-7, coordinate
   generators 8-11. Then, each switched by its own bit: channel control (bit 25, 4 bytes), material colour (27, 4), TEV swap (12,
   4), indirect SRTs (13-14 count, 20 each), indirect stages (15-17, 4), TEV stages (18-22, 16 each), alpha compare (23, 4), blend
   mode (24, 4). Checked on the material sizes of the banners and icons read.
2. An archive with one plain `banner.brlan` (no "start" or "loop" in the name) plays it as the loop.
3. The first two TEV colour registers of a material (offsets 0x14 and 0x1C, s16 RGBA) are what the texture maps black and white
   to: `black + (white - black) * texel`, alpha included. A checker material sets black to opaque light grey, which is the grey
   checker the emulator draws.
4. GX intensity textures (I4, I8) expand to (I, I, I, I): the intensity is also the alpha, so black texels are transparent.
5. `txl1`, `pic1` at 0x4C and the material layout up to the flag word held.

Against the emulator's tile the mean difference per channel went from 40 to 130 (blank or garbled) to 13 to 16 on the entries that
match. Max channel differences stay 150 to 245: the tile is resampled by the emulator and is a frame or two off, so single pixels
on edges differ a lot; the mean is the number to watch.

## What is still off

All of it is the same missing piece: **TEV stages** are not rendered (the renderer multiplies texture x vertex colour x material
colour). Real materials use them: pictures that pair a colour texture with an alpha texture and combine them in two stages (the
scene is missing), layered logos, and colour animations that go through a stage. Notes for whoever decodes them (unverified): a
stage is 16 bytes; in the colour-input half-word the nibbles read (B, A) then (D, C) in GX's colour-input numbering (0 CPREV,
2 C0, 4 C1, 8 TEXC, 15 ZERO), e.g. `42f8` is `black + (white - black) * TEXC`. The alpha inputs, op bits, konst selects, tev swap,
indirect stages, alpha compare and blend mode are read past but not decoded. Also not drawn: `wnd1`, `txt1`; the texture SRT pivot
is not judged.

## Improving the renderer from a comparison

1. Have a reference set (above); run the reference test and find an entry with a high mean.
2. Look at it: render the icon (`gamemeta wii-banner <icon.bin> --scale 1.19 --frame N -o out.png`) beside `refs/<slug>/f<N>.png`.
3. Write the failing test first (an authored archive from `gamemeta-fixtures::wii_banner` that shows the feature; add the field to
   `Mat` and the writer, `cargo run -p gamemeta-fixtures --bin write-wii-banner-fixtures` rewrites the pinned archives, and the 113
   reference-parity frames must stay bit-exact unless you changed what they draw).
4. Fix the renderer, rerun the reference test, lower the entry's ceiling or remove it from `KNOWN_MISMATCH`.

## What a Kotlin port of the renderer would need

The same fixes: the `mat1` low-nibble counts and optional blocks (1), the plain `banner.brlan` as the loop (2), the black/white
colour registers (3), intensity textures with alpha (4), and `icon.bin` as the surface for a tile.
