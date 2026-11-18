# HOME Menu reference captures

These scripts capture the real HOME Menu drawing a banner, running headless in the emulator, and save reference keyframes
beside the banner. The renderers are compared against those keyframes (see `AGENTS.md`, "The reference loop").

```sh
scripts/azahar/preflight                       # what is set up and what is missing
scripts/azahar/capture [--clips "300 360 420"] [--seconds 4] [--out <dir>] <banner.cbmd|.3ds|.cia>...
```

For each banner `<dir>/<stem>.<ext>`, the capture writes `<dir>/<stem>.azahar/` (or `--out`, for one banner only). It holds:

- `c<clip>.png`: the top screen, 400 x 240, `<clip>` refreshes (60 to the second) after the banner appeared. Everything
  outside HOME's banner window and the status bars is pure magenta.
- `clip.mp4`, when `--seconds` is above 0: that many seconds from the first clip, masked the same way (H.264, 30 fps, no
  audio).
- `manifest.json`: the emulator version, the date, the banner's sha256, the banner start frame, each keyframe's clip and
  dump frame, and the mask.

Captures of your own banners are yours to commit. The HOME wall and status bars are not, which is why every saved frame is
masked.

## Bring your own system files

Nothing here ships emulator or console data. Point the scripts at your own install:

| variable | what |
|---|---|
| `AZAHAR_BIN` | the emulator executable (an extracted AppImage's `usr/bin/azahar`) |
| `AZAHAR_USER` | a portable user dir with the HOME Menu and system titles installed from your own console |
| `AZAHAR_SLOT` | the visible HOME app whose banner slot is borrowed (default `0004001000021400`) |
| `AZAHAR_SLOT_CIA` | optional: that app's NoCrypto CIA; without it, the copy installed in `AZAHAR_USER` is rewritten |
| `AZAHAR_WORK` | scratch dir (default `~/.cache/gamemeta-azahar`); refused inside this repository |
| `AZAHAR_DISPLAY` | the Xvfb display number (default 88) |

Rules the scripts keep:

- They never print system files, keys or NAND contents, only paths and sizes.
- They never decrypt anything. `banner.py` reads and writes NoCrypto NCCH only and refuses encrypted contents.
- They never write into the repository, except the masked frames and the manifest beside the banner.
- The emulator runs on a private working copy of `AZAHAR_USER` under `AZAHAR_WORK` (mode 0700). Your install is never
  modified. `rm -r "$AZAHAR_WORK"` removes the copy.
- Audio is silent: the null sink, volume 0, and SDL's dummy driver.

Tools needed: `Xvfb`, `xdotool`, `ffmpeg`/`ffprobe`, and `python3` with numpy and Pillow. Run `preflight` to check them.

## How a capture runs

1. `banner.py extract` takes the banner from the input: a `.cbmd` as is, or the ExeFS `banner` of a NoCrypto `.3ds`/`.cia`.
2. HOME lists only titles it knows, so the banner replaces the borrowed slot app's banner:
   - the first banner of a run: `banner.py swap`, then install (with `AZAHAR_SLOT_CIA`), or `banner.py swap-app` on the
     installed content;
   - later banners: `swap-app` live.
3. The emulator boots HOME (File > Boot Home Menu), with internal scale 1 and the default layout. The capture closes the
   first-run tutorial whenever it shows up and resets the app list to its left end.
4. The cursor parks on the game card slot (a neutral wall). Tools > Dump Video starts a lossless dump (`crf:0`). The cursor
   moves onto the borrowed slot.
5. The capture waits for emulated frames, not wall time, then stops the dump.
6. `frames.py` finds the banner start. HOME's own icon fly-by comes first. The start is the frame after which the top
   screen stays past half way from the wall to the banner's level, stepped back to where the fade began. Then it maps clips
   to dump frames (HOME drops about 3 refreshes in 1066), masks, and writes the outputs.

The window geometry (1280 x 960 Xvfb, scale 1) is measured once and kept in `capture`'s header. If a boot fails, the
screen is saved to `$AZAHAR_WORK/instance/boot-fail.png` (private: it shows HOME). The last dump stays at
`$AZAHAR_WORK/instance/tmp/last-dump.mkv`.

## What HOME does, measured here (emulator 2126.2)

- **Scene size:** the decompressed CGFX must stay under 512 KiB. A 521,344-byte scene shows, while 552,192 and 692,480 do
  not: HOME draws only the wall, with no error. Commercial banners stay under about 325 KB. The file itself can be larger
  (localized scenes and audio).
- **Turntable:** HOME turns the model 0.6° a refresh (600 refreshes a turn). It holds the scene still only when every bone
  with a mesh is a billboard at x = z = 0 and no clip moves a bone in x or z. A billboard root over plain bones still
  turns.
- **Visibility clips are ignored:** meshes keep their static visibility. Use a scale to 0.001, or a colour fade, to hide
  something.
- **Language slots:** all 30 can point at the common scene, and HOME shows it.

## Recapturing

```sh
# the SuperTuxKart showcase references (fixtures/showcase/supertuxkart/banner.home/)
scripts/azahar/capture --clips "240 300 360 405 430" --seconds 4 fixtures/showcase/supertuxkart/banner.cbmd
# the feature banners, each beside its .cbmd (fixtures/scene/features/<name>.azahar/)
scripts/azahar/capture --clips "300 330 360 390" fixtures/scene/features/*.cbmd
```

Repeat captures of one banner agree to a mean of 0.001 and a largest channel difference of 12 (the dump's 4:2:0 chroma).
The `alpha-test` feature banner captured here is identical to the reference implementation's committed frame (mean 0,
max 0).
