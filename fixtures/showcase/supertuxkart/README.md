# SuperTuxKart showcase banner

A real 3DS HOME banner built from SuperTuxKart's free-licensed art. Tux drives toward the viewer down a tree-lined track
under the SuperTuxKart logo. An item box drops in, bobs, and bursts into sparks. It is a 4-second loop (240 refreshes),
silent, and holds still on HOME (no turntable). See `CREDITS.md` for the art.

| file | what |
|---|---|
| `banner.cbmd` | the banner: 244,704 bytes; its scene is 488,448 bytes decompressed (HOME's cap is 512 KiB); all 30 language slots show the scene |
| `banner.json` | the scene dump (golden; `tests/stk_banner.rs` checks it) |
| `banner.home/` | HOME's frames on the emulator: 5 keyframes, a 4 s clip, `manifest.json` (`scripts/azahar/capture`) |
| `assets/`, `source/` | the resized textures and exported meshes the builder reads |
| `build.sh` | regenerates `banner.cbmd` and `banner.json` (`--assets` re-fetches and re-exports the art) |

## Features (16 of 18)

The 18 renderer features are the feature banners in `fixtures/scene/features/`.

| feature | where |
|---|---|
| alpha-test | pine rows: foliage alpha-tested at 128 |
| blend-additive | nitro flames, spark burst, sun flare |
| blend-modes | kart shadow (multiply), dust and logo (alpha) |
| tex-color-formats | RGB565 wheels and nitro, RGB8 sky, RGBA5551 asphalt, RGBA8 gift box, RGBA4 flare and logo |
| tex-luma-formats | LA8 spark, L8 shadow, A8 dust, L4 curb checker |
| tex-etc1 | ETC1 kart, Tux, engine, grass; ETC1A4 pine row |
| tex-wrap | road clamps across, grass and curbs mirror, sky, flare and logo clamp |
| combiner-modes | modulate (kart, trees), interpolate (haze), replace (sky, logo), add (spark), add-mult (curbs) |
| combiner-buffer | haze on grass, road and curbs: the lit texel buffered, mixed with the haze colour by vertex alpha |
| vertex-colors | kart lighting baked from normals, grass stripes, haze in alpha, tree fade |
| uv-scroll | road, grass, curbs, tree rows and nitro scroll |
| uv-rotate-scale | flare rotates and breathes, spark turns |
| color-anim | gift box hue cycle, nitro flicker, dust pulse, spark fade |
| bone-anim | kart bob; box drop, bob, grow and pop (scale); spark growth |
| depth-order | opaque first, then translucent layers (shadow and dust, then additive flames and sparks) |
| camera-framing | everything inside HOME's banner window, with the logo under the top edge |
| bone-rotate | **not used**: HOME holds a banner still only when every mesh bone is a billboard, and billboards drop rotations |
| visibility | **not used**: HOME ignores visibility clips (the original implementation's feature banner shows both squares all loop), so the pop is a scale |

## Renderers against HOME

The score is the reference implementation's `FrameScore`: luma at 100 x 60, rows 6-54, `100 * MAD / 255 + 50 * (1 - SSIM)`.
0 is identical, and up to 3 counts as a match. The RGB columns are the per-channel difference over the unmasked pixels.
Measured 2026-11-17 against the emulator 2126.2, at offset 0.

| renderer | mean score | c0240 | c0300 | c0360 | c0405 | c0430 | RGB mean | RGB max |
|---|---|---|---|---|---|---|---|---|
| gamemeta soft | 1.13 | 0.62 | 0.64 | 0.66 | 0.69 | 3.04 | 4.36-7.24 | 255 |
| gamemeta gles | 1.12 | 0.61 | 0.63 | 0.64 | 0.68 | 3.04 | 4.34-7.23 | 255 |
| original Kotlin CPU renderer | 1.13 | 0.62 | 0.64 | 0.66 | 0.69 | 3.04 | 4.36-7.24 | 255 |

The renderers agree with each other, and with HOME except in one place:

- On c0240-c0405, about 90 pixels per frame differ by more than 100, all along edges: the kart outline, the trees, the curb
  seams and the logo.
- c0430 is the known mismatch (`stk_banner::KNOWN_MISMATCHES`, ceiling 5.0). It catches the spark burst 30 refreshes in: an
  LA8 texture added to a fading gold constant, blended additively. HOME draws it dimmer, with its rings showing. All three
  renderers saturate it to a white disc. The cause is not found yet.
- Every keyframe scores best at exactly its own frame: one frame early or late scores about 3.

`crates/gamemeta-fixtures/tests/stk_banner.rs` (soft) and `crates/banner3ds-gles/tests/headless.rs` (gles) fail on a new
mismatch on any keyframe.
