#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Tests for scripts/publish. Nothing here touches the network or the real repository: each test copies the script into a
scratch git repo whose `origin` is a local bare repo (reached through an insteadOf rule so its URL reads like a GitHub one),
puts stub `gh` and `cargo` on PATH (they log their calls), and stubs scripts/release and scripts/site-build in that repo.
The optional time and history hooks (PUBLISH_TIME_CHECK, PUBLISH_HISTORY_CHECK) are stub commands here; PUBLISH_NOW fixes the clock.
  python3 -I scripts/test_publish.py"""
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
PUBLISH = os.path.join(HERE, "publish")

ALLOWED_NOW = "2026-11-21T15:00:00Z"   # Saturday
BLOCKED_NOW = "2026-11-17T16:00:00Z"   # Tuesday 10:00 in Chicago
OK_DATE = "2026-01-03T12:00:00+0000"   # Saturday
IN_WINDOW_DATE = "2026-01-06T16:00:00+0000"  # Tuesday 10:00 in Chicago

TIME_STUB = """#!/usr/bin/env bash
[ "$PUBLISH_NOW" = "%s" ] && { echo "refused"; exit 1; }
exit 0
""" % BLOCKED_NOW
HISTORY_STUB = """#!/usr/bin/env bash
git -C "$1" log --format=%aI | grep -q '^2026-01-06T16' && { echo "a commit in the refused range"; exit 1; }
exit 0
"""

GH_STUB = r"""#!/usr/bin/env bash
echo "gh $*" >> "$STUB_LOG"
case "$1 $2" in
  "repo view") echo "${GH_VISIBILITY-PUBLIC}" ;;
  "release view")
    if [[ -n "${GH_RELEASE_EXISTS:-}" ]]; then printf '%s\n' ${GH_RELEASE_ASSETS:-}; else echo "release not found" >&2; exit 1; fi ;;
  "release create")
    while [[ $# -gt 0 ]]; do [[ "$1" == --notes-file ]] && cp "$2" "$STUB_LOG.notes"; shift; done ;;
esac
exit 0
"""
CARGO_STUB = r"""#!/usr/bin/env bash
echo "cargo $*" >> "$STUB_LOG"
mkdir -p target/release && printf 'fake cli binary' > target/release/gamemeta && chmod +x target/release/gamemeta
"""
RELEASE_STUB = r"""#!/usr/bin/env bash
cd "$(dirname "$0")/.."
echo "scripts/release" >> "$STUB_LOG"
v="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
out="dist/game-metadata-$v"; mkdir -p "$out"
printf 'aar bytes' > "$out/game-metadata-$v.aar"; printf 'jar bytes' > "$out/game-metadata-host-$v.jar"
printf '/* header */' > "$out/gamemeta.h"; cp LICENSE NOTICE.md "$out/"
(cd "$out" && sha256sum game-metadata-*.aar game-metadata-host-*.jar gamemeta.h LICENSE NOTICE.md > SHA256SUMS)
"""
SITE_STUB = r"""#!/usr/bin/env bash
cd "$(dirname "$0")/.."
echo "scripts/site-build" >> "$STUB_LOG"
mkdir -p site/dist/pkg && printf 'js glue' > site/dist/pkg/gamemeta_wasm.js && printf 'wasm bytes' > site/dist/pkg/gamemeta_wasm_bg.wasm
mkdir -p site/dist/pkg/snippets/x && printf 'js' > site/dist/pkg/snippets/x/inline0.js
"""
CHANGELOG = """# Changelog

## [0.6.0]

- Wii banners.
- Cart headers.

## [0.5.1]

- The site.
"""


def sh(*args, cwd=None, env=None, check=True):
    r = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True)
    if check and r.returncode:
        raise RuntimeError("%s failed: %s" % (args, r.stderr))
    return r


class Base(unittest.TestCase):
    version = "0.6.0"

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="publish-test-")
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.bin = os.path.join(self.tmp, "bin")
        os.mkdir(self.bin)
        for name, body in (("gh", GH_STUB), ("cargo", CARGO_STUB), ("timecheck", TIME_STUB), ("historycheck", HISTORY_STUB)):
            self.write(os.path.join(self.bin, name), body, mode=0o755)
        self.log = os.path.join(self.tmp, "calls.log")
        open(self.log, "w").close()
        self.origin = os.path.join(self.tmp, "origin.git")
        self.work = os.path.join(self.tmp, "work")
        self.make_repo()

    # ---- fixtures
    @staticmethod
    def write(path, text, mode=None):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(text)
        if mode:
            os.chmod(path, mode)

    def genv(self, date):
        return dict(os.environ, GIT_AUTHOR_NAME="T", GIT_AUTHOR_EMAIL="t@example.invalid", GIT_COMMITTER_NAME="T",
                    GIT_COMMITTER_EMAIL="t@example.invalid", GIT_AUTHOR_DATE=date, GIT_COMMITTER_DATE=date)

    def git(self, *args, date=OK_DATE, cwd=None):
        return sh("git", *args, cwd=cwd or self.work, env=self.genv(date)).stdout.strip()

    def make_repo(self, gradle_version=None, changelog=CHANGELOG, history_dates=()):
        w = self.work
        sh("git", "init", "-q", "--bare", "-b", "main", self.origin)
        sh("git", "init", "-q", "-b", "main", w)
        self.git("remote", "add", "origin", "https://github.com/acme/game-metadata.git")
        self.git("config", "url.%s.insteadOf" % self.origin, "https://github.com/acme/game-metadata.git")
        self.write(os.path.join(w, ".gitignore"), "dist/\ntarget/\nsite/dist/\n")
        self.write(os.path.join(w, "Cargo.toml"), '[workspace.package]\nversion = "%s"\nlicense = "MPL-2.0"\n' % self.version)
        self.write(os.path.join(w, "bindings/kotlin/build.gradle.kts"),
                   'plugins {\n    kotlin("jvm") version "2.3.0"\n}\n\ngroup = "gamemeta"\nversion = "%s"\n' % (gradle_version or self.version))
        self.write(os.path.join(w, "CHANGELOG.md"), changelog)
        self.write(os.path.join(w, "LICENSE"), "MPL text\n")
        self.write(os.path.join(w, "NOTICE.md"), "notices\n")
        self.write(os.path.join(w, "scripts/release"), RELEASE_STUB, 0o755)
        self.write(os.path.join(w, "scripts/site-build"), SITE_STUB, 0o755)
        shutil.copy(PUBLISH, os.path.join(w, "scripts/publish")) if os.path.exists(PUBLISH) else None
        for i, d in enumerate(history_dates):
            self.write(os.path.join(w, "history.txt"), "%d\n" % i)
            self.git("add", "-A")
            self.git("commit", "-q", "-m", "old %d" % i, date=d)
        self.write(os.path.join(w, "history.txt"), "tip\n")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "tip")
        self.git("push", "-q", "origin", "main")

    def run_publish(self, *args, now=ALLOWED_NOW, **env):
        e = dict(os.environ, PATH=self.bin + os.pathsep + os.environ["PATH"], STUB_LOG=self.log,
                 PUBLISH_TIME_CHECK="timecheck", PUBLISH_HISTORY_CHECK="historycheck", PUBLISH_NOW=now, GIT_AUTHOR_NAME="T", GIT_AUTHOR_EMAIL="t@example.invalid",
                 GIT_COMMITTER_NAME="T", GIT_COMMITTER_EMAIL="t@example.invalid")
        e.update(env)
        return subprocess.run(["scripts/publish", *args], cwd=self.work, env=e, capture_output=True, text=True)

    # ---- observations
    def calls(self):
        with open(self.log) as f:
            return f.read().splitlines()

    def origin_tags(self):
        return sh("git", "-C", self.origin, "tag", "-l").stdout.split()

    def assert_nothing_published(self, r):
        c = self.calls()
        self.assertFalse([l for l in c if l.startswith(("gh release create", "gh release upload"))], c)
        self.assertEqual(self.origin_tags(), [])
        self.assertNotIn("scripts/release", c, "a refusal must happen before anything is built")
        self.assertEqual(sh("git", "tag", "-l", cwd=self.work).stdout.split(), [], "no local tag either")

    def assert_refused(self, r, needle):
        self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn(needle, r.stdout + r.stderr)
        self.assert_nothing_published(r)

    def dist(self, name=""):
        return os.path.join(self.work, "dist", "game-metadata-" + self.version, name)


EXPECTED = lambda v: {
    "game-metadata-%s.aar" % v, "game-metadata-host-%s.jar" % v, "gamemeta.h", "LICENSE", "NOTICE.md",
    "gamemeta-%s-x86_64-linux.tar.gz" % v, "gamemeta-wasm-%s.zip" % v, "game-metadata-%s.pom" % v, "SHA256SUMS"}


class Gates(Base):
    """Every refusal: non-zero, a clear message, and nothing built, tagged, pushed or uploaded."""

    def test_no_go_flag_is_a_dry_run_that_publishes_nothing(self):
        r = self.run_publish(self.version)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("dry run", r.stdout)
        c = self.calls()
        self.assertFalse([l for l in c if l.startswith(("gh release create", "gh release upload"))], c)
        self.assertEqual(self.origin_tags(), [])
        self.assertEqual(sh("git", "tag", "-l", cwd=self.work).stdout.split(), [])

    def test_go_refused_when_the_repo_is_private(self):
        r = self.run_publish(self.version, "--go", GH_VISIBILITY="PRIVATE")
        self.assert_refused(r, "not public")

    def test_go_refused_when_visibility_cannot_be_read(self):
        r = self.run_publish(self.version, "--go", GH_VISIBILITY="")
        self.assert_refused(r, "not public")

    def test_go_refused_when_the_time_check_refuses(self):
        r = self.run_publish(self.version, "--go", now=BLOCKED_NOW)
        self.assert_refused(r, "time check")

    def test_go_refused_on_a_dirty_tree(self):
        self.write(os.path.join(self.work, "LICENSE"), "changed\n")
        self.assert_refused(self.run_publish(self.version, "--go"), "not clean")

    def test_go_refused_on_an_untracked_file(self):
        self.write(os.path.join(self.work, "stray.txt"), "x\n")
        self.assert_refused(self.run_publish(self.version, "--go"), "not clean")

    def test_go_refused_when_head_is_not_what_origin_main_has(self):
        self.write(os.path.join(self.work, "NOTICE.md"), "newer\n")
        self.git("commit", "-q", "-am", "unpushed")
        self.assert_refused(self.run_publish(self.version, "--go"), "origin/main")

    def test_go_refused_when_the_argument_is_not_the_cargo_version(self):
        self.assert_refused(self.run_publish("0.7.0", "--go"), "Cargo.toml")

    def test_go_refused_when_the_changelog_has_no_section(self):
        self.write(os.path.join(self.work, "CHANGELOG.md"), "# Changelog\n\n## [0.5.1]\n\n- old\n")
        self.git("commit", "-q", "-am", "changelog")
        self.git("push", "-q", "origin", "main")
        self.assert_refused(self.run_publish(self.version, "--go"), "CHANGELOG.md")


class GradleBehind(Base):
    def make_repo(self, **kw):
        super().make_repo(gradle_version="0.5.9", **kw)

    def test_go_refused_when_gradle_and_cargo_disagree(self):
        self.assert_refused(self.run_publish(self.version, "--go"), "build.gradle.kts")


class Hooks(Base):
    def test_without_hooks_both_gates_pass_as_not_configured(self):
        r = self.run_publish(self.version, PUBLISH_TIME_CHECK="", PUBLISH_HISTORY_CHECK="")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertRegex(r.stdout, r"PASS\s+time check")
        self.assertRegex(r.stdout, r"PASS\s+history check")

    def test_a_hook_named_in_the_untracked_local_file_is_used(self):
        self.write(os.path.join(self.work, ".publish.local"), "PUBLISH_TIME_CHECK=timecheck\n")
        r = self.run_publish(self.version, "--go", now=BLOCKED_NOW, PUBLISH_TIME_CHECK="")
        # the exported (empty) variable wins over the file, so the gate passes; with it removed the file's hook refuses
        self.assertNotIn("time check refuses", r.stdout + r.stderr)
        e = {k: v for k, v in os.environ.items()}
        e.update(PATH=self.bin + os.pathsep + os.environ["PATH"], STUB_LOG=self.log, PUBLISH_NOW=BLOCKED_NOW)
        e.pop("PUBLISH_TIME_CHECK", None)
        e.pop("PUBLISH_HISTORY_CHECK", None)
        r = subprocess.run(["scripts/publish", self.version, "--go"], cwd=self.work, env=e, capture_output=True, text=True)
        self.assert_refused(r, "time check")

    def test_a_missing_hook_command_refuses(self):
        r = self.run_publish(self.version, "--go", PUBLISH_TIME_CHECK="no-such-hook-command")
        self.assert_refused(r, "not installed")


class GatesWithHistory(Base):
    def make_repo(self, **kw):
        super().make_repo(history_dates=[IN_WINDOW_DATE], **kw)

    def test_go_refused_when_go_public_check_fails(self):
        self.assert_refused(self.run_publish(self.version, "--go"), "history check")

    def test_dry_run_reports_the_failing_gate_but_still_exits_zero(self):
        r = self.run_publish(self.version)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertRegex(r.stdout, r"FAIL\s+history check")


class DryRun(Base):
    def test_lists_the_exact_asset_set_with_sizes(self):
        r = self.run_publish(self.version)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        found = dict(re.findall(r"^asset (\S+)\s+(\d+) bytes", r.stdout, re.M))
        self.assertEqual(set(found), EXPECTED(self.version))
        for name, size in found.items():
            self.assertEqual(os.path.getsize(self.dist(name)), int(size), name)

    def test_builds_through_release_and_site_build_and_the_cli(self):
        self.run_publish(self.version)
        c = self.calls()
        self.assertIn("scripts/release", c)
        self.assertIn("scripts/site-build", c)
        self.assertTrue([l for l in c if l.startswith("cargo build") and "gamemeta-cli" in l and "--release" in l], c)

    def test_prints_every_step(self):
        r = self.run_publish(self.version)
        for step in ("gates", "build", "assets", "tag v%s" % self.version, "release v%s" % self.version):
            self.assertIn(step, r.stdout)

    def test_the_cli_tarball_holds_the_binary_and_licence(self):
        self.run_publish(self.version)
        with tarfile.open(self.dist("gamemeta-%s-x86_64-linux.tar.gz" % self.version)) as t:
            names = t.getnames()
            top = "gamemeta-%s-x86_64-linux" % self.version
            self.assertEqual(sorted(names), sorted([top, top + "/gamemeta", top + "/LICENSE", top + "/NOTICE.md"]))
            self.assertTrue(t.getmember(top + "/gamemeta").mode & 0o111, "executable")

    def test_the_wasm_zip_holds_the_bindgen_output(self):
        self.run_publish(self.version)
        with zipfile.ZipFile(self.dist("gamemeta-wasm-%s.zip" % self.version)) as z:
            top = "gamemeta-wasm-%s/" % self.version
            self.assertEqual(set(z.namelist()), {top + "gamemeta_wasm.js", top + "gamemeta_wasm_bg.wasm", top + "snippets/x/inline0.js", top + "LICENSE", top + "NOTICE.md"})

    def test_the_combined_checksums_verify_every_other_asset(self):
        self.run_publish(self.version)
        sums = open(self.dist("SHA256SUMS")).read()
        listed = {l.split()[-1].lstrip("*") for l in sums.splitlines()}
        self.assertEqual(listed, EXPECTED(self.version) - {"SHA256SUMS"})
        r = sh("sha256sum", "-c", "SHA256SUMS", cwd=self.dist())
        self.assertEqual(r.stderr, "")

    def test_the_pom_has_aar_packaging_coordinates_and_the_licence(self):
        self.run_publish(self.version)
        root = ET.parse(self.dist("game-metadata-%s.pom" % self.version)).getroot()
        ns = {"m": "http://maven.apache.org/POM/4.0.0"}
        t = lambda p: root.findtext(p, namespaces=ns)
        self.assertEqual(t("m:packaging"), "aar")
        self.assertEqual(t("m:groupId"), "com.github.acme")
        self.assertEqual(t("m:artifactId"), "game-metadata")
        self.assertEqual(t("m:version"), self.version)
        self.assertEqual(t("m:licenses/m:license/m:name"), "MPL-2.0")
        self.assertIn("mozilla.org/MPL/2.0", t("m:licenses/m:license/m:url"))
        dep = root.find("m:dependencies/m:dependency", ns)
        self.assertEqual(dep.findtext("m:artifactId", namespaces=ns), "kotlin-stdlib")
        self.assertEqual(dep.findtext("m:version", namespaces=ns), "2.3.0")
        # coordinates come first so jitpack.yml can rewrite the first occurrence of each
        text = open(self.dist("game-metadata-%s.pom" % self.version)).read()
        self.assertLess(text.index("<groupId>"), text.index("<artifactId>"))
        self.assertLess(text.index("<artifactId>"), text.index("<version>"))


class Go(Base):
    def test_tags_pushes_and_creates_the_release_with_the_changelog_notes(self):
        r = self.run_publish(self.version, "--go")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        tag = "v" + self.version
        self.assertEqual(self.origin_tags(), [tag])
        self.assertEqual(sh("git", "-C", self.origin, "cat-file", "-t", tag).stdout.strip(), "tag", "annotated")
        main = sh("git", "-C", self.origin, "rev-parse", "main").stdout.strip()
        self.assertEqual(sh("git", "-C", self.origin, "rev-parse", tag + "^{commit}").stdout.strip(), main)
        raw = sh("git", "-C", self.origin, "cat-file", "tag", tag).stdout
        m = re.search(r"^tagger .* (\d+) \+0000$", raw, re.M)
        self.assertTrue(m, raw)
        self.assertEqual(int(m.group(1)), 1795273200)  # ALLOWED_NOW, UTC
        creates = [l for l in self.calls() if l.startswith("gh release create")]
        self.assertEqual(len(creates), 1, self.calls())
        self.assertIn("v%s" % self.version, creates[0])
        self.assertIn("--verify-tag", creates[0])
        for name in EXPECTED(self.version):
            self.assertIn("/" + name, creates[0])
        notes = open(self.log + ".notes").read()
        self.assertIn("Wii banners.", notes)
        self.assertIn("Cart headers.", notes)
        self.assertNotIn("The site.", notes)

    def test_rerun_with_everything_published_uploads_nothing(self):
        self.run_publish(self.version, "--go")
        open(self.log, "w").close()
        r = self.run_publish(self.version, "--go", GH_RELEASE_EXISTS="1", GH_RELEASE_ASSETS=" ".join(sorted(EXPECTED(self.version))))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertFalse([l for l in self.calls() if l.startswith(("gh release create", "gh release upload"))], self.calls())
        self.assertIn("already", r.stdout)

    def test_rerun_uploads_only_the_missing_assets_and_never_clobbers(self):
        self.run_publish(self.version, "--go")
        open(self.log, "w").close()
        have = ["game-metadata-%s.aar" % self.version, "SHA256SUMS"]
        r = self.run_publish(self.version, "--go", GH_RELEASE_EXISTS="1", GH_RELEASE_ASSETS=" ".join(have))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        c = self.calls()
        self.assertFalse([l for l in c if l.startswith("gh release create")], c)
        ups = [l for l in c if l.startswith("gh release upload")]
        self.assertEqual(len(ups), 1, c)
        self.assertNotIn("--clobber", ups[0])
        for name in EXPECTED(self.version) - set(have):
            self.assertIn("/" + name, ups[0])
        for name in have:
            self.assertNotIn("/" + name, ups[0])

    def test_a_tag_already_on_origin_is_not_pushed_again(self):
        self.run_publish(self.version, "--go")
        r = self.run_publish(self.version, "--go", GH_RELEASE_EXISTS="1", GH_RELEASE_ASSETS="x")
        self.assertIn("tag v%s already on origin" % self.version, r.stdout)

    def test_a_tag_on_another_commit_is_refused(self):
        main = sh("git", "-C", self.origin, "rev-parse", "main").stdout.strip()
        self.write(os.path.join(self.work, "x.txt"), "x")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "other")
        other = self.git("rev-parse", "HEAD")
        self.git("tag", "-a", "v" + self.version, "-m", "wrong", other)
        self.git("push", "-q", "origin", "v" + self.version)
        self.git("reset", "-q", "--hard", main)
        r = self.run_publish(self.version, "--go")
        self.assertNotEqual(r.returncode, 0, r.stdout)
        self.assertIn("another commit", r.stdout + r.stderr)
        self.assertFalse([l for l in self.calls() if l.startswith("gh release create")])


if __name__ == "__main__":
    unittest.main()
