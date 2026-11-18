// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import assert from 'node:assert/strict';
import test from 'node:test';
import { formatSize, groupEntries, titleRoots, words } from '../src/files.js';

const entry = (path) => ({ file: { name: path }, path });

test('a folder with meta/meta.xml is one title; the rest are loose files', () => {
  const units = groupEntries(
    ['Games/Zelda/code/a.rpx', 'Games/Zelda/meta/meta.xml', 'Games/Zelda/content/x.bin', 'Games/readme.txt'].map(entry),
  );
  assert.equal(units.length, 2);
  assert.deepEqual([units[0].kind, units[0].name, units[0].entries.length], ['title', 'Zelda', 3]);
  assert.deepEqual([units[1].kind, units[1].name, units[1].inFolder], ['file', 'readme.txt', true]);
});

test('lookalike paths are not title roots, and the top folder can be one', () => {
  assert.deepEqual(titleRoots(['x/notmeta/meta.xml', 'a/xmeta/meta.xml']), []);
  assert.deepEqual(titleRoots(['meta/meta.xml', 'code/a.rpx']), ['']);
  const [unit] = groupEntries(['meta/meta.xml', 'code/a.rpx'].map(entry));
  assert.equal(unit.name, 'Wii U title');
});

test('two titles in one folder stay two units, each with its own files', () => {
  const units = groupEntries(['G/A/meta/meta.xml', 'G/A/code/a.rpx', 'G/B/meta/meta.xml'].map(entry));
  assert.deepEqual(units.map((u) => [u.name, u.entries.length]), [['A', 2], ['B', 1]]);
});

test('loose files know their neighbours (a cue sheet finds its tracks) and junk is dropped', () => {
  const units = groupEntries(['d/game.cue', 'd/game.bin', 'd/.DS_Store', 'other/x.iso', 'top.nds'].map(entry));
  const cue = units.find((u) => u.name === 'game.cue');
  assert.deepEqual(cue.siblings.map((s) => s.path), ['d/game.bin']);
  assert.equal(units.length, 4);
  assert.equal(units.find((u) => u.name === 'top.nds').inFolder, false);
});

test('sizes and names read well', () => {
  assert.equal(formatSize(999), '999 B');
  assert.equal(formatSize(31 * 1024), '31.0 KiB');
  assert.equal(formatSize(4.5 * 1024 ** 3), '4.5 GiB');
  assert.equal(words('SimplifiedChinese'), 'Simplified Chinese');
});
