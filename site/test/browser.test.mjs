// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The built site (site/dist, scripts/site-build) in a headless Chromium: the demo draws, picked fixtures show the right title
// and ids, folders work, a CPU fallback draws, unsupported files get a clear error, and nothing touches the network after load.
// Chromium: $GAMEMETA_CHROME, else the newest headless shell or Chrome Playwright installed in ~/.cache/ms-playwright.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { crc32, deflateRawSync } from 'node:zlib';
import { closeSync, copyFileSync, mkdirSync, mkdtempSync, openSync, readdirSync, rmdirSync, truncateSync, unlinkSync, writeFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { homedir, tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

const axeSource = await readFile(createRequire(import.meta.url).resolve('axe-core/axe.min.js'), 'utf8'); // vendored in node_modules: no network at test time
const root = fileURLToPath(new URL('../..', import.meta.url));
const dist = join(root, 'site/dist');

// A zip of `entries` ({ name, data, deflate }), written by hand: stored or deflated, each with its CRC-32.
function makeZip(entries) {
  const locals = [];
  const central = [];
  let offset = 0;
  for (const { name, data, deflate } of entries) {
    const body = deflate ? deflateRawSync(data) : data;
    const nameBytes = Buffer.from(name);
    const common = Buffer.alloc(26);
    common.writeUInt16LE(20, 0); // version
    common.writeUInt16LE(deflate ? 8 : 0, 4); // method
    common.writeUInt32LE(crc32(data), 10);
    common.writeUInt32LE(body.length, 14);
    common.writeUInt32LE(data.length, 18);
    common.writeUInt16LE(nameBytes.length, 22);
    const local = Buffer.concat([Buffer.from([0x50, 0x4b, 3, 4]), common, nameBytes, body]);
    const dir = Buffer.alloc(46);
    dir.writeUInt32LE(0x02014b50, 0);
    dir.writeUInt16LE(20, 4);
    common.copy(dir, 6, 0, 26);
    dir.writeUInt32LE(offset, 42);
    central.push(Buffer.concat([dir, nameBytes]));
    locals.push(local);
    offset += local.length;
  }
  const cd = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(cd.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, cd, end]);
}
const fixture = (name) => join(root, 'fixtures', name);
const MIME = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' };

function findChrome() {
  if (process.env.GAMEMETA_CHROME) return process.env.GAMEMETA_CHROME;
  const cache = join(homedir(), '.cache/ms-playwright');
  const found = [];
  let dirs = [];
  try { dirs = readdirSync(cache); } catch {}
  for (const dir of dirs.sort().reverse()) {
    for (const exe of [`${dir}/chrome-headless-shell-linux64/chrome-headless-shell`, `${dir}/chrome-linux64/chrome`]) {
      found.push(join(cache, exe));
    }
  }
  const exe = found.find((p) => { try { return readdirSync(join(p, '..')).length > 0; } catch { return false; } });
  if (!exe) throw new Error('no Chromium: run `npx playwright-core install chromium-headless-shell` in site/, or set GAMEMETA_CHROME');
  return exe;
}

// Scratch files the tests make (named one by one, so nothing is removed by pattern), deleted at the end.
const scratch = { files: [], dirs: [] };
function scratchDir(...subdirs) {
  const dir = mkdtempSync(join(tmpdir(), 'gamemeta-site-'));
  scratch.dirs.push(dir);
  for (const sub of subdirs) {
    mkdirSync(join(dir, sub), { recursive: true });
    scratch.dirs.push(join(dir, sub));
  }
  return dir;
}
function scratchFile(path, content) {
  if (content !== undefined) writeFileSync(path, content);
  scratch.files.push(path);
  return path;
}

let server;
let browser;
let base;
const served = [];

test.before(async () => {
  server = createServer(async (req, res) => {
    const path = normalize(new URL(req.url, 'http://x').pathname).replace(/^(\.\.[/\\])+/, '');
    served.push(path);
    try {
      const file = path.endsWith('/') ? `${path}index.html` : path;
      const body = await readFile(join(dist, file));
      res.writeHead(200, { 'content-type': MIME[extname(file)] ?? 'application/octet-stream' });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch({
    executablePath: findChrome(),
    args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--no-sandbox'],
  });
});
test.after(async () => {
  for (const f of scratch.files) try { unlinkSync(f); } catch {}
  for (const d of scratch.dirs.reverse()) try { rmdirSync(d); } catch {}
  await browser?.close();
  server?.close();
});

async function open(query = '') {
  const context = await browser.newContext({ viewport: { width: 900, height: 900 } });
  const page = await context.newPage();
  const requests = [];
  context.on('request', (r) => requests.push(r.url()));
  const problems = [];
  page.on('pageerror', (e) => problems.push(String(e)));
  page.on('console', (m) => m.type() === 'error' && problems.push(m.text()));
  await page.goto(`${base}${query}`);
  await page.waitForFunction(() => document.documentElement.dataset.ready === 'true' && window.__demoReady === true);
  return { page, context, requests, problems, afterLoad: requests.length };
}

// How many pixels of a banner canvas are not transparent, and how many distinct colours: a drawn diorama has both.
const ink = (page, selector) =>
  page.evaluate((sel) => {
    const c = document.querySelector(sel);
    const t = document.createElement('canvas');
    t.width = c.width;
    t.height = c.height;
    const ctx = t.getContext('2d');
    ctx.drawImage(c, 0, 0);
    const d = ctx.getImageData(0, 0, t.width, t.height).data;
    const colours = new Set();
    let opaque = 0;
    for (let i = 0; i < d.length; i += 4) {
      if (d[i + 3] > 0) opaque++;
      colours.add((d[i] << 16) | (d[i + 1] << 8) | d[i + 2]);
    }
    return { opaque, colours: colours.size };
  }, selector);

const cardTexts = (page) => page.$$eval('#results .card', (cards) => cards.map((c) => c.textContent));

test('the demo shows the sample file and an animated banner (WebGL 2 in the worker)', async () => {
  const { page, requests, afterLoad, problems } = await open();
  await page.click('#demo');
  await page.waitForSelector('.card[data-title="Sample Game Subtitle"]');
  await page.waitForSelector('.card[data-renderer="webgl2"] canvas:not([width="0"])');
  await page.waitForTimeout(500);
  const first = await ink(page, '.banner canvas');
  assert.ok(first.opaque > 5000 && first.colours > 8, `drew a diorama: ${JSON.stringify(first)}`);
  const shot = (await page.locator('.banner canvas').screenshot()).toString('base64');
  await page.waitForTimeout(700);
  assert.notEqual((await page.locator('.banner canvas').screenshot()).toString('base64'), shot, 'the banner animates');
  const texts = await cardTexts(page);
  assert.ok(texts.some((t) => t.includes('Sample Game Subtitle') && t.includes('0004000000123400') && t.includes('CTR-P-SMPL')));
  assert.equal(requests.length, afterLoad, 'the demo makes no request');
  assert.deepEqual(problems, []);
});

test('the same banner on the CPU renderer when WebGL 2 is not used', async () => {
  const { page, problems } = await open('?renderer=soft');
  await page.click('#demo');
  await page.waitForSelector('.card[data-renderer="soft"] canvas');
  await page.waitForTimeout(800);
  const px = await ink(page, '.banner canvas');
  assert.ok(px.opaque > 5000 && px.colours > 8, `drew a diorama: ${JSON.stringify(px)}`);
  assert.deepEqual(problems, []);
});

test('picked fixtures show their own titles, ids and hashes, with no network request', async () => {
  const { page, requests, afterLoad, problems } = await open();
  await page.setInputFiles('#pick-files', [
    fixture('sample.3ds'), fixture('sample.nds'), fixture('sample-gc.rvz'), fixture('sample.wua'),
    fixture('ids/sample.nes'), fixture('ids/sample-ps2.iso'), fixture('ids/sample.chd'), fixture('ids/sample-rom.zip'),
    fixture('carts/sample.gba'), fixture('carts/sample.md'),
  ]);
  await page.waitForFunction(() => document.querySelectorAll('#results .card[data-state]').length === 10 && ![...document.querySelectorAll('#results .card')].some((c) => c.textContent.includes('Reading')));
  await page.waitForFunction(() => [...document.querySelectorAll('[data-value]')].every((e) => e.dataset.value));
  const all = (await cardTexts(page)).join('\n---\n');
  for (const want of [
    'Sample Game Subtitle', '0004000000123400', 'CTR-P-SMPL', 'Example Pub', // 3DS
    'SMPL', // NDS game code
    'Sample Quest', 'Quete Exemple', // GameCube RVZ, French title
    'Sample Title', // Wii U archive
    '8830563f3383210f4af9fadbb7f60fe3', // RetroAchievements MD5 of the NES file
    'SLUS00003', // PS2 serial
    '0123456789abcdef0123456789abcdef01234567', // CHD SHA-1
    '06beafd4', // zip CRC
    'SYNTHGAME', 'AGB-ASYE', // Game Boy Advance header
    'SYNTH DRIVE GAME', 'GM 00001009-02', 'Game Boy Advance', 'Sega Genesis / Mega Drive', 'Header fields', // Genesis header and the platform names
  ]) assert.ok(all.includes(want), `missing ${want} in:\n${all}`);
  assert.equal(requests.length, afterLoad, `no request after load: ${requests.slice(afterLoad)}`);
  assert.deepEqual(problems, []);
});

test('a title folder is read as a Wii U title; other files in a folder pick are ignored', async () => {
  const dir = scratchDir('Game', 'Game/meta', 'Game/code');
  const tag = (name, v) => `  <${name} type="string" length="256">${v}</${name}>\n`;
  scratchFile(join(dir, 'Game/meta/meta.xml'),
    `<?xml version="1.0" encoding="utf-8"?>\n<menu>\n  <title_id type="hexBinary" length="8">0005000010123456</title_id>\n${tag('longname_en', 'Folder Quest')}${tag('publisher_en', 'Folder Games')}</menu>\n`);
  scratchFile(join(dir, 'Game/code/app.rpx'), 'not code');
  scratchFile(join(dir, 'Game/notes.txt'), 'hello');
  const { page, requests, afterLoad } = await open();
  await page.setInputFiles('#pick-folder', join(dir, 'Game'));
  await page.waitForSelector('.card[data-title="Folder Quest"]');
  const texts = await cardTexts(page);
  assert.equal(texts.length, 1, 'one card for the whole folder');
  assert.ok(texts[0].includes('0005000010123456') && texts[0].includes('Folder Games'));
  assert.equal(requests.length, afterLoad);
});

test('a cue sheet finds its track among the picked files', async () => {
  const { page } = await open();
  await page.setInputFiles('#pick-files', [fixture('ids/sample.cue'), fixture('ids/sample.bin')]);
  await page.waitForFunction(() => [...document.querySelectorAll('#results .card')].some((c) => c.textContent.includes('SLUS00003') && c.getAttribute('aria-label') === 'sample.cue'));
});

test('big files are read in place: a 3 GiB file gives its title at once, a 200 MiB hash streams with progress', async () => {
  const dir = scratchDir();
  scratchFile(join(dir, 'big.3ds'));
  copyFileSync(fixture('sample.3ds'), join(dir, 'big.3ds'));
  truncateSync(join(dir, 'big.3ds'), 3 * 1024 ** 3); // sparse: no disk, and no browser could hold it in memory
  const rom = scratchFile(join(dir, 'big.gba'));
  closeSync(openSync(rom, 'w'));
  truncateSync(rom, 200 * 1024 ** 2);
  const { page } = await open();
  const started = Date.now();
  await page.setInputFiles('#pick-files', join(dir, 'big.3ds'));
  await page.waitForSelector('.card[data-title="Sample Game Subtitle"]');
  assert.ok(Date.now() - started < 10000, 'read without loading the file');
  await page.setInputFiles('#pick-files', rom);
  const compute = page.locator('.card[aria-label="big.gba"] button', { hasText: 'Calculate' });
  await compute.waitFor();
  await compute.click();
  await page.waitForFunction(() => document.querySelector('.card[aria-label="big.gba"] progress')?.value > 0, null, { timeout: 60000 });
  const md5 = createHash('md5');
  const zeros = Buffer.alloc(1024 ** 2);
  for (let i = 0; i < 200; i++) md5.update(zeros);
  await page.waitForSelector(`[data-value="${md5.digest('hex')}"]`, { timeout: 120000 });
});

test('a file nothing reads gets a clear error, and Clear removes the cards', async () => {
  const { page } = await open();
  await page.setInputFiles('#pick-files', { name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') });
  await page.waitForSelector('.card[data-state="error"]');
  assert.match((await cardTexts(page))[0], /notes\.txt[\s\S]*isn’t supported/);
  await page.click('#clear');
  assert.equal((await cardTexts(page)).length, 0);
  assert.equal(await page.locator('#clear').isVisible(), false, 'Clear hides again once nothing is left');
});

test('the page loads only its own files and names no outside host', async () => {
  const { requests } = await open();
  for (const url of requests) assert.ok(url.startsWith(base), `outside request: ${url}`);
  const paths = new Set(requests.map((u) => new URL(u).pathname));
  for (const p of ['/', '/app.js', '/worker.js', '/pkg/gamemeta_wasm_bg.wasm']) assert.ok(paths.has(p), `${p} was loaded`);
});

// At any width the title, GitHub link, drop zone, pickers and the privacy line are on the first screen. The empty drop zone is a
// compact box, centred and as wide as the header, with its prompt centred; Clear is hidden until there is something to clear.
// No README one-liners.
test('the drop zone is compact and centred at phone and desktop widths, in both colour schemes', async () => {
  for (const [width, height] of [[375, 667], [1280, 800]]) {
    for (const colorScheme of ['light', 'dark']) {
      const context = await browser.newContext({ viewport: { width, height }, colorScheme, isMobile: width < 500 });
      const page = await context.newPage();
      await page.goto(base);
      await page.waitForFunction(() => document.documentElement.dataset.ready === 'true');
      const where = `${width}px ${colorScheme}`;
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
      assert.ok(overflow <= 0, `${where}: page scrolls sideways by ${overflow}px`);
      for (const sel of ['h1', 'a[aria-label="Source on GitHub"]', '#drop', '#pick-files', '#pick-folder', '#demo', '.privacy']) {
        const box = await page.locator(sel).boundingBox();
        assert.ok(box && box.y + box.height <= height + 1, `${where}: ${sel} is on the first screen (${JSON.stringify(box)})`);
      }
      const drop = await page.locator('#drop').boundingBox();
      assert.ok(drop.height < 360, `${where}: the empty drop zone is compact (${drop.height}px)`);
      const header = await page.locator('header').boundingBox();
      assert.ok(Math.abs(drop.x - header.x) <= 1 && Math.abs(drop.width - header.width) <= 1, `${where}: the drop zone is as wide as the header`);
      const prompt = await page.locator('#drop-h').evaluate((e) => getComputedStyle(e).textAlign);
      assert.equal(prompt, 'center', `${where}: the prompt is centred`);
      assert.equal(await page.locator('#clear').isVisible(), false, `${where}: Clear is hidden with nothing loaded`);
      assert.equal(await page.locator('.privacy strong').count(), 0, `${where}: the privacy line is quiet text`);
      assert.equal(await page.locator('table, [data-readme]').count(), 0, 'nothing of the README is repeated');
      const link = page.locator('a[aria-label="Source on GitHub"]');
      assert.ok((await link.getAttribute('href')).startsWith('http'), 'the repo link has an address');
      assert.equal(await link.locator('svg').count(), 1, 'the GitHub mark is inline');
      await context.close();
    }
  }
});

test('the demo loads every file demo/index.json lists; the credit line is in the footer before and after', async () => {
  const { page, requests, afterLoad, problems } = await open();
  const index = await page.evaluate(() => fetch('demo/index.json').then((r) => r.json()));
  assert.ok(index.files.includes('sample.3ds') && index.credits.length > 0, JSON.stringify(index));
  assert.ok((await page.textContent('footer')).includes(index.credits[1]), 'the credit line shows before a demo is loaded');
  requests.length = afterLoad; // the fetch just made is the test's own
  await page.click('#demo');
  await page.waitForSelector('.card[data-title="Sample Game Subtitle"]');
  assert.ok((await page.textContent('footer')).includes('MPL-2.0'));
  assert.ok((await page.textContent('footer')).includes(index.credits[1]), 'and after');
  assert.equal(requests.length, afterLoad);
  assert.deepEqual(problems, []);
});

test('the footer always shows the SuperTuxKart credit (author, license, source), the trademark notice and the notice links', async () => {
  const { page, problems } = await open();
  const footer = await page.textContent('footer');
  for (const want of ['SuperTuxKart', 'XGhost', 'CC-BY-SA 3.0', 'https://supertuxkart.net', 'MPL-2.0',
    'not affiliated with, authorized, sponsored or endorsed by Nintendo, Sony, Microsoft, Sega', 'Nintendo 3DS HOME Menu', 'Sega Genesis',
    'trademarks of their respective owners']) {
    assert.ok(footer.replace(/\s+/g, ' ').includes(want), want);
  }
  for (const href of ['NOTICE.md', 'CREDITS.md']) {
    assert.equal(await page.locator(`footer a[href="${href}"]`).count(), 1, href);
    const status = await page.evaluate((h) => fetch(h).then((r) => r.status), href);
    assert.equal(status, 200, `${href} is served`);
  }
  assert.deepEqual(problems, []);
});

test('the Tux demo files are in the demo: a platform each, a Wii U folder, and the credit line', async () => {
  const { page, requests, afterLoad, problems } = await open();
  const index = await page.evaluate(() => fetch('demo/index.json').then((r) => r.json()));
  for (const f of ['tux.nds', 'tux.dsi', 'tux.3ds', 'tux-gc.iso', 'tux-wii.rvz', 'tux-wiiu/code/tux.rpx', 'tux.gba', 'tux.md']) {
    assert.ok(index.files.includes(f), `${f} is listed`);
  }
  assert.ok(!index.files.includes('README.md') && !index.files.includes('CREDITS.md'), 'the docs are not demo files');
  requests.length = afterLoad;
  await page.click('#demo');
  await page.waitForSelector('.card[data-title="SuperTuxKart Kart racing"]');
  await page.waitForFunction(() => ![...document.querySelectorAll('#results .card')].some((c) => c.textContent.includes('Reading')));
  await page.waitForFunction(() => [...document.querySelectorAll('[data-value]')].every((e) => e.dataset.value));
  const texts = (await cardTexts(page)).join('\n---\n');
  for (const want of ['SuperTuxKart', '000400000FF3FF00', 'CTR-P-TUXK', 'SUPERTUXKART', 'SLUS99901']) assert.ok(texts.includes(want), want);
  assert.ok((await page.textContent('#credits')).includes('CC-BY-SA 3.0'));
  assert.equal(requests.length, afterLoad, 'the demo makes no request');
  assert.deepEqual(problems, []);
});

// ~20 synthetic files: DS ROMs and zips of DS ROMs (stored, deflated, two ROMs in one), a corrupt ROM, a corrupt zip, a text file.
// Each ROM is the sample with its own 4-character game code, so every card can be told apart. Like a real ROM, the banner
// sits well inside it, behind the program code (zeros here); `mib` sizes the ROMs.
async function romSet(mib) {
  const sample = await readFile(fixture('sample.nds'));
  const at = sample.readUInt32LE(0x68);
  const rom = (code) => {
    const b = Buffer.alloc(mib << 20);
    sample.copy(b, 0, 0, at);
    sample.copy(b, (mib - 1) << 20, at, at + 0x23c0);
    b.writeUInt32LE((mib - 1) << 20, 0x68);
    b.write(code, 0x0c, 'latin1');
    return b;
  };
  const files = [];
  for (let i = 1; i <= 5; i++) files.push({ name: `plain${i}.nds`, data: rom(`RM0${i}`), codes: [`RM0${i}`] });
  for (let i = 1; i <= 4; i++) {
    files.push({ name: `stored${i}.zip`, data: makeZip([{ name: `game${i}.nds`, data: rom(`ZS0${i}`) }]), codes: [`ZS0${i}`] });
    files.push({ name: `deflated${i}.zip`, data: makeZip([{ name: `game${i}.nds`, data: rom(`ZD0${i}`), deflate: true }]), codes: [`ZD0${i}`] });
  }
  const twin = [{ name: 'a.nds', data: rom('TW01'), deflate: true }, { name: 'notes.txt', data: Buffer.from('hi') }, { name: 'b.nds', data: rom('TW02') }];
  files.push({ name: 'twin.zip', data: makeZip(twin), codes: ['TW01', 'TW02'], crcs: twin.filter((e) => e.name.endsWith('.nds')).map((e) => crc32(e.data).toString(16).padStart(8, '0')) });
  files.push({ name: 'broken.nds', data: Buffer.alloc(3000, 7), broken: true });
  files.push({ name: 'broken.zip', data: Buffer.from('PK\u0003\u0004 not really a zip at all'.padEnd(500, 'x')), broken: true });
  files.push({ name: 'readme.txt', data: Buffer.from('hello'), ignored: true });
  return files;
}

// 5 plain + 8 single-ROM zips + twin.zip (two ROMs) + 2 broken files + readme.txt = 17 units, and one more card for twin's second ROM.
const UNITS = 18;

async function expectAllCards(page, files, { loose }) {
  await page.waitForFunction((n) => document.querySelector('#progress').textContent.startsWith(`Read ${n} files`), UNITS, { timeout: 60000 });
  await page.waitForFunction(() => [...document.querySelectorAll('#results .card')].every((c) => c.dataset.state), null, { timeout: 60000 });
  const texts = await cardTexts(page);
  const all = texts.join('\n---\n');
  const cards = await page.$$eval('#results .card', (list) => list.map((c) => ({ text: c.textContent, state: c.dataset.state })));
  for (const f of files) {
    for (const code of f.codes ?? []) assert.equal(cards.filter((c) => c.state === 'ok' && c.text.includes(code)).length, 1, `one card for ${f.name} (${code})`);
    for (const crc of f.crcs ?? []) assert.ok(all.includes(crc), `${f.name}: the ROM's CRC-32 ${crc} is on its card`);
    if (f.broken) assert.equal(cards.filter((c) => c.state === 'error' && c.text.includes(f.name)).length, 1, `${f.name} has its own error card`);
  }
  assert.ok(all.includes('Sample Game Subtitle'), 'the inner games\' titles are shown');
  assert.ok(all.includes('twin.zip › a.nds') && all.includes('twin.zip › b.nds'), 'each ROM of a zip is named');
  if (loose) assert.ok(cards.some((c) => c.state === 'error' && c.text.includes('readme.txt')), 'a dropped text file says it is not read');
  else {
    assert.equal(cards.length, UNITS - 1, 'a folder pick gives no card to a file that is not a game');
    assert.ok((await page.textContent('#skipped')).includes('readme.txt'), 'it is listed as skipped');
  }
}

test('a picked folder of ROMs and zipped ROMs gets a card for each, errors included, with no network request', async () => {
  const files = await romSet(4);
  const dir = scratchDir('Roms');
  for (const f of files) scratchFile(join(dir, 'Roms', f.name), f.data);
  const { page, requests, afterLoad, problems } = await open();
  await page.setInputFiles('#pick-folder', join(dir, 'Roms'));
  await expectAllCards(page, files, { loose: false });
  assert.equal(requests.length, afterLoad);
  assert.deepEqual(problems, []);
});

test('the same files dropped on the page', async () => {
  const files = await romSet(2);
  const { page, requests, afterLoad, problems } = await open();
  await page.evaluate((list) => {
    const dt = new DataTransfer();
    for (const f of list) dt.items.add(new File([Uint8Array.from(atob(f.b64), (c) => c.charCodeAt(0))], f.name));
    document.querySelector('#drop').dispatchEvent(new DragEvent('drop', { dataTransfer: dt, bubbles: true, cancelable: true }));
  }, files.map((f) => ({ name: f.name, b64: f.data.toString('base64') })));
  await expectAllCards(page, files, { loose: true });
  assert.equal(requests.length, afterLoad);
  assert.deepEqual(problems, []);
});

test('a 7z is named as not read, and its neighbours still load', async () => {
  const dir = scratchDir('Mixed');
  const sample = scratchFile(join(dir, 'Mixed', 'game.nds'), await readFile(fixture('sample.nds')));
  const seven = scratchFile(join(dir, 'Mixed', 'game.7z'), Buffer.from('7z\u00bc\u00af\u0027\u001c not read'));
  const { page } = await open();
  await page.setInputFiles('#pick-files', [sample, seven]);
  await page.waitForSelector('.card[data-title="Sample Game Subtitle"]');
  await page.waitForSelector('.card[data-state="error"]');
  const all = (await cardTexts(page)).join('\n');
  assert.ok(/game\.7z[\s\S]*Couldn’t read this archive/.test(all), all);
});

// ---------------------------------------------------------------- accessibility

// axe-core (WCAG 2.0 to 2.2 A and AA, plus its best-practice rules) over the page as it stands: every <details> open so its text is
// checked too. Evaluated through the DevTools protocol, so the page's CSP stays as it ships.
async function axeViolations(page) {
  await page.evaluate(() => document.querySelectorAll('details').forEach((d) => (d.open = true)));
  await page.evaluate(axeSource);
  const result = await page.evaluate(() =>
    axe.run(document, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'] } }),
  );
  const ran = result.passes.some((p) => p.id === 'color-contrast') && result.passes.some((p) => p.id === 'heading-order' || p.id === 'label');
  if (!ran) return ['axe did not run its contrast and label rules'];
  return result.violations.map((v) => `${v.id} (${v.impact}): ${v.help}\n${v.nodes.map((n) => `  ${n.target.join(' ')}: ${n.failureSummary}`).join('\n')}`);
}

for (const colorScheme of ['light', 'dark']) {
  test(`axe finds no violation: the empty page, the demo cards and an error card (${colorScheme})`, async () => {
    const context = await browser.newContext({ viewport: { width: 900, height: 900 }, colorScheme });
    const page = await context.newPage();
    await page.goto(base);
    await page.waitForFunction(() => document.documentElement.dataset.ready === 'true' && window.__demoReady === true);
    assert.deepEqual(await axeViolations(page), [], 'the empty page');

    await page.click('#demo');
    await page.waitForSelector('.card[data-title="Sample Game Subtitle"]');
    await page.waitForSelector('.card[data-renderer] canvas');
    await page.waitForFunction(() => document.querySelector('#progress').textContent.startsWith('Read '));
    assert.deepEqual(await axeViolations(page), [], 'with the demo cards');

    await page.click('.banner .export-open');
    assert.equal(await page.getAttribute('.banner .export-open', 'aria-expanded'), 'true');
    assert.deepEqual(await axeViolations(page), [], 'with the Export as list open');
    await page.click('.banner [data-format="gif"]');
    await page.waitForSelector('.banner .export-status:not(:empty)');
    assert.deepEqual(await axeViolations(page), [], 'while a file is being made, and after');
    await page.waitForFunction(() => document.querySelector('.banner .export-status').textContent.startsWith('Saved'));
    assert.deepEqual(await axeViolations(page), [], 'once the file is saved');

    const dir = scratchDir('A11y');
    const text = scratchFile(join(dir, 'A11y', 'notes.txt'), 'not a game');
    const broken = scratchFile(join(dir, 'A11y', 'broken.nds'), Buffer.alloc(3000, 7));
    await page.setInputFiles('#pick-files', [text, broken]);
    await page.waitForFunction(() => document.querySelectorAll('.card[data-state="error"]').length >= 2);
    assert.deepEqual(await axeViolations(page), [], 'with error cards');
    await context.close();
  });
}

test('text at 200% still fits: no sideways scrolling and nothing hidden, at a phone width', async () => {
  const context = await browser.newContext({ viewport: { width: 375, height: 667 }, isMobile: true });
  const page = await context.newPage();
  await page.goto(base);
  await page.waitForFunction(() => document.documentElement.dataset.ready === 'true' && window.__demoReady === true);
  await page.click('#demo');
  await page.waitForSelector('.card[data-title="Sample Game Subtitle"]');
  await page.evaluate(() => {
    document.documentElement.style.fontSize = '200%';
    document.querySelectorAll('details').forEach((d) => (d.open = true));
  });
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  assert.ok(overflow <= 0, `scrolls sideways by ${overflow}px`);
  const clipped = await page.evaluate(() => [...document.querySelectorAll('h1, h3, dt, dd, summary, button, label, p')]
    .filter((e) => e.getBoundingClientRect().width > 0 && e.getBoundingClientRect().right > innerWidth + 1).map((e) => e.textContent.slice(0, 30)));
  assert.deepEqual(clipped, []);
  await context.close();
});

test('headings are in order, the controls are labelled, and reduced motion starts the banner paused', async () => {
  const context = await browser.newContext({ viewport: { width: 900, height: 900 }, reducedMotion: 'reduce' });
  const page = await context.newPage();
  await page.goto(base);
  await page.waitForFunction(() => document.documentElement.dataset.ready === 'true' && window.__demoReady === true);
  assert.equal(await page.getAttribute('html', 'lang'), 'en');
  await page.click('#demo');
  await page.waitForSelector('.card[data-title="Sample Game Subtitle"]');
  const levels = await page.$$eval('h1, h2, h3, h4', (list) => list.map((h) => Number(h.tagName[1])));
  assert.equal(levels[0], 1);
  levels.reduce((prev, level) => { assert.ok(level <= prev + 1, `heading levels skip: ${levels}`); return level; }, 1);
  for (const sel of ['#pick-files', '#pick-folder']) assert.ok(await page.$eval(sel, (i) => i.labels.length === 1 && i.labels[0].textContent.trim().length > 0), `${sel} has a label`);
  for (const sel of ['#status', '#progress']) assert.equal(await page.getAttribute(sel, 'aria-live'), 'polite');
  assert.equal(await page.getAttribute('.banner button', 'aria-pressed'), 'true', 'the banner starts paused');
  await context.close();
});

// The supported-files list is the library's own (`gamemeta formats`), put into the page at build time.
test('the page lists the supported files per platform, as plain text, from the library', async () => {
  const { page } = await open();
  const rows = await page.$$eval('.formats li', (items) => items.map((li) => li.textContent));
  assert.ok(rows.length >= 12, `${rows.length} rows`);
  for (const want of ['Nintendo DS, DSi — .nds .dsi .srl .ids .zip', 'Game Boy Advance — .gba .agb', 'Nintendo 64 — .z64 .v64 .n64', 'Super Nintendo — .sfc', 'NES — .nes', 'Sega Genesis, 32X — .md', 'Master System, Game Gear — .sms .gg']) {
    assert.ok(rows.some((r) => r.startsWith(want)), `${want}\n${rows.join('\n')}`);
  }
  assert.equal(await page.locator('.formats code, .formats table, .formats p').count(), 0, 'no prose or code in the list');
});

// Field names are technical terms and stay as they are.
test('the CHD row label reads CHD SHA-1', async () => {
  const { page } = await open();
  await page.click('#demo');
  await page.waitForSelector('.card[data-title="Sample Game Subtitle"]');
  await page.waitForFunction(() => document.querySelectorAll('#results .card[data-state]').length > 0 && ![...document.querySelectorAll('#results .card')].some((c) => c.textContent.includes('Reading')));
  const labels = await page.$$eval('#results dt', (dts) => dts.map((d) => d.textContent));
  assert.ok(labels.includes('CHD SHA-1'), labels.join(', '));
});

// What the page says is in a visitor's words: nothing about how it works.
test('no visible text names an implementation detail', async () => {
  const banned = /request|wasm|webassembly|webgl|worker|cpu|render|pars(ed|ing)|offset|network|upload|byte|decrypt|blob|thread|canvas|sha-?1\b/i;
  const { page } = await open();
  const dir = scratchDir('Words');
  const text = scratchFile(join(dir, 'Words', 'notes.txt'), 'x');
  const broken = scratchFile(join(dir, 'Words', 'broken.nds'), Buffer.alloc(3000, 7));
  const seven = scratchFile(join(dir, 'Words', 'old.7z'), 'x');
  // Field labels (the dt of a metadata row) may carry these technical names; everything else is prose.
  const fieldNames = /\b(sha-?1|crc-?32|md5|byte order|offset)\b/gi;
  const visible = () => page.evaluate(() => {
    const labels = [...document.querySelectorAll('dt')];
    const text = document.body.innerText;
    return text + '\n' + [...document.querySelectorAll('[title], [aria-label]')].map((e) => e.title + ' ' + e.getAttribute('aria-label')).join('\n') + '\n@@labels@@' + labels.map((l) => l.textContent).join('\n');
  }).then((t) => {
    const [prose, labels = ''] = t.split('\n@@labels@@');
    // Labels are shown once in the page text: drop them from the prose by line, then check them minus the allowed names.
    const labelLines = new Set(labels.split('\n'));
    const kept = prose.split('\n').filter((line) => !labelLines.has(line.trim()));
    return kept.join('\n') + '\n' + labels.replace(fieldNames, '');
  });
  const check = async (when) => {
    const t = await visible();
    assert.equal(t.match(banned)?.[0], undefined, `${when}: "${t.match(banned)?.[0]}" in\n${t}`);
  };
  await check('empty page');
  await page.click('#demo');
  await page.waitForSelector('.card[data-title="Sample Game Subtitle"]');
  await page.waitForSelector('.card[data-renderer] canvas');
  await check('demo cards');
  await page.click('.banner .export-open');
  await check('the Export as list');
  await page.click('.banner [data-format="gif"]');
  await page.waitForFunction(() => document.querySelector('.banner .export-status').textContent.startsWith('Saved'));
  await check('after an export');
  await page.setInputFiles('#pick-files', [text, broken, seven]);
  await page.waitForFunction(() => document.querySelectorAll('.card[data-state="error"]').length >= 3);
  await check('error cards');
});

// ---------------------------------------------------------------- exporting a banner

// A GIF's frames, summed delays (1/100 s), size and repeat count, read block by block.
function gifInfo(b) {
  const info = { width: b.readUInt16LE(6), height: b.readUInt16LE(8), frames: 0, centis: 0, minDelay: Infinity, loop: null, transparent: false };
  let at = 13;
  if (b[10] & 0x80) at += 3 * (2 << (b[10] & 7));
  const skipBlocks = () => { while (b[at] !== 0) at += b[at] + 1; at++; };
  while (at < b.length) {
    const t = b[at++];
    if (t === 0x3b) return info;
    if (t === 0x21) {
      const label = b[at++];
      if (label === 0xf9) {
        const delay = b.readUInt16LE(at + 2);
        info.centis += delay;
        info.minDelay = Math.min(info.minDelay, delay);
        info.transparent ||= (b[at + 1] & 1) === 1;
      }
      if (label === 0xff && b.toString('latin1', at + 1, at + 12) === 'NETSCAPE2.0') info.loop = b.readUInt16LE(at + 14);
      skipBlocks();
    } else if (t === 0x2c) {
      info.frames++;
      const flags = b[at + 8];
      at += 9;
      if (flags & 0x80) at += 3 * (2 << (flags & 7));
      at++;
      skipBlocks();
    } else throw new Error(`bad GIF block ${t} at ${at}`);
  }
  throw new Error('a GIF with no trailer');
}

// An animated WebP's chunks: canvas size, loop count, and each frame's duration (ms).
function webpInfo(b) {
  assert.equal(b.toString('latin1', 0, 4), 'RIFF');
  assert.equal(b.toString('latin1', 8, 12), 'WEBP');
  assert.equal(b.readUInt32LE(4), b.length - 8, 'the RIFF size covers the file');
  const info = { names: [], durations: [], width: 0, height: 0, loop: null, flags: 0 };
  for (let at = 12; at < b.length;) {
    const name = b.toString('latin1', at, at + 4);
    const size = b.readUInt32LE(at + 4);
    info.names.push(name);
    if (name === 'VP8X') {
      info.flags = b[at + 8];
      info.width = b.readUIntLE(at + 12, 3) + 1;
      info.height = b.readUIntLE(at + 15, 3) + 1;
    }
    if (name === 'ANIM') info.loop = b.readUInt16LE(at + 12);
    if (name === 'ANMF') info.durations.push(b.readUIntLE(at + 8 + 12, 3));
    at += 8 + size + (size & 1);
  }
  return info;
}

const sum = (xs) => xs.reduce((a, b) => a + b, 0);

// "Try a demo" loads several 3DS banners (bone-anim.cbmd, tux.3ds, tux.cia): the export tests use bone-anim.cbmd's card.
const DEMO_BANNER = '[aria-label="bone-anim.cbmd"] .banner';
async function exportFrom(page, format, { double = false, at = DEMO_BANNER } = {}) {
  const open = page.locator(`${at} .export-open`);
  await open.click();
  const box = page.locator(`${at} .export-double input`);
  if ((await box.isChecked()) !== double) await box.setChecked(double);
  const [download] = await Promise.all([page.waitForEvent('download'), page.click(`${at} [data-format="${format}"]`)]);
  const bytes = await readFile(await download.path());
  return { download, bytes };
}

test('Export as saves the banner as a GIF and as an animated WebP, from the list on its card', async () => {
  const { page, requests, afterLoad, problems } = await open();
  await page.click('#demo');
  await page.waitForSelector('[aria-label="bone-anim.cbmd"] .banner .export');
  const title = await page.textContent('[aria-label="bone-anim.cbmd"] h3');
  assert.equal(await page.getAttribute('[aria-label="bone-anim.cbmd"] .banner .export-open', 'aria-expanded'), 'false');
  assert.equal(await page.isVisible('[aria-label="bone-anim.cbmd"] .banner .export-formats'), false, 'the list is closed until asked for');

  const gif = await exportFrom(page, 'gif');
  assert.equal(gif.download.suggestedFilename(), `${title}-banner.gif`);
  const g = gifInfo(gif.bytes);
  assert.equal(gif.bytes.toString('latin1', 0, 6), 'GIF89a');
  assert.deepEqual([g.width, g.height], [400, 240]);
  assert.equal(g.loop, 0, 'repeats forever');
  assert.equal(g.centis, 200, 'the 2-second loop at 50 pictures a second, however many were merged');
  assert.ok(g.frames >= 50 && g.minDelay >= 2, `${g.frames} frames, shortest delay ${g.minDelay}/100 s`);
  await page.waitForFunction(() => document.querySelector('[aria-label="bone-anim.cbmd"] .banner .export-status').textContent.startsWith('Saved'));
  assert.match(await page.textContent('[aria-label="bone-anim.cbmd"] .banner .export-status'), /^Saved .*-banner\.gif \(.*(KB|MB)\)\.$/);
  assert.equal(await page.evaluate(() => document.activeElement === document.querySelector('[aria-label="bone-anim.cbmd"] .banner .export-open')), true, 'the focus is back on Export as');

  const webp = await exportFrom(page, 'webp');
  assert.equal(webp.download.suggestedFilename(), `${title}-banner.webp`);
  const w = webpInfo(webp.bytes);
  assert.deepEqual(w.names.slice(0, 2), ['VP8X', 'ANIM']);
  assert.equal(w.flags & 0x02, 0x02, 'the animation flag');
  assert.deepEqual([w.width, w.height, w.loop], [400, 240, 0]);
  assert.equal(sum(w.durations), 2000, 'the exact loop length in milliseconds');
  assert.ok(w.durations.length >= 60);
  // The browser itself draws the file (in a blank tab, so the page's own tab stays as it ships).
  const blank = await page.context().newPage();
  const size = await blank.evaluate(async (b64) => {
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const bitmap = await createImageBitmap(new Blob([bytes], { type: 'image/webp' }));
    return [bitmap.width, bitmap.height];
  }, webp.bytes.toString('base64'));
  assert.deepEqual(size, [400, 240]);
  await blank.close();

  assert.equal(requests.length, afterLoad, 'exporting makes no request');
  assert.deepEqual(problems, []);
});

test('an export does not read the banner on screen: paused gives the same file, and double size doubles the picture', async () => {
  const { page } = await open();
  await page.click('#demo');
  await page.waitForSelector('[aria-label="bone-anim.cbmd"] .banner .export');
  const running = (await exportFrom(page, 'gif')).bytes;
  await page.click('[aria-label="bone-anim.cbmd"] .banner figcaption button[aria-pressed]');
  assert.equal(await page.getAttribute('[aria-label="bone-anim.cbmd"] .banner button[aria-pressed]', 'aria-pressed'), 'true', 'the banner is paused');
  const paused = (await exportFrom(page, 'gif')).bytes;
  assert.ok(running.equals(paused), 'the same pictures whether the banner on screen runs or not');
  const big = gifInfo((await exportFrom(page, 'gif', { double: true })).bytes);
  assert.deepEqual([big.width, big.height], [800, 480]);
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  const offscreen = (await exportFrom(page, 'gif')).bytes;
  assert.ok(offscreen.equals(running));
});

test('a banner that does not move still exports: one picture as a GIF or WebP', async () => {
  const { page } = await open();
  await page.setInputFiles('#pick-files', join(import.meta.dirname, '../../fixtures/scene/features/alpha-test.cbmd'));
  await page.waitForSelector('.banner .export');
  assert.equal(await page.locator('.banner button[aria-pressed]').count(), 0, 'nothing to pause');
  const gif = gifInfo((await exportFrom(page, 'gif', { at: '.banner' })).bytes);
  assert.equal(gif.frames, 1);
  assert.equal(gif.loop, 0);
  const webp = webpInfo((await exportFrom(page, 'webp', { at: '.banner' })).bytes);
  assert.equal(webp.durations.length, 1);
});

test('the Export as list works from the keyboard: Escape closes it and the focus goes back', async () => {
  const { page } = await open();
  await page.click('#demo');
  await page.waitForSelector('[aria-label="bone-anim.cbmd"] .banner .export');
  await page.focus('[aria-label="bone-anim.cbmd"] .banner .export-open');
  await page.keyboard.press('Enter');
  assert.equal(await page.getAttribute('[aria-label="bone-anim.cbmd"] .banner .export-open', 'aria-expanded'), 'true');
  assert.equal(await page.evaluate(() => document.activeElement.dataset.format), 'gif', 'the first choice has the focus');
  await page.keyboard.press('Escape');
  assert.equal(await page.getAttribute('[aria-label="bone-anim.cbmd"] .banner .export-open', 'aria-expanded'), 'false');
  assert.equal(await page.evaluate(() => document.activeElement === document.querySelector('[aria-label="bone-anim.cbmd"] .banner .export-open')), true);
  await page.keyboard.press('Enter');
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');
  assert.equal(await page.evaluate(() => document.activeElement.dataset.format), 'webp', 'Tab walks GIF, MP4 (when offered), WebP');
});

test('a started export can be cancelled: no file, and the button comes back', async () => {
  const { page } = await open();
  await page.click('#demo');
  await page.waitForSelector('[aria-label="bone-anim.cbmd"] .banner .export');
  let saved = false;
  page.on('download', () => { saved = true; });
  await page.click('[aria-label="bone-anim.cbmd"] .banner .export-open');
  await page.check('[aria-label="bone-anim.cbmd"] .banner .export-double input');
  await page.click('[aria-label="bone-anim.cbmd"] .banner [data-format="webp"]');
  assert.match(await page.textContent('[aria-label="bone-anim.cbmd"] .banner .export-status'), /^Making WebP… \d+%$/);
  assert.equal(await page.isDisabled('[aria-label="bone-anim.cbmd"] .banner .export-open'), true, 'one export at a time');
  await page.click('[aria-label="bone-anim.cbmd"] .banner .export-cancel');
  await page.waitForFunction(() => document.querySelector('[aria-label="bone-anim.cbmd"] .banner .export-status').textContent === 'Export cancelled.');
  assert.equal(await page.isDisabled('[aria-label="bone-anim.cbmd"] .banner .export-open'), false);
  await page.waitForTimeout(500);
  assert.equal(saved, false);
});

// The tracks of an MP4: [{ handler, entry (sample entry type), samples, keys (1-based, null: all), ticks, timescale, width }].
function mp4Tracks(b) {
  const boxes = (buf) => {
    const out = [];
    for (let at = 0; at < buf.length;) {
      const size = buf.readUInt32BE(at);
      assert.ok(size >= 8 && at + size <= buf.length, `box size ${size} at ${at}`);
      out.push({ type: buf.toString('latin1', at + 4, at + 8), body: buf.subarray(at + 8, at + size) });
      at += size;
    }
    return out;
  };
  const find = (buf, ...path) => path.reduce((cur, t) => boxes(cur).find((x) => x.type === t)?.body, buf);
  const top = boxes(b);
  assert.deepEqual(top.map((x) => x.type), ['ftyp', 'moov', 'mdat'], 'the moov comes first');
  return boxes(top[1].body).filter((x) => x.type === 'trak').map(({ body: trak }) => {
    const stbl = find(trak, 'mdia', 'minf', 'stbl');
    const stss = boxes(stbl).find((x) => x.type === 'stss')?.body;
    const mdhd = find(trak, 'mdia', 'mdhd');
    const stsd = find(stbl, 'stsd');
    return {
      handler: find(trak, 'mdia', 'hdlr').toString('latin1', 8, 12),
      entry: stsd.toString('latin1', 12, 16),
      width: stsd.readUInt16BE(8 + 32),
      samples: find(stbl, 'stsz').readUInt32BE(8),
      keys: stss ? Array.from({ length: stss.readUInt32BE(4) }, (_, i) => stss.readUInt32BE(8 + 4 * i)) : null,
      timescale: mdhd.readUInt32BE(12),
      ticks: mdhd.readUInt32BE(16),
    };
  });
}

// Whether this browser has an H.264 encoder (the headless Chromium used here does, through a software encoder).
const canMakeVideo = (page) => page.evaluate(async () => {
  if (typeof VideoEncoder !== 'function' || typeof VideoFrame !== 'function') return false;
  for (const codec of ['avc1.4d0028', 'avc1.42001f', 'avc1.640028']) {
    try {
      if ((await VideoEncoder.isConfigSupported({ codec, width: 400, height: 240 })).supported) return true;
    } catch {
      // next
    }
  }
  return false;
});

const hasAudioEncoder = (page, rate) => page.evaluate(async (hz) => typeof AudioEncoder === 'function'
  && (await Promise.all([{ codec: 'mp4a.40.2', sampleRate: hz }, { codec: 'opus', sampleRate: 48000 }].map((c) => AudioEncoder.isConfigSupported({ ...c, numberOfChannels: 1 }).then((r) => r.supported, () => false)))).some(Boolean), rate);

// ffprobe, when installed, reads every stream and decodes the whole file without a complaint.
function ffprobeJudges(file) {
  const probe = spawnSync('ffprobe', ['-v', 'error', '-show_entries', 'stream=codec_name,width,height,nb_frames', '-of', 'csv=p=0', file], { encoding: 'utf8' });
  if (probe.error) return null;
  const decode = spawnSync('ffmpeg', ['-v', 'error', '-i', file, '-f', 'null', '-'], { encoding: 'utf8' });
  return { streams: probe.stdout, problems: `${probe.stderr}${decode.stderr}` };
}

test('MP4 is saved with a video track where the browser can make video, and says why not where it cannot', async () => {
  const { page, requests, afterLoad } = await open();
  await page.click('#demo');
  await page.waitForSelector('[aria-label="bone-anim.cbmd"] .banner .export');
  await page.click('[aria-label="bone-anim.cbmd"] .banner .export-open');
  const button = page.locator('[aria-label="bone-anim.cbmd"] .banner [data-format="mp4"]');
  if (!(await canMakeVideo(page))) {
    assert.equal(await button.isDisabled(), true);
    assert.match(await page.textContent('[aria-label="bone-anim.cbmd"] .banner .export-formats .muted'), /MP4 isn’t available\. This browser can’t make video files\./);
    assert.equal(await button.getAttribute('aria-describedby'), await page.getAttribute('[aria-label="bone-anim.cbmd"] .banner .export-formats .muted', 'id'));
    return;
  }
  await page.waitForFunction(() => !document.querySelector('[aria-label="bone-anim.cbmd"] .banner [data-format="mp4"]').disabled);
  const [download] = await Promise.all([page.waitForEvent('download'), button.click()]);
  const path = await download.path();
  const bytes = await readFile(path);
  assert.match(download.suggestedFilename(), /-banner\.mp4$/);
  assert.equal(bytes.toString('latin1', 4, 8), 'ftyp');
  const [video, ...rest] = mp4Tracks(bytes);
  // bone-anim.cbmd carries a 100 ms jingle: a sound track where this browser has an audio encoder, else a silent video.
  const sound = await hasAudioEncoder(page, 32000);
  assert.equal(rest.length, sound ? 1 : 0, 'the jingle is a second track when the browser can encode it');
  if (sound) assert.equal(rest[0].handler, 'soun');
  assert.deepEqual([video.handler, video.entry, video.width, video.timescale], ['vide', 'avc1', 400, 60]);
  assert.equal(video.samples, 120, 'the 2-second loop at 60 pictures a second');
  assert.equal(video.ticks, 120);
  assert.deepEqual(video.keys.slice(0, 2), [1, 61], 'a key frame every second');
  const judged = ffprobeJudges(path);
  if (judged) {
    assert.match(judged.streams, /h264,400,240,120/);
    assert.equal(judged.problems, '', 'ffmpeg decodes it without a complaint');
  }
  await page.waitForFunction(() => document.querySelector('[aria-label="bone-anim.cbmd"] .banner .export-status').textContent.startsWith('Saved'));
  assert.equal(requests.length, afterLoad);
});

// A BCWAV of mono 16-bit PCM: header, INFO (wave info, one channel), DATA; and the banner with it after its pictures.
function bcwav(rate, samples) {
  const pad = (n) => Math.ceil(n / 0x20) * 0x20;
  const infoLen = pad(0x1c + 4 + 8 + 0x40);
  const dataLen = pad(8 + samples.length * 2);
  const b = Buffer.alloc(0x40 + infoLen + dataLen);
  b.write('CWAV', 0, 'latin1');
  b.writeUInt16LE(0xfeff, 4);
  b.writeUInt16LE(0x40, 6);
  b.writeUInt32LE(0x01020000, 8);
  b.writeUInt32LE(b.length, 0xc);
  b.writeUInt16LE(2, 0x10);
  b.writeUInt16LE(0x7000, 0x14); b.writeUInt32LE(0x40, 0x18); b.writeUInt32LE(infoLen, 0x1c);
  b.writeUInt16LE(0x7001, 0x20); b.writeUInt32LE(0x40 + infoLen, 0x24); b.writeUInt32LE(dataLen, 0x28);
  const info = 0x40;
  b.write('INFO', info, 'latin1');
  b.writeUInt32LE(infoLen, info + 4);
  b[info + 8] = 1; // PCM16
  b.writeUInt32LE(rate, info + 0xc);
  b.writeUInt32LE(samples.length, info + 0x14);
  b.writeUInt32LE(1, info + 0x1c); // one channel
  b.writeUInt16LE(0x7100, info + 0x20); b.writeUInt32LE(0x0c, info + 0x24); // its info, 12 bytes after the table
  const channel = info + 0x1c + 0x0c;
  b.writeUInt16LE(0x1f00, channel); b.writeUInt32LE(0, channel + 4);
  const data = 0x40 + infoLen;
  b.write('DATA', data, 'latin1');
  b.writeUInt32LE(dataLen, data + 4);
  samples.forEach((v, i) => b.writeInt16LE(v, data + 8 + 2 * i));
  return b;
}

test('a banner with a jingle gives an MP4 with a sound track where the browser can encode it, and says so where not', async () => {
  const { page } = await open();
  const bone = await readFile(join(import.meta.dirname, '../../fixtures/scene/features/bone-anim.cbmd'));
  const tone = Array.from({ length: 16000 }, (_, i) => Math.round(8000 * Math.sin((i / 16000) * 2 * Math.PI * 440)));
  const withSound = Buffer.concat([bone, bcwav(16000, tone)]);
  withSound.writeUInt32LE(bone.length, 0x84); // the jingle follows the pictures
  await page.setInputFiles('#pick-files', { name: 'jingle.cbmd', mimeType: 'application/octet-stream', buffer: withSound });
  await page.waitForSelector('.banner .export');
  await page.click('.banner .export-open');
  if (!(await canMakeVideo(page))) return; // the previous test says why
  await page.waitForFunction(() => !document.querySelector('.banner [data-format="mp4"]').disabled);
  const [download] = await Promise.all([page.waitForEvent('download'), page.click('.banner [data-format="mp4"]')]);
  const path = await download.path();
  const tracks = mp4Tracks(await readFile(path));
  const status = await page.evaluate(async () => {
    await new Promise((resolve) => { const t = setInterval(() => document.querySelector('.banner .export-status').textContent.startsWith('Saved') && (clearInterval(t), resolve()), 50); });
    return document.querySelector('.banner .export-status').textContent;
  });
  const hasAudio = await hasAudioEncoder(page, 16000);
  if (hasAudio) {
    assert.equal(tracks.length, 2, status);
    assert.equal(tracks[1].handler, 'soun');
    assert.ok(['mp4a', 'Opus'].includes(tracks[1].entry), tracks[1].entry);
    assert.doesNotMatch(status, /No sound/);
    const judged = ffprobeJudges(path);
    if (judged) {
      assert.match(judged.streams, /h264,400,240/);
      assert.match(judged.streams, /(aac|opus)/);
      assert.equal(judged.problems, '', 'ffmpeg decodes picture and sound without a complaint');
    }
  } else {
    assert.equal(tracks.length, 1);
    assert.match(status, /No sound in this browser\./);
  }
});
