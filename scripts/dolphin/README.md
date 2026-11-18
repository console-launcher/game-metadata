# scripts/dolphin: Wii banner references from the emulator

Capture how Dolphin's Wii Menu draws a disc's channel banner, and check the renderer against it (an optional, private reference set that is not part of this repository). The method, the findings and the
reference test are in `docs/wii-references.md`. Everything these scripts read and write is private: your NAND, your discs, the
frames. Nothing is copied into the repository, and keys are never read, hashed or printed.

You bring: Dolphin (flatpak `org.DolphinEmu.dolphin-emu`), Xvfb, xdotool, ffmpeg, 7z, and your own NAND backup (a BootMii
`nand.bin` + `keys.bin`, or the `.7z` of both) and Wii discs (`.rvz`). Where things go: `$WII_REFS` (default
`~/.local/share/dolphin-wii-refs`, mode 700): `nand-dump/` (your backup), `user/` (Dolphin's own user dir, with the NAND imported
by Dolphin's Tools > Import BootMii NAND Backup), `discs/`, `lib/<slug>/` (the banner archives), `out/<slug>/` (captures).

```
scripts/dolphin/preflight                     says what is missing (tools, NAND, imported Wii Menu), READY when it can capture
scripts/dolphin/capture <disc.rvz> <slug> [frames=1800] [internal-resolution=1]
                                              one command: boots the Wii Menu on its own Xvfb, inserts the disc, dumps frames, writes
                                              out/<slug>/{frames/NNNN.png, meta.json}; kills only the processes it recorded
gamemeta wii-banner <disc.rvz> --bin lib/<slug>/banner.bin;  ... --icon --bin lib/<slug>/icon.bin
                                              the banner and icon archives of a disc (no decryption: an RVZ stores the partition decrypted)
scripts/dolphin/align.py <slug> [--samples 3] [--write]
                                              which icon frame each captured frame shows (needs numpy, Pillow, target/release/gamemeta)
scripts/dolphin/ref_library.py stage --lib DIR   builds the set from lib/ and out/ (banner archives, meta.json, sampled frames)
scripts/dolphin/ref_library.py publish --lib DIR uploads it to a store you configured (below)
scripts/dolphin/fetch-refs [--env]            syncs it into ~/.cache/gamemeta-wii-refs by manifest hash; with no store or no access:
                                              one "skipped" line, exit 0
python3 -I scripts/dolphin/test_ref_library.py   the fetch / stage / publish tests (run by scripts/check)
```

Environment: `WII_REFS` (the private root), `GM_WII_REFS_BUCKET` (a `gs://` URL; or `bucket = "..."` in `~/.config/gamemeta/refs.toml`, with no
default: unset means no store) and optionally `GM_WII_REFS_PROJECT`, `GM_WII_REFS_CACHE`, `GM_WII_REFS` (the set the Rust reference test reads). Output carries titles and numbers only, never frames or bytes. Audio is off; the display is the
script's own Xvfb, never yours.
