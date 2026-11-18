# game-metadata

Read titles, icons, ids and art from game files, and render Nintendo 3DS HOME banners. A Rust core (`no_std` + `alloc`,
no I/O of its own) with a C API, Kotlin bindings (Android and desktop JVM) and a command-line tool.

It reads files you own.

game-metadata is an independent project. It is not affiliated with, authorized, sponsored or endorsed by Nintendo, Sony, Microsoft, Sega or any other game or console company. Nintendo 3DS, Nintendo 3DS HOME Menu, Wii, Wii U, Game Boy, Nintendo DS, GameCube, PlayStation, Xbox, Sega Genesis, Mega Drive, Master System and Game Gear, and the other console, format and game names used here, are trademarks of their respective owners and are used only to say which file formats the library reads. The project contains no game data, keys or console firmware and never unlocks protected files.

## What it reads

| Reader | Files | Gives |
|---|---|---|
| `threeds` | `.3ds .cci .cxi .app .cia` | titles (12 languages), publisher, icons, ids, regions, ratings, banner extent |
| `nds` | `.nds .dsi .srl .ids`, first ROM of a `.zip` | titles (up to 8), icon, DSi animated icon + banner block, game code, maker, ratings |
| `gc` | `.iso .gcm .ciso .gcz .rvz` (a `.wia` GameCube disc gives nothing) | `opening.bnr` (BNR1/BNR2) titles, banner image, region, header title, game id |
| `wii` | the disc containers + `.wbfs .wia`, `.wad` | header title, game id, region; from an RVZ also `opening.bnr` titles and the logo; WAD title id and game id |
| `ids` | by extension: `.iso` (PS1/PS2 serial from `SYSTEM.CNF`, PSP `PARAM.SFO`, GC/Wii id), `.bin/.img` (raw 2048 or 2352-byte sectors, mode 1 or 2), `.cue` (first data track via the opener), `.pbp`, `.chd`, `.zip`, `.nsp`, `.xbe`, `.wad`, `.3ds`/`.cia`, `.rpx`/`.wua`, the GC/Wii containers; RA hashing by name | serial (normalized `SLUS00003`), game id, title id, CHD raw SHA-1, zip ROM CRC-32 + size, PSP title + disc id, Xbox title id, RA MD5, disc id with region/console or the reason for none |
| `carts` | Game Boy / Color `.gb .gbc .sgb`, Game Boy Advance `.gba .agb`, Nintendo 64 `.z64 .v64 .n64`, SNES `.sfc .smc .swc .fig`, NES `.nes`, Genesis / 32X `.md .gen .smd .32x` (and a Genesis `.bin`), Master System / Game Gear `.sms .gg`, or one ROM in a `.zip` | title, game code, maker, version, regions and every header field as JSON; header checks (checksum matching is a separate opt-in call) |
| `wiiu` | unpacked title (via `.rpx`), `.wua` | titles (12), publisher, ids, region, icon, boot media sizes, splashes on demand |

`ids` also gives RetroAchievements MD5 hashes (by name), and `gamemeta-core` caps every length before allocating.
Details, field lists and the decisions behind them: [docs/design.md](docs/design.md).

## Renderers

- `banner3ds-scene`: parses a 3DS banner (CBMD/CGFX) into a plain scene: models, meshes, materials, textures, skeleton,
  cameras, lights, animations.
- `banner3ds-soft`: a CPU renderer. Pure Rust, `no_std`, any size, same pixels everywhere.
- `banner3ds-gles`: an OpenGL ES 3.0 / WebGL 2 renderer over a table of GL function pointers you fill with your loader;
  no GL library is linked.

| Crate / dir | What |
|---|---|
| `crates/gamemeta-core` | The readers: pure Rust, caller-supplied bytes in, plain structs out. |
| `crates/banner3ds-scene`, `-soft`, `-gles` | The banner parser and its two renderers. |
| `crates/gamemeta-ffi` | The C ABI (`include/gamemeta.h`), built as `libgamemeta.{so,a}`. |
| `crates/gamemeta-jni` | The JNI library behind the Kotlin API (`libgamemeta_jni.so`). |
| `crates/gamemeta-cli` | `gamemeta <file>...`: dump what the readers find, as JSON; `gamemeta render` writes a banner PNG. |
| `crates/gamemeta-wasm` | The wasm-bindgen binding behind the website: reads a picked `File` in place, hashes in chunks, draws banners (CPU or WebGL 2). |
| `crates/gamemeta-json` | `GameInfo` and `Ids` as JSON (the CLI's output, the website's input). |
| `site/` | The static website: a demo and a page that reads your own files in the browser. |
| `crates/gamemeta-fixtures` | Synthetic test files built in code (no game data in the repo). |
| `bindings/kotlin` | The Kotlin API (`gamemeta.GameMeta`, `BannerRenderer`), its JVM tests, the Gradle build. |
| `fuzz/` | cargo-fuzz targets (nightly). |

## Build

```sh
cargo build --release                 # all crates; libgamemeta.so/.a, libgamemeta_jni.so, the gamemeta CLI
cargo test --workspace
scripts/check                         # fmt, clippy -D warnings, tests, no_std + portability builds, header drift, C smoke, Kotlin test
scripts/release                       # dist/game-metadata-<v>/: AAR, host jar, header, LICENSE, NOTICE.md, SHA256SUMS
scripts/publish <v>                   # dry run of a GitHub release: all assets and every step (docs/releasing.md)
```

The AAR needs the four Android Rust targets, an NDK (`scripts/android-env`) and a JDK 17; the host jar adds macOS and
Windows libraries when `cargo-zigbuild` / `cargo-xwin` are installed. Rust 1.87 or newer.

## Install

Every [release](../../releases) carries the files below, with a `SHA256SUMS`. `OWNER` is the GitHub account that hosts this repository
and `VERSION` a tag, `v0.6.0` for example.

Gradle (Android or desktop JVM), through [JitPack](https://jitpack.io), which serves the release's AAR:

```kotlin
repositories { maven("https://jitpack.io") }
dependencies { implementation("com.github.OWNER:game-metadata:VERSION") }
```

Desktop JVM without Gradle's Android support: use `game-metadata-host-<version>.jar` from the release; it holds the classes and
the host libraries.

C: `gamemeta.h` from the release, plus `libgamemeta` from `cargo build -p gamemeta-ffi --release` (a `.so` and a `.a`); on Android the
`libgamemeta_jni.so` files inside the AAR export the same C functions.

Command line: unpack `gamemeta-<version>-x86_64-linux.tar.gz` and run `gamemeta game.3ds`.

WebAssembly: `gamemeta-wasm-<version>.zip` holds the wasm-bindgen output (`gamemeta_wasm.js` and the `.wasm`), the same files the
website loads.

## Use

Rust (`gamemeta-core`; the caller owns all I/O, here a slice):

```rust
use gamemeta_core::{SliceSource, read};

let bytes = std::fs::read("game.3ds")?;
if let Some(info) = read(&mut SliceSource::new(&bytes), "game.3ds") {
    println!("{:?}", info.title());
}
```

C (`examples/c/readme.c`; the callback reads over `stdio`, a descriptor or memory works too):

```c
GmInfo *info = NULL;
GmStatus status = gm_read_callback(size, read_file, file, "game.3ds", &info);
if (status == GM_STATUS_OK) {
    printf("%s\n", gm_info_title(info, (uint32_t)gm_info_preferred_language(info), GM_TITLE_FIELD_NAME));
    gm_info_free(info);
}
```

Kotlin (Android or desktop JVM; AAR or host jar):

```kotlin
val bytes = File("game.3ds").readBytes()
val info = GameMeta.read(bytes, "game.3ds")
println(info?.titles)
BannerRenderer.open(bytes)?.use { banner -> val frame = banner.render(0f, 400, 240) }   // top-down RGBA
```

CLI:

```sh
cargo run -p gamemeta-cli -- game.3ds                       # one JSON object per file
cargo run -p gamemeta-cli -- ids game.cue                   # serials, game ids, checksums, RA hashes
cargo run -p gamemeta-cli -- render game.3ds -o banner.png  # a banner frame
```

## Website

`site/` is a one-page static viewer: it reads files you pick or drop (or a whole Wii U title folder, or a folder of ROMs) with this
library compiled to WebAssembly, and has a "Try a demo" button (the files in `demo/` plus the fixtures; list credits one per
line in `demo/CREDITS.md`). Everything happens in your browser: files are read in place (a Web Worker
slices the `File`, so a multi-gigabyte ROM is never loaded whole), nothing is uploaded, and the page makes no network request
after it loads. A 3DS file's banner is drawn as the animated diorama with `banner3ds-gles` in a WebGL 2 context the page makes, or
with `banner3ds-soft` on the CPU where WebGL 2 is missing.

```sh
scripts/site-build                  # wasm + page into site/dist (needs the wasm32 target and wasm-bindgen-cli, same version as Cargo.lock)
python3 -m http.server -d site/dist # then open http://localhost:8000
scripts/site-test                   # build, node unit tests, headless Chromium (demo, picks, folder, big files, no requests)
scripts/site-deploy --push          # publish site/dist to the gh-pages branch of origin (dry run without --push)
```

## Portability

The libraries are `no_std` + `alloc` and `scripts/check` builds them for `thumbv7em-none-eabihf` and
`wasm32-unknown-unknown`, and compile-checks them for iOS, macOS, Windows, and Android (arm64, armv7, with the JNI
library). Bringing it to another host: [PORTING.md](PORTING.md).

## Docs

[docs/design.md](docs/design.md) (architecture and decisions), [docs/releasing.md](docs/releasing.md), [CHANGELOG.md](CHANGELOG.md), [PORTING.md](PORTING.md), [NOTICE.md](NOTICE.md) (format
sources and third-party crates), [SECURITY.md](SECURITY.md), [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MPL-2.0: use it in any app, open or closed; changes to this library's files are shared under the same license. See
[LICENSE](LICENSE). Format sources and third-party crates: [NOTICE.md](NOTICE.md).

`demo/` and `fixtures/showcase/supertuxkart/` are not MPL-2.0: they carry art from SuperTuxKart and are CC BY-SA 3.0 (item_spark: CC0),
see `demo/CREDITS.md` and `fixtures/showcase/supertuxkart/CREDITS.md`. None of it is linked into the library, the Android and JVM
binding, the command line tool or the wasm package; the website serves the demo files with their credit line.

game-metadata is an independent project. It is not affiliated with, authorized, sponsored or endorsed by Nintendo, Sony, Microsoft, Sega or any other game or console company. Nintendo 3DS, Nintendo 3DS HOME Menu, Wii, Wii U, Game Boy, Nintendo DS, GameCube, PlayStation, Xbox, Sega Genesis, Mega Drive, Master System and Game Gear, and the other console, format and game names used here, are trademarks of their respective owners and are used only to say which file formats the library reads. The project contains no game data, keys or console firmware and never unlocks protected files.
