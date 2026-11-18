// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// MP4 for the worker: the banner's pictures through the browser's H.264 encoder (WebCodecs), its jingle through an AAC or Opus
// encoder when the browser has one, and the wasm muxer around the encoded chunks. The codec classes come in as `g` (the worker's
// global scope; a test passes fakes).
import { pickH264 } from './bannerexport.js';

const FPS = 60;
const STILL_SECONDS = 3; // a banner that does not move is a 3-second video
const KEY_EVERY = 60;
const SLICE = 4096; // audio frames per AudioData

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
export class Cancelled extends Error {}

const copyOut = (chunk) => {
  const data = new Uint8Array(chunk.byteLength);
  chunk.copyTo(data);
  return data;
};
const bytesOf = (description) => (description ? new Uint8Array(description instanceof ArrayBuffer ? description : description.buffer.slice(description.byteOffset, description.byteOffset + description.byteLength)) : null);

// Encodes `pcm` (interleaved s16) with the first audio config the browser takes: AAC at the file's rate, AAC at 48 kHz, Opus at 48 kHz.
// -> { kind: 'aac' | 'opus', rate, channels, config: Uint8Array | null, chunks: [{ data, samples }] } or null.
async function encodeAudio(g, scene, seconds, isCancelled) {
  if (typeof g.AudioEncoder !== 'function' || typeof g.AudioData !== 'function') return null;
  const channels = scene.sound_channels();
  const native = scene.sound_rate();
  if (channels < 1 || channels > 2 || !native) return null;
  const tries = [
    { kind: 'aac', codec: 'mp4a.40.2', rate: native, extra: { aac: { format: 'aac' } } },
    { kind: 'aac', codec: 'mp4a.40.2', rate: 48000, extra: { aac: { format: 'aac' } } },
    { kind: 'opus', codec: 'opus', rate: 48000, extra: {} },
  ];
  for (const t of tries) {
    const config = { codec: t.codec, sampleRate: t.rate, numberOfChannels: channels, bitrate: 96000 * channels, ...t.extra };
    try {
      if (!(await g.AudioEncoder.isConfigSupported(config))?.supported) continue;
      const pcm = scene.sound_pcm(t.rate, seconds);
      if (!pcm.length) return null;
      const out = { kind: t.kind, rate: t.rate, channels, nativeRate: native, config: null, chunks: [] };
      let failure = null;
      const encoder = new g.AudioEncoder({
        output: (chunk, meta) => {
          const description = meta?.decoderConfig?.description;
          if (description && !out.config) out.config = bytesOf(description);
          out.chunks.push({ data: copyOut(chunk), samples: Math.max(1, Math.round((chunk.duration * t.rate) / 1e6)) });
        },
        error: (e) => { failure = e; },
      });
      encoder.configure(config);
      const frames = Math.floor(pcm.length / channels);
      for (let at = 0; at < frames; at += SLICE) {
        if (isCancelled()) { encoder.close(); throw new Cancelled(); }
        const n = Math.min(SLICE, frames - at);
        const data = new g.AudioData({
          format: 's16', sampleRate: t.rate, numberOfFrames: n, numberOfChannels: channels,
          timestamp: Math.round((at * 1e6) / t.rate), data: pcm.subarray(at * channels, (at + n) * channels),
        });
        encoder.encode(data);
        data.close();
      }
      await encoder.flush();
      encoder.close();
      if (failure || !out.chunks.length) continue;
      return out;
    } catch (e) {
      if (e instanceof Cancelled) throw e;
    }
  }
  return null;
}

// -> { bytes: Uint8Array, note: string } (note: what the visitor should know, e.g. "No sound in this browser.")
export async function encodeMp4({ frames, scene, Mp4, onProgress = () => {}, isCancelled = () => false, g = globalThis }) {
  const width = frames.width();
  const height = frames.height();
  const config = await pickH264(g, width, height);
  if (!config) throw new Error('no H.264 encoder');
  const still = !frames.animated();
  const count = still ? STILL_SECONDS * FPS : frames.count(FPS);
  const chunks = [];
  let avcc = null;
  let failure = null;
  const encoder = new g.VideoEncoder({
    output: (chunk, meta) => {
      const description = meta?.decoderConfig?.description;
      if (description && !avcc) avcc = bytesOf(description);
      chunks.push({ data: copyOut(chunk), key: chunk.type === 'key' });
    },
    error: (e) => { failure = e; },
  });
  encoder.configure(config);
  const stillPixels = still ? frames.render(0, FPS, true) : null;
  let lastYield = performance.now();
  for (let i = 0; i < count; i++) {
    if (isCancelled()) { encoder.close(); throw new Cancelled(); }
    if (failure) throw failure;
    const pixels = stillPixels ?? frames.render(i, FPS, true);
    const frame = new g.VideoFrame(pixels, { format: 'RGBA', codedWidth: width, codedHeight: height, timestamp: Math.round((i * 1e6) / FPS), duration: Math.round(1e6 / FPS) });
    encoder.encode(frame, { keyFrame: i % KEY_EVERY === 0 });
    frame.close();
    while (encoder.encodeQueueSize > 6) await tick();
    if (performance.now() - lastYield > 50) {
      onProgress(i + 1, count);
      await tick();
      lastYield = performance.now();
    }
  }
  await encoder.flush();
  encoder.close();
  if (failure) throw failure;
  if (!avcc || chunks.length === 0) throw new Error('the encoder gave no video');
  let note = '';
  let audio = null;
  if (scene.has_sound()) {
    audio = await encodeAudio(g, scene, count / FPS, isCancelled).catch((e) => { if (e instanceof Cancelled) throw e; return null; });
    if (!audio) note = 'No sound in this browser.';
  }
  const mp4 = new Mp4(width, height, FPS, avcc);
  try {
    if (audio?.kind === 'aac' && audio.config) mp4.set_aac(audio.config, audio.rate, audio.channels);
    else if (audio?.kind === 'opus') mp4.set_opus(audio.config && audio.config.length >= 12 ? audio.config[10] | (audio.config[11] << 8) : 312, audio.nativeRate, audio.channels);
    else {
      audio = null;
      if (scene.has_sound()) note = 'No sound in this browser.';
    }
    for (const c of chunks) mp4.push_video(c.data, 1, c.key);
    if (audio) for (const c of audio.chunks) mp4.push_audio(c.data, c.samples);
  } catch (e) {
    mp4.free();
    throw e;
  }
  return { bytes: mp4.finish(), note };
}
