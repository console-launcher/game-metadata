#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""The JitPack path, locally: the commands in jitpack.yml's `install:` run in a temp HOME against a directory standing in for the
GitHub release (a file:// URL), with a stub `mvn` that writes the same ~/.m2 layout `install:install-file` does (the JitPack image
has the real mvn; this machine has none). Then a scratch Gradle project resolves com.github.<owner>:game-metadata:v<version>
from that local repository and compiles a call to the Kotlin API. What stays unproven until the repository is public: JitPack
itself fetching the real release over the network and building the `jitpack.yml` in its own image.
  python3 -I scripts/test_jitpack.py"""
import importlib.machinery
import importlib.util
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True  # the loader below would otherwise drop a .pyc next to scripts/publish

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
KOTLIN = os.path.join(ROOT, "bindings", "kotlin")

MVN_STUB = r"""#!/usr/bin/env bash
# install:install-file, as far as JitPack needs it: <repo>/<group path>/<artifact>/<version>/<artifact>-<version>.<packaging>[.pom]
set -euo pipefail
echo "mvn $*" >> "$STUB_LOG"
declare -A p
for a in "$@"; do [[ "$a" == -D*=* ]] && p[${a#-D}]= && k=${a#-D} && p[${k%%=*}]=${k#*=}; done
[[ " $* " == *" install:install-file "* ]] || { echo "unexpected mvn call" >&2; exit 1; }
dir="$HOME/.m2/repository/${p[groupId]//.//}/${p[artifactId]}/${p[version]}"
mkdir -p "$dir"
cp "${p[file]}" "$dir/${p[artifactId]}-${p[version]}.${p[packaging]}"
cp "${p[pomFile]}" "$dir/${p[artifactId]}-${p[version]}.pom"
"""


def load_publish():
    loader = importlib.machinery.SourceFileLoader("publish", os.path.join(ROOT, "scripts", "publish"))
    spec = importlib.util.spec_from_loader("publish", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


def install_commands():
    """The list items under `install:` in jitpack.yml."""
    out, inside = [], False
    for line in open(os.path.join(ROOT, "jitpack.yml"), encoding="utf-8").read().splitlines():
        if re.match(r"^install:\s*$", line):
            inside = True
        elif inside and re.match(r"^\s+-\s+", line):
            out.append(re.sub(r"^\s+-\s+", "", line).strip())
        elif inside and re.match(r"^\S", line):
            inside = False
    return out


class JitPack(unittest.TestCase):
    owner = "acme"

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="jitpack-test-")
        pub = load_publish()
        cls.version = re.search(r'^version = "(.*)"', open(os.path.join(ROOT, "Cargo.toml")).read(), re.M).group(1)
        # A real AAR of the Kotlin API (no native libraries: only the classes are needed to compile against).
        subprocess.run(["./gradlew", "--no-daemon", "-q", "jar"], cwd=KOTLIN, check=True)
        classes = os.path.join(KOTLIN, "build", "libs", "game-metadata-classes-%s.jar" % cls.version)
        cls.dl = os.path.join(cls.tmp, "release")
        os.makedirs(cls.dl)
        r = os.path.join(cls.tmp, "R.txt")
        open(r, "w").close()
        aar = os.path.join(cls.dl, "game-metadata-%s.aar" % cls.version)
        subprocess.run(["python3", "-I", os.path.join(ROOT, "scripts", "package.py"), aar,
                        "AndroidManifest.xml=" + os.path.join(ROOT, "bindings", "android", "AndroidManifest.xml"),
                        "classes.jar=" + classes, "R.txt=" + r], check=True)
        with open(os.path.join(cls.dl, "game-metadata-%s.pom" % cls.version), "w") as f:
            f.write(pub.make_pom(cls.version, cls.owner, "game-metadata"))
        sums = subprocess.run(["sha256sum", os.path.basename(aar), "game-metadata-%s.pom" % cls.version], cwd=cls.dl,
                              capture_output=True, text=True, check=True).stdout
        open(os.path.join(cls.dl, "SHA256SUMS"), "w").write(sums)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, True)

    def jitpack_install(self, home, base=None, tamper=False):
        bindir = os.path.join(home, "bin")
        os.makedirs(bindir, exist_ok=True)
        with open(os.path.join(bindir, "mvn"), "w") as f:
            f.write(MVN_STUB)
        os.chmod(os.path.join(bindir, "mvn"), 0o755)
        log = os.path.join(home, "mvn.log")
        env = dict(os.environ, HOME=home, PATH=bindir + os.pathsep + os.environ["PATH"], STUB_LOG=log,
                   GROUP="com.github.%s" % self.owner, ARTIFACT="game-metadata", VERSION="v" + self.version,
                   GAMEMETA_RELEASE_BASE=base or "file://" + self.dl)
        cmds = install_commands()
        self.assertTrue(cmds, "jitpack.yml has no install: commands")
        res = None
        for c in cmds:
            res = subprocess.run(["bash", "-c", c], cwd=ROOT, env=env, capture_output=True, text=True)
            if res.returncode:
                break
        return res, log

    def test_install_puts_the_aar_and_a_rewritten_pom_in_the_local_repository(self):
        home = os.path.join(self.tmp, "home1")
        res, log = self.jitpack_install(home)
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        d = os.path.join(home, ".m2/repository/com/github/acme/game-metadata/v" + self.version)
        self.assertTrue(os.path.getsize(os.path.join(d, "game-metadata-v%s.aar" % self.version)) > 0)
        pom = open(os.path.join(d, "game-metadata-v%s.pom" % self.version)).read()
        self.assertIn("<version>v%s</version>" % self.version, pom)  # JitPack's version is the tag
        self.assertIn("<packaging>aar</packaging>", pom)
        self.assertIn("-Dpackaging=aar", open(log).read())

    def test_a_download_that_fails_the_checksum_installs_nothing(self):
        bad = os.path.join(self.tmp, "bad")
        shutil.copytree(self.dl, bad)
        with open(os.path.join(bad, "game-metadata-%s.aar" % self.version), "ab") as f:
            f.write(b"tampered")
        home = os.path.join(self.tmp, "home2")
        res, _ = self.jitpack_install(home, base="file://" + bad)
        self.assertNotEqual(res.returncode, 0)
        self.assertFalse(os.path.exists(os.path.join(home, ".m2")))

    def test_a_missing_release_fails_the_build(self):
        res, _ = self.jitpack_install(os.path.join(self.tmp, "home3"), base="file://" + os.path.join(self.tmp, "nope"))
        self.assertNotEqual(res.returncode, 0)

    def test_gradle_resolves_the_coordinates_and_compiles_a_call_to_the_kotlin_api(self):
        home = os.path.join(self.tmp, "home4")
        res, _ = self.jitpack_install(home)
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        proj = os.path.join(self.tmp, "consumer")
        os.makedirs(os.path.join(proj, "src/main/kotlin"))
        coord = "com.github.%s:game-metadata:v%s" % (self.owner, self.version)
        kotlin = re.search(r'kotlin\("jvm"\) version "(.*)"', open(os.path.join(KOTLIN, "build.gradle.kts")).read()).group(1)
        open(os.path.join(proj, "settings.gradle.kts"), "w").write('rootProject.name = "consumer"\n')
        open(os.path.join(proj, "build.gradle.kts"), "w").write("""
plugins { kotlin("jvm") version "%(kotlin)s" }
kotlin { jvmToolchain(17) }
repositories { mavenLocal(); mavenCentral() }
// A JVM project cannot use an AAR directly: take its classes.jar. Resolution of the coordinates is what is under test.
val aar by configurations.creating { isCanBeConsumed = false }
dependencies {
    aar("%(coord)s@aar")
    implementation(kotlin("stdlib"))
    implementation(files(zipTree(aar.singleFile).matching { include("classes.jar") }.singleFile))
}
""" % dict(kotlin=kotlin, coord=coord))
        open(os.path.join(proj, "src/main/kotlin/Use.kt"), "w").write(
            "import gamemeta.GameMeta\nfun use(): Any? = GameMeta.read(ByteArray(0), \"game.3ds\")\n")
        env = dict(os.environ, JAVA_TOOL_OPTIONS="-Dmaven.repo.local=%s" % os.path.join(home, ".m2/repository"))
        r = subprocess.run([os.path.join(KOTLIN, "gradlew"), "-p", proj, "--no-daemon", "--offline", "-q", "compileKotlin"],
                           cwd=KOTLIN, env=env, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout[-2000:] + r.stderr[-3000:])


if __name__ == "__main__":
    unittest.main()
