// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The wasm lives here, off the page's thread: it reads each picked File in place (FileReaderSync over Blob.slice, so a
// multi-gigabyte ROM is never loaded whole), hashes in chunks, and draws banners into OffscreenCanvases. Nothing here makes a
// network request after the wasm module loads.
import init, { ExportFrames, ExportJob, Gles, Mp4, RaHash, SoftRenderer, inspect, inspect_folder } from './pkg/gamemeta_wasm.js';
import { Cancelled, encodeMp4 } from './video.js';

const CHUNK = 4 * 1024 * 1024;
const files = new Map(); // id -> File
const scenes = new Map(); // id -> wasm Scene
const banners = new Map(); // id -> { stop() }
let ready;

const post = (message, transfer = []) => postMessage(message, transfer);
const baseOf = (path) => path.slice(path.lastIndexOf('/') + 1).toLowerCase();

async function inspectUnit({ id, unit }) {
  let result;
  if (unit.kind === 'title') {
    const paths = unit.entries.map((e) => e.path);
    result = inspect_folder(paths, unit.entries.map((e) => e.file), unit.name);
  } else {
    const byName = new Map(unit.siblings.map((s) => [baseOf(s.path), s.file]));
    result = inspect(unit.file, unit.name, (path) => byName.get(baseOf(path)), unit.zipEntry);
    files.set(id, unit.file);
  }
  const json = result.json();
  const icon = result.icon_rgba() ?? null;
  const smallIcon = result.small_icon_rgba() ?? null;
  const scene = result.take_scene();
  result.free();
  if (scene) scenes.set(id, scene);
  post({ ev: 'inspected', id, json, icon: icon?.buffer ?? null, smallIcon: smallIcon?.buffer ?? null }, [icon?.buffer, smallIcon?.buffer].filter(Boolean));
}

// The RetroAchievements hash reads the whole file: stream it in chunks and report progress.
async function hashFile({ id, name }) {
  const file = files.get(id);
  const hash = file && RaHash.start(name, file.size);
  if (!hash) return post({ ev: 'hash', id, md5: null });
  try {
    for (let at = 0; at < file.size; at += CHUNK) {
      hash.update(new Uint8Array(await file.slice(at, at + CHUNK).arrayBuffer()));
      post({ ev: 'hash-progress', id, done: Math.min(at + CHUNK, file.size), total: file.size });
    }
    post({ ev: 'hash', id, md5: hash.finish() ?? null });
  } catch (e) {
    post({ ev: 'hash', id, md5: null, error: String(e?.message ?? e) });
  } finally {
    hash.free();
  }
}

const W = 400;
const H = 240;
const frameAt = (t0, now) => ((now - t0) / 1000) * 60;
const next = (f) => (typeof requestAnimationFrame === 'function' ? requestAnimationFrame(f) : setTimeout(() => f(performance.now()), 16));

// Draws the banner into the page's canvas with the GLES renderer through a WebGL 2 context made here.
function startGl({ id, canvas, paused }) {
  const scene = scenes.get(id);
  stopBanner(id);
  let gl;
  let gles;
  try {
    canvas.width = W;
    canvas.height = H;
    gl = canvas.getContext('webgl2', { alpha: true, premultipliedAlpha: false, antialias: false, depth: true });
    if (!gl || !scene) throw new Error('no WebGL 2 context');
    gles = new Gles(scene, gl);
  } catch (e) {
    return post({ ev: 'banner', id, renderer: null, error: String(e?.message ?? e) });
  }
  const state = { paused, t0: performance.now(), held: 0, dead: false };
  const draw = (now) => {
    if (state.dead) return;
    const frame = state.paused ? state.held : frameAt(state.t0, now);
    if (!state.paused || state.dirty) {
      state.held = frame;
      state.dirty = false;
      try {
        gles.draw(gl, frame, W, H);
      } catch (e) {
        state.dead = true;
        return post({ ev: 'banner', id, renderer: null, error: String(e?.message ?? e) });
      }
    }
    next(draw);
  };
  state.dirty = true;
  banners.set(id, {
    pause(p) {
      if (p === state.paused) return;
      if (!p) state.t0 = performance.now() - (state.held / 60) * 1000;
      state.paused = p;
    },
    stop() {
      state.dead = true;
      gles.destroy(gl);
      gl.getExtension('WEBGL_lose_context')?.loseContext();
    },
  });
  post({ ev: 'banner', id, renderer: 'webgl2' });
  next(draw);
}

// One CPU-rendered frame, for a page that cannot hand its canvas to this worker (or whose WebGL 2 failed).
const softs = new Map();
function softFrame({ id, frame }) {
  let soft = softs.get(id);
  if (!soft && scenes.has(id)) softs.set(id, (soft = new SoftRenderer(scenes.get(id))));
  if (!soft) return post({ ev: 'frame', id, pixels: null });
  const pixels = soft.render(frame, W, H);
  post({ ev: 'frame', id, pixels: pixels.buffer }, [pixels.buffer]);
}

// ---------------------------------------------------------------- exports

// An export draws its own pictures with the CPU renderer (never the page's canvas), so it runs the same for a card that is off
// screen or paused. It gives the event loop back often enough for a cancel to arrive.
const exports_ = new Map(); // id -> { cancelled }
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
const MIME = { gif: 'image/gif', webp: 'image/webp', mp4: 'video/mp4' };

async function exportBanner({ id, format, scale }) {
  const scene = scenes.get(id);
  if (!scene) return post({ ev: 'export-error', id, message: 'no banner' });
  const run = { cancelled: false };
  exports_.set(id, run);
  const progress = (done, total) => post({ ev: 'export-progress', id, done, total });
  try {
    let bytes;
    let note = '';
    if (format === 'mp4') {
      const frames = new ExportFrames(scene, scale);
      try {
        ({ bytes, note } = await encodeMp4({ frames, scene, Mp4, onProgress: progress, isCancelled: () => run.cancelled }));
      } finally {
        frames.free();
      }
    } else {
      const job = new ExportJob(scene, format, scale);
      const total = job.total();
      let last = performance.now();
      let finished = false;
      try {
        while (job.step()) {
          if (run.cancelled) throw new Cancelled();
          if (performance.now() - last > 50) {
            progress(job.done(), total);
            await tick();
            last = performance.now();
          }
        }
        if (run.cancelled) throw new Cancelled();
        finished = true; // finish() takes the job
        bytes = job.finish();
      } catch (e) {
        if (!finished) job.free();
        throw e;
      }
    }
    post({ ev: 'export-done', id, format, mime: MIME[format], note, bytes: bytes.buffer }, [bytes.buffer]);
  } catch (e) {
    if (e instanceof Cancelled || run.cancelled) post({ ev: 'export-cancelled', id });
    else post({ ev: 'export-error', id, message: String(e?.message ?? e) });
  } finally {
    exports_.delete(id);
  }
}

function stopBanner(id) {
  banners.get(id)?.stop();
  banners.delete(id);
}

function forget(id) {
  const run = exports_.get(id);
  if (run) run.cancelled = true;
  stopBanner(id);
  softs.get(id)?.free();
  softs.delete(id);
  scenes.get(id)?.free();
  scenes.delete(id);
  files.delete(id);
}

const handlers = {
  inspect: inspectUnit,
  hash: hashFile,
  'banner-gl': startGl,
  'banner-pause': ({ id, paused }) => banners.get(id)?.pause(paused),
  'banner-stop': ({ id }) => stopBanner(id),
  frame: softFrame,
  export: exportBanner,
  'export-cancel': ({ id }) => { const run = exports_.get(id); if (run) run.cancelled = true; },
  forget: ({ id }) => forget(id),
};

onmessage = async (event) => {
  const message = event.data;
  try {
    await ready;
    await handlers[message.op](message);
  } catch (e) {
    post({ ev: 'error', id: message.id, op: message.op, message: String(e?.message ?? e) });
  }
};

ready = init({ module_or_path: new URL('./pkg/gamemeta_wasm_bg.wasm', import.meta.url) }).then(() => post({ ev: 'ready' }));
ready.catch((e) => post({ ev: 'error', op: 'init', message: String(e?.message ?? e) }));
