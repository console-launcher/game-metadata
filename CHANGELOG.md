# Changelog

Notable changes per release. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/). `scripts/publish` uses the section of the version it releases as the release notes.

## [0.7.1]

- Wii channel banners now draw shipped banners: the material flag counts (low nibbles) and the optional blocks after them, a single
  plain `banner.brlan` as the loop, the black/white TEV colours of a material, intensity (I4/I8) textures with alpha.
- `gamemeta wii-banner --icon` and `wii::channel_icon_bin`: the channel icon archive (what the Wii Menu's tile draws).
- `scripts/dolphin/`: bring-your-own-files captures for an optional private reference set, `docs/wii-references.md`.

## [0.7.0]

- Banner export: `banner3ds-export` writes a banner as an animated GIF, an animated WebP or an MP4 (H.264 with AAC or Opus audio),
  from the CPU renderer, on one timeline (intro once, then a loop).
- The banner's jingle (BCWAV: PCM8, PCM16, DSP-ADPCM) is read into 16-bit PCM and can be trimmed and resampled.
- `gamemeta banner3ds --gif|--webp`; export jobs in the wasm binding; "Export as" on a banner card of the website.

## [0.6.0]

- Wii channel banners: layout, animation and a CPU renderer in the core, bit-exact with the original implementation on 113 frames.
- C API, JNI and Kotlin (`WiiBanner`) for them; `gamemeta wii-banner` on the command line.
- Header readers for Game Boy / Color, Game Boy Advance, Nintendo 64, SNES, NES, Genesis / 32X, Master System / Game Gear;
  zip entries; the N64 RetroAchievements hash in any byte order.
- Platform names and `gm_info_version` in the C API; cart header fields as JSON.

## [0.5.1]

- The website: a static page that reads your own files in the browser (WebAssembly, no upload).
- `gamemeta-wasm` binding and `gamemeta-json`, shared by the CLI and the site.
- Kotlin: each class loader gets its own copy of the host library.
- MPL-2.0 licence and notices on every source file.

## [0.5.0]

- `banner3ds-soft` (CPU renderer, `no_std`) and `banner3ds-gles` (OpenGL ES 3.0 / WebGL 2 over a caller-filled function table).
- `gm_render_soft`, `gm_render_frames`, `gm_gles_*` in the C API; `BannerRenderer` and `GlesBanner` in Kotlin.
- `gamemeta render <file> -o banner.png`.

## [0.4.0]

- The `ids` readers: ISO 9660 and PlayStation serials, `.cue`, PSP `PARAM.SFO`, GameCube/Wii game ids, CHD SHA-1, zip CRC-32,
  NSP, XBE, RetroAchievements hashes.
- `gm_ids_*`, `gm_ra_hasher_*`, `gm_disc_id_*` in the C API; `readIds`, `RaHasher` and `discId` in Kotlin; `gamemeta ids`.

## [0.3.0]

- `banner3ds-scene`: CBMD/CGFX banners to a plain scene (models, materials, textures, skeleton, cameras, lights, animations).
- `gm_scene_*` in the C API; `readBannerScene` in Kotlin; `gamemeta scene`.

## [0.2.0]

- Wii U: `meta.xml`, TGA and ZArchive (`.wua`) readers; the file-callback C API.
- GameCube and Wii discs (ISO, WBFS, WIA, RVZ, CISO, GCZ), RVZ `opening.bnr`, WAD.
- Nintendo DS / DSi ROMs: banner, animated icon, zip.

## [0.1.0]

- The Nintendo 3DS reader in `gamemeta-core`: titles in 12 languages, publisher, icons, ids, regions, ratings.
- The C API (`gamemeta.h`, cbindgen) and the JNI library with the Kotlin API (`gamemeta.GameMeta`).
- `scripts/release`: the Android AAR (arm64-v8a, armeabi-v7a, x86_64, x86), the desktop host jar, header and checksums.
- `gamemeta` command-line tool (JSON dump).
