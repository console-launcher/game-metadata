# Releasing

A release is a GitHub release of tag `v<version>` with the build outputs attached. Gradle users get it through JitPack, which only
re-serves the release's AAR. Run `scripts/publish` for everything below; it does nothing outside a dry run until it is told
`--go`.

## Prerequisites

- The repository is public and `origin` is its GitHub URL; `gh` is logged in with rights to create releases.
- `main` is pushed and `HEAD` has the same tree as `origin/main` (the tree has to match, not the SHA).
- `Cargo.toml` and `bindings/kotlin/build.gradle.kts` both say the version; `CHANGELOG.md` has a `## [<version>]` section (the
  release notes).
- The build tools of `scripts/release` (Rust Android targets, NDK, JDK 17), plus the wasm32 target and a matching `wasm-bindgen` CLI
  for `scripts/site-build`. Cross tools for the macOS and Windows host libraries are optional; the CLI tarball is Linux x86_64 only.
- Optionally, a time check and a history check (see "Optional hooks"). Neither is required.

## Dry run

```sh
scripts/publish 0.6.0
```

Evaluates the gates (PASS/FAIL each, never stopping), builds into `dist/game-metadata-0.6.0/` (`scripts/release`, the CLI, the
wasm package), prints every asset with its size and what it would tag and create or upload. Exit 0 even when a gate would refuse;
nothing is tagged, pushed or uploaded. The assets:

| File | What |
|---|---|
| `game-metadata-<v>.aar`, `game-metadata-host-<v>.jar` | Android AAR (four ABIs); desktop JVM jar |
| `gamemeta.h`, `LICENSE`, `NOTICE.md` | the C header and the notices |
| `gamemeta-<v>-x86_64-linux.tar.gz` | the CLI |
| `gamemeta-wasm-<v>.zip` | wasm-bindgen output (`pkg/` of the site build) |
| `game-metadata-<v>.pom` | the POM JitPack installs next to the AAR (packaging `aar`, MPL-2.0, Kotlin stdlib) |
| `SHA256SUMS` | one list over all of the above |

## `--go`

```sh
scripts/publish 0.6.0 --go
```

Refuses with a message, before building anything, unless every gate holds: public repository, the optional time check, clean tree and
`HEAD` matching `origin/main`, the version in both files, a changelog section, the optional history check clean on a fresh clone
of origin, and no `v<version>` tag on origin pointing elsewhere. Then it builds, creates the annotated tag on `origin/main`'s
commit with the tagger date set to now in UTC (`+0000`), pushes it (plain push, never forced, the time check run again just before),
and runs `gh release create v<version> <assets> --verify-tag --notes-file <the changelog section>`.

Re-running is safe: an existing tag is not pushed again, and an existing release only gets the assets it lacks (`gh release upload`,
never `--clobber`). To replace a bad asset, delete it by hand and run again.

## Optional hooks

Both are unset by default and then their gates pass as "not configured". Each is a command line; a non-zero exit refuses.

| Variable | Run as | When |
|---|---|---|
| `PUBLISH_TIME_CHECK` | `<command>` | in the gates, again before the tag push, and before the release is touched |
| `PUBLISH_HISTORY_CHECK` | `<command> <bare clone of origin>` | in the gates |

Export them, or write `NAME=value` lines in an untracked `.publish.local` at the repository root (git-ignored; an exported variable
wins). Use them for whatever policy a release has to meet, such as when it may go public. `PUBLISH_NOW` (an ISO time) overrides the
clock used for the tag date.

## JitPack

`jitpack.yml` makes JitPack skip its own build (no Rust or NDK there) and run `scripts/jitpack-install`: it downloads the AAR, the
POM and `SHA256SUMS` from the release of the tag being built, verifies the checksums, rewrites the POM's coordinates to JitPack's
`GROUP:ARTIFACT:VERSION` (`com.github.<owner>:game-metadata:v<version>`), and runs `mvn install:install-file`. Users then add
`maven("https://jitpack.io")` and `implementation("com.github.<owner>:game-metadata:v<version>")`. The first request for a tag
makes JitPack build it; look at its build log at `https://jitpack.io` for that tag if it fails. `scripts/test_jitpack.py` runs
this path locally against a directory standing in for the release.

What only a public repository proves: JitPack fetching the real release over the network and running `jitpack.yml` in its own
image (with a real `mvn`; the local test uses a stub that writes the same `~/.m2` layout).

## crates.io (later, not done)

`publish = false` is set once in `[workspace.package]`. To publish: set `publish = true` on the crates that ship, make every
`path` dependency carry a `version`, and give each crate a `description`, `repository` and `readme`. Publish bottom-up, waiting for
the index between crates: `gamemeta-core`, `banner3ds-scene`, `banner3ds-soft`, `banner3ds-gles`, `gamemeta-json`, `gamemeta-ffi`,
then `gamemeta-cli`. `gamemeta-fixtures`, `gamemeta-jni` and `gamemeta-wasm` stay unpublished. Use an API token kept in a file and
passed as `cargo publish --token "$(cat <path>)"` (or `CARGO_REGISTRY_TOKEN`); run `cargo publish --dry-run` first, and publish
inside the window rule too.
