// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

import java.io.File

/** The Kotlin snippet of the README, compiled with the tests so it cannot rot (scripts/readme-snippets checks the README shows
 *  exactly the marked lines). It is never called: it needs a `game.3ds` in the working directory. */
@Suppress("unused")
private fun readmeSnippet() {
    // readme:begin
    val bytes = File("game.3ds").readBytes()
    val info = GameMeta.read(bytes, "game.3ds")
    println(info?.titles)
    BannerRenderer.open(bytes)?.use { banner -> val frame = banner.render(0f, 400, 240) }   // top-down RGBA
    // readme:end
}
