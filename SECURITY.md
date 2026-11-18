# Security

game-metadata parses untrusted files: disc images, ROMs, archives and banners from anywhere. A crafted file must never
cause a panic, an out-of-bounds read, an unbounded allocation or a hang; the readers cap every length before allocating,
and the FFI and JNI layers treat all input as hostile.

## Reporting

Use GitHub private vulnerability reporting: the repository's **Security** tab, **Report a vulnerability**. Please do not
open a public issue for a crash on a malicious file. Include the file (or how to build it) and the command that triggers it.

## Fuzzing

cargo-fuzz targets in `fuzz/fuzz_targets/` (seeds from `gamemeta-fixtures`; see `docs/design.md`, Testing):
`threeds_walk`, `nds_walk`, `gc_walk`, `wii_walk`, `wiiu_walk`, `ids_walk`, `banner3ds_scene`, `banner3ds_render`.

```sh
cd fuzz && cargo +nightly fuzz run threeds_walk -- -max_total_time=300
```
