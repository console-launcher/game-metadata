// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

/**
 * The files of one Wii U title, by path from the title's root (`meta/meta.xml`, `meta/iconTex.tga`, `meta/bootMovie.h264`).
 * An app that lists a `.rpx` (it sits in the title's `code/` folder) backs this with the sibling folders. Throw an
 * `IOException` when a file exists but cannot be read; return -1 or null for a file that is not there.
 */
interface TitleFiles {
    /** The size of the file at [path], or -1 when there is none. */
    fun size(path: String): Long

    /** The file's bytes, or null when there is none or it is over [maxBytes]. */
    fun read(path: String, maxBytes: Int): ByteArray?
}
