#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""An optional private reference set for the Wii banner tests: `fetch` syncs it into a local cache, `publish` uploads it.

The set is not part of this repository and nothing in the repository says where it is kept: you point the tooling at a store you
control (`GM_WII_REFS_BUCKET`, or `~/.config/gamemeta/refs.toml`). With no store configured, or no access to it, `fetch` is a clean
skip: one "skipped: ..." line on stderr and exit 0 (the reference test then skips too).

Layout (bucket under `wii/`, and the cache): banners/<slug>/banner.bin and icon.bin, refs/<slug>/meta.json, refs/<slug>/f<N>.png (N: the capture frame), and
`manifest.json` (bucket only): {"files": {rel: {size, sha256}}, "titles": {slug: {"title": ..., "files": [rel...]}}}. `fetch`
downloads only files whose manifest hash differs from what the cache holds (a warm run downloads nothing) and never deletes.
Output carries counts only, never file names or bytes.

  ref_library.py fetch [--cache DIR] [--env]   --env prints `export GM_WII_REFS=<cache>` on stdout (eval it)
  ref_library.py stage [--root DIR] --lib DIR  builds the library to publish from the private capture root (lib/, out/)
  ref_library.py publish --lib DIR             uploads that library; needs write access to the bucket

The store: GM_WII_REFS_BUCKET (a gs:// URL) and optionally GM_WII_REFS_PROJECT, or the same as `bucket = "..."` and `project = "..."`
in ~/.config/gamemeta/refs.toml (the env wins). No default: unset means no store. GM_WII_REFS_CACHE is the cache dir (default
~/.cache/gamemeta-wii-refs). It never changes gcloud's default project: with a project set, every call passes --project.
"""

import argparse
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

CONFIG = Path(".config") / "gamemeta" / "refs.toml"
PREFIX = "wii"
STATE = ".synced.json"
# What may be in the library: the banner archive, the capture's meta.json and its reference frames. Nothing else is fetched or published.
SAFE = re.compile(r"^(banners/[a-z0-9-]+/(banner|icon)\.bin|refs/[a-z0-9-]+/meta\.json|refs/[a-z0-9-]+/f\d+\.png)$")


def settings(env, home):
    """(bucket, project or None) from the env, else the config file under `home`; None when no bucket is set. The bucket is a gs:// URL."""
    found = {}
    cfg = Path(home) / CONFIG
    if cfg.is_file():
        for line in cfg.read_text().splitlines():
            m = re.fullmatch(r'\s*(bucket|project)\s*=\s*"([^"]*)"\s*(#.*)?', line)
            if m:
                found[m.group(1)] = m.group(2)
    bucket = env.get("GM_WII_REFS_BUCKET") or found.get("bucket")
    project = env.get("GM_WII_REFS_PROJECT") or found.get("project")
    if not bucket:
        return None
    if not re.fullmatch(r"gs://[a-z0-9][a-z0-9._-]*(/[A-Za-z0-9._/-]*)?", bucket):
        raise Corrupt("the bucket is not a gs:// URL")
    return bucket, project or None


class NoAccess(Exception):
    pass


class Corrupt(Exception):
    pass


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


class GcsStore:
    def __init__(self, bucket, project=None):
        self.root = f"{bucket.rstrip('/')}/{PREFIX}"
        self.project = project

    def _gcloud(self, *args):
        flags = ["--project", self.project] if self.project else []
        try:
            return subprocess.run(["gcloud", "storage", *args, *flags], capture_output=True, text=True)
        except FileNotFoundError:  # no gcloud at all: a contributor, the same clean skip as no credential
            raise NoAccess("no gcloud") from None

    def read_manifest(self):
        r = self._gcloud("cat", f"{self.root}/manifest.json")
        if r.returncode != 0:
            raise NoAccess("no access")
        return json.loads(r.stdout)

    @staticmethod
    def _by_dir(rels):
        groups = {}
        for rel in rels:
            groups.setdefault(rel.rpartition("/")[0], []).append(rel)
        return groups

    def download(self, rels, dest):
        # One gcloud call per directory (it parallelizes inside a call); each file lands at dest/<rel>.
        for d, group in self._by_dir(rels).items():
            (Path(dest) / d).mkdir(parents=True, exist_ok=True)
            r = self._gcloud("cp", "--no-user-output-enabled", *[f"{self.root}/{rel}" for rel in group], str(Path(dest) / d) + "/")
            if r.returncode != 0:
                raise Corrupt(f"download failed ({r.returncode})")

    def upload(self, files):
        """files: rel -> local path."""
        for d, group in self._by_dir(files).items():
            r = self._gcloud("cp", "--no-user-output-enabled", *[str(files[rel]) for rel in group], f"{self.root}/{d}/" if d else f"{self.root}/")
            if r.returncode != 0:
                raise RuntimeError(f"upload failed: {r.stderr.strip()[-200:]}")


def fetch(store, cache):
    """Sync the bucket into cache; returns the number of files and how many were downloaded, or None when there is no access."""
    cache = Path(cache)
    try:
        manifest = store.read_manifest()
    except NoAccess:
        return None
    files = manifest["files"]
    for rel in files:
        if not SAFE.match(rel):
            raise Corrupt("manifest holds an unexpected path")
    state_file = cache / STATE
    state = json.loads(state_file.read_text()) if state_file.is_file() else {}
    todo = []
    for rel, meta in files.items():
        local = cache / rel
        if not (local.is_file() and local.stat().st_size == meta["size"] and state.get(rel) == meta["sha256"]):
            todo.append(rel)
    if todo:
        cache.mkdir(parents=True, exist_ok=True)
        store.download(todo, cache)
        for rel in todo:
            local = cache / rel
            if not local.is_file() or sha256(local) != files[rel]["sha256"]:
                if local.is_file():
                    local.unlink()
                state.pop(rel, None)
                state_file.write_text(json.dumps(state))
                raise Corrupt("a downloaded file does not match the manifest")
            state[rel] = files[rel]["sha256"]
        state_file.write_text(json.dumps(state))
    return len(files), len(todo)


def local_files(lib):
    """rel -> local path of exactly what the reference test reads: each title's banner archives, and where it has a capture its
    meta.json and reference frames."""
    out, titles = {}, {}
    lib = Path(lib)
    banners = lib / "banners"
    for d in sorted(banners.iterdir()) if banners.is_dir() else []:
        rels = {f"banners/{d.name}/{n}": d / n for n in ("banner.bin", "icon.bin") if (d / n).is_file()}
        refs = lib / "refs" / d.name
        title = d.name
        if (refs / "meta.json").is_file():
            rels[f"refs/{d.name}/meta.json"] = refs / "meta.json"
            title = json.loads((refs / "meta.json").read_text()).get("title", d.name)
            rels.update({f"refs/{d.name}/{f.name}": f for f in sorted(refs.iterdir()) if re.fullmatch(r"f\d+\.png", f.name)})
        if not rels:
            continue
        out.update(rels)
        titles[d.name] = {"title": title, "files": sorted(rels)}
    return out, titles


def stage(root, out):
    """Build the library to publish from the private capture root: lib/<slug>/{banner,icon}.bin and, for a captured title,
    out/<slug>/{meta.json,samples.json,frames/NNNN.png}. Only the banner archives, one meta.json (the capture's meta plus the
    samples) and the sampled frames are copied (mode 600); no other file under the root is read."""
    root, out = Path(root), Path(out)
    for d in sorted((root / "lib").iterdir()) if (root / "lib").is_dir() else []:
        if not re.fullmatch(r"[a-z0-9-]+", d.name):
            continue
        for n in ("banner.bin", "icon.bin"):
            if (d / n).is_file():
                _copy(d / n, out / "banners" / d.name / n)
        cap = root / "out" / d.name
        if (cap / "meta.json").is_file() and (cap / "samples.json").is_file():
            meta = json.loads((cap / "meta.json").read_text())
            samples = json.loads((cap / "samples.json").read_text())
            for smp in samples["samples"]:
                smp["tile_frame"] = smp["dolphin_frame"] = smp.get("tile_frame", smp.get("dolphin_frame"))  # dolphin_frame: the key older readers use
            meta.update(title=d.name, samples=samples["samples"], scale=samples["scale"], window=samples["window"])
            dest = out / "refs" / d.name
            dest.mkdir(parents=True, exist_ok=True)
            (dest / "meta.json").write_text(json.dumps(meta, indent=1, sort_keys=True))
            os.chmod(dest / "meta.json", 0o600)
            for smp in samples["samples"]:
                n = smp["tile_frame"]
                _copy(cap / "frames" / f"{n:04d}.png", dest / f"f{n}.png")


def _copy(src, dest):
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(src.read_bytes())
    os.chmod(dest, 0o600)


def publish(store, lib):
    files, titles = local_files(lib)
    for rel in files:
        if not SAFE.match(rel):
            raise Corrupt("refusing to publish an unexpected path")
    try:
        old = store.read_manifest()["files"]
    except NoAccess:
        old = {}
    new = {rel: {"size": p.stat().st_size, "sha256": sha256(p)} for rel, p in files.items()}
    up = [rel for rel in new if old.get(rel, {}).get("sha256") != new[rel]["sha256"]]
    store.upload({rel: files[rel] for rel in up})
    with tempfile.TemporaryDirectory() as d:
        m = Path(d) / "manifest.json"
        m.write_text(json.dumps({"files": new, "titles": titles}, indent=1, sort_keys=True))
        store.upload({"manifest.json": m})  # last: a half-published library is never advertised
    return len(new), len(up)


def main(argv):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    f = sub.add_parser("fetch")
    f.add_argument("--cache", default=os.environ.get("GM_WII_REFS_CACHE") or str(Path.home() / ".cache" / "gamemeta-wii-refs"))
    f.add_argument("--env", action="store_true")
    p = sub.add_parser("publish")
    p.add_argument("--lib", required=True)
    g = sub.add_parser("stage")
    g.add_argument("--root", default=os.environ.get("WII_REFS") or str(Path.home() / ".local" / "share" / "dolphin-wii-refs"))
    g.add_argument("--lib", required=True)
    a = ap.parse_args(argv)
    if a.cmd == "stage":
        stage(a.root, a.lib)
        return 0
    try:
        store = settings(os.environ, Path.home())
    except Corrupt as e:
        print(f"ref_library: {e}", file=sys.stderr)
        return 2
    if a.cmd == "publish":
        if store is None:
            print("publish: no store configured: set GM_WII_REFS_BUCKET (a gs:// URL) or bucket in ~/.config/gamemeta/refs.toml", file=sys.stderr)
            return 2
        total, up = publish(GcsStore(*store), a.lib)
        print(f"publish: {total} files, {up} uploaded", file=sys.stderr)
        return 0
    if store is None:
        print("fetch-refs: skipped: no reference set configured", file=sys.stderr)
        return 0
    try:
        got = fetch(GcsStore(*store), a.cache)
    except (Corrupt, OSError, ValueError, KeyError) as e:
        print(f"fetch-refs: failed: {e}", file=sys.stderr)
        return 1
    if got is None:
        print("fetch-refs: skipped: no access", file=sys.stderr)
        return 0
    print(f"fetch-refs: {got[0]} files, {got[1]} downloaded", file=sys.stderr)
    if a.env:
        print(f"export GM_WII_REFS={shlex.quote(a.cache)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
