# Porting game-metadata to another host

The library ships Android artifacts. Nothing in the core is Android-specific, so another host (a Linux handheld, a
desktop frontend, iOS) needs only what is below. No other host is built or tested here; `scripts/release` only
compile-checks `gamemeta-core` and `gamemeta-ffi` for `aarch64-unknown-linux-gnu` to keep that door open, and
`scripts/check` builds core, scene and ffi for `wasm32-unknown-unknown` and compile-checks them for `aarch64-apple-ios` and
`aarch64-apple-darwin`.

## What a host supplies

1. **Bytes.** The core never opens files. Give it one of:
   - a read callback: `gm_read_callback(size, read, user_data, file_name, &info)`, where `read(user_data, offset, buf,
     len)` fills `buf` and returns `GM_READ_RESULT_OK`, `_OUT_OF_RANGE` or `_ERROR`. Works for files, archives,
     network ranges, platform document APIs;
   - a POSIX descriptor: `gm_read_fd(fd, file_name, &info)` (positioned reads; the descriptor is not moved or closed);
   - memory: `gm_read_bytes(data, len, file_name, &info)`.
   Rust hosts implement `gamemeta_core::ByteSource` directly and skip the C layer.
2. **A file name** (or just an extension) per read: it selects the reader.
3. **Threads.** Calls are synchronous and reentrant; no global state. Call them off any UI thread.
4. **A GL context.** `banner3ds-gles` (C: `gm_gles_create` / `_draw` / `_destroy`) draws into the caller's current GLES 3.0 or
   WebGL 2 context, loading each GL function through the caller's loader (`eglGetProcAddress`, `SDL_GL_GetProcAddress`, a
   WebGL 2 shim); the library never creates windows, contexts or threads. Without GL, `gm_render_soft` draws the same frame
   on the CPU into a caller buffer. The renderer rules:
   - `banner3ds-gles` uses only the GLES 3.0 subset that WebGL 2 also has, so a wasm/WebGL build works;
   - Apple goes through ANGLE (GLES on Metal), not a second renderer;
   - `banner3ds-soft` (CPU) is the pixel reference both renderers are tested against.
5. **Banner scenes.** `gm_scene_read_bytes` / `_callback` / `_fd` give the same byte sources as above; a scene is the parse
   only (no drawing), so a host can use it with its own renderer.
6. **Ids and sibling files.** `gm_ids_read_*` take an optional `GmOpenFn`: the host opens a file beside the game file (a cue
   sheet's `.bin`, a `.rpx`'s `../meta/meta.xml`) by path relative to the game file's folder, or says there is none. The library
   never opens paths itself. The RA MD5 reads the whole file, so it is streamed by the host through `gm_ra_hasher_*`.

## Building

- `cargo build -p gamemeta-ffi --release --target <triple>` gives `libgamemeta.so`/`.a` (+ `include/gamemeta.h`).
- JVM hosts: `cargo build -p gamemeta-jni --release` and the Kotlin API in `bindings/kotlin`; the host jar loads
  `gamemeta/native/<os>-<arch>/` from its resources, or `-Dgamemeta.library=<path>`.
- Release profile: `opt-level = "s"`, LTO, stripped, `panic = "unwind"` (the C/JNI boundaries catch panics; keep
  unwind on).

## Packaging

Ship the shared library beside your binary (or link the static one), plus `NOTICE.md`. No data files, no runtime
dependencies beyond libc.
