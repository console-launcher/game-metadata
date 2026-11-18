// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import assert from 'node:assert/strict';
import test from 'node:test';
import { FORMATS, exportFileName, formatBytes, probeMp4, progressText } from '../src/bannerexport.js';

test('the file is named <title>-banner.<ext>, with what a file name cannot hold taken out', () => {
  assert.equal(exportFileName('Super Game', 'gif'), 'Super Game-banner.gif');
  assert.equal(exportFileName('A/B: C?*"<>|\\D', 'webp'), 'A B C D-banner.webp');
  assert.equal(exportFileName('  ..hidden.. ', 'mp4'), 'hidden-banner.mp4');
  assert.equal(exportFileName('', 'gif'), 'game-banner.gif');
  assert.equal(exportFileName(undefined, 'gif'), 'game-banner.gif');
  assert.equal(exportFileName('tab\tand\nnewline\u0000nul', 'gif'), 'tab and newline nul-banner.gif');
  assert.ok(exportFileName('x'.repeat(500), 'gif').length <= 80 + '-banner.gif'.length);
  assert.equal(exportFileName('日本語のタイトル', 'gif'), '日本語のタイトル-banner.gif', 'other scripts are kept');
});

test('the three formats are GIF, MP4 and animated WebP', () => {
  assert.deepEqual(FORMATS.map((f) => [f.id, f.ext, f.mime]), [
    ['gif', 'gif', 'image/gif'],
    ['mp4', 'mp4', 'video/mp4'],
    ['webp', 'webp', 'image/webp'],
  ]);
});

test('sizes and progress read plainly', () => {
  assert.equal(formatBytes(500), '1 KB');
  assert.equal(formatBytes(2048), '2 KB');
  assert.equal(formatBytes(1572864), '1.5 MB');
  assert.equal(progressText('GIF', 0, 0), 'Making GIF… 0%');
  assert.equal(progressText('GIF', 50, 200), 'Making GIF… 25%');
  assert.equal(progressText('MP4', 200, 200), 'Making MP4… 99%', 'never 100% before the file is saved');
});

test('MP4 is offered only where an H.264 encoder exists', async () => {
  const reason = 'This browser can’t make video files.';
  assert.deepEqual(await probeMp4({}), { ok: false, reason });
  class Frame {}
  const supports = (codecs) => class { static async isConfigSupported(c) { return { supported: codecs.includes(c.codec) }; } };
  assert.deepEqual(await probeMp4({ VideoFrame: Frame, VideoEncoder: supports([]) }), { ok: false, reason });
  assert.deepEqual(await probeMp4({ VideoFrame: Frame, VideoEncoder: supports(['avc1.42001f']) }), { ok: true });
  const throwing = class { static async isConfigSupported() { throw new Error('nope'); } };
  assert.equal((await probeMp4({ VideoFrame: Frame, VideoEncoder: throwing })).ok, false);
  assert.equal((await probeMp4({ VideoEncoder: supports(['avc1.4d0028']) })).ok, false, 'no VideoFrame, no video');
});
