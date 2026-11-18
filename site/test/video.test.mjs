// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The MP4 path with fake WebCodecs classes (the headless test browser has no H.264 encoder) and the real wasm muxer from site/dist
// (scripts/site-test builds it first).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { Cancelled, encodeMp4 } from '../src/video.js';
import { initSync, Mp4 } from '../dist/pkg/gamemeta_wasm.js';

initSync({ module: readFileSync(new URL('../dist/pkg/gamemeta_wasm_bg.wasm', import.meta.url)) });

const AVCC = new Uint8Array([1, 0x4d, 0, 0x28, 0xff, 0xe1, 0, 2, 0x67, 0x4d, 0]);
const OPUS_HEAD = new Uint8Array([0x4f, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64, 1, 1, 0x38, 0x01, 0x80, 0x3e, 0, 0, 0, 0, 0]);
const ASC = new Uint8Array([0x12, 0x10]);

// ---- fakes

const chunkOf = (bytes, extra = {}) => ({ byteLength: bytes.length, copyTo: (dst) => dst.set(bytes), ...extra });

function fakeCodecs({ h264 = true, aac = true, opus = true, log = {} } = {}) {
  class VideoFrame {
    constructor(data, init) { Object.assign(this, { data, init }); log.frames = (log.frames ?? 0) + 1; }
    close() { log.closed = (log.closed ?? 0) + 1; }
  }
  class VideoEncoder {
    static async isConfigSupported(c) { return { supported: h264 && c.codec === 'avc1.4d0028' }; }
    constructor(cb) { this.cb = cb; this.encodeQueueSize = 0; this.n = 0; log.video = this; }
    configure(config) { log.videoConfig = config; }
    encode(frame, opts) {
      const first = this.n++ === 0;
      this.cb.output(chunkOf(new Uint8Array([0, 0, 0, 1, frame.init.timestamp & 255]), { type: opts.keyFrame ? 'key' : 'delta' }), first ? { decoderConfig: { description: AVCC } } : {});
    }
    async flush() {}
    close() { log.videoClosed = true; }
  }
  class AudioData {
    constructor(init) { Object.assign(this, init); }
    close() {}
  }
  const supports = (codec) => (codec === 'mp4a.40.2' ? aac : codec === 'opus' && opus);
  class AudioEncoder {
    static async isConfigSupported(c) { return { supported: supports(c.codec) }; }
    constructor(cb) { this.cb = cb; this.n = 0; }
    configure(config) { this.config = config; log.audioConfig = config; }
    encode(data) {
      const first = this.n++ === 0;
      const description = first ? { decoderConfig: { description: this.config.codec === 'opus' ? OPUS_HEAD : ASC } } : {};
      this.cb.output(chunkOf(new Uint8Array(6).fill(this.n), { duration: (data.numberOfFrames / data.sampleRate) * 1e6 }), description);
    }
    async flush() {}
    close() {}
  }
  return { VideoEncoder, VideoFrame, AudioEncoder, AudioData };
}

function fakeFrames({ animated = true, count = 130, log = {} } = {}) {
  return {
    width: () => 400, height: () => 240, animated: () => animated,
    count: (fps) => { log.countFps = fps; return count; },
    render: (i, fps, opaque) => { log.rendered = (log.rendered ?? 0) + 1; assert.equal(opaque, true, 'video is over opaque black'); return new Uint8Array(4 * 400 * 240).fill(i & 255); },
    free() {},
  };
}

const fakeScene = (sound) => ({
  has_sound: () => !!sound,
  sound_rate: () => sound?.rate ?? 0,
  sound_channels: () => sound?.channels ?? 0,
  sound_pcm: (rate, seconds) => { sound.asked = { rate, seconds }; return new Int16Array(Math.floor(Math.min(seconds, sound.seconds) * rate) * sound.channels).fill(100); },
});

// ---- a box reader

function boxes(b) {
  const out = [];
  for (let at = 0; at < b.length;) {
    const size = new DataView(b.buffer, b.byteOffset + at, 4).getUint32(0);
    out.push({ type: String.fromCharCode(...b.subarray(at + 4, at + 8)), body: b.subarray(at + 8, at + size) });
    at += size;
  }
  return out;
}
const find = (b, ...path) => path.reduce((cur, t) => boxes(cur).find((x) => x.type === t)?.body, b);
const u32 = (b, at) => new DataView(b.buffer, b.byteOffset + at, 4).getUint32(0);
const tracks = (file) => boxes(find(file, 'moov')).filter((b) => b.type === 'trak').map((b) => b.body);
const stbl = (trak) => find(trak, 'mdia', 'minf', 'stbl');
const entryType = (trak) => String.fromCharCode(...find(stbl(trak), 'stsd').subarray(12, 16));

// ---- tests

test('an animated banner becomes an H.264 track of one sample per picture, a key frame each second', async () => {
  const log = {};
  const frames = fakeFrames({ count: 130, log });
  const { bytes, note } = await encodeMp4({ frames, scene: fakeScene(null), Mp4, g: fakeCodecs({ log }) });
  assert.equal(note, '');
  assert.equal(log.countFps, 60);
  assert.equal(log.frames, 130);
  assert.equal(log.closed, 130, 'every VideoFrame is closed');
  assert.equal(log.videoConfig.codec, 'avc1.4d0028');
  assert.deepEqual(boxes(bytes).map((b) => b.type), ['ftyp', 'moov', 'mdat']);
  const [video, ...others] = tracks(bytes);
  assert.equal(others.length, 0, 'silent: one track');
  assert.equal(entryType(video), 'avc1');
  const sizes = find(stbl(video), 'stsz');
  assert.equal(u32(sizes, 8), 130);
  const keys = find(stbl(video), 'stss');
  assert.deepEqual([u32(keys, 4), u32(keys, 8), u32(keys, 12), u32(keys, 16)], [3, 1, 61, 121], 'key frames at 0, 60 and 120');
  const mdhd = find(video, 'mdia', 'mdhd');
  assert.deepEqual([u32(mdhd, 12), u32(mdhd, 16)], [60, 130], 'timescale 60: 130 pictures, 1 tick each');
});

test('a banner that does not move is a 3-second video of one picture', async () => {
  const log = {};
  const { bytes } = await encodeMp4({ frames: fakeFrames({ animated: false, log }), scene: fakeScene(null), Mp4, g: fakeCodecs() });
  assert.equal(log.rendered, 1, 'drawn once');
  assert.equal(u32(find(stbl(tracks(bytes)[0]), 'stsz'), 8), 180);
});

test('the jingle becomes an AAC track, cut to the video\'s length', async () => {
  const sound = { rate: 32000, channels: 2, seconds: 10 };
  const log = {};
  const { bytes, note } = await encodeMp4({ frames: fakeFrames({ count: 120 }), scene: fakeScene(sound), Mp4, g: fakeCodecs({ log }) });
  assert.equal(note, '');
  assert.equal(log.audioConfig.codec, 'mp4a.40.2');
  assert.deepEqual(sound.asked, { rate: 32000, seconds: 2 }, 'asked for the video\'s 2 seconds at the file\'s own rate');
  const t = tracks(bytes);
  assert.equal(t.length, 2);
  assert.equal(entryType(t[1]), 'mp4a');
});

test('Opus in MP4 when the browser has no AAC; no sound, said so, when it has neither', async () => {
  const sound = { rate: 32000, channels: 1, seconds: 1 };
  const opus = await encodeMp4({ frames: fakeFrames({ count: 30 }), scene: fakeScene(sound), Mp4, g: fakeCodecs({ aac: false }) });
  assert.equal(entryType(tracks(opus.bytes)[1]), 'Opus');
  assert.equal(opus.note, '');
  const none = await encodeMp4({ frames: fakeFrames({ count: 30 }), scene: fakeScene(sound), Mp4, g: fakeCodecs({ aac: false, opus: false }) });
  assert.equal(tracks(none.bytes).length, 1);
  assert.equal(none.note, 'No sound in this browser.');
  const { AudioEncoder, AudioData, ...noAudioClasses } = fakeCodecs();
  const bare = await encodeMp4({ frames: fakeFrames({ count: 30 }), scene: fakeScene(sound), Mp4, g: noAudioClasses });
  assert.equal(bare.note, 'No sound in this browser.');
  const silentBanner = await encodeMp4({ frames: fakeFrames({ count: 30 }), scene: fakeScene(null), Mp4, g: noAudioClasses });
  assert.equal(silentBanner.note, '', 'a banner with no jingle has nothing to apologise for');
});

test('no H.264 encoder: it says so instead of making a file', async () => {
  await assert.rejects(encodeMp4({ frames: fakeFrames(), scene: fakeScene(null), Mp4, g: fakeCodecs({ h264: false }) }), /no H.264 encoder/);
});

test('cancelling stops the encoder and gives nothing', async () => {
  const log = {};
  let calls = 0;
  await assert.rejects(
    encodeMp4({ frames: fakeFrames({ log }), scene: fakeScene(null), Mp4, g: fakeCodecs({ log }), isCancelled: () => ++calls > 5 }),
    Cancelled,
  );
  assert.equal(log.videoClosed, true);
  assert.ok(log.frames <= 6);
});

test('progress is reported while the pictures go through', async () => {
  const seen = [];
  const frames = fakeFrames({ count: 40 });
  const slow = frames.render;
  frames.render = (...a) => { const end = performance.now() + 4; while (performance.now() < end); return slow(...a); };
  await encodeMp4({ frames, scene: fakeScene(null), Mp4, g: fakeCodecs(), onProgress: (done, total) => seen.push([done, total]) });
  assert.ok(seen.length >= 1 && seen.every(([d, t]) => d <= t && t === 40), JSON.stringify(seen));
});
