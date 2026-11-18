# AGENTS.md

Read by AI agents and humans. This file covers how to check a reader or renderer against the real thing, and how to improve it
from the comparison. Rules for contributors are in `CONTRIBUTING.md`; the design is in `docs/design.md`.

## Gates

```sh
scripts/check                  # the gate (no CI): fmt, clippy -D warnings, tests, no_std, portability, header drift, C, Kotlin
scripts/license-headers --fix  # MPL notice on every source file (check fails without it)
scripts/site-test              # builds the site (scripts/site-build) and runs its browser tests; never run site-deploy
scripts/release                # AAR, host jar and C header into dist/
cargo test -p <crate> --test <file> -- <name>   # one test
```

Fuzz from `fuzz/`, 5 minutes per target. Seed the corpus first with `cargo run -p gamemeta-fixtures --bin write-fuzz-seeds fuzz/corpus`.

```sh
cd fuzz && cargo +nightly fuzz run <target> -- -max_total_time=300 -max_len=300000
# targets: banner3ds_render banner3ds_scene carts gc_walk ids_walk nds_walk threeds_walk wii_banner wii_walk wiiu_walk
```

## Fixtures

- Synthetic data built in code (`crates/gamemeta-fixtures`), or free-licensed art with every file credited in a `CREDITS.md` beside it.
- No commercial game data, banners, art, fonts or logos. No keys and no key-derived values. Never decrypt.
- No emulator system files, NAND, keys or installed titles in the repo, and never printed in a log.
- Every committed fixture is what its generator writes. A drift test fails when the bytes differ:

| fixtures | regenerate | drift test |
|---|---|---|
| `fixtures/*` samples | `cargo run -p gamemeta-fixtures --bin write-fixtures fixtures` | `gamemeta-fixtures --test committed` |
| `demo/` | `cargo run -p gamemeta-fixtures --bin write-demo` | `gamemeta-fixtures --test demo_committed` |
| `fixtures/showcase/supertuxkart/` | `fixtures/showcase/supertuxkart/build.sh` | `gamemeta-fixtures --test stk_banner` |

## Golden JSON

A golden is the reader's dump of a fixture, committed beside it as JSON. A change to a reader that moves a golden is either a
bug, or a fix you explain in the commit.

```sh
cargo test -p banner3ds-scene --test golden                 # fixtures/scene/{features,cases}/*.json, from the original implementation
GM_WRITE_GOLDEN=1 cargo test -p gamemeta-fixtures --test stk_banner   # rewrite banner.json, then review the git diff
```

The reference goldens (`fixtures/scene`, `fixtures/render`) are never rewritten by hand. They come from the original implementation.

## What "the original implementation" means

There are two references:

- **The original implementation**: the earlier Kotlin implementation. `docs/design.md`, first section, explains it, and its goldens are in
  `fixtures/`. Parity tests port its tests one for one (`*_parity.rs`, `kotlin_goldens.rs`).
- **The real thing**: the console's own HOME Menu, run on an emulator. Its frames are captured with `scripts/azahar/`. Where
  the two references disagree, the real thing wins. Record that in the test's known-mismatch list.

## The reference loop (3DS banners)

1. Capture. You bring your own system files (`scripts/azahar/README.md`). The preflight checks them and refuses to work
   inside the repo.

   ```sh
   scripts/azahar/preflight
   scripts/azahar/capture --clips "240 300 360 405 430" --seconds 4 fixtures/showcase/supertuxkart/banner.cbmd
   # writes banner.home/: c<clip>.png keyframes (the HOME wall masked in magenta), clip.mp4, manifest.json
   ```

   It works on any `.cbmd`, `.3ds` or `.cia` that you have the right to use. Only commit the frames of banners we made.

2. Render and compare:

   ```sh
   cargo test -p gamemeta-fixtures --test stk_banner soft_draws_the_banner_like_home -- --nocapture   # soft
   cargo test -p banner3ds-gles --test headless -- showcase                                            # gles
   ```

   - `gamemeta_fixtures::reference::fit` searches the clip offset, then scores each keyframe with the original implementation's
     `FrameScore`: `100 * MAD / 255 + 50 * (1 - SSIM)` on luma at 100 x 60, rows 6-54.
   - 0 is identical. `reference::MATCH` (3.0) counts as a match.
   - `--nocapture` prints the score for each frame, plus the RGB mean and max.

3. The mean must be at most MATCH, and so must each keyframe, unless that keyframe is in the banner's known-mismatch list as
   `(clip, ceiling, reason)`. For STK, that is `gamemeta_fixtures::stk_banner::KNOWN_MISMATCHES`, shared by soft and gles and checked by `Fit::check`.
   - The ceiling is the measured score + 1.5, rounded up to 0.5.
   - The reason names the feature that differs.
   - The test fails above the ceiling. It also fails when a listed keyframe starts to match, so the list only shrinks.
   - The STK list has one entry today: c0430, the spark burst. HOME draws it dimmer than all three renderers (soft, gles, Kotlin).

4. When you find a mismatch:
   - Find the feature: `GM_GL_SHOTS=<dir>` (gles headless test) writes the gles and soft frames as PNG; set them beside the keyframe.
   - Write a red test that isolates it, either a feature banner or a case in `fixtures/scene`.
   - Fix the renderer, and see the test go green.
   - Lower or remove the ceiling, then recapture if HOME's behaviour was the question.

HOME limits measured this way are in `scripts/azahar/README.md`. They cover the scene size cap, the turntable, the ignored
visibility clips, and the language slots.

Tolerances in use:

| check | tolerance |
|---|---|
| soft vs reference goldens (`banner3ds-soft --test kotlin_goldens`) | max channel diff 1, mean 0.01 |
| gles vs soft (`banner3ds-gles --test headless`) | share of differing pixels 0.01, mean 0.5 |
| soft and gles vs HOME (above) | mean and each keyframe at most MATCH, or a listed keyframe at most its ceiling |

## Wii banners: emulator references (optional, private)

Real channel banners come from discs we cannot commit, so there is no reference set in this repository. An optional private set,
not part of the repo, can be pointed at with `GM_WII_REFS=<dir>`; the reference tests skip without it, which is the normal state
for a contributor. The method, the findings and the open list are in `docs/wii-references.md`; the scripts are in
`scripts/dolphin/` (README there). The loop is the 3DS one with a different surface: the Wii Menu's tile is drawn from the channel's
`icon.bin` (the same archive format as `banner.bin`), so that is what is compared.

1. If you have a set: `cargo test -p gamemeta-core --test wii_references -- --nocapture` prints each entry's mean difference per
   channel (0..255) between our render and the reference tile, by opaque id. (`scripts/dolphin/fetch-refs --env` syncs a set from a
   store you configured yourself; see the README.)
2. An entry matches under `MATCH_MEAN` (20). One that does not is in `KNOWN_MISMATCH` (by id) with a ceiling (its measured mean plus
   a little) and a generic reason. The test fails above the ceiling, and fails when a listed entry starts to match, so the list only
   shrinks.
3. When you find a mismatch: render the icon (`gamemeta wii-banner icon.bin --scale 1.19 --frame N -o out.png`) beside the reference
   frame, find the feature, write a red test on an authored archive (`gamemeta-fixtures::wii_banner`; rewrite the pinned archives
   with `write-wii-banner-fixtures`), fix the renderer, lower or remove the ceiling.
4. To make a set from files you own: `scripts/dolphin/capture` (bring your own NAND and discs), `align.py <slug> --write`, then
   `ref_library.py stage`. Never commit a NAND, keys, a disc, a banner archive or a frame, and never print them.

What is still off today is one thing: TEV stages are not rendered. The authored banners (`fixtures/wii-banner`) stay self-consistent
with the original Kotlin on 113 frames; real banners are judged against the emulator.
