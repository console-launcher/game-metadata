#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""scripts/dolphin/ref_library.py: the private Wii banner reference library (bucket -> local cache).

Run with: python3 -I scripts/dolphin/test_ref_library.py
"""

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True  # no bytecode next to the scripts
sys.path.insert(0, str(Path(__file__).resolve().parent))
import ref_library as lib  # noqa: E402


def sha(b):
    return hashlib.sha256(b).hexdigest()


class FakeStore:
    """A bucket in memory; counts what was downloaded."""

    def __init__(self, files):
        self.files = files
        self.downloaded = []

    def read_manifest(self):
        return {"files": {rel: {"size": len(b), "sha256": sha(b)} for rel, b in self.files.items()}}

    def download(self, rels, dest):
        for rel in rels:
            self.downloaded.append(rel)
            target = Path(dest) / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(self.files[rel])


class NoAccess:
    def read_manifest(self):
        raise lib.NoAccess("no credential")


FILES = {"banners/a/banner.bin": b"AAA", "refs/a/meta.json": b"{}", "refs/a/f100.png": b"PNG1", "refs/a/f200.png": b"PNG2"}


class FetchTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.cache = Path(self.tmp.name) / "cache"

    def tearDown(self):
        self.tmp.cleanup()

    def test_no_credential_is_a_clean_skip(self):
        self.assertIsNone(lib.fetch(NoAccess(), self.cache))
        self.assertFalse(self.cache.exists())

    def test_cold_cache_downloads_everything(self):
        store = FakeStore(dict(FILES))
        self.assertEqual(lib.fetch(store, self.cache), (4, 4))
        self.assertEqual(sorted(store.downloaded), sorted(FILES))
        self.assertEqual((self.cache / "banners/a/banner.bin").read_bytes(), b"AAA")

    def test_warm_cache_downloads_nothing(self):
        lib.fetch(FakeStore(dict(FILES)), self.cache)
        store = FakeStore(dict(FILES))
        self.assertEqual(lib.fetch(store, self.cache), (4, 0))
        self.assertEqual(store.downloaded, [])

    def test_a_changed_entry_downloads_only_that_file(self):
        lib.fetch(FakeStore(dict(FILES)), self.cache)
        changed = dict(FILES, **{"refs/a/f200.png": b"PNG2-NEW"})
        store = FakeStore(changed)
        self.assertEqual(lib.fetch(store, self.cache), (4, 1))
        self.assertEqual(store.downloaded, ["refs/a/f200.png"])
        self.assertEqual((self.cache / "refs/a/f200.png").read_bytes(), b"PNG2-NEW")

    def test_a_corrupt_download_is_refused_and_removed(self):
        class Bad(FakeStore):
            def download(self, rels, dest):
                super().download(rels, dest)
                (Path(dest) / "refs/a/f100.png").write_bytes(b"tampered")

        with self.assertRaises(lib.Corrupt):
            lib.fetch(Bad(dict(FILES)), self.cache)
        self.assertFalse((self.cache / "refs/a/f100.png").exists())

    def test_a_manifest_path_outside_the_whitelist_is_refused(self):
        with self.assertRaises(lib.Corrupt):
            lib.fetch(FakeStore({"../evil": b"x"}), self.cache)
        self.assertFalse(self.cache.exists())


class SettingsTest(unittest.TestCase):
    """Where the private store lives is the user's to say: an env var or a config file outside the repo, never a default."""

    def test_nothing_set_means_no_store(self):
        with tempfile.TemporaryDirectory() as home:
            self.assertIsNone(lib.settings({}, Path(home)))

    def test_the_env_names_the_bucket_and_optionally_the_project(self):
        with tempfile.TemporaryDirectory() as home:
            got = lib.settings({"GM_WII_REFS_BUCKET": "gs://b", "GM_WII_REFS_PROJECT": "p"}, Path(home))
            self.assertEqual(got, ("gs://b", "p"))
            self.assertEqual(lib.settings({"GM_WII_REFS_BUCKET": "gs://b"}, Path(home)), ("gs://b", None))

    def test_the_config_file_is_read_and_the_env_wins(self):
        with tempfile.TemporaryDirectory() as home:
            cfg = Path(home) / ".config/gamemeta/refs.toml"
            cfg.parent.mkdir(parents=True)
            cfg.write_text('# private store\nbucket = "gs://from-file"\nproject = "file-project"\n')
            self.assertEqual(lib.settings({}, Path(home)), ("gs://from-file", "file-project"))
            self.assertEqual(lib.settings({"GM_WII_REFS_BUCKET": "gs://env"}, Path(home)), ("gs://env", "file-project"))

    def test_a_bucket_that_is_not_a_gs_url_is_refused(self):
        with tempfile.TemporaryDirectory() as home:
            with self.assertRaises(lib.Corrupt):
                lib.settings({"GM_WII_REFS_BUCKET": "not a bucket; rm"}, Path(home))

    def test_the_repo_names_no_bucket(self):
        text = (Path(__file__).resolve().parent / "ref_library.py").read_text()
        self.assertNotRegex(text, r"gs://[a-z0-9]")


class ScriptTest(unittest.TestCase):
    """The entry point as a contributor runs it."""

    def run_script(self, *args, env=None):
        script = Path(__file__).resolve().parent / "ref_library.py"
        with tempfile.TemporaryDirectory() as tmp:
            empty = Path(tmp) / "bin"
            empty.mkdir()
            full = {"PATH": str(empty), "HOME": tmp, "GM_WII_REFS_CACHE": str(Path(tmp) / "cache"), **(env or {})}
            r = subprocess.run([sys.executable, "-I", str(script), *args], capture_output=True, text=True, env=full)
            return r, Path(tmp) / "cache"

    def test_without_a_configured_store_fetch_is_one_skipped_line_and_exit_0(self):
        r, cache = self.run_script("fetch")
        self.assertEqual((r.returncode, r.stdout, r.stderr), (0, "", "fetch-refs: skipped: no reference set configured\n"))
        self.assertFalse(cache.exists())

    def test_without_gcloud_it_is_one_skipped_line_and_exit_0(self):
        r, cache = self.run_script("fetch", env={"GM_WII_REFS_BUCKET": "gs://fake"})
        self.assertEqual((r.returncode, r.stdout, r.stderr), (0, "", "fetch-refs: skipped: no access\n"))
        self.assertFalse(cache.exists())

    def test_publish_without_a_configured_store_says_what_to_set(self):
        r, _ = self.run_script("publish", "--lib", "/nonexistent")
        self.assertEqual(r.returncode, 2)
        self.assertIn("GM_WII_REFS_BUCKET", r.stderr)


class PublishTest(unittest.TestCase):
    def test_publishes_exactly_what_the_test_reads(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "banners/a").mkdir(parents=True)
            (root / "refs/a").mkdir(parents=True)
            (root / "banners/a/banner.bin").write_bytes(b"AAA")
            (root / "banners/a/icon.bin").write_bytes(b"III")
            (root / "banners/a/keys.bin").write_bytes(b"never")  # not a banner file: stays home
            (root / "refs/a/meta.json").write_text(json.dumps({"title": "A Title"}))
            (root / "refs/a/f100.png").write_bytes(b"PNG")
            (root / "refs/a/clip.avi").write_bytes(b"never")  # not a frame: stays home
            (root / "banners/orphan").mkdir()
            (root / "banners/orphan/banner.bin").write_bytes(b"banner only")  # a title without references still ships
            files, titles = lib.local_files(root)
            self.assertEqual(
                sorted(files),
                [
                    "banners/a/banner.bin",
                    "banners/a/icon.bin",
                    "banners/orphan/banner.bin",
                    "refs/a/f100.png",
                    "refs/a/meta.json",
                ],
            )
            self.assertEqual(titles["a"]["title"], "A Title")
            self.assertEqual(titles["orphan"]["files"], ["banners/orphan/banner.bin"])

    def test_only_changed_files_are_uploaded_and_the_manifest_goes_last(self):
        class Store(FakeStore):
            def __init__(self, files):
                super().__init__(files)
                self.uploads = []

            def upload(self, files):
                self.uploads.append(sorted(files))

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "banners/a").mkdir(parents=True)
            (root / "refs/a").mkdir(parents=True)
            (root / "banners/a/banner.bin").write_bytes(b"AAA")
            (root / "refs/a/meta.json").write_text("{}")
            (root / "refs/a/f100.png").write_bytes(b"PNG")
            store = Store({"banners/a/banner.bin": b"AAA", "refs/a/meta.json": b"{}", "refs/a/f100.png": b"OLD"})
            self.assertEqual(lib.publish(store, root), (3, 1))
            self.assertEqual(store.uploads, [["refs/a/f100.png"], ["manifest.json"]])


class StageTest(unittest.TestCase):
    def test_stages_the_banner_files_and_only_the_sampled_frames_of_each_capture(self):
        with tempfile.TemporaryDirectory() as tmp:
            root, out = Path(tmp) / "refs-root", Path(tmp) / "library"
            (root / "lib/a").mkdir(parents=True)
            (root / "lib/a/banner.bin").write_bytes(b"AAA")
            (root / "lib/a/icon.bin").write_bytes(b"III")
            (root / "lib/a/icon.done").write_bytes(b"")  # a marker: stays home
            (root / "lib/b").mkdir()
            (root / "lib/b/banner.bin").write_bytes(b"BBB")
            (root / "out/a/frames").mkdir(parents=True)
            for n in (1, 2, 3):
                (root / f"out/a/frames/{n:04d}.png").write_bytes(b"PNG%d" % n)
            (root / "out/a/meta.json").write_text(json.dumps({"slug": "a", "dolphin": "1"}))
            (root / "out/a/samples.json").write_text(json.dumps({"scale": 1.0, "window": [0, 0, 2, 2], "samples": [{"tile_frame": 2, "icon_frame": 7}]}))
            lib.stage(root, out)
            self.assertEqual(
                sorted(str(p.relative_to(out)) for p in out.rglob("*") if p.is_file()),
                ["banners/a/banner.bin", "banners/a/icon.bin", "banners/b/banner.bin", "refs/a/f2.png", "refs/a/meta.json"],
            )
            meta = json.loads((out / "refs/a/meta.json").read_text())
            self.assertEqual((meta["title"], meta["dolphin"], meta["samples"][0]["icon_frame"]), ("a", "1", 7))
            self.assertEqual((meta["samples"][0]["tile_frame"], meta["samples"][0]["dolphin_frame"]), (2, 2))  # the old key stays for older readers
            self.assertEqual((out / "refs/a/f2.png").read_bytes(), b"PNG2")
            self.assertEqual(oct((out / "banners/a/banner.bin").stat().st_mode & 0o777), "0o600")

    def test_an_empty_root_stages_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            lib.stage(Path(tmp) / "nothing", Path(tmp) / "library")
            self.assertEqual(list((Path(tmp) / "library").rglob("*")), [])


if __name__ == "__main__":
    unittest.main()
