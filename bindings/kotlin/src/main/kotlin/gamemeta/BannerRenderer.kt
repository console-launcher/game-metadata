// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

import java.io.IOException

/**
 * A 3DS banner ready to draw ([open]): the CPU renderer into an image ([render]), or the OpenGL ES 3 renderer in the caller's
 * context ([createGles]). Frames count 60 to the second, looped by the banner's clips. Holds a native scene: [close] it.
 */
class BannerRenderer private constructor(private var handle: Long) : AutoCloseable {
    /** The loop length in frames; 0 when the banner does not animate. */
    val frames: Float get() = GameMeta.renderFrames(live())

    /**
     * The banner at [frame] through HOME's view over transparent black, [width] x [height] (top-down RGBA).
     * @throws IllegalArgumentException for a zero or too large size (over 64 Mpx).
     */
    fun render(frame: Float, width: Int, height: Int): RgbaImage {
        require(width > 0 && height > 0 && width.toLong() * height <= MAX_PIXELS) { "bad size ${width}x$height" }
        val pixels = GameMeta.renderSoft(live(), frame, width, height) ?: throw IllegalStateException("game-metadata: render failed")
        return RgbaImage(width, height, pixels)
    }

    /**
     * The GL renderer for this banner in the OpenGL ES 3 context current on the calling thread (call it, [GlesBanner.draw] and
     * [GlesBanner.close] on that thread), its functions taken from [glLibrary]. Null when the library or a function is missing,
     * no context is current, or nothing in the banner could be set up.
     */
    fun createGles(glLibrary: String = DEFAULT_GL_LIBRARY): GlesBanner? =
        GameMeta.glesCreate(live(), glLibrary).takeIf { it != 0L }?.let(::GlesBanner)

    override fun close() {
        if (handle != 0L) GameMeta.sceneFree(handle)
        handle = 0
    }

    private fun live(): Long = handle.also { check(it != 0L) { "closed" } }

    companion object {
        private const val MAX_PIXELS = 1L shl 26

        /** The GLES library on this platform: Android's `libGLESv3.so`, else `libGLESv2.so.2` (Mesa, glvnd). */
        val DEFAULT_GL_LIBRARY: String =
            if (System.getProperty("java.vendor").orEmpty().contains("Android", ignoreCase = true)) "libGLESv3.so" else "libGLESv2.so.2"

        /**
         * The banner of [bytes] (as [GameMeta.readBannerScene] reads it), or null when there is none.
         * @throws IOException when the source cannot be read.
         */
        fun open(bytes: ByteArray): BannerRenderer? = GameMeta.sceneHandle(GameMeta.sceneBytes(bytes))?.let(::BannerRenderer)

        /** [open] for the open, seekable descriptor [fd] (positioned reads; neither moved nor closed). */
        fun open(fd: Int): BannerRenderer? = GameMeta.sceneHandle(GameMeta.sceneFd(fd))?.let(::BannerRenderer)
    }
}

/** A banner's GL resources in one context ([BannerRenderer.createGles]). Use and [close] it on that context's thread. */
class GlesBanner internal constructor(private var handle: Long) : AutoCloseable {
    /**
     * Draws the banner at [frame] through HOME's view into the viewport ([x], [y] of its bottom-left corner in GL window
     * coordinates, [width] x [height]) of the bound framebuffer, cleared there to transparent black first.
     */
    fun draw(frame: Float, x: Int, y: Int, width: Int, height: Int) {
        check(handle != 0L) { "closed" }
        require(width > 0 && height > 0) { "empty viewport" }
        GameMeta.glesDraw(handle, frame, x, y, width, height)
    }

    /** Deletes the GL resources (the context must be current). */
    override fun close() {
        if (handle != 0L) GameMeta.glesDestroy(handle)
        handle = 0
    }
}
