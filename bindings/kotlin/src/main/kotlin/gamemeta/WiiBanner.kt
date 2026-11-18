// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

import java.io.IOException

/**
 * A Wii channel banner ready to draw ([open]) on the CPU. Frames count 60 to the second: the start animation plays once
 * ([startFrames]), then the loop repeats. Holds a native banner: [close] it.
 *
 * Written from the NW4R layout format and tested on authored banners only (see `docs/design.md`): [skipped] counts the panes
 * that are parsed but not drawn (text, window frames, bounding boxes) so a mismatch with the console can be explained.
 */
class WiiBanner private constructor(private var handle: Long) : AutoCloseable {
    private val numbers: LongArray = GameMeta.wiiBannerNumbers(handle) ?: throw IllegalStateException("game-metadata: no banner numbers")

    /** Frames of the start animation (0 when there is none): the loop begins after it. */
    val startFrames: Int get() = numbers[0].toInt()

    /** Frames of the loop animation (0 when there is none). */
    val loopFrames: Int get() = numbers[1].toInt()

    /** Pane kinds parsed but not drawn: `wnd1` (windows), `txt1` (text), `bnd1` (bounding boxes) with how many; only kinds present. */
    val skipped: Map<String, Int> get() = listOf("wnd1" to numbers[2], "txt1" to numbers[3], "bnd1" to numbers[4]).filter { it.second > 0 }.associate { it.first to it.second.toInt() }

    /** Textures the layout names that the archive lacks (nothing is drawn for those maps). */
    val missingTextures: List<String> get() = GameMeta.wiiBannerMissing(live())?.toList() ?: emptyList()

    /** The size of a frame at [scale] pixels per layout unit (608 x 456 at 1 for a channel banner). */
    fun size(scale: Float = 1f): Pair<Int, Int> {
        val s = GameMeta.wiiBannerSize(live(), scale) ?: throw IllegalArgumentException("bad scale $scale")
        return s[0] to s[1]
    }

    /**
     * The banner [frame] frames after it appears, [scale] pixels per layout unit (top-down RGBA over transparent).
     * @throws IllegalArgumentException for a scale that is not positive and finite, or makes a frame over 8192 pixels a side or 64 Mpx.
     */
    fun render(frame: Float, scale: Float = 1f): RgbaImage {
        val (w, h) = size(scale)
        val pixels = GameMeta.wiiBannerRender(live(), frame, scale) ?: throw IllegalStateException("game-metadata: render failed")
        return RgbaImage(w, h, pixels)
    }

    /** The banner settled: the start animation done, the loop at its first frame. */
    fun poster(scale: Float = 1f): RgbaImage = render(startFrames.toFloat(), scale)

    override fun close() {
        if (handle != 0L) GameMeta.wiiBannerFree(handle)
        handle = 0
    }

    private fun live(): Long = handle.also { check(it != 0L) { "closed" } }

    companion object {
        /**
         * The banner of [bytes]: an RVZ disc image, an `opening.bnr`, or a `meta/banner.bin` archive, told by content. Null when
         * there is none (or the disc is encrypted: nothing is ever decrypted) or it does not parse.
         * @throws IOException when the source cannot be read.
         */
        fun open(bytes: ByteArray): WiiBanner? = GameMeta.sceneHandle(GameMeta.wiiBannerBytes(bytes))?.let(::WiiBanner)

        /** [open] for the open, seekable descriptor [fd] (positioned reads; neither moved nor closed). */
        fun open(fd: Int): WiiBanner? = GameMeta.sceneHandle(GameMeta.wiiBannerFd(fd))?.let(::WiiBanner)
    }
}
