// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The page: picks files, hands them to the worker (which reads them in place), and shows what comes back. Text from a file only
// ever reaches the page through textContent, never as HTML.
import { basename, formatSize, groupEntries, words } from './files.js';
import { FORMATS, exportFileName, formatBytes, probeMp4, progressText } from './bannerexport.js';

const $ = (selector) => document.querySelector(selector);
const results = $('#results');
const status = $('#status');
const progress = $('#progress');
const skippedBox = $('#skipped');
const params = new URLSearchParams(location.search);
const forceSoft = params.get('renderer') === 'soft'; // a test hook: skip WebGL 2
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;

const h = (tag, attrs = {}, ...children) => {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'class') el.className = v;
    else if (v === true) el.setAttribute(k, '');
    else if (v !== false && v != null) el.setAttribute(k, v);
  }
  el.append(...children.flat().filter((c) => c != null && c !== false));
  return el;
};

// A few workers read files side by side (each holds its own copy of the wasm); a card's banner, hash and frames are always
// asked of the worker that inspected it, because that is where its scene lives.
const MAX_WORKERS = Math.max(2, Math.min(4, navigator.hardwareConcurrency || 2));
const MAX_HASHES = 2;
let readyCount = 0;
const pool = []; // { worker, inspecting: card id | null }
const owner = new Map(); // card id -> pool entry
const queue = []; // inspect jobs: { card, message }
const hashQueue = []; // cards whose RetroAchievements hash waits until every file has its card
let hashing = 0;
const batch = { total: 0, done: 0, skipped: [] };
const cards = new Map(); // id -> card state
let nextId = 1;
let demo = null;

// ---------------------------------------------------------------- worker messages

function spawn() {
  const entry = { worker: new Worker(new URL('./worker.js', import.meta.url), { type: 'module' }), inspecting: null };
  entry.worker.onmessage = ({ data }) => onMessage(entry, data);
  entry.worker.onerror = (e) => {
    status.textContent = 'Couldn’t get ready. Try reloading the page.';
  };
  pool.push(entry);
  return entry;
}
const to = (card, message, transfer = []) => owner.get(card.id)?.worker.postMessage(message, transfer);

function onMessage(entry, data) {
  const card = cards.get(data.id);
  switch (data.ev) {
    case 'ready':
      // Every worker loads its wasm up front, so nothing is fetched once the page is in use.
      if (++readyCount === MAX_WORKERS) {
        document.documentElement.dataset.ready = 'true';
        if (status.textContent === 'Getting ready…') status.textContent = '';
      }
      break;
    case 'inspected':
      finishInspect(entry);
      if (card) showInspection(card, data);
      pump();
      break;
    case 'hash-progress':
      if (card?.hashProgress) {
        card.hashProgress.value = data.done;
        card.hashProgress.max = data.total;
      }
      break;
    case 'hash':
      hashing--;
      if (card) showHash(card, data.md5, data.error);
      pump();
      break;
    case 'banner':
      if (card) bannerStarted(card, data);
      break;
    case 'frame':
      if (card) softPaint(card, data.pixels);
      break;
    case 'export-progress':
    case 'export-done':
    case 'export-cancelled':
    case 'export-error':
      if (card) exportEvent(card, data);
      break;
    case 'error':
      if (data.op === 'hash') {
        hashing--;
        if (card) showHash(card, null, data.message);
      } else if (data.op === 'inspect') {
        finishInspect(entry);
        if (card) showError(card, 'Couldn’t read this file.');
      } else if (card) showError(card, 'Something went wrong with this file.');
      else status.textContent = 'Something went wrong.';
      pump();
      break;
  }
}

function finishInspect(entry) {
  entry.inspecting = null;
  batch.done++;
}

// Starts what can start: an inspect on every idle worker, and, once no file is waiting for its card, the hashes (a couple at a time).
function pump() {
  while (queue.length) {
    const entry = pool.find((e) => !e.inspecting);
    if (!entry) break;
    const { card, message } = queue.shift();
    if (!cards.has(card.id)) {
      batch.done++;
      continue;
    }
    entry.inspecting = card.id;
    owner.set(card.id, entry);
    entry.worker.postMessage({ op: 'inspect', id: card.id, unit: message });
  }
  if (!queue.length && pool.every((e) => !e.inspecting)) {
    while (hashQueue.length && hashing < MAX_HASHES) {
      const { card, name } = hashQueue.shift();
      if (!cards.has(card.id)) continue;
      hashing++;
      to(card, { op: 'hash', id: card.id, name });
    }
  }
  showProgress();
}

function showProgress() {
  const { total, done, skipped } = batch;
  const parts = [];
  if (done < total) parts.push(`Reading ${done} of ${total} files…`);
  else if (total) parts.push(`Read ${total} file${total === 1 ? '' : 's'}.`);
  const waiting = hashQueue.length + hashing;
  if (waiting && done >= total) parts.push(`Calculating ${waiting} checksum${waiting === 1 ? '' : 's'}…`);
  progress.textContent = parts.join(' ');
  skippedBox.hidden = !skipped.length;
  if (skipped.length) {
    skippedBox.replaceChildren(h('summary', {}, `${skipped.length} file${skipped.length === 1 ? '' : 's'} skipped (not games)`), h('p', { class: 'muted' }, skipped.join(', ')));
  }
}

for (let i = 0; i < MAX_WORKERS; i++) spawn(); // all load now, from the page's own files

// ---------------------------------------------------------------- building cards

function newCard(unit) {
  const id = nextId++;
  const title = h('h3', {}, unit.name);
  const body = h('div', { class: 'body' }, h('p', { class: 'muted' }, 'Reading…'));
  const el = h('article', { class: 'card', 'aria-label': unit.name }, h('div', { class: 'head' }, title), body);
  const card = { id, unit, el, title, body, banner: null };
  cards.set(id, card);
  results.append(el);
  return card;
}

function removeCard(card) {
  if (card.export?.running) to(card, { op: 'export-cancel', id: card.id });
  card.banner?.observer.disconnect();
  stopBanner(card);
  to(card, { op: 'forget', id: card.id });
  owner.delete(card.id);
  cards.delete(card.id);
  card.el.remove();
}

function showError(card, message) {
  card.el.dataset.state = 'error';
  card.body.replaceChildren(h('p', { class: 'error' }, message));
}

function drawIcon(bytes, { width, height }) {
  const canvas = h('canvas', { class: 'icon', width, height, 'aria-hidden': 'true' });
  canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(bytes), width, height), 0, 0);
  return canvas;
}

const PLATFORMS = { Ds: 'Nintendo DS', Dsi: 'Nintendo DSi', ThreeDs: 'Nintendo 3DS', GameCube: 'Nintendo GameCube', Wii: 'Nintendo Wii', WiiU: 'Nintendo Wii U',
  GameBoy: 'Game Boy', GameBoyColor: 'Game Boy Color', GameBoyAdvance: 'Game Boy Advance', Nintendo64: 'Nintendo 64', Snes: 'Super Nintendo', Nes: 'NES',
  MegaDrive: 'Sega Genesis / Mega Drive', Sega32x: 'Sega 32X', MasterSystem: 'Sega Master System', GameGear: 'Sega Game Gear' };

const row = (term, value, extra = {}) => value == null || value === '' ? null : [h('dt', {}, term), h('dd', extra, value)];

function showInspection(card, message) {
  try {
    show(card, message);
  } catch (e) {
    showError(card, 'Couldn’t show this file.');
  }
}

function show(card, { json, icon, smallIcon }) {
  const r = JSON.parse(json);
  const inner = r.zip ? ` › ${r.zip.entry}` : '';
  const info = r.info;
  const ids = r.ids ?? {};
  const bannerOk = r.banner.state === 'ok';
  if (!info && !r.ids && !r.ra && !bannerOk && r.banner.state !== 'encrypted') {
    if (card.unit.kind === 'title') return showError(card, 'No Wii U game found in that folder. Choose the game’s top folder.');
    if (r.known) return showError(card, 'Couldn’t read this file. It may be damaged or an unsupported version.');
    if (card.unit.inFolder) {
      // A folder pick holds all sorts of files: the ones that are not games are listed in one place, not given a card each.
      batch.skipped.push(r.name);
      return removeCard(card);
    }
    return showError(card, 'This type of file isn’t supported. See Supported files below.');
  }
  if (r.zip?.others.length) addZipCards(card, r);
  card.el.dataset.state = 'ok';
  card.el.dataset.title = info?.title ?? '';
  if (r.zip) card.el.setAttribute('aria-label', `${r.name}${inner}`);
  const head = card.el.querySelector('.head');
  head.replaceChildren();
  if (icon && info?.icon) head.append(drawIcon(icon, info.icon));
  else if (smallIcon && info?.smallIcon) head.append(drawIcon(smallIcon, info.smallIcon));
  const titles = info ? Object.values(info.titles) : [];
  const shown = titles.find((t) => t.name === info.title) ?? titles[0];
  const pieces = [shown?.publisher, info && (PLATFORMS[info.platform] ?? words(info.platform)), formatSize(r.size)].filter(Boolean);
  card.title.textContent = info?.title ?? r.name;
  head.append(h('div', { class: 'titles' }, card.title, h('p', { class: 'muted' }, r.name === (info?.title ?? r.name) && !inner ? '' : `${r.name}${inner} · `, pieces.join(' · '))));
  const remove = h('button', { type: 'button', class: 'quiet', 'aria-label': `Remove ${r.name}${inner}` }, '×');
  remove.addEventListener('click', () => removeCard(card));
  head.append(remove);

  const parts = [];
  if (info && Object.keys(info.titles).length) {
    const langs = Object.entries(info.titles);
    parts.push(h('details', { class: 'titles-list' }, h('summary', {}, `Titles in ${langs.length} language${langs.length === 1 ? '' : 's'}`),
      h('dl', {}, langs.flatMap(([lang, t]) => row(words(lang), [t.name, t.publisher ? ` · ${t.publisher}` : ''].join(''))))));
  }
  const facts = [
    row('Title ID', info?.titleId ?? ids.titleId, { class: 'mono' }),
    row('Product code', info?.productCode, { class: 'mono' }),
    row('Game ID', info?.gameId ?? ids.gameId, { class: 'mono' }),
    row('Maker code', info?.makerCode, { class: 'mono' }),
    row('Serial', ids.serial, { class: 'mono' }),
    row('Xbox title ID', ids.xboxId, { class: 'mono' }),
    row('PSP title', ids.pspTitle),
    row('PSP disc ID', ids.pspDiscId, { class: 'mono' }),
    row('Version', info?.version == null ? null : String(info.version)),
    row('Regions', info && (info.regionFree ? 'Region free' : info.regions.map(words).join(', '))),
    row('Ratings', info && Object.keys(info.ratings).length ? Object.entries(info.ratings).map(([b, a]) => `${b.toUpperCase()} ${a}`).join(', ') : null),
    row('CHD SHA-1', ids.chdSha1, { class: 'mono' }),
    row('ZIP ROM CRC-32', ids.zipCrc && `${ids.zipCrc}${ids.zipSize != null ? ` (${formatSize(ids.zipSize)})` : ''}`, { class: 'mono' }),
  ].filter(Boolean);
  if (r.ra) {
    card.hashProgress = h('progress', { value: 0, max: 1, 'aria-label': `Calculating checksum for ${r.name}` });
    card.hashCell = h('dd', { class: 'mono' }, card.hashProgress);
    facts.push([h('dt', {}, 'RetroAchievements MD5'), card.hashCell]);
  }
  if (facts.length) parts.push(h('dl', { class: 'facts' }, facts.flat()));
  if (info?.cart) { // a cartridge ROM: every decoded header field, raw codes included
    const fields = Object.entries(info.cart).filter(([k]) => k !== 'family');
    parts.push(h('details', { class: 'titles-list' }, h('summary', {}, 'Header fields'),
      h('dl', {}, fields.flatMap(([k, v]) => row(k.replaceAll('_', ' '), v == null ? null : String(v), { class: 'mono' })))));
  }
  if (r.banner.state === 'encrypted') parts.push(h('p', { class: 'muted' }, 'This 3DS file’s banner is encrypted, so it can’t be shown.'));
  if (r.banner.state === 'damaged') parts.push(h('p', { class: 'muted' }, 'This 3DS file’s banner is damaged and can’t be shown.'));
  card.body.replaceChildren(...parts);
  if (bannerOk) mountBanner(card, r.banner);
  if (r.ra) startHash(card, r);
}

// A zip of several ROMs gets a card for each: the first is read already, the others are asked for by their path in the zip.
function addZipCards(card, r) {
  let after = card.el;
  for (const entry of r.zip.others) {
    const unit = { ...card.unit, zipEntry: entry };
    const extra = newCard({ ...unit, name: `${r.name} › ${entry}` });
    extra.unit = unit;
    after.after(extra.el);
    after = extra.el;
    enqueue(extra, messageFor(unit));
  }
}

// ---------------------------------------------------------------- hashing

const AUTO_HASH_BYTES = 64 * 1024 * 1024;

function startHash(card, r) {
  if (r.size <= AUTO_HASH_BYTES) return hashQueue.push({ card, name: r.name });
  const button = h('button', { type: 'button' }, `Calculate (${formatSize(r.size)})`);
  button.addEventListener('click', () => {
    button.replaceWith(card.hashProgress);
    hashing++;
    to(card, { op: 'hash', id: card.id, name: r.name });
  });
  card.hashCell.replaceChildren(button);
}

function showHash(card, md5, error) {
  if (!card.hashCell) return;
  card.hashCell.replaceChildren(md5 ?? (error ? 'Couldn’t calculate' : 'Not available'));
  card.hashCell.dataset.value = md5 ?? '';
}

// ---------------------------------------------------------------- banners

const W = 400;
const H = 240;

// A banner is drawn only while its card is on screen: browsers allow a limited number of WebGL contexts, and a long list of
// animations is pointless. Each time it comes into view it gets a fresh canvas.
function mountBanner(card, banner) {
  const note = h('p', { class: 'muted banner-note' });
  const pause = h('button', { type: 'button', class: 'quiet', 'aria-pressed': String(reducedMotion) }, reducedMotion ? 'Play' : 'Pause');
  const stage = h('div', { class: 'stage' });
  const wrap = h('figure', { class: 'banner' }, stage, h('figcaption', {}, note, banner.animated ? pause : null, mountExport(card)));
  card.body.append(wrap);
  const state = { stage, note, pause, paused: reducedMotion, observer: null, canvas: null, mode: null, gen: 0 };
  card.banner = state;
  pause.addEventListener('click', () => {
    state.paused = !state.paused;
    pause.textContent = state.paused ? 'Play' : 'Pause';
    pause.setAttribute('aria-pressed', String(state.paused));
    if (state.mode === 'gl') to(card, { op: 'banner-pause', id: card.id, paused: state.paused });
  });
  state.observer = new IntersectionObserver(([entry]) => (entry.isIntersecting ? startBanner(card) : stopBanner(card)), { rootMargin: '100px' });
  state.observer.observe(stage);
}

// ---------------------------------------------------------------- export

let mp4Support; // one answer for the page: { ok, reason? }
const mp4 = () => (mp4Support ??= probeMp4());

// "Export as" in a banner's caption: a button that opens a short list of formats (real buttons, Escape closes it and gives the
// focus back). The worker draws and encodes; the file is saved when it arrives. Only one export per card runs at a time.
function mountExport(card) {
  const panelId = `export-formats-${card.id}`;
  const open = h('button', { type: 'button', class: 'quiet export-open', 'aria-haspopup': 'true', 'aria-expanded': 'false', 'aria-controls': panelId }, 'Export as…');
  const status = h('p', { class: 'muted export-status', role: 'status', 'aria-live': 'polite' });
  const cancel = h('button', { type: 'button', class: 'quiet export-cancel', hidden: true }, 'Cancel');
  const double = h('input', { type: 'checkbox', id: `export-double-${card.id}` });
  const buttons = new Map(FORMATS.map((f) => [f.id, h('button', { type: 'button', 'data-format': f.id }, f.label)]));
  buttons.get('mp4').disabled = true; // until the browser has said whether it can make video
  const reason = h('span', { class: 'muted', id: `export-reason-${card.id}`, hidden: true });
  const panel = h('div', { id: panelId, class: 'export-formats', role: 'group', 'aria-label': 'Export format', hidden: true },
    ...buttons.values(), h('label', { class: 'export-double' }, double, ' Double size (800×480)'), reason);
  const state = { running: false, panel, open, status, cancel, buttons, reason, double, file: null };
  card.export = state;
  const close = (focus) => {
    panel.hidden = true;
    open.setAttribute('aria-expanded', 'false');
    if (focus) open.focus();
  };
  open.addEventListener('click', () => {
    const show = panel.hidden;
    panel.hidden = !show;
    open.setAttribute('aria-expanded', String(show));
    if (show) buttons.get('gif').focus();
  });
  panel.addEventListener('keydown', (e) => { if (e.key === 'Escape') close(true); });
  mp4().then((support) => {
    const b = buttons.get('mp4');
    b.disabled = !support.ok;
    reason.hidden = support.ok;
    reason.textContent = support.ok ? '' : `MP4 isn’t available. ${support.reason}`;
    if (!support.ok) b.setAttribute('aria-describedby', reason.id);
  });
  for (const f of FORMATS) {
    buttons.get(f.id).addEventListener('click', () => {
      close(false);
      startExport(card, f);
    });
  }
  cancel.addEventListener('click', () => to(card, { op: 'export-cancel', id: card.id }));
  return h('div', { class: 'export' }, open, panel, cancel, status);
}

function startExport(card, format) {
  const s = card.export;
  if (s.running) return;
  s.running = true;
  s.format = format;
  s.open.disabled = true;
  s.cancel.hidden = false;
  s.status.textContent = progressText(format.label.split(' ')[0], 0, 0);
  s.cancel.focus();
  to(card, { op: 'export', id: card.id, format: format.id, scale: s.double.checked ? 2 : 1 });
}

function exportEvent(card, data) {
  const s = card.export;
  if (!s) return;
  const finish = (text) => {
    s.running = false;
    s.open.disabled = false;
    s.cancel.hidden = true;
    s.status.textContent = text;
    s.open.focus();
  };
  switch (data.ev) {
    case 'export-progress':
      if (s.running) s.status.textContent = progressText(s.format.label.split(' ')[0], data.done, data.total);
      break;
    case 'export-done': {
      const name = exportFileName(card.title.textContent, s.format.ext);
      const url = URL.createObjectURL(new Blob([data.bytes], { type: data.mime }));
      const a = h('a', { href: url, download: name });
      document.body.append(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 60000);
      card.el.dataset.exported = s.format.id;
      finish([`Saved ${name} (${formatBytes(data.bytes.byteLength)}).`, data.note].filter(Boolean).join(' '));
      break;
    }
    case 'export-cancelled':
      finish('Export cancelled.');
      break;
    case 'export-error':
      finish('Couldn’t make the file.');
      break;
  }
}

function startBanner(card) {
  const b = card.banner;
  if (!b || b.canvas) return;
  b.gen++;
  const canvas = h('canvas', { width: W, height: H, role: 'img', 'aria-label': `3DS banner of ${card.title.textContent}` });
  b.canvas = canvas;
  b.stage.replaceChildren(canvas);
  if (!forceSoft && 'transferControlToOffscreen' in canvas) {
    const offscreen = canvas.transferControlToOffscreen();
    b.mode = 'gl';
    to(card, { op: 'banner-gl', id: card.id, canvas: offscreen, paused: b.paused }, [offscreen]);
  } else {
    startSoft(card);
  }
}

function bannerStarted(card, data) {
  const b = card.banner;
  if (!b?.canvas) return;
  if (data.renderer === 'webgl2') {
    b.note.textContent = '';
    card.el.dataset.renderer = 'webgl2';
    return;
  }
  // WebGL 2 is not available (or failed): a canvas that was handed to the worker cannot be reused, so start over with a new one.
  b.canvas = null;
  b.mode = null;
  const canvas = h('canvas', { width: W, height: H, role: 'img', 'aria-label': `3DS banner of ${card.title.textContent}` });
  b.canvas = canvas;
  b.stage.replaceChildren(canvas);
  startSoft(card);
}

function startSoft(card) {
  const b = card.banner;
  b.mode = 'soft';
  b.note.textContent = '';
  card.el.dataset.renderer = 'soft';
  const ctx = b.canvas.getContext('2d');
  const t0 = performance.now();
  const gen = b.gen;
  b.ctx = ctx;
  b.inflight = false;
  b.held = 0;
  const tick = (now) => {
    if (b.gen !== gen || b.mode !== 'soft' || !b.canvas) return;
    if (!b.inflight && (!b.paused || b.held === 0 || b.dirty)) {
      b.inflight = true;
      b.dirty = false;
      b.held = b.paused ? b.held : ((now - t0) / 1000) * 60;
      to(card, { op: 'frame', id: card.id, frame: b.held || 0.0001 });
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

function softPaint(card, pixels) {
  const b = card.banner;
  if (!b) return;
  b.inflight = false;
  if (pixels && b.mode === 'soft' && b.ctx) b.ctx.putImageData(new ImageData(new Uint8ClampedArray(pixels), W, H), 0, 0);
}

function stopBanner(card) {
  const b = card.banner;
  if (!b?.canvas) return;
  if (b.mode === 'gl') to(card, { op: 'banner-stop', id: card.id });
  b.gen++;
  b.mode = null;
  b.canvas = null;
  b.stage.replaceChildren();
}

// ---------------------------------------------------------------- picking, dropping, the demo

// Names a unit for the worker; the Files travel as-is (a File is a handle, not a copy of the bytes).
const messageFor = (unit) => unit.kind === 'title'
  ? { kind: 'title', name: unit.name, entries: unit.entries.map(({ file, path }) => ({ file, path })) }
  : { kind: 'file', name: unit.name, file: unit.file, siblings: unit.siblings.map(({ file, path }) => ({ file, path })), inFolder: unit.inFolder, zipEntry: unit.zipEntry };

function enqueue(card, message) {
  batch.total++;
  queue.push({ card, message });
}

const UNREAD = /\.(7z|rar)$/i;

function inspectUnits(entries) {
  const units = groupEntries(entries);
  status.textContent = units.length ? '' : 'No files found.';
  if (batch.done >= batch.total) Object.assign(batch, { total: 0, done: 0, skipped: [] }); // a new batch, unless one is still running
  for (const unit of units) {
    const card = newCard(unit);
    unit.file = unit.entry?.file;
    if (UNREAD.test(unit.name)) {
      // Not read: no 7z or RAR reader here (zip is read in place), and the card says so rather than saying nothing.
      batch.total++;
      batch.done++;
      showError(card, 'Couldn’t read this archive. Extract the game first, or use a zip.');
      continue;
    }
    enqueue(card, messageFor(unit));
  }
  pump();
}

const fromInput = (input) => [...input.files].map((file) => ({ file, path: file.webkitRelativePath || file.name }));

$('#pick-files').addEventListener('change', (e) => { inspectUnits(fromInput(e.target)); e.target.value = ''; });
$('#pick-folder').addEventListener('change', (e) => { inspectUnits(fromInput(e.target)); e.target.value = ''; });
$('#clear').addEventListener('click', () => { for (const card of [...cards.values()]) removeCard(card); status.textContent = ''; });

// Dropped folders arrive as FileSystemEntry trees, which must be asked for before the first await.
async function walk(entry, prefix, out) {
  if (entry.isFile) {
    const file = await new Promise((resolve, reject) => entry.file(resolve, reject));
    out.push({ file, path: prefix + entry.name });
  } else if (entry.isDirectory) {
    const reader = entry.createReader();
    for (;;) {
      const batch = await new Promise((resolve, reject) => reader.readEntries(resolve, reject));
      if (!batch.length) break;
      for (const child of batch) await walk(child, `${prefix}${entry.name}/`, out);
    }
  }
}

const drop = $('#drop');
for (const type of ['dragenter', 'dragover']) drop.addEventListener(type, (e) => { e.preventDefault(); drop.dataset.over = 'true'; });
for (const type of ['dragleave', 'drop']) drop.addEventListener(type, () => { delete drop.dataset.over; });
drop.addEventListener('drop', async (e) => {
  e.preventDefault();
  const entries = [...e.dataTransfer.items].filter((i) => i.kind === 'file').map((i) => i.webkitGetAsEntry?.() ?? i.getAsFile());
  const out = [];
  for (const entry of entries) {
    if (entry instanceof File) out.push({ file: entry, path: entry.name });
    else if (entry) await walk(entry, '', out);
  }
  inspectUnits(out);
});

// The demo's files are fetched while the page loads (a few KB from this site), so pressing the button makes no request.
const demoButton = $('#demo');
fetchDemo().then((entries) => {
  demo = entries;
  demoButton.disabled = false;
  window.__demoReady = true;
}).catch(() => { demoButton.title = 'The demo isn’t available.'; });
demoButton.addEventListener('click', () => {
  if (!demo) return;
  inspectUnits(demo.map(({ file, path }) => ({ file: new File([file], basename(path)), path })));
});

// The demo is whatever demo/index.json lists (scripts/site-build writes it from the files in site/demo/ and the fixtures): add
// files to that folder to add demos. Paths may be nested, so a demo can be a folder.
async function fetchDemo() {
  const { files } = await (await fetch('demo/index.json')).json();
  return Promise.all(files.map(async (path) => ({ file: await (await fetch(`demo/${path}`)).blob(), path })));
}
