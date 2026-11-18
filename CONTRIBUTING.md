# Contributing

- `scripts/check` must pass (fmt, clippy `-D warnings`, tests, `no_std` and portability builds, header drift, C smoke,
  Kotlin test, MPL notice on every source file: `scripts/license-headers --fix` adds it).
- Tests first: a bug fix or reader change comes with a test that failed before it.
- Synthetic fixtures only, built in code (`crates/gamemeta-fixtures`). No game data, no keys, no key-derived values, no
  decryption code, ever.
- Facts sources only: format documentation, public specifications and public-domain code (see `NOTICE.md`). Do not read
  or copy GPL tools (Ohana3DS-Rebirth, Kuriimu, melonDS, DeSmuME, Dolphin or Cemu source); take offsets, sizes and magic
  numbers, never text or code. Add each new source to `NOTICE.md`.
- By contributing you agree your changes are licensed under MPL-2.0.
