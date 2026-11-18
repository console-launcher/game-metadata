// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

import java.io.File
import java.io.IOException
import java.nio.file.Files

/**
 * Reads titles, icons, ids, regions and ratings from game files, through the native game-metadata library.
 *
 * The caller owns the I/O: pass the bytes, or an open file descriptor (on Android, `ParcelFileDescriptor.fd` from a
 * content URI; the descriptor is read with positioned reads and is neither moved nor closed). [fileName] picks the
 * reader by its extension (3DS: `.3ds`, `.cci`, `.cxi`, `.app`, `.cia`; DS: `.nds`, `.dsi`, `.srl`, `.ids`, `.zip`; GameCube and Wii: `.iso`,
 * `.gcm`, `.ciso`, `.gcz`, `.rvz`, `.wia`, `.tgc`, `.wbfs`, `.wad`; Wii U: `.wua`, and a title's files through [readTitle]).
 *
 * Calls are thread-safe and blocking (they read the file's headers and icon); call them off the main thread.
 */
object GameMeta {
    /** The ABI version these bindings expect from the native library. */
    const val ABI_VERSION = 1

    /** What [read] found, or null for a file of an unknown kind or one that yields nothing. */
    fun read(bytes: ByteArray, fileName: String): GameInfo? = result(nativeReadBytes(bytes, fileName))

    /**
     * Reads the open, seekable descriptor [fd]. Null for an unknown kind or a file that yields nothing.
     * @throws IOException when the descriptor cannot be read.
     */
    fun read(fd: Int, fileName: String): GameInfo? = result(nativeReadFd(fd, fileName))

    /**
     * Reads a Wii U title from its files ([TitleFiles]: `meta/meta.xml`, the TGA pictures, the boot media), as an
     * app that lists a `.rpx` finds them in the title's folder. Null when there is no readable `meta/meta.xml`.
     * @throws IOException when [files] throws one.
     */
    fun readTitle(files: TitleFiles): GameInfo? = result(nativeReadTitle(files))

    /**
     * One picture of a Wii U title ([ImageKind.Icon], [ImageKind.TvSplash], [ImageKind.GamePadSplash] or
     * [ImageKind.BootLogo]), decoded; null when the file is missing or not a readable TGA. The large splashes are
     * decoded on demand, so a read stays cheap.
     * @throws IOException when [files] throws one.
     */
    fun titleImage(files: TitleFiles, kind: ImageKind): RgbaImage? = image(nativeTitleImage(files, kind.bit))

    /** [titleImage] for the base title of a `.wua` held in [bytes]. */
    fun wuaImage(bytes: ByteArray, kind: ImageKind): RgbaImage? = image(nativeWuaImageBytes(bytes, kind.bit))

    /**
     * [titleImage] for the base title of the `.wua` in the open, seekable descriptor [fd] (read with positioned reads;
     * neither moved nor closed).
     * @throws IOException when the descriptor cannot be read.
     */
    fun wuaImage(fd: Int, kind: ImageKind): RgbaImage? = image(nativeWuaImageFd(fd, kind.bit))

    /** The picture [encoded] as the native side hands it over: width and height as big-endian ints, then RGBA. */
    private fun image(encoded: ByteArray?): RgbaImage? {
        if (encoded == null) return null
        fun int(at: Int) = (0..3).fold(0) { acc, i -> (acc shl 8) or (encoded[at + i].toInt() and 0xff) }
        return RgbaImage(int(0), int(4), encoded.copyOfRange(8, encoded.size))
    }

    /**
     * The status a native read returned, or null when [handle] is a pointer. Statuses cross as `-status` in
     * `-MAX_STATUS..0`; a sign test is not enough, because Android's arm64 allocator tags heap pointers in the top
     * byte, which makes them negative as a Long.
     */
    internal fun statusOf(handle: Long): Int? = if (handle in -MAX_STATUS..0L) (-handle).toInt() else null

    private fun result(handle: Long): GameInfo? = result(handle, ::decode, ::nativeFree)

    private fun <T> result(handle: Long, decode: (Long) -> T, free: (Long) -> Unit): T? {
        val status = statusOf(handle)
        if (status != null) {
            return when (status) {
                STATUS_NOT_FOUND -> null
                STATUS_ERR_IO -> throw IOException("game-metadata: the file could not be read")
                else -> throw IllegalStateException("game-metadata: native status $status")
            }
        }
        try {
            return decode(handle)
        } finally {
            free(handle)
        }
    }

    /**
     * The banner of a 3DS game as a renderer-neutral scene: [bytes] is a CBMD or CGFX file, or a decrypted `.3ds`, `.cia` or
     * `.cxi` whose ExeFS holds the banner. Null when there is no banner, it is encrypted (nothing is ever decrypted) or it does not parse.
     * @throws IOException when the source cannot be read.
     */
    fun readBannerScene(bytes: ByteArray): BannerSceneInfo? = result(nativeSceneBytes(bytes), ::decodeScene, ::nativeSceneFree)

    /**
     * [readBannerScene] for the open, seekable descriptor [fd] (positioned reads; neither moved nor closed).
     * @throws IOException when the descriptor cannot be read.
     */
    fun readBannerScene(fd: Int): BannerSceneInfo? = result(nativeSceneFd(fd), ::decodeScene, ::nativeSceneFree)

    // BannerRenderer's natives: a live scene handle (or a status), the renderers on it.
    internal fun sceneBytes(bytes: ByteArray): Long = nativeSceneBytes(bytes)
    internal fun sceneFd(fd: Int): Long = nativeSceneFd(fd)
    internal fun sceneFree(handle: Long) = nativeSceneFree(handle)
    internal fun renderFrames(handle: Long): Float = nativeRenderFrames(handle)
    internal fun renderSoft(handle: Long, frame: Float, width: Int, height: Int): ByteArray? = nativeRenderSoft(handle, frame, width, height)
    internal fun glesCreate(handle: Long, glLibrary: String): Long = nativeGlesCreate(handle, glLibrary)
    internal fun glesDraw(gles: Long, frame: Float, x: Int, y: Int, width: Int, height: Int) = nativeGlesDraw(gles, frame, x, y, width, height)
    internal fun glesDestroy(gles: Long) = nativeGlesDestroy(gles)

    // WiiBanner's natives: a live Wii banner handle (or a status).
    internal fun wiiBannerBytes(bytes: ByteArray): Long = nativeWiiBannerBytes(bytes)
    internal fun wiiBannerFd(fd: Int): Long = nativeWiiBannerFd(fd)
    internal fun wiiBannerNumbers(handle: Long): LongArray? = nativeWiiBannerNumbers(handle)
    internal fun wiiBannerMissing(handle: Long): Array<String>? = nativeWiiBannerMissing(handle)
    internal fun wiiBannerSize(handle: Long, scale: Float): IntArray? = nativeWiiBannerSize(handle, scale)
    internal fun wiiBannerRender(handle: Long, frame: Float, scale: Float): ByteArray? = nativeWiiBannerRender(handle, frame, scale)
    internal fun wiiBannerFree(handle: Long) = nativeWiiBannerFree(handle)

    /** The handle of a scene read, null when there is no banner. @throws IOException when the source cannot be read. */
    internal fun sceneHandle(handle: Long): Long? = when (statusOf(handle)) {
        null -> handle
        STATUS_NOT_FOUND -> null
        STATUS_ERR_IO -> throw IOException("game-metadata: the file could not be read")
        else -> throw IllegalStateException("game-metadata: native status ${statusOf(handle)}")
    }

    private fun decodeScene(handle: Long): BannerSceneInfo {
        val s = nativeSceneStrings(handle) ?: throw IllegalStateException("game-metadata: no scene strings")
        val n = nativeSceneNumbers(handle) ?: throw IllegalStateException("game-metadata: no scene numbers")
        val textures = (0 until n[2].toInt()).associate { i ->
            s[1 + i] to (image(nativeSceneTexture(handle, i)) ?: throw IllegalStateException("game-metadata: no texture $i"))
        }
        return BannerSceneInfo(
            json = s[0],
            models = n[0].toInt(),
            meshes = n[1].toInt(),
            textures = textures,
            lights = n[3].toInt(),
            cameras = n[4].toInt(),
            skeletalAnimations = n[5].toInt(),
            materialAnimations = n[6].toInt(),
            visibilityAnimations = n[7].toInt(),
            unsupported = s.drop(1 + n[2].toInt()).toSet(),
        )
    }

    private fun decode(handle: Long): GameInfo {
        val s = nativeStrings(handle) ?: throw IllegalStateException("game-metadata: no strings")
        val n = nativeNumbers(handle) ?: throw IllegalStateException("game-metadata: no numbers")
        val titles = Language.entries.mapNotNull { l ->
            val name = s[l.ordinal * 3] ?: return@mapNotNull null
            l to Title(name, s[l.ordinal * 3 + 1].orEmpty(), s[l.ordinal * 3 + 2])
        }.toMap()
        val ids = Language.entries.size * 3
        fun icon(which: Int, w: Long, h: Long) = nativeIcon(handle, which)?.let { RgbaImage(w.toInt(), h.toInt(), it) }
        val steps = (0 until n[29].toInt()).mapNotNull { i ->
            icon(2 + i, n[30 + i * 3], n[31 + i * 3])?.let { IconStep(it, n[32 + i * 3].toInt()) }
        }
        return GameInfo(
            platform = Platform.entries.first { it.code == n[0].toInt() },
            titles = titles,
            icon = icon(0, n[6], n[7]),
            smallIcon = icon(1, n[8], n[9]),
            titleId = s[ids],
            productCode = s[ids + 1],
            gameId = s[ids + 2],
            makerCode = s[ids + 3],
            regions = Region.entries.filter { n[1] and (1L shl it.ordinal) != 0L }.toSet(),
            regionFree = n[2] == 1L,
            ratings = RatingBoard.entries.mapNotNull { b -> n[10 + b.slot].takeIf { it >= 0 }?.let { b to it.toInt() } }.toMap(),
            banner = if (n[4] >= 0) Extent(n[4], n[5]) else null,
            animatedIcon = steps,
            images = ImageKind.entries.filter { n[26] and (1L shl it.bit) != 0L }.toSet(),
            bootMovieSize = n[27].takeIf { it >= 0 },
            bootSoundSize = n[28].takeIf { it >= 0 },
            headerTitle = s[ids + 4],
            version = nativeVersion(handle).takeIf { it >= 0 }?.toInt(),
            cartHeaderJson = s[ids + 5],
        )
    }

    /**
     * The ids of [bytes], a file called [fileName] (the extension picks the reader): serial, game id, title id, CHD and zip
     * checksums, PSP and Xbox ids. [opener] serves files beside it by path relative to its folder
     * (a cue sheet's tracks, a `.rpx`'s `meta/meta.xml`); without one those formats give nothing. Null when the file yields no id.
     * @throws IOException when a read fails.
     */
    fun readIds(bytes: ByteArray, fileName: String, opener: FileOpener? = null): Ids? =
        result(nativeIdsBytes(bytes, fileName, opener), ::decodeIds, ::nativeIdsFree)

    /**
     * [readIds] for the open, seekable descriptor [fd] (positioned reads; neither moved nor closed).
     * @throws IOException when the descriptor cannot be read.
     */
    fun readIds(fd: Int, fileName: String, opener: FileOpener? = null): Ids? =
        result(nativeIdsFd(fd, fileName, opener), ::decodeIds, ::nativeIdsFree)

    private fun decodeIds(handle: Long): Ids {
        val s = nativeIdsStrings(handle) ?: throw IllegalStateException("game-metadata: no id strings")
        return Ids(s[0], s[1], s[2], s[3], s[4], nativeIdsZipSize(handle).takeIf { it >= 0 }, s[5], s[6], s[7], s[9])
    }

    /** A streaming RetroAchievements hasher for a file called [fileName] of [size] bytes (-1 if unknown); null for a file this does not hash. */
    fun raHasher(fileName: String, size: Long = -1): RaHasher? = nativeRaNew(fileName, size).takeIf { it != 0L }?.let(::RaHasher)

    internal fun raUpdate(handle: Long, bytes: ByteArray, length: Int) = nativeRaUpdate(handle, bytes, length)
    internal fun raFinish(handle: Long): String? = nativeRaFinish(handle)
    internal fun raFree(handle: Long) = nativeRaFree(handle)

    /**
     * The GameCube, Wii or Xbox disc id in [header], the first [DISC_ID_HEADER_BYTES] bytes of a plain disc image (an XBE for Xbox),
     * or the reason there is none. Container images (WBFS, CISO, GCZ) are refused here; [readIds] reads their `gameId`.
     */
    fun discId(header: ByteArray): DiscIdResult =
        unpackDiscId(nativeDiscIdBytes(header) ?: throw IllegalStateException("game-metadata: no disc id"))

    /**
     * [discId] for the open, seekable descriptor [fd] (positioned reads; neither moved nor closed).
     * @throws IOException when the descriptor cannot be read.
     */
    fun discId(fd: Int): DiscIdResult = unpackDiscId(nativeDiscIdFd(fd) ?: ioError())

    /** How many bytes from the start of a file [discId] reads. */
    const val DISC_ID_HEADER_BYTES = 2048

    private const val MAX_STATUS = 255L
    private const val STATUS_NOT_FOUND = 1
    private const val STATUS_ERR_IO = 3

    init {
        NativeLibrary.load()
        val abi = nativeAbiVersion()
        check(abi == ABI_VERSION) { "game-metadata: native ABI $abi, bindings expect $ABI_VERSION" }
    }

    @JvmStatic private external fun nativeAbiVersion(): Int
    @JvmStatic private external fun nativeReadBytes(bytes: ByteArray, fileName: String): Long
    @JvmStatic private external fun nativeReadFd(fd: Int, fileName: String): Long
    @JvmStatic private external fun nativeReadTitle(files: TitleFiles): Long
    @JvmStatic private external fun nativeTitleImage(files: TitleFiles, kind: Int): ByteArray?
    @JvmStatic private external fun nativeWuaImageBytes(bytes: ByteArray, kind: Int): ByteArray?
    @JvmStatic private external fun nativeWuaImageFd(fd: Int, kind: Int): ByteArray?
    @JvmStatic private external fun nativeStrings(handle: Long): Array<String?>?
    @JvmStatic private external fun nativeNumbers(handle: Long): LongArray?
    @JvmStatic private external fun nativeVersion(handle: Long): Long
    @JvmStatic private external fun nativeIcon(handle: Long, which: Int): ByteArray?
    @JvmStatic private external fun nativeFree(handle: Long)
    @JvmStatic private external fun nativeSceneBytes(bytes: ByteArray): Long
    @JvmStatic private external fun nativeSceneFd(fd: Int): Long
    @JvmStatic private external fun nativeSceneStrings(handle: Long): Array<String>?
    @JvmStatic private external fun nativeSceneNumbers(handle: Long): LongArray?
    @JvmStatic private external fun nativeSceneTexture(handle: Long, index: Int): ByteArray?
    @JvmStatic private external fun nativeSceneFree(handle: Long)
    @JvmStatic private external fun nativeWiiBannerBytes(bytes: ByteArray): Long
    @JvmStatic private external fun nativeWiiBannerFd(fd: Int): Long
    @JvmStatic private external fun nativeWiiBannerNumbers(handle: Long): LongArray?
    @JvmStatic private external fun nativeWiiBannerMissing(handle: Long): Array<String>?
    @JvmStatic private external fun nativeWiiBannerSize(handle: Long, scale: Float): IntArray?
    @JvmStatic private external fun nativeWiiBannerRender(handle: Long, frame: Float, scale: Float): ByteArray?
    @JvmStatic private external fun nativeWiiBannerFree(handle: Long)
    @JvmStatic private external fun nativeRenderFrames(handle: Long): Float
    @JvmStatic private external fun nativeRenderSoft(handle: Long, frame: Float, width: Int, height: Int): ByteArray?
    @JvmStatic private external fun nativeGlesCreate(handle: Long, glLibrary: String): Long
    @JvmStatic private external fun nativeGlesDraw(gles: Long, frame: Float, x: Int, y: Int, width: Int, height: Int)
    @JvmStatic private external fun nativeGlesDestroy(gles: Long)
    @JvmStatic private external fun nativeIdsBytes(bytes: ByteArray, fileName: String, opener: FileOpener?): Long
    @JvmStatic private external fun nativeIdsFd(fd: Int, fileName: String, opener: FileOpener?): Long
    @JvmStatic private external fun nativeIdsStrings(handle: Long): Array<String?>?
    @JvmStatic private external fun nativeIdsZipSize(handle: Long): Long
    @JvmStatic private external fun nativeIdsFree(handle: Long)
    @JvmStatic private external fun nativeRaNew(fileName: String, size: Long): Long
    @JvmStatic private external fun nativeRaUpdate(handle: Long, bytes: ByteArray, length: Int)
    @JvmStatic private external fun nativeRaFinish(handle: Long): String?
    @JvmStatic private external fun nativeRaFree(handle: Long)
    @JvmStatic private external fun nativeDiscIdBytes(header: ByteArray): LongArray?
    @JvmStatic private external fun nativeDiscIdFd(fd: Int): LongArray?
}

/**
 * Loads `libgamemeta_jni`: the `gamemeta.library` system property (a path), else the platform's library path (Android
 * loads it from the APK), else the copy a host jar carries under `gamemeta/native/<os>-<arch>/`.
 */
internal object NativeLibrary {
    private const val NAME = "gamemeta_jni"

    fun load() {
        System.getProperty("gamemeta.library")?.takeIf { it.isNotEmpty() }?.let { return System.load(File(it).absolutePath) }
        try {
            System.loadLibrary(NAME)
        } catch (e: UnsatisfiedLinkError) {
            loadFromJar() ?: throw e
        }
    }

    private fun loadFromJar(): Unit? {
        val os = System.getProperty("os.name").lowercase().let {
            when {
                it.startsWith("linux") -> "linux"
                it.startsWith("mac") -> "macos"
                it.startsWith("windows") -> "windows"
                else -> return null
            }
        }
        val arch = when (System.getProperty("os.arch")) {
            "amd64", "x86_64" -> "x86_64"
            "aarch64", "arm64" -> "aarch64"
            else -> return null
        }
        val file = System.mapLibraryName(NAME)
        val stream = NativeLibrary::class.java.getResourceAsStream("/gamemeta/native/$os-$arch/$file") ?: return null
        // One directory per load: a JVM can hold several class loaders that each load these classes, and a library cannot
        // be loaded by two of them from one path. Writing over a file another loader has mapped crashes the JVM.
        val dir = Files.createTempDirectory("game-metadata-${GameMeta.ABI_VERSION}-").toFile()
        val target = File(dir, file)
        stream.use { input -> target.outputStream().use { input.copyTo(it) } }
        target.deleteOnExit()
        dir.deleteOnExit()
        System.load(target.absolutePath)
        return Unit
    }
}
