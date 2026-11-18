// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Turns what the visitor picked or dropped into the units the worker inspects. Pure (no DOM), so `node --test` covers it.
//
// An entry is { file, path }: `path` is `webkitRelativePath` for a picked folder, the name for a loose file. A folder that holds
// a Wii U title (a `meta/meta.xml`) is one unit; every other file is a unit of its own.

const META = 'meta/meta.xml';
const JUNK = new Set(['.ds_store', 'thumbs.db', 'desktop.ini']);

export const basename = (path) => path.slice(path.lastIndexOf('/') + 1);

// The folders (with a trailing '/', '' for the top) that hold a `meta/meta.xml`, shallowest first.
export function titleRoots(paths) {
  const roots = [];
  for (const p of paths) {
    if (p.endsWith(META) && (p.length === META.length || p[p.length - META.length - 1] === '/')) {
      roots.push(p.slice(0, p.length - META.length));
    }
  }
  return [...new Set(roots)].sort((a, b) => a.split('/').length - b.split('/').length);
}

// -> [{ kind: 'title', name, entries } | { kind: 'file', name, entry, siblings, inFolder }]
// `siblings` (for a `.cue` sheet only) are the other files in its folder, which its tracks are found among.
export function groupEntries(entries) {
  const kept = entries.filter((e) => !JUNK.has(basename(e.path).toLowerCase()));
  const roots = titleRoots(kept.map((e) => e.path));
  const titles = new Map(roots.map((r) => [r, []]));
  const loose = [];
  for (const e of kept) {
    const root = roots.find((r) => e.path.startsWith(r));
    if (root !== undefined) titles.get(root).push(e);
    else loose.push(e);
  }
  const units = [];
  for (const [root, list] of titles) {
    const folder = root.replace(/\/$/, '');
    units.push({ kind: 'title', name: folder ? basename(folder) : 'Wii U title', entries: list });
  }
  const dir = (p) => p.slice(0, p.lastIndexOf('/') + 1);
  for (const e of loose) {
    units.push({
      kind: 'file',
      name: basename(e.path),
      entry: e,
      siblings: e.path.toLowerCase().endsWith('.cue') ? loose.filter((o) => o !== e && dir(o.path) === dir(e.path)) : [],
      inFolder: e.path.includes('/'),
    });
  }
  return units;
}

export function formatSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KiB', 'MiB', 'GiB', 'TiB'];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

// Camel-case language and region names as words: "SimplifiedChinese" -> "Simplified Chinese".
export const words = (name) => name.replace(/([a-z])([A-Z])/g, '$1 $2');
