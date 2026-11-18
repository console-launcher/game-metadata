// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The trademark notice is one text in three places: README, NOTICE and the site footer. The license section of the README names the
// CC BY-SA directories.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const read = (p) => readFileSync(new URL(`../../${p}`, import.meta.url), 'utf8').replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ');
const NOTICE = 'game-metadata is an independent project. It is not affiliated with, authorized, sponsored or endorsed by Nintendo, Sony, ' +
  'Microsoft, Sega or any other game or console company. Nintendo 3DS, Nintendo 3DS HOME Menu, Wii, Wii U, Game Boy, Nintendo DS, GameCube, ' +
  'PlayStation, Xbox, Sega Genesis, Mega Drive, Master System and Game Gear, and the other console, format and game names used here, are ' +
  'trademarks of their respective owners and are used only to say which file formats the library reads. The project contains no game data, ' +
  'keys or console firmware and never unlocks protected files.';

for (const file of ['README.md', 'NOTICE.md', 'site/src/index.html']) {
  test(`${file} carries the trademark notice`, () => {
    assert.ok(read(file).includes(NOTICE), file);
  });
}

test('the README license section names the CC BY-SA 3.0 directories', () => {
  const license = read('README.md').split('## License')[1];
  assert.ok(license.includes('CC BY-SA 3.0') && license.includes('`demo/`') && license.includes('`fixtures/showcase/supertuxkart/`'));
});
