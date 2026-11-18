#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Tests for scripts/third-party-notices: NOTICE.md must carry every crate the artifacts link, with a copyright line and a license text.
  python3 -I scripts/test_third_party_notices.py"""
import importlib.machinery
import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
loader = importlib.machinery.SourceFileLoader("tpn", os.path.join(HERE, "third-party-notices"))
tpn = importlib.util.module_from_spec(importlib.util.spec_from_loader("tpn", loader))
loader.exec_module(tpn)

MUST_LIST = ["cfg-if", "libc", "wasm-bindgen", "js-sys", "futures-core", "futures-task", "futures-util", "once_cell",
             "pin-project-lite", "slab", "bumpalo", "encoding_rs"]


class Notices(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = tpn.current()
        cls.rows = tpn.entries()

    def test_every_linked_crate_is_listed(self):
        self.assertEqual(tpn.missing(self.text, self.rows), [])

    def test_the_crates_the_review_found_missing_are_linked_and_listed(self):
        names = {p["name"] for p, _ in self.rows}
        for n in MUST_LIST:
            self.assertIn(n, names)

    def test_a_removed_crate_is_reported(self):
        cut = self.text.replace("### cfg-if ", "### gone ")
        self.assertEqual(tpn.missing(cut, self.rows), ["cfg-if %s" % next(p["version"] for p, _ in self.rows if p["name"] == "cfg-if")])

    def test_the_section_is_current(self):
        self.assertEqual(tpn.replace(self.text, tpn.render()), self.text)

    def test_the_license_texts_and_copyright_lines_are_there(self):
        for kind in ["Permission is hereby granted", "Apache License", "Redistribution and use", "Copyright (c)"]:
            self.assertIn(kind, self.text)

    def test_split_pulls_copyright_lines_and_ignores_apache_boilerplate(self):
        n, t = tpn.split("MIT\n\nCopyright (c) 2020 A B\n\nBody\n\n   Copyright [yyyy] [name of copyright owner]\n")
        self.assertEqual(n, ["Copyright (c) 2020 A B"])
        self.assertIn("Copyright [yyyy]", t)
        self.assertNotIn("A B", t)


if __name__ == "__main__":
    unittest.main()
