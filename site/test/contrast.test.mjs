// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// WCAG 2.2 AA contrast of the page's colour tokens (style.css), light and dark: every pair the page paints text or a control
// boundary with. Text needs 4.5:1; focus rings and the boundaries of controls and the drop zone need 3:1.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const css = readFileSync(new URL('../src/style.css', import.meta.url), 'utf8');

// The custom properties of the first `:root { }` (light) and of the `:root { }` inside the dark media query.
function tokens(block) {
  return Object.fromEntries([...block.matchAll(/--([a-z-]+):\s*(#[0-9a-f]{6})\s*;/gi)].map((m) => [m[1], m[2]]));
}
const lightBlock = css.match(/:root\s*\{([^}]*)\}/)[1];
const darkBlock = css.match(/@media \(prefers-color-scheme: dark\)\s*\{\s*:root\s*\{([^}]*)\}/)[1];
const themes = { light: tokens(lightBlock), dark: { ...tokens(lightBlock), ...tokens(darkBlock) } };

const channel = (v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const luminance = (hex) => {
  const [r, g, b] = [1, 3, 5].map((i) => channel(parseInt(hex.slice(i, i + 2), 16) / 255));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
export const ratio = (a, b) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

// [foreground token, background token, minimum, what it paints]
const PAIRS = [
  ['text', 'bg', 4.5, 'body text'],
  ['text', 'surface', 4.5, 'text on a card and the drop zone'],
  ['muted', 'bg', 4.5, 'secondary text: lead, status, progress, footer'],
  ['muted', 'surface', 4.5, 'secondary text on cards and the drop zone'],
  ['accent', 'bg', 4.5, 'links'],
  ['accent', 'surface', 4.5, 'links and quiet buttons on cards'],
  ['accent-text', 'accent', 4.5, 'primary button label'],
  ['error', 'surface', 4.5, 'error card text'],
  ['error', 'bg', 4.5, 'error text outside a card'],
  ['ui', 'bg', 3, 'drop zone border against the page'],
  ['ui', 'surface', 3, 'drop zone border against its own fill'],
  ['accent', 'bg', 3, 'focus ring on the page'],
  ['accent', 'surface', 3, 'focus ring on a card or the drop zone'],
];

for (const [theme, t] of Object.entries(themes)) {
  test(`${theme}: every text and control colour pair meets WCAG 2.2 AA`, () => {
    const lowest = { text: Infinity, ui: Infinity };
    for (const [fg, bg, min, what] of PAIRS) {
      assert.ok(t[fg] && t[bg], `token --${fg} and --${bg} exist`);
      const r = ratio(t[fg], t[bg]);
      lowest[min === 3 ? 'ui' : 'text'] = Math.min(lowest[min === 3 ? 'ui' : 'text'], r);
      assert.ok(r >= min, `${theme}: ${what}: --${fg} ${t[fg]} on --${bg} ${t[bg]} is ${r.toFixed(2)}:1, needs ${min}:1`);
    }
    console.log(`# lowest contrast, ${theme}: text ${lowest.text.toFixed(2)}:1 (needs 4.5), controls ${lowest.ui.toFixed(2)}:1 (needs 3)`);
  });
}
