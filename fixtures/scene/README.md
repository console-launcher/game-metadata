# Scene fixtures

Synthetic CBMD/CGFX inputs and their expected scene dumps, used by `crates/banner3ds-scene/tests/golden.rs`.

- `features/<name>.cbmd` + `.json`: 18 banners built by the earlier Kotlin implementation's test builders (the "reference" of `docs/design.md`), one per feature.
- `cases/<name>.{cgfx,cbmd}` + `.json`: 53 single-feature inputs (including truncated and garbage variants) from the same builders.
- Each `.json` is that implementation's deterministic dump of the input (sorted keys; floats to 6 significant digits; textures as
  height, width and the SHA-256 of their RGBA bytes). The Rust parse must reproduce it byte for byte.

The inputs contain no commercial game data: every byte comes from the builders. The dumper that produced the goldens was a throwaway and is not part of this repository.
