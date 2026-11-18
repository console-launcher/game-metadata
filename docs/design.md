# game-metadata design

"The original implementation" below is an earlier Kotlin implementation of these readers and renderers (same author) that this library was
ported from and is checked against: its outputs are committed as goldens in `fixtures/`, and its tests were ported one per
test onto synthetic fixtures. Its code is not in this repository.

Status (2026-11-17), version 0.7.1:

- 3DS (NCSD, NCCH, CIA) end to end: core, C ABI, JNI + Kotlin, release artifacts.
- DS and DSi ROMs (`.nds`, `.dsi`, `.srl`, `.ids`, and the first ROM in a `.zip`): titles, DSi animated icon,
  banner block, ids, ratings.
- GameCube (ISO, CISO, GCZ, RVZ), Wii (the same containers plus WBFS and WIA, the RVZ partition's
  `opening.bnr`, WADs) and Wii U (unpacked titles through a file callback, Cemu `.wua`).
- `banner3ds-scene`: a 3DS banner (CBMD, localized CGFX, bare CGFX) parsed into a renderer-neutral scene (models,
  meshes, materials, PICA textures incl. ETC1, skeleton and bone weights, cameras, lights, animations as data); C ABI,
  JNI + Kotlin binding, `gamemeta scene`, fuzz target.
- the `ids` module: the id readers a game launcher needs (PS1/PS2 serials, PSP `PARAM.SFO`, GameCube/Wii
  game ids and disc ids, Xbox ids, CHD and zip checksums, Switch NSP title ids, RetroAchievements MD5 hashes), with parity to
  the original Kotlin (47 ported tests); C ABI, JNI + Kotlin binding, `gamemeta ids`, fuzz target.
- 0.7.0: `banner3ds-scene` reads a banner's jingle (BCWAV: PCM8, PCM16, DSP-ADPCM). `banner3ds-export` and the website's "Export as"
  (GIF, MP4, animated WebP of a 3DS banner; MP4 through the browser's H.264 encoder and this muxer, with the jingle when the browser can
  encode AAC or Opus). `gamemeta banner3ds --gif|--webp`.
- the 3DS banner renderers. `banner3ds-soft` (CPU, the pixel reference: bit-exact with the original Kotlin
  `SoftwareRenderer` on all 54 feature frames and 40 case frames) and `banner3ds-gles` (GLSL ES 3.00 in the WebGL 2 subset,
  through a GL table the caller fills; within the stated tolerance of soft on the same 94 frames, headless EGL in the tests);
  C ABI, JNI + Kotlin (`BannerRenderer`, `GlesBanner`), `gamemeta render`, fuzz target, bench. See Renderer API.
- cartridge headers (`carts`): Game Boy / Color, Game Boy Advance, Nintendo 64, SNES, NES, Genesis / Mega Drive / 32X
  and Master System / Game Gear ROMs, and the first ROM in a `.zip`. Typed header fields on `GameInfo.cart`, the title,
  game code, maker, version and regions in the usual `GameInfo` fields; the N64 RetroAchievements hash now normalizes a
  `.v64` / `.n64` image to big-endian first. C ABI (`gm_info_version`, `gm_info_cart_json`), JNI + Kotlin, `gamemeta info`
  (the default verb), the website, fuzz target `carts`. 73 core tests plus ABI, CLI and Kotlin ones, each red before green.
- Wii channel banners (`wii::banner`): the `opening.bnr` / `meta/banner.bin` archive (U8, LZ77, IMD5), the layout
  (`.brlyt`), its animations (`.brlan`) and the textures (`.tpl`) as data, with pure frame sampling and a CPU renderer into a
  caller RGBA buffer at any scale; bit-exact with the original Kotlin `wiibanner/` on 113 frames of 35 authored banners.
  C ABI (`gm_wii_banner_*`), JNI + Kotlin (`WiiBanner`), `gamemeta wii-banner`, fuzz target `wii_banner`. Checked against shipped
  banners and an emulator's Wii Menu (2026-11-17): `mat1` flags, plain `banner.brlan`, black/white TEV colours and intensity alpha
  corrected; TEV stages still not drawn. See `docs/wii-references.md`.

## Goals and rules

- Read what a game file says about itself (titles, publisher, icons, ids, regions, ratings, art) on any host, and
  later render 3DS HOME banners into a caller-owned GL context.
- **The caller does all I/O.** Android games sit behind content URIs, so the library never opens paths: it takes a
  byte source (callback, descriptor, or memory). The same rule makes it portable (PORTING.md).
- No application names in the library; the Kotlin package is `gamemeta`.
- Readers never panic on hostile input (fuzzed), never allocate more than the file holds (every range is checked
  against the source size before allocating), and read headers only (a few KB plus the icon; caps: icon 64 KiB,
  banner 4 MiB, located but not read).

## Crates

```
gamemeta-core       no_std + alloc. ByteSource in, GameInfo out. Readers: threeds (+ smdh, pica: Morton tiling, which the
                    banner crates will reuse), nds, gc (containers, rvz, disc FS, bnr), wii (opening.bnr: IMET, U8, LZ77,
                    IMD5, TPL; WAD), wiiu (meta.xml, TGA, ZArchive), ids (ISO 9660, PlayStation serials and cue sheets, PSP
                    PARAM.SFO, CHD, zip, NSP, XBE, GameCube/Wii ids, disc ids, RetroAchievements MD5). Shared: zip + deflate, zstd,
                    text, image. `carts`: header readers for the cartridge consoles (gb, gba, n64, snes, nes, md, sms) and the
                    `.zip` wrapper, on one `Family` / `CartHeader` / `Field` model. `wii::banner`: a Wii channel banner (archive,
                    BRLYT layout, BRLAN animation, CPU renderer) on the wii reader's U8, IMD5, LZ77 and TPL code.
gamemeta-ffi        C ABI, lib name `gamemeta` (cdylib + staticlib + rlib). cbindgen header include/gamemeta.h.
gamemeta-jni        cdylib `gamemeta_jni`: JNI entry points for bindings/kotlin, plus (through the ffi rlib) the C ABI.
gamemeta-cli        `gamemeta [info] <file>...` -> JSON lines (a Wii U `.rpx` is read through the folders beside its `code/`);
                    `gamemeta scene <file>...` -> the banner scene as JSON, one line per file (`null` when there is none);
                    `gamemeta ids <file>...` -> the ids as JSON, one line per file, with the RA MD5 (siblings read from the file's folder);
                    `gamemeta render <file> [--t <s>] [--size WxH] -o out.png` -> one CPU-rendered frame (default 0 s, 400x240).
                    `gamemeta banner3ds <file> --gif|--webp [--scale 1|2|3] -o out` -> the banner's animation (the intro once, one loop) as a GIF or
                    animated WebP, drawn by the CPU renderer (MP4 is the website's: it needs the browser's video encoder).
                    `gamemeta wii-banner <rvz|opening.bnr|banner.bin> [--frame N]... [--poster] [--scale S] [-o out.png] [--bin out.bin]`
                    -> a Wii channel banner's frames as PNG and one JSON line (frames, size, skipped panes, missing textures).
gamemeta-json       `GameInfo` / `Ids` as JSON (std, no dependencies); the CLI and the wasm binding print the same objects.
gamemeta-wasm       wasm-bindgen binding for a Web Worker (`crates/gamemeta-wasm`, built by `scripts/site-build`): `inspect` (metadata,
                    ids, scene, as JSON plus icon bytes), `RaHash` (chunked), `SoftRenderer`, `Gles`. A `ByteSource` over a `Blob`
                    (FileReaderSync + slice: only the bytes asked for) and a `Gl` table of forwarding functions into `js/gl.js`.
gamemeta-fixtures   synthetic files built in code; `write-fixtures` writes fixtures/ (drift-tested), `write-fuzz-seeds` the corpora,
                    `write-demo` the SuperTuxKart demo set in `demo/` (one file per format, CC-BY-SA art; drift-tested, goldens in `fixtures/showcase/supertuxkart/formats/golden`).
banner3ds-scene     no_std + alloc, forbid(unsafe_code), depends on core only. CBMD/CGFX -> BannerScene: `Cbmd::parse`,
                    `cgfx::parse`, `source::read` (a bare banner, or a 3DS file through core's `threeds`), LZ11, the PICA
                    texture decoders (ETC1 and the rest, on core's Morton tiling), `dump` (deterministic JSON), `Keyframes::sample`,
                    and `draw`: what both renderers share on the CPU (the player and pose, HOME's camera and framing, draw order,
                    skinning, billboards, the fragment-lighting maths), on `libm` for no_std floats.
banner3ds-soft      no_std + alloc, core + scene only. The CPU rasteriser: the pixel reference, RGBA8 into a caller buffer.
banner3ds-gles      no_std + alloc, core + scene only. GLSL ES 3.00 programs generated per material, drawn into the caller's
                    current GLES 3.0 / WebGL 2 context through a `Gl` table its loader fills. No context, window or thread.
banner3ds-export    std, pure Rust (not in the C library or the Android binding). Animated exports of a banner: `timeline` (the intro once, then
                    one loop, at most a minute; one picture for a banner that does not move), `gif` (a palette per frame, exact up to 255
                    colours else NeuQuant, hard transparency, repeats forever, 50 pictures a second = 2/100 s each: browsers show anything
                    under 2/100 s as 1/10 s, so 60 a second cannot be written), `webp` (lossless VP8L frames from `image-webp` in a
                    hand-written VP8X/ANIM/ANMF container, exact millisecond durations 17/17/16 at 60 a second, equal neighbours merged),
                    `mp4` (a moov-first muxer for the browser's H.264 and AAC or Opus chunks), `audio` (resample and trim the jingle),
                    `render` (the CPU renderer's pictures and a `Job` that writes a GIF or WebP one picture at a time).
fuzz/               cargo-fuzz (nightly, own workspace): threeds_walk, nds_walk, gc_walk, wii_walk, wiiu_walk, banner3ds_scene,
                    ids_walk, banner3ds_render, carts, wii_banner.
```

### Core API

```rust
pub trait ByteSource { fn size(&self) -> u64; fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ReadError>; }
pub enum ReadError { OutOfRange, Io }
pub fn read<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<GameInfo>;   // by extension
pub struct GameInfo { platform, titles: Vec<(Language, Title)>, icon, small_icon: Option<RgbaImage>, ids: GameIds,
                      regions: Vec<Region>, region_free, ratings: Vec<(RatingBoard, u8)>, banner: Option<Extent> }
pub mod threeds { pub fn locate(..) -> Layout; pub fn read(..) -> Result<Found, Reason>; pub fn title_id(..); pub fn read_info(..); }
pub fn format_for(file_name: &str) -> Option<Format>;   // ThreeDs, Ds, DsZip, Disc, Wad, WiiUArchive, WiiUTitle
// GameInfo also carries: animated_icon, images: Vec<ImageKind>, boot_movie / boot_sound (Wii U sizes), header_title,
// version: Option<u32>, cart: Option<carts::CartHeader> (Format::Cart(Family) for the cartridge extensions).

pub trait TitleFiles { fn size(&mut self, path: &str) -> Option<u64>; fn read(&mut self, path: &str, max: usize) -> Option<Vec<u8>>; }
pub mod wiiu {
    pub fn read_title<F: TitleFiles + ?Sized>(files: &mut F) -> Option<GameInfo>;       // meta/meta.xml + TGAs
    pub fn image<F: TitleFiles + ?Sized>(files: &mut F, kind: ImageKind) -> Option<RgbaImage>;   // splashes on demand
    pub fn open_wua<S: ByteSource + ?Sized>(source: &mut S) -> Option<WuaFiles<'_, S>>;  // a title inside a .wua
    pub struct OpenerFiles<O>(pub O);   // O: FnMut(&str) -> Option<Box<dyn ByteSource>>, paths as "../meta/meta.xml"
}
```

`.rpx` is a Wii U title whose files only the caller can open, so `read` gives `None` for it: use `wiiu::read_title`
with `OpenerFiles` (or any `TitleFiles`). A `.wua` goes through `read` like any file.

`OutOfRange` vs `Io` mirrors the original reader (null vs exception) so outcomes match it case for case.
Text rules (trim, blank, line splitting) follow Kotlin/JVM `Char.isWhitespace` and `lines()` exactly
(`core::text`), so strings compare byte for byte with the original reader.

`no_std`: checked by building for `thumbv7em-none-eabihf` in `scripts/check`. The `std` feature only adds
`Error` impls.

### Cartridge API (`gamemeta_core::carts`)

```rust
pub enum Family { GameBoy, Gba, N64, Snes, Nes, MegaDrive, MasterSystem }   // for_extension, for_file, prefix_len
pub enum CartHeader { GameBoy(gb::Header), Gba(..), N64(..), Snes(..), Nes(..), MegaDrive(..), MasterSystem(..) }
impl CartHeader { pub fn family(&self) -> &'static str; pub fn fields(&self) -> Vec<(&'static str, Field)>; pub fn to_json(&self) -> String; }
pub enum Field { Text(String), Number(i64), Flag(bool), Missing }
pub fn read_info<S: ByteSource + ?Sized>(source: &mut S, family: Family, file_name: &str) -> Option<GameInfo>;
pub fn read_zip_info<S: ByteSource + ?Sized>(source: &mut S) -> Option<GameInfo>;   // one ROM in a .zip, inflated only as far as its header
pub fn checksum_matches<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<bool>;   // opt-in, streams the whole file
```

`read` is header-only (bounded reads). `fields()` / `to_json()` are the one mapping every binding uses (the C ABI, JNI and
`gamemeta-json` pass the JSON through). A `.bin` is tried as a Genesis image (`SEGA` at 0x100, or 0x101 for a mirrored
header) and otherwise gives nothing: a title is never guessed. `checksum_matches` covers the Game Boy global checksum, the
Genesis word sum (also through SMD interleave) and the Master System sum; it is `None` for the rest.

### Ids API (`gamemeta_core::ids`)

```rust
pub struct Ids { serial, game_id, title_id, chd_sha1, zip_crc: Option<String>, zip_size: Option<u64>,
                 psp_title, psp_disc_id, xbox_id, ra_md5: Option<String> }   // all optional; primary_id() = serial, else game_id, else title_id
pub fn read<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<Ids>;                  // by extension, no siblings
pub fn read_with<S: ByteSource + ?Sized>(source: &mut S, file_name: &str, opener: &mut dyn Opener) -> Option<Ids>;
pub trait Opener { fn open(&mut self, path: &str) -> Option<Box<dyn ByteSource + '_>>; }   // path relative to the game file's folder
pub struct RaHasher;  // for_file(name, size) -> Option<RaHasher>; update(&[u8]); finish(self) -> Option<String>  (streaming MD5)
pub fn ra_hash<S: ByteSource + ?Sized>(source: &mut S, file_name: &str) -> Option<String>;            // the same, over a whole source
pub fn disc_id(header: &[u8]) -> DiscIdResult;  pub fn disc_id_of<S: ByteSource + ?Sized>(source: &mut S) -> DiscIdResult;
pub enum DiscIdResult { Ok { id, region: DiscRegion, system: DiscSystem }, Refused(DiscIdRefusal) }
```

`read` never hashes (the RA MD5 reads the whole file): the caller streams it through `RaHasher` or calls `ra_hash`, and sets
`ra_md5` itself (the CLI does). The `Opener` unifies the original implementation's `sibling` (a cue sheet's tracks) and `related` (a Wii U
`meta/meta.xml`): both are a path relative to the game file's folder (`game (Track 1).bin`, `../meta/meta.xml`).

### C ABI (`include/gamemeta.h`)

```c
uint32_t    gm_abi_version(void);                 /* == GM_ABI_VERSION (1) */
const char *gm_version(void);                     /* "0.2.0" */
GmStatus gm_read_bytes(const uint8_t *data, size_t len, const char *file_name, GmInfo **out);
GmStatus gm_read_callback(uint64_t size, int32_t (*read)(void*, uint64_t, uint8_t*, size_t), void *user_data,
                          const char *file_name, GmInfo **out);
GmStatus gm_read_fd(int fd, const char *file_name, GmInfo **out);   /* pread; not moved, not closed */
void        gm_info_free(GmInfo *info);
uint32_t    gm_info_platform(const GmInfo *info);
const char *gm_info_title(const GmInfo *info, uint32_t language, uint32_t field /* GmTitleField */);
int32_t     gm_info_preferred_language(const GmInfo *info);
const char *gm_info_id(const GmInfo *info, uint32_t id /* GmId */);
const uint8_t *gm_info_icon(const GmInfo *info, uint32_t which /* GmIcon */, uint32_t *width, uint32_t *height);
uint32_t    gm_info_regions(const GmInfo *info);   bool gm_info_region_free(const GmInfo *info);
int32_t     gm_info_rating(const GmInfo *info, uint32_t board);
bool        gm_info_banner(const GmInfo *info, uint64_t *offset, uint64_t *size);
/* added in 0.2.0 (ABI stays 1: nothing existing changed) */
uint32_t    gm_info_images(const GmInfo *info);                      /* bit mask over GmImage */
int64_t     gm_info_boot_asset(const GmInfo *info, uint32_t which);  /* Wii U bootMovie / bootSound size, -1 none */
const char *gm_info_header_title(const GmInfo *info);                /* GC / Wii disc header title */
uint32_t    gm_info_animation_count(const GmInfo *info);
const uint8_t *gm_info_animation_frame(const GmInfo *info, uint32_t index, uint32_t *w, uint32_t *h, uint32_t *duration);
/* added in 0.3.0 (ABI stays 1): banner scenes, an opaque GmScene freed with gm_scene_free */
GmStatus    gm_scene_read_bytes(const uint8_t *data, size_t len, GmScene **out);   /* CBMD, CGFX, or a decrypted .3ds/.cia/.cxi */
GmStatus    gm_scene_read_callback(uint64_t size, GmReadFn read, void *user, GmScene **out);
GmStatus    gm_scene_read_fd(int fd, GmScene **out);
uint32_t    gm_scene_count(const GmScene *scene, uint32_t what /* GmSceneCount: models, meshes, textures, lights, cameras, 3 animation kinds, unsupported */);
const char *gm_scene_json(const GmScene *scene);                                /* the deterministic dump; owned by the handle */
const char *gm_scene_texture_name(const GmScene *scene, uint32_t index);
const uint8_t *gm_scene_texture_pixels(const GmScene *scene, uint32_t index, uint32_t *w, uint32_t *h);
const char *gm_scene_unsupported(const GmScene *scene, uint32_t index);         /* e.g. "dot3-combiner" */
void        gm_scene_free(GmScene *scene);
/* added in 0.4.0 (ABI stays 1): ids, the RA hasher, disc ids. GmIds is opaque, freed with gm_ids_free */
typedef struct { uint64_t size; GmReadFn read; void *user_data; void (*close)(void *user_data); } GmOpened;   /* one file beside the game file */
typedef int32_t (*GmOpenFn)(void *user, const char *path, GmOpened *out);   /* path relative to the game file's folder; 0 ok, 1 none, else error */
GmStatus    gm_ids_read_bytes(const uint8_t *data, size_t len, const char *file_name, GmOpenFn open, void *open_user, GmIds **out);
GmStatus    gm_ids_read_callback(uint64_t size, GmReadFn read, void *user, const char *file_name, GmOpenFn open, void *open_user, GmIds **out);
GmStatus    gm_ids_read_fd(int fd, const char *file_name, GmOpenFn open, void *open_user, GmIds **out);   /* open may be NULL: no siblings; NOT_FOUND: no id */
const char *gm_ids_get(const GmIds *ids, uint32_t field /* GmIdField: serial, game id, title id, CHD SHA-1, zip CRC, PSP title, PSP disc id, Xbox id, RA md5, primary */);
int64_t     gm_ids_zip_size(const GmIds *ids);   void gm_ids_free(GmIds *ids);
GmRaHasher *gm_ra_hasher_new(const char *file_name, int64_t size /* -1 unknown */);   /* NULL: the file is not hashed here */
void        gm_ra_hasher_update(GmRaHasher *h, const uint8_t *data, size_t len);
const char *gm_ra_hasher_finish(GmRaHasher *h);   /* lower-case hex MD5 or NULL; owned by the handle, freed with gm_ra_hasher_free */
void        gm_ra_hasher_free(GmRaHasher *h);
typedef struct { GmDiscIdStatus status; GmDiscRegion region; GmDiscSystem system; char id[9]; } GmDiscId;   /* status: ok, too short, bad magic, unsupported container, unreadable */
GmStatus    gm_disc_id_bytes(const uint8_t *data, size_t len, GmDiscId *out);   /* also gm_disc_id_callback, gm_disc_id_fd; Ok whenever *out is filled */
/* added in 0.5.0 (ABI stays 1): rendering a GmScene. Frames are 60 to the second, looped by the banner's clips */
float       gm_render_frames(const GmScene *scene);                     /* loop length in frames; 0 when still */
GmStatus    gm_render_soft(const GmScene *scene, float frame, uint32_t width, uint32_t height, uint8_t *out, size_t out_len);
                                                                        /* RGBA8, top row first, out_len == w*h*4, at most 64 Mpx */
typedef const void *(*GmGlLoadFn)(void *user_data, const char *name);  /* "glDrawElements" -> address, or NULL */
GmStatus    gm_gles_create(const GmScene *scene, const void *(*load)(void*, const char*), void *user_data, GmGles **out);
                                                                        /* in the current context; ERR_UNSUPPORTED: a GL function missing, NOT_FOUND: nothing drawable */
GmStatus    gm_gles_draw(GmGles *gles, float frame, int32_t x, int32_t y, int32_t width, int32_t height);   /* clears and draws that viewport */
void        gm_gles_destroy(GmGles *gles);                              /* context current; the scene handle may be freed first */
/* Wii U: the files of one title through a callback; a .wua is an ordinary file for gm_read_*. */
typedef int32_t (*GmFileFn)(void *user, const char *path, uint8_t *buf, size_t cap, uint64_t *size);  /* 0 ok, 1 none, else error */
GmStatus gm_read_title_files(GmFileFn files, void *user, GmInfo **out);
GmStatus gm_title_image(GmFileFn files, void *user, uint32_t kind /* GmImage */, GmPixels **out);
GmStatus gm_wua_image(uint64_t size, GmReadFn read, void *user, uint32_t kind, GmPixels **out);
const uint8_t *gm_pixels_data(const GmPixels *pixels, uint32_t *width, uint32_t *height);
void        gm_pixels_free(GmPixels *pixels);
/* added in 0.6.0 (ABI stays 1): cartridges; GmPlatform gains 7 GameBoy .. 16 GameGear */
int64_t     gm_info_version(const GmInfo *info);                     /* header version / revision, -1 none */
const char *gm_info_cart_json(const GmInfo *info);                   /* the typed header as one JSON object, NULL for other files */
/* added in 0.6.0 (ABI stays 1): Wii channel banners. GmWiiBanner is opaque, freed with gm_wii_banner_free */
GmStatus    gm_wii_banner_read_bytes(const uint8_t *data, size_t len, GmWiiBanner **out);   /* an RVZ, opening.bnr, 00000000.app or banner.bin, by content; also _callback, _fd */
uint32_t    gm_wii_banner_count(const GmWiiBanner *b, uint32_t what);  /* GM_WII_BANNER_COUNT_START_FRAMES, LOOP_FRAMES, SKIPPED_WINDOWS, SKIPPED_TEXT, SKIPPED_BOUNDS, MISSING_TEXTURES */
const char *gm_wii_banner_missing_texture(const GmWiiBanner *b, uint32_t index);        /* owned by the handle */
GmStatus    gm_wii_banner_size(const GmWiiBanner *b, float scale, uint32_t *width, uint32_t *height);
GmStatus    gm_wii_banner_render(const GmWiiBanner *b, float frame, float scale, uint8_t *out, size_t out_len);   /* RGBA8, out_len == w*h*4 */
void        gm_wii_banner_free(GmWiiBanner *b);
```

- Status codes: `OK`, `NOT_FOUND`, `ERR_ARGUMENT`, `ERR_IO` (a read failed), `ERR_UNSUPPORTED` (fd on Windows),
  `ERR_PANIC`. Every entry point runs inside `catch_unwind` (release keeps `panic = "unwind"` for this).
- Enum-typed inputs cross as `uint32_t`/`int32_t` (the enums are exported as named constants): a C caller passing an
  unknown value gets null/-1, never Rust UB.
- Strings and pixels are owned by the handle until `gm_info_free`.
- Versioning: `GM_ABI_VERSION` bumps on any incompatible change; bindings check it at load (Kotlin throws).

### Android / JVM binding: hand JNI, not UniFFI

Measured 2026-11-16 (same API surface, release profile `opt-level=s`, LTO, stripped, arm64-v8a):

| | hand JNI (`jni` 0.21) | UniFFI 0.32 |
|---|---|---|
| our .so, arm64-v8a | 307 KB | 457 KB |
| runtime dependency | none | JNA 5.17 AAR: classes.jar 220 KB + libjnidispatch.so 177 KB (arm64) / 126 KB (v7a) / 127 KB (x86_64) |
| arm64-only APK cost (uncompressed) | 307 KB | ~854 KB |
| Kotlin to maintain | 182 lines, hand-written | 1524 generated lines (+ a bindgen version pin) |
| host jar for JVM unit tests | loader in the same source | needs JNA + its desktop natives too |

JNI wins on size (about a third of the APK cost), on dependencies (the consumer does not ship JNA today) and on
simplicity; UniFFI's advantage (generated multi-language bindings) does not pay while Kotlin is the only binding and
the C ABI covers everything else. The surface is small and stable: a handle, a string array, a number array, the
icon pixels. Revisit if a Swift binding is wanted (UniFFI would then generate both).

The Kotlin API (`gamemeta.GameMeta.read(bytes|fd, fileName): GameInfo?`) decodes one handle in four JNI calls and
frees it in `finally`. Not found is null; an unreadable descriptor throws `IOException`; an ABI mismatch fails class
init. R8: the AAR ships `proguard.txt` keeping `gamemeta.GameMeta`'s native methods.

0.4.0 adds `readIds(bytes|fd, fileName, opener: FileOpener?): Ids?`, `raHasher(fileName, size): RaHasher?` (`update`, `finish`),
`discId(header|fd): DiscIdResult` and the `FileOpener` / `FileSource` interfaces (a sibling file: `size`, `read(offset, length)`, `close`);
the JNI side calls them by name, so the AAR's consumer rules keep their methods from R8 (as for `TitleFiles`). An exception thrown
by a `FileSource` is a failed read of that sibling, never a crash.

0.5.0 adds `BannerRenderer.open(bytes|fd)` (`frames`, `render(frame, w, h): RgbaImage` on the CPU, `createGles(glLibrary)`:
a `GlesBanner` with `draw(frame, x, y, w, h)` in the calling thread's current context, or null). The JNI side loads the GL
functions with `dlopen`/`dlsym` from `glLibrary` (default `libGLESv3.so` on Android, `libGLESv2.so.2` elsewhere).

0.6.0 adds `WiiBanner.open(bytes|fd)` (`startFrames`, `loopFrames`, `skipped`, `missingTextures`, `size(scale)`,
`render(frame, scale): RgbaImage`, `poster(scale)`; null when the file has no banner, is encrypted or does not parse).

A handle crosses as a `jlong`: a pointer, or `-status` in `-255..0`. The split is by range, not sign: Android's
arm64 allocator (Scudo) tags heap pointers in the top byte (`0xb4..`), so a real handle is negative. Desktop JVM tests
never see that; the first on-device run did (every read came back as a bogus status).

Measured on a Galaxy S21 (a debug build, 21 3DS files from the maintainer's own library, read by fd from
SAF descriptors; Kotlin is the original reader on the same file, run right after):

| | first call (incl. `System.loadLibrary`) | later reads | Kotlin reader | differences |
|---|---|---|---|---|
| arm64-v8a | 18.9 ms | 1.2-3.8 ms | 5.9-30.9 ms | 0 of 21 |
| armeabi-v7a (armv7 on the arm64 SoC, `install --abi`) | 4.8 ms | 1.2-6.3 ms | 10.3-29.7 ms | 0 of 21 |

APK cost (0.1.0, 3DS only): the four `.so` files are stored uncompressed (307 520 + 227 028 + 340 000 + 346 308 bytes
= 1.22 MB for a universal APK, about 0.3 MB per ABI split) plus 22 KB of classes.

0.6.0 (adds the cartridge readers and the Wii banner module, measured together against 0.5.0): arm64-v8a 1 219 016 (+103 648),
armeabi-v7a 906 328 (+72 024), x86_64 1 370 720 (+119 984), x86 1 426 332 (+121 080). Host libraries: linux-x86_64 1 346 384
(+119 872), windows-x86_64 1 305 600 (+141 824), macos-aarch64 1 232 992 (+100 272).

0.5.0 (adds both renderers and `libm`): arm64-v8a 1 115 368 (+153 432), armeabi-v7a 834 304 (+112 904), x86_64 1 250 736
(+185 528), x86 1 305 252 (+195 464) = 4.51 MB for a universal APK, about 1.13 MB per ABI split. Host libraries: linux-x86_64
1 226 512 (+185 416), windows-x86_64 1 163 776 (+223 232), macos-aarch64 1 132 720 (+149 456).

0.4.0 (adds the ids module and `md5`): arm64-v8a 961 936 (+88 952), armeabi-v7a 721 400 (+57 968), x86_64 1 065 208 (+89 992), x86
1 109 788 (+93 216) = 3.86 MB for a universal APK, about 0.96 MB per ABI split. Host libraries: linux-x86_64 1 041 096 (+89 720),
windows-x86_64 940 544 (+91 136), macos-aarch64 983 264 (+84 064). Nearly all of it is the readers (ISO 9660, PSP SFO, zip central
directory, the C ABI and JNI glue); `md5` is a few KB (the builds are stripped with LTO, so it cannot be isolated: an estimate).

0.3.0 (adds banner3ds-scene, no new dependency): arm64-v8a 872 984 (+191 448), armeabi-v7a 663 432 (+138 528), x86_64 975 216
(+225 816), x86 1 016 572 (+245 668) = 3.53 MB for a universal APK, about 0.88 MB per ABI split. Host libraries: linux-x86_64
951 376, windows-x86_64 849 408, macos-aarch64 899 200.

0.2.0 (all of the above): arm64-v8a 681 536 (+374 016), armeabi-v7a 524 904 (+297 876), x86_64 749 400 (+409 400),
x86 770 904 (+424 596) = 2.73 MB for a universal APK, about 0.68 MB per ABI split. Where it goes, on arm64 (symbol
sizes of an unstripped build, `-Os` + LTO, so approximate): the readers 140 KB, `encoding_rs` 140 KB, `ruzstd` 24 KB,
`miniz_oxide` 19 KB, the rest std/alloc/jni.

### Scene API (banner3ds-scene)

```rust
pub struct BannerScene { models, textures: BTreeMap<String, RgbaImage>, animations: BannerAnimations, unsupported: BTreeSet<String>,
                         lights, light_sets, cameras, environment_cameras, vertex_lights, hemisphere_lights, scene_ambient }
// BannerModel -> BannerMesh { vertices: Arc<VertexData>, indices, skinning, bone_palette, material: Arc<BannerMaterial>, .. };
// BannerMaterial: texture slots + coordinators, 6 TexEnv stages, lighting (LUTs, fresnel, bump), blend, depth, alpha test.
// BannerAnimation { tracks: Vec<AnimTrack { target, kind: TrackKind, component, keys: Keyframes, slot }> }; Keyframes::sample(frame).
pub fn cbmd::Cbmd::parse(bytes) -> Option<Cbmd>;  Cbmd::scene(&self) -> Option<BannerScene>;   // USA English CGFX, else EUR English, over the common one
pub fn cgfx::parse(bytes) -> Option<BannerScene>;  cgfx::parse_with_localized(common, localized);  cgfx::textures(bytes);
pub fn source::read<S: ByteSource + ?Sized>(source: &mut S) -> Result<BannerScene, SceneError>;  // NotFound | Encrypted | Damaged
pub fn dump::scene_json(&BannerScene) -> String;   // sorted keys, floats to 6 significant digits, textures as size + SHA-256
```

The shape mirrors the original reader's `BannerScene` (a renderer-neutral description: no GL types, no I/O). Animations
are data; a renderer samples them with `Keyframes::sample` (clamped outside the keys, stepped, hermite or linear).
`unsupported` lists what a file uses that the scene does not model (`dot3-combiner`, `camera-frustum`, `light-spot`,
`animation-element-<type>`, `animation-damaged`, ...), so a renderer can say what it skipped.

Parity with the original implementation: the original implementation's test builders wrote 71 banner and CGFX inputs (the 18 feature banners and 53
single-feature cases, truncated and garbage variants included; `fixtures/scene`), and a throwaway dumper (not in this
repository) wrote the original reader's scene of each as JSON. The Rust parse must reproduce all 71 dumps byte for byte
(`tests/golden.rs`), and the original implementation's parse-level tests are ported one per test (43, `tests/kotlin_parity.rs`).
Deliberate differences: positions are 64-bit while walking (no 32-bit wrap), one parse may allocate at most 256 MiB in
total (vertices, indices, texture pixels, keys: a hostile file can point thousands of entries at the same data), a CBMD
CGFX is capped at 8 MiB, and read errors inside an animation drop that animation (`animation-damaged`) as the original implementation
does, while any other read error fails the whole parse.

### Renderer API (banner3ds-soft, banner3ds-gles)

```rust
// soft
pub fn render(scene: &BannerScene, frame: f32, width: u32, height: u32, out: &mut [u8]) -> Result<(), RenderError>;   // BadSize | BadBuffer
pub struct Renderer<'a>;  Renderer::new(&scene);  .frames();  .player();  .render(frame, w, h, out);
                          .render_with(frame, w, h, &Options { background, backdrop, camera, clip_frame }, out);
// gles: every call needs the context `gl` was loaded for current on the calling thread
pub unsafe fn Gl::load(load: impl FnMut(&CStr) -> *const c_void) -> Result<Gl, MissingFunction>;   // 52 GLES 3.0 entry points
pub unsafe fn Renderer::new(scene: &'a BannerScene, gl: Gl) -> Result<Renderer<'a>, Error>;     // Error::NothingDrawable(why)
pub unsafe fn draw(&mut self, frame: f32, viewport: Viewport);                                   // also draw_with(.., &DrawOptions), draw_pose
pub unsafe fn destroy(self);                                                                     // deletes every GL object it made
```

Frames count 60 to the second; the turntable and the clips loop on the banner's length (`frames()`, `gm_render_frames`).
Both draw through HOME's fixed view over transparent black, so the same frame number gives the same picture.

How gles matches soft: the CPU poses and skins every mesh with soft's code (`scene::draw`) and uploads world-space positions
and normals each frame, so only view-projection is on the GPU. Sphere maps are per fragment, linear and clamp-to-border
slots sample with soft's rules written out in GLSL (`texelFetch`, neighbours clamped to the edge), blend codes 10-13 are
`ONE` as in soft, lighting tables sit in one RGBA32F atlas read with `texelFetch`. Nothing outside WebGL 2 is used (no
extension, no `GL_CLAMP_TO_BORDER`, no geometry shaders); a feature WebGL 2 cannot do is emulated in the shader.

**Tolerances.** soft against the original Kotlin `SoftwareRenderer` (its goldens, `fixtures/render`, 54 feature frames =
18 feature banners x 3 times, and 40 drawable cases): at most 1 per channel and a mean of 0.01, allowing `libm` against the
JVM's 1-ulp differences; measured 94/94 bit-exact (max 0, mean 0). gles against soft, per frame: at most 1% of pixels whose
worst channel differs by more than 8, and a mean difference of at most 0.5 per channel. Measured on llvmpipe: features 54/54
(worst share 0.67%, worst mean 0.418, max 206), cases 40/40 (worst share 0.01%, mean 0.039, max 255), the original implementation's
`DesktopGlRendererTest` quad bit-exact. What is left is rasterisation tie rules: an edge exactly on pixel centres (soft's
inclusive `w >= 0` against GL's top-left rule) flips single pixels on quad seams (`combiner-modes`, `camera-framing`).

**Reference GL against reference soft** (soft is the original implementation; the Rust gles follows soft where they differ): blend codes
10-13 (the original implementation GL read the zero blend constant; here `ONE`), alpha test equal / not-equal (GL compares with a 0.002
tolerance; kept, since 8-bit alpha lands within it), bump maps (GL takes tangents from `dFdx`; the same on flat triangles),
sphere maps (the original implementation GL was per vertex; here per fragment), linear filtering across a texture's edge (the original implementation GL
wrapped the neighbours; here clamped as soft does). The original implementation's GL against its CPU measured max 206, mean 0.3-0.8 and
1.4-2.9% of pixels over 8 on the same 54 frames (`BannerGlBench`); the Rust gles brings that to at most 0.67%.

**Speed** (`cargo bench -p banner3ds-gles --bench frames`, 400 x 240, the 18 feature banners, this Linux host, 12 cores,
load about 4): soft 16.6-19.9 ms a frame (mean 18.2 ms, 55 fps); gles on llvmpipe 0.14-0.23 ms (6 256 fps), 0.21-0.30 ms with
`glReadPixels` (4 186 fps). The original implementation `BannerGlBench` on the same host (load about 30): CPU 52-77 ms, GL 0.27-1.17 ms,
0.47-1.37 ms with readback.

### Wii banner API (`gamemeta_core::wii::banner`)

```rust
pub struct WiiBanner;   // layout + start / loop animations + textures, as data
impl WiiBanner {
    pub fn open(banner_bin: &[u8]) -> Option<WiiBanner>;    // U8, plain / LZ77 / IMD5; first .brlyt, .brlan named *start* / *loop*, <= 64 .tpl
    pub fn start_frames(&self) -> u32;  pub fn loop_frames(&self) -> u32;  pub fn layout_size(&self) -> (f32, f32);
    pub fn skipped(&self) -> &[(&'static str, u32)];        // "wnd1", "txt1", "bnd1": kept in the tree, not drawn
    pub fn missing_textures(&self) -> &[String];            // named by the layout, absent from the archive
    pub fn pixel_size(&self, scale: f32) -> Option<(u32, u32)>;                      // None: scale not finite and > 0, or > 8192 px / 64 Mpx
    pub fn render_into(&self, frame: f32, scale: f32, out: &mut [u8]) -> Option<(u32, u32)>;   // RGBA8, not premultiplied, over transparent
    pub fn render(&self, frame: f32, scale: f32) -> Option<RgbaImage>;  pub fn poster(&self, scale: f32) -> Option<RgbaImage>;
}
pub fn banner_bin(opening_bnr: &[u8]) -> Option<Vec<u8>>;                            // the archive inside an opening.bnr
// gamemeta_core::wii: channel_banner_bin / channel_banner_archive / open_channel_banner(source) from an RVZ, opening.bnr or banner.bin;
// channel_banner(source, file_name) -> the poster at scale 1 (608 x 456 at rest) for an RVZ.
```

Frames count 60 to the second: the start animation plays once, the loop repeats after it; `poster` is the start finished.
Every `Layout::parse` and `Animation::parse` read is bounds-checked (`Option`), nothing panics. Module layout: `be` (reader),
`layout` (BRLYT), `anim` (BRLAN, `Track::at`), `render`; the archive code is the wii reader's own (`opening.rs`, `tpl.rs`).
It is a module of core, not a crate: it needs no dependency beyond `libm` and the wii reader's code, and the C ABI and JNI
already link core.

Parity with the original implementation: the original implementation's test builders (`BrlytWriter`) wrote 35 authored banners; a throwaway test in a
scratch checkout of the original implementation (never committed there; the oracle is not in this repository) rendered them at 113 frames
(start, middle, loop, scales 1 to 2.5, the poster) and wrote the PNGs (`fixtures/wii-banner`). `gamemeta-fixtures` rebuilds
every archive byte for byte (drift test) and the Rust renderer must equal every PNG: measured 113/113 frames bit-exact (max
channel difference 0, mean 0). The float and double mix of the original implementation is replayed operation for operation (`Math.round` as
`floor(x + 0.5)`, `toRadians` as a multiply, `cos`/`sin` in double through `libm`). The 21 `WiiBannerTest` checks are ported one
per test (`tests/wii_banner.rs`), plus the `channelBanner` check, so the module, its fixtures and the real-banner checks
below are all under test.

Deliberate differences from the original implementation: offsets are unsigned (no signed-offset wrap); caps before allocating: 4096 panes,
4096 materials, nesting depth 256, 4096 sections, 2^20 keys, 2^16 tracks, 64 texture files, 16 M decoded texture pixels in total,
a frame of at most 8192 pixels a side and 64 Mpx, scale finite and above 0; a NaN width or height is refused; a NaN rounds
to 0. Where the original implementation threw (a NaN size, a huge frame) the Rust returns `None`.

**Not drawn** (the same list as the original implementation, so expect these to differ from a Wii): text (`txt1`), window frames (`wnd1`),
bounding boxes (`bnd1`), X/Y rotation and depth, TEV stages (approximated as texture x vertex colour x material colour, further
texture maps over by alpha), blend modes (all "over"), texture coordinate generators, indirect textures.

**Format guesses** (the first list is the state before the real-banner check below; guess 1 and 6 are now settled, see the
corrections there and `docs/wii-references.md`):

1. `mat1` flag bits (texture maps 28-31, SRTs 24-27, coordinate generators 20-23, channel control bit 4, material colour bit 3).
2. `txl1` offset base and stride; 3. `pai1` offset bases; 4. `pic1` fields at 0x4C; 5. texture SRT scale and rotation pivot on
the texture centre; 6. an animation is the "start" one or the "loop" one by its file name.

**Notes from an optional private reference set (2026-11-17).** Real banners belonging to the maintainer, read in place with the CLI
(`wii-banner --bin`), drew identically in this renderer and the original Kotlin (both followed the same guesses), so the port is
faithful; the guesses themselves were what the real files contradicted. Nothing from that set is in this repository and none of it
is needed to build or test the authored banners. Findings, now corrected:

- **Guess 1 was wrong.** The `mat1` flag word (at material offset 0x3C) holds its counts in the *low* nibbles: texture maps bits 0-3,
  texture SRTs 4-7, coordinate generators 8-11 (material sizes against flag words fit exactly, e.g. `0x0111` -> 28 bytes after the
  flag word = 1 map (4) + 1 SRT (20) + 1 generator (4)). The blocks after the generators follow by their own bits: channel control
  25, material colour 27, TEV swap 12, indirect SRTs 13-14, indirect stages 15-17, TEV stages 18-22, alpha compare 23, blend mode 24.
  The original implementation read the top nibbles, so a real banner drew blank or garbled.
- Guess 2 holds: `txl1` entries are 8 bytes (name offset, 0), offsets from the first entry. Guess 4 holds: the `pic1` colours start
  at section offset 0x4C. The `mat1` offset table and the material layout up to the flag word (name 20, three TEV colours, four
  konst colours) hold. A texture coordinate generator is `{type, source, matrix, 0}` with real values `01 04 1e 00`.
- Guess 6 was partly wrong: many archives have a single `banner.brlan` (no "start" / "loop" in its name): it is the loop.
- The first two TEV colour registers are what an intensity texture maps black and white to; I4/I8 intensity is also alpha.
- Guess 5 (SRT pivot) is not judged. `wnd1` and `txt1` occur in real banners and are counted in `skipped`, as designed.

Against the emulator's Wii Menu tile (the channel icon, `icon.bin`; method, set layout and open list in `docs/wii-references.md`) the
mean difference per channel went from 40-130 to 13-16 on the entries that match; TEV stages (colour/alpha texture pairs, layered
logos) are what remains. The authored archives were rewritten to the corrected layout (35 archives; the 113 Kotlin-parity frames
unchanged bit for bit: the authored banners use the same features). Tests: `tests/wii_banner.rs`, and `tests/wii_references.rs`
against an optional private reference set that is not part of this repository (it skips without it).

## Formats

| Reader | Files | Gives |
|---|---|---|
| `threeds` | `.3ds .cci .cxi .app .cia` | titles (12 languages), publisher, icons, ids, regions, ratings, banner extent |
| `nds` | `.nds .dsi .srl .ids`, first ROM of a `.zip` | titles (up to 8), icon, DSi animated icon + banner block, game code, maker, ratings |
| `gc` | `.iso .gcm .ciso .gcz .rvz .wia` | `opening.bnr` (BNR1/BNR2) titles, banner image, region, header title, game id |
| `wii` | the disc containers + `.wbfs .wia`, `.wad` | header title, game id, region; from an RVZ or WIA (any compression) also `opening.bnr` titles and the logo; WAD title id and game id |
| `wii::banner` | `meta/banner.bin` (plain, LZ77 or IMD5), an `opening.bnr` or `00000000.app`, the RVZ's banner | the channel banner's layout, animations and textures; frames drawn on the CPU at any scale; counts of panes not drawn |
| `ids` | by extension: `.iso` (PS1/PS2 serial from `SYSTEM.CNF`, PSP `PARAM.SFO`, GC/Wii id), `.bin/.img` (raw 2048 or 2352-byte sectors, mode 1 or 2), `.cue` (first data track via the opener), `.pbp`, `.chd`, `.zip`, `.nsp`, `.xbe`, `.wad`, `.3ds`/`.cia`, `.rpx`/`.wua`, the GC/Wii containers; RA hashing by name | serial (normalized `SLUS00003`), game id, title id, CHD raw SHA-1, zip ROM CRC-32 + size, PSP title + disc id, Xbox title id, RA MD5, disc id with region/console or the reason for none |
| `sony` | PSP `EBOOT.PBP` and PSP `.iso`; PS3 and Vita title folders through a file callback; Vita `.vpk` | title, disc or title id, region, `ICON0.PNG` icon, `PIC1.PNG` background on demand, `ICON1.PMF` bytes; PS3 per-language titles |
| `wiiu` | unpacked title (via `.rpx`), `.wua` | titles (12), publisher, ids, region, icon, boot media sizes, splashes on demand |

| `carts` | `.gb .gbc .sgb`, `.gba .agb`, `.z64 .v64 .n64`, `.sfc .smc .swc .fig`, `.nes`, `.md .gen .smd .32x` and a Genesis-shaped `.bin`, `.sms .gg`, and one ROM in a `.zip` | per family, the title, game / serial code, maker, version, region, size fields and the header's own check values (see below) |
| `banner3ds-scene` | a CBMD or CGFX, or the banner inside a decrypted `.3ds .cci .cxi .app .cia` | models, meshes, materials, textures, skeleton, cameras, lights, animations |

Cartridge readers (facts only, from each console's public header documentation; see NOTICE.md). All are header-only:

| Family | Fields read | Checks |
|---|---|---|
| Game Boy / Color | title (shortened when the CGB flag and manufacturer code take its tail), manufacturer code, CGB / SGB flags, new or old licensee, cartridge type, ROM and RAM size, destination, version, the stored checksums | logo, header checksum; global checksum through `checksum_matches` |
| Game Boy Advance | title, game code (region from its last letter), maker, version | the fixed byte `0x96`, the complement over `0xA0..=0xBC` |
| Nintendo 64 | title (trimmed), game code, country, version, both CRCs, byte order | `.z64`, `.v64`, `.n64` are told by the first word; the CRCs are reported, not validated |
| SNES | title (JIS X 0201), map mode, fast ROM, ROM type, ROM and RAM size, destination, developer id or maker and game code (extended header), version, the checksum pair, the layout found, whether a copier header was skipped | LoROM, HiROM and ExHiROM windows are scored and the best wins (minimum score 6); a 512-byte copier header is skipped when the size is `1024n + 512` |
| NES | iNES, NES 2.0 or archaic: PRG ROM / CHR ROM / RAM / NVRAM sizes (including NES 2.0's exponent form), mapper, submapper, mirroring, battery, trainer, console type, timing | no title exists in the format, so none is invented |
| Genesis / Mega Drive / 32X | system, copyright, domestic and overseas titles, serial, revision, checksum, I/O, ROM range, region letters or the hex digit | `SEGA` at 0x100 (or 0x101); an SMD (512-byte header, 16 KiB blocks, odd bytes first) is de-interleaved; the word sum through `checksum_matches` |
| Master System / Game Gear | region code, product code, version, size code, checksum, which offset held the header, Game Gear or Master System | `TMR SEGA` at 0x7FF0, 0x3FF0 or 0x1FF0; the sum through `checksum_matches` |

- A `.zip` holding one ROM is read through the same readers, inflated only as far as the family's header reaches.
- **RetroAchievements N64 hash**: RA hashes the big-endian image, so `RaHasher` normalizes a `.v64` (byte-swapped) or `.n64`
  (little-endian) file word by word, by the order its first word shows, before hashing (regression test in `carts_n64.rs`).
- Not done: the N64 CIC guess (the bootcode CRCs cannot be checked offline and a fixture cannot cheaply carry one), N64 CRC
  validation, and the SNES whole-ROM checksum (only the complement pair is checked).

- **No keys, never a decryption.** A Wii partition is read only where the container already holds it decrypted (RVZ);
  an ISO, WBFS, CISO or GCZ Wii disc gives its unencrypted header only; `.wud`/`.wux` give nothing.
- Containers are told by magic, not extension (CISO, GCZ, RVZ, WIA, WBFS); the disc type by the GC (0xC2339F3D) or Wii
  (0x5D1C9EA3) magic. An RVZ is walked through its raw-data table, group table, packing and partition runs, with
  every length capped before allocating.
- Parity with the original Kotlin readers: its test suites are ported one test per test onto synthetic fixtures built
  in `gamemeta-fixtures` (3DS 18, NDS 18, GC 23, Wii 44, Wii U 25), plus extra Rust tests. Red first, then green.
- Differences from the original implementation, all deliberate: a failure inside the Wii `opening.bnr` U8 walk drops only the logo
  (Kotlin nulls the whole result); a numeric entity naming a lone surrogate becomes U+FFFD; Shift_JIS decodes through
  `encoding_rs` (WHATWG tables; the JVM's differ on a few rare bytes); lengths are bounded before allocating (Kotlin
  allocates up to its cap); case-insensitive path matching is ASCII only.
- Not ported, because the core does no I/O: `RelativePath` and `WiiUFolderFiles` (an app resolves `../meta/...` and
  opens folders; the core gives it `TitleFiles` / `OpenerFiles`, and the CLI shows the pattern).
- A WIA reads like an RVZ: its groups are bzip2 (an own decoder, `bzip2.rs`), LZMA or LZMA2 (`lzma-rust2`, no_std) as well as zstd or stored. An LZMA stream has no end marker, so a group is decoded twice: once for a prefix that gives its size (the exception lists and the sector count), once to that size.

### Wii banners: what replaces what

| Rust (`gamemeta_core::wii::banner`) | Kotlin it replaces (`dsmeta/wiibanner/`) |
|---|---|
| `mod` (`WiiBanner::open`, `render`, `poster`; `wii::channel_banner`), reusing `opening.rs` and `tpl.rs` | `WiiBanner.kt`, `WiiMetadata.channelBanner` |
| `layout` | `Brlyt.kt` |
| `anim` | `Brlan.kt` |
| `render` | `WiiBannerRenderer.kt` |
| `be` | `Be.kt` |

Tests: `WiiBannerTest` 21/21 and the `channelBanner` check ported (`tests/wii_banner.rs`, 26 tests with the archive drift test,
the oracle parity test, `banner_bin` and the hostile-scale regression found by the fuzzer), ABI 5, CLI 2, Kotlin 1, fuzz
`wii_banner`. `BrlytWriter.kt` is `gamemeta-fixtures::wii_banner` (35 cases in `wii_banner_cases`). The reference's "not drawn" and
"format guesses" lists are the two lists in Wii banner API above, with what the real banners said. The banner slot of the art
pipeline waits for the TEV stages.

### Ids readers: what replaces what

| Rust (`gamemeta_core::ids`) | Kotlin it replaces | Ported tests (Rust, passing) |
|---|---|---|
| `iso9660`, `playstation`, `gamecube` (container ids through `wii::disc_header`) | `DiscIds.kt` | in `HeaderIdReaderTest` below |
| `mod` (`read`, `read_with`, `Ids`), `small` (CHD, zip, NSP) | `HeaderIds.kt`, `NspIds.kt` | `HeaderIdReaderTest` 19/19 |
| `ra` (`RaHasher`, `ra_hash`) | `RaHasher.kt` | `RaHasherTest` 6/6 |
| `psp` | `PspParamSfo.kt` | `PspParamSfoTest` 10/10 |
| `disc_id` | `DiscId.kt` (frontend) | `DiscIdTest` 12/12 |

47 reference tests, one Rust test per test (`tests/ids_parity.rs`, red first: 0 passed / 47 failed against stubs, then 47 green),
plus 12 extra Rust tests (`tests/ids_extra.rs`: raw `.bin` of either mode, PBP fields, truncated CISO, zip64, hostile NSP and
directory sizes, a bounded-read check over every reader), 10 C ABI tests (`tests/ids_abi.rs`), 1 CLI test, 5 Kotlin tests.
The ABI, JNI, CLI and Kotlin tests were written after the glue they cover, not red first.

Where `DiscIds.gameCubeWii` and `DiscId.read` overlap there is one container-and-header implementation: `ids::game_id` (inside
`Ids.game_id`) is container-aware (plain, RVZ, WIA, WBFS, CISO, GCZ) and reads headers only, through `wii::disc_header`;
`ids::disc_id` / `disc_id_of` is the refusing reader over the first 2048 bytes (lower-case ids accepted as the Kotlin did, WBFS /
CISO / GCZ refused with a reason, plus the XBE certificate). Reference `titleId` is `Ids::primary_id()`.

Not ported: `LocalIdentityTest` (5) and `RomIdentityScannerTest` (8) test the library scanner and its cache (stamps, resumable
runs, merging stored extras), which is application plumbing; the one reader case in them (a `.rpx` title id through the `meta`
folder) is in `HeaderIdReaderTest`. Platform arguments (a `.rpx` only on "switch", a `.wad` only on "gc") are the caller's gate:
the Rust entry takes none, so those assertions are dropped.

Deliberate differences from the original implementation: a GCZ gives a game id (Kotlin gave none); an NES file under 16 bytes gives no hash;
a `.pbp` also fills `serial` (its `DISC_ID`); one ISO 9660 implementation for plain and raw sectors (block size from the volume
descriptor, directory capped at 256 KiB, malformed names skipped instead of ending the walk); one SFO parser (the printable-text
rule applies to `DISC_ID` too); the PSP serial is normalized with Unicode letters + ASCII digits; every length is bounded before
allocating; `ra_md5` is never filled by `read` (the caller streams the file).

## Dependencies

Runtime crates added in 0.2.0, 0.4.0 and 0.5.0, all pure Rust and `no_std`-capable. Allowed licenses: MIT, Apache-2.0, BSD, zlib,
Unlicense; nothing GPL, LGPL or MPL. (`jni` 0.21 and its tree were already in 0.1.0.)

| Crate | License | Used for | arm64 `.so` size |
|---|---|---|---|
| `miniz_oxide` 0.9 (+ `adler2`: 0BSD OR MIT OR Apache-2.0) | MIT OR Zlib OR Apache-2.0 | raw deflate in a `.zip` (DS ROM), zlib in GCZ | about 19 KB |
| `ruzstd` 0.9 (+ `twox-hash` 2: MIT) | MIT | zstd in RVZ groups and ZArchive blocks, into bounded buffers | about 24 KB |
| `encoding_rs` 0.8, `default-features = false`, `alloc` | (Apache-2.0 OR MIT) AND BSD-3-Clause | Shift_JIS for Japanese GameCube banners | about 140 KB |
| `md5` 0.8.1, `default-features = false` (no_std) | Apache-2.0 OR MIT | the streaming RetroAchievements hash (`RaHasher`) | a few KB (not isolable in a stripped LTO build) |
| `libm` 0.2.16 | MIT | no_std `sin`/`cos`/`pow`/`sqrt`/`atan2` for `scene::draw`, both renderers and (since 0.6.0, in core) `wii::banner` | a few KB of the 0.5.0 growth |

Test-only (never linked into the artifacts): `khronos-egl` 6 (MIT OR Apache-2.0, `dynamic`: loads the system `libEGL` at run time)
for the headless GL context in `gamemeta-fixtures` (feature `gl`).

Windows-1252 (GameCube banners) is a hand table with Java's semantics (the five undefined bytes are U+FFFD). `encoding_rs` is the one costly
choice: its decoder drags in every legacy encoding's tables. A dedicated Shift_JIS table would be roughly 15-20 KB,
about 120 KB smaller; left for when the size matters. `rust-version` is 1.87 (`ruzstd`, `miniz_oxide`).
Build-only crates (`proc-macro2`, `syn`, `quote`, `unicode-ident`) are not linked into the artifacts.

## Distribution

`scripts/release` -> `dist/game-metadata-<v>/`:

- `game-metadata-<v>.aar`: classes.jar (Kotlin, JVM 17 bytecode), `jni/{arm64-v8a,armeabi-v7a,x86_64,x86}/libgamemeta_jni.so`
  (NDK clang, API 27, 16 KB-aligned segments), consumer ProGuard rules, manifest (minSdk 27).
- `game-metadata-host-<v>.jar`: the same classes + `gamemeta/native/{linux-x86_64,macos-aarch64,windows-x86_64}/`
  libraries (macOS via cargo-zigbuild, Windows via cargo-xwin; both optional, skipped with a note when the tool is
  missing). Verified loading on Linux, macOS arm64 and Windows x64.
- `gamemeta.h`, `SHA256SUMS`. Zips have fixed timestamps (reproducible given the same toolchain).

Consumers commit the pinned artifacts, so their build hosts need no Rust. The release script also runs the open-door
check: `cargo check` of core + ffi for `aarch64-unknown-linux-gnu` (no Linux artifact).

## Testing

- Parity: `crates/gamemeta-core/tests/threeds_parity.rs` ports the original Kotlin suite one test per test, on the
  same synthetic fixtures (`gamemeta-fixtures` mirrors the original fixture builders byte for byte).
- Extra Rust tests: icon tile order, the small icon, plain CIA, the sample fixture, text rules.
- C ABI tests through the extern functions; a C program compiled against the header (`examples/c/dump.c`).
- Kotlin: `bindings/kotlin` test loads the host library and reads `fixtures/sample.3ds`.
- Fuzz (5 minutes each, `cargo +nightly fuzz run <target> -- -max_total_time=300 -max_len=300000`, seeds from
  `write-fuzz-seeds fuzz/corpus`): `threeds_walk`, `nds_walk` (5.97 M runs), `gc_walk` (619 792), `wii_walk` (2 228 615),
  `wiiu_walk` (252 549), `banner3ds_scene` (1 213 279 runs, seeded with every `fixtures/scene` input, `-max_len=300000`); all clean. Each asserts no panic, in-range extents, consistent sizes, well-formed images.
- Kotlin: also reads `sample.nds`, `sample-nds.zip`, `sample-gc.rvz`, `sample-wii.rvz`, `sample.wad`, `sample.wua`, and a
  Wii U title through a `TitleFiles` object.
- Wii banners: parity 113 frames bit-exact (35 authored banners, `fixtures/wii-banner`), 21 `WiiBannerTest` + `channelBanner`
  ported, drift, hostile-scale regression (26 in `tests/wii_banner.rs`), ABI 5, CLI 2, Kotlin 1. Fuzz `wii_banner` (byte 0 the scale, byte 1 the frame, the rest a `banner.bin`;
  also the RVZ / `opening.bnr` readers): 2 755 783 runs in 301 s, clean (it found a harness bug, a denormal layout size
  overflowing the scale; the library refuses it).
- Scene: goldens 18 + 53, ported tests 43, source reader 5, ABI 6, CLI 1, Kotlin 2 (a bare CBMD against its golden, unsupported flags).
- Ids: parity 47 (HeaderIdReaderTest 19, RaHasherTest 6, PspParamSfoTest 10, DiscIdTest 12), extra 12, ABI 10, CLI 1, Kotlin 5
  (the 12 id fixtures under `fixtures/ids/` are drift-tested like the others); fuzz `ids_walk` (first byte picks the file name, the
  rest is the file, every sibling is the same bytes; also checks the chunked RA hasher against the whole-file hash): 2 146 614
  runs in 301 s, clean.
- Renderers: soft against the original goldens (`fixtures/render`, 54 feature frames + 40 cases, `tests/kotlin_goldens.rs`),
  ported `SoftwareRendererTest` 3 and `LitRenderTest` 2 (+1 size check); gles ported `GlslTest` 5 and `GlLightingTest` 3
  (`tests/glsl.rs`), and `tests/headless.rs` (`harness = false`, run by `cargo test`): the ported `DesktopGlRendererTest`,
  every feature and case against soft within the tolerance, a viewport, a context reused after destroy. Its context: EGL 1.5
  `eglGetPlatformDisplay(EGL_PLATFORM_SURFACELESS_MESA)` with an RGBA8 + depth 24 pbuffer and an ES 3 context, Mesa's llvmpipe
  forced (`LIBGL_ALWAYS_SOFTWARE=1`, glvnd pointed at Mesa's vendor file); no X, Wayland or GPU. `GM_GL_SHOTS=<dir>` writes
  each GL and soft frame as PNG. ABI 4 + 1 (the C ABI's GL frame equals the Rust renderer's), CLI 2, Kotlin 2. Fuzz
  `banner3ds_render` (byte 0-1 size 1-64 px, byte 2 the frame, the rest a CBMD or CGFX; no panic, the frame does not depend on
  the buffer's old contents, a short buffer is refused): 1 691 544 runs in 301 s, clean. The original renderers' other
  feature tests (border wrap, bump, sphere map, ...) render the scenes the case goldens cover, so they are covered there, not
  ported one per test; the original implementation's local-files test (a folder of real files, never committed) is not ported; an optional local check against a folder of your own files, never committed, is the equivalent. Workspace: 326 Rust tests, 17 Kotlin.
- Gate: `scripts/check` (no CI): fmt, clippy `-D warnings`, tests, `thumbv7em-none-eabihf` (core, scene, soft, gles: no_std), portability
  (below), header drift, the C example, the Kotlin test.

### Portability gates

`scripts/check` builds `gamemeta-core`, `banner3ds-scene`, `banner3ds-soft`, `banner3ds-gles` and `gamemeta-ffi` for
`wasm32-unknown-unknown` and compile-checks them for `aarch64-apple-ios`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`,
`aarch64-linux-android` and `armv7-linux-androideabi` (Android with `gamemeta-jni`); a missing target is skipped with a printed
note. They all pass unchanged: the libraries are `no_std` + alloc (ffi uses std only for the C boundary; its descriptor reads
are `cfg(unix)`). The GL table is plain function pointers, so on wasm32 a WebGL 2 shim fills it like any other loader.

The shim is `crates/gamemeta-wasm`: each table entry is an `extern "system"` function that passes its arguments (as doubles, wasm
addresses included) to `js/gl.js`, which acts on the `WebGL2RenderingContext` made current by the caller; GL objects are numbers on
the Rust side, mapped to WebGL objects per context. Two differences from GLES are absorbed there: WebGL has no info-log-length query
(derived from the log) and uniform locations are objects (an index table). The website (`site/`) runs the binding in a Web Worker on an
`OffscreenCanvas`, falling back to `SoftRenderer` frames painted on the page where WebGL 2 or `OffscreenCanvas` is missing.
`scripts/site-test` drives it in a headless Chromium: the demo draws in WebGL 2 and on the CPU, picked fixtures show the right titles
and ids, a folder is a Wii U title, a 3 GiB file is read in place and a 200 MiB file hashes in chunks, and nothing is requested after load.

## Decisions log

- Crate names stay `gamemeta-*` (short); the project, docs and artifacts are "game-metadata".
- `read` picks the reader by extension, as the original reader does; content sniffing can come with more formats.
- The JNI library re-exports the C ABI so native code in the same app (a future GL renderer) links one library.
- The C ABI stays at version 1 in 0.2.0: every change is an addition. Kotlin gains `readTitle`, `titleImage`,
  `wuaImage` and the `TitleFiles` interface; the JNI image calls return width and height (big-endian ints) before
  the RGBA bytes, and a failing source is an `IOException`.
- The C ABI stays at version 1 in 0.3.0: scene entry points are additions. Kotlin gains `readBannerScene(bytes|fd)` ->
  `BannerSceneInfo` (JSON, counts, textures, unsupported flags); an encrypted file is null, nothing is ever decrypted.
- The scene crate has no dependency beyond core: the SHA-256 the dump uses is hand-written (about 60 lines), the LZ11
  decompressor and the PICA texture decoders are ports, and the ETC1 decoder reuses core's Morton tiling.
- The C ABI stays at version 1 in 0.4.0: ids, RA hasher and disc id entry points are additions. Siblings come through a
  caller-supplied opener (`GmOpenFn`, Kotlin `FileOpener`) with one path rule (relative to the game file's folder), not through
  the platform; the core never hashes on its own because the RA MD5 reads the whole file.
- The C ABI stays at version 1 in 0.5.0: render and GLES entry points are additions. The GLES handle copies the scene, so the
  scene handle's life is independent; the loader is called only inside `gm_gles_create`.
- The C ABI stays at version 1 in 0.6.0: Wii banner entry points are additions. The banner is a module of core
  (`wii::banner`), not a crate: one U8 / IMD5 / LZ77 / TPL implementation. It is a faithful port of the original implementation with its format
  guesses kept in one place; the real banners then contradicted the `mat1` flag guess (see Wii banner API), corrected in that one
  place in 0.7.1. The art pipeline's banner slot waits for the TEV stages.
- Format facts (GBATEK, wiibrew, Dolphin's `WiaAndRvz.md`, the ZArchive description) are in `NOTICE.md`; no code
  from them.

## Renderer rules

Decided 2026-11-17 for the banner renderers (done in 0.5.0):

- `banner3ds-gles` uses only the GLES 3.0 subset that WebGL 2 also has, so a wasm/WebGL build works from the same code.
- Apple goes through ANGLE (GLES on Metal), not a second renderer.
- `banner3ds-soft` (CPU) is the pixel reference: both renderers are tested against it.
- The library never creates windows, contexts or threads: the caller owns the GL context, makes it current, and calls draw.
