// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

import java.io.File
import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertNull

/** Loads the host library and reads the committed synthetic fixture (see scripts/kotlin-test). */
class GameMetaTest {
    private fun fixture(name: String) = File(System.getProperty("gamemeta.fixtures"), name).readBytes()
    private val sample = fixture("sample.3ds")

    @Test
    fun `reads the sample fixture through the native library`() {
        val info = assertNotNull(GameMeta.read(sample, "sample.3ds"))
        assertEquals(Platform.ThreeDS, info.platform)
        assertEquals("Sample Game Subtitle", info.title)
        assertEquals(Title("Sample Game Subtitle", "Sample", "Example Pub"), info.titles[Language.English])
        assertEquals("サンプル", info.titles[Language.Japanese]?.shortName)
        assertNull(info.titles[Language.French]?.publisher)
        assertEquals(setOf(Language.Japanese, Language.English, Language.French), info.titles.keys)
        assertEquals("0004000000123400", info.titleId)
        assertEquals("CTR-P-SMPL", info.productCode)
        assertEquals(48, info.icon?.width)
        assertEquals(24 * 24 * 4, info.smallIcon?.pixels?.size)
        assertEquals(setOf(Region.NorthAmerica, Region.Europe), info.regions)
        assertEquals(mapOf(RatingBoard.CERO to 12, RatingBoard.ESRB to 10, RatingBoard.PEGI to 0), info.ratings)
        assertNotNull(info.banner)
    }

    @Test
    fun `reads the DSi sample, bare and zipped`() {
        for ((bytes, name) in listOf(fixture("sample.nds") to "sample.nds", fixture("sample-nds.zip") to "sample-nds.zip")) {
            val info = assertNotNull(GameMeta.read(bytes, name), name)
            assertEquals(Platform.DSi, info.platform)
            assertEquals(Title("Sample Game Subtitle", "Sample Game", "Example Pub"), info.titles[Language.English])
            assertEquals("SMPL", info.gameId)
            assertEquals("01", info.makerCode)
            assertEquals(32, info.icon?.width)
            assertEquals(listOf(10, 8), info.animatedIcon.map { it.durationFrames })
            assertNotNull(info.banner)
        }
    }

    @Test
    fun `reads the GameCube sample from its RVZ`() {
        val info = assertNotNull(GameMeta.read(fixture("sample-gc.rvz"), "sample-gc.rvz"))
        assertEquals(Platform.GameCube, info.platform)
        assertEquals(Title("Sample Quest", "Sample Quest", "Sample Quest Maker Inc."), info.titles[Language.English])
        assertEquals("Beispiel Quest", info.titles[Language.German]?.name)
        assertEquals(6, info.titles.size)
        assertEquals("GSMP01", info.gameId)
        assertEquals("01", info.makerCode)
        assertEquals("Sample Disc", info.headerTitle)
        assertEquals(setOf(Region.Europe), info.regions)
        assertEquals(setOf(ImageKind.Banner), info.images)
        assertEquals(96, info.icon?.width)
        assertEquals(32, info.icon?.height)
    }

    @Test
    fun `reads the Wii sample from its RVZ and a WAD`() {
        val info = assertNotNull(GameMeta.read(fixture("sample-wii.rvz"), "sample-wii.rvz"))
        assertEquals(Platform.Wii, info.platform)
        assertEquals("Sample Channel", info.titles[Language.English]?.name)
        assertEquals("Beispielkanal", info.titles[Language.German]?.name)
        assertEquals("サンプル", info.titles[Language.Japanese]?.name)
        assertEquals("RSMP01", info.gameId)
        assertEquals("Header Title", info.headerTitle)
        assertEquals(setOf(Region.Europe), info.regions)
        assertEquals(setOf(ImageKind.Logo), info.images)
        assertEquals(64, info.icon?.width)
        assertEquals(16, info.icon?.height)
        val wad = assertNotNull(GameMeta.read(fixture("sample.wad"), "sample.wad"))
        assertEquals(Platform.Wii, wad.platform)
        assertEquals("000100015253" + "4D50", wad.titleId)
        assertEquals("RSMP", wad.gameId)
        assertNull(wad.icon)
    }

    @Test
    fun `reads the Wii U sample from a WUA and decodes its splashes on demand`() {
        val bytes = fixture("sample.wua")
        val info = assertNotNull(GameMeta.read(bytes, "sample.wua"))
        assertEquals(Platform.WiiU, info.platform)
        assertEquals("Sample Title", info.titles[Language.English]?.name)
        assertEquals("Example Games", info.titles[Language.English]?.publisher)
        assertEquals("サンプル", info.titles[Language.Japanese]?.name)
        assertEquals("Beispieltitel", info.titles[Language.German]?.name)
        assertEquals("00050000101ABCDE", info.titleId)
        assertEquals("WUP-P-SMPE", info.productCode)
        assertEquals(setOf(Region.Europe), info.regions)
        assertEquals(setOf(ImageKind.Icon, ImageKind.TvSplash, ImageKind.GamePadSplash, ImageKind.BootLogo), info.images)
        assertEquals(1234L, info.bootMovieSize)
        assertEquals(99L, info.bootSoundSize)
        assertEquals(4, info.icon?.width)
        val pad = assertNotNull(GameMeta.wuaImage(bytes, ImageKind.GamePadSplash))
        assertEquals(8, pad.width)
        assertEquals(5, pad.height)
        assertEquals(8 * 5 * 4, pad.pixels.size)
        assertNull(GameMeta.wuaImage(bytes, ImageKind.Banner))
    }

    private class MapFiles(val files: Map<String, ByteArray>, val broken: Boolean = false) : TitleFiles {
        override fun size(path: String): Long = if (broken) throw IOException("unreadable") else files[path]?.size?.toLong() ?: -1
        override fun read(path: String, maxBytes: Int): ByteArray? =
            if (broken) throw IOException("unreadable") else files[path]?.takeIf { it.size <= maxBytes }
    }

    @Test
    fun `reads a Wii U title from its files through a callback object`() {
        val meta = "<menu><longname_en>Folder Title</longname_en><title_id>0005000010AAAAAA</title_id></menu>"
        // A bottom-up 24 bit 2x1 TGA: BGR pixels (1,2,3) and (4,5,6).
        val tga = byteArrayOf(0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 24, 0, 1, 2, 3, 4, 5, 6)
        val files = MapFiles(mapOf("meta/meta.xml" to meta.toByteArray(), "meta/iconTex.tga" to tga))
        val info = assertNotNull(GameMeta.readTitle(files))
        assertEquals(Platform.WiiU, info.platform)
        assertEquals("Folder Title", info.title)
        assertEquals("0005000010AAAAAA", info.titleId)
        assertEquals(setOf(ImageKind.Icon), info.images)
        val icon = assertNotNull(info.icon)
        assertEquals(2, icon.width)
        assertEquals(listOf<Byte>(3, 2, 1, -1, 6, 5, 4, -1), icon.pixels.toList())
        assertEquals(icon, GameMeta.titleImage(files, ImageKind.Icon))
        assertNull(GameMeta.titleImage(files, ImageKind.TvSplash))
        assertNull(GameMeta.readTitle(MapFiles(emptyMap())))
        assertFailsWith<IOException> { GameMeta.readTitle(MapFiles(emptyMap(), broken = true)) }
        assertFailsWith<IOException> { GameMeta.titleImage(MapFiles(emptyMap(), broken = true), ImageKind.Icon) }
    }

    @Test
    fun `an unknown or empty file is null and a bad descriptor is an IO error`() {
        assertNull(GameMeta.read(ByteArray(0), "game.3ds"))
        assertNull(GameMeta.read(sample, "notes.txt"))
        assertFailsWith<IOException> { GameMeta.read(Int.MAX_VALUE, "game.3ds") }
    }

    @Test
    fun `statuses and tagged arm64 pointers are told apart`() {
        assertEquals(1, GameMeta.statusOf(-1))
        assertEquals(3, GameMeta.statusOf(-3))
        assertEquals(0, GameMeta.statusOf(0))
        assertNull(GameMeta.statusOf(0x7fff_1234_5670L))
        assertNull(GameMeta.statusOf(0xb400_0071_2345_6780uL.toLong()))
    }

    private fun scene(name: String) = File(System.getProperty("gamemeta.fixtures"), "scene/$name").readBytes()

    @Test
    fun `reads a banner scene from a bare CBMD`() {
        val s = assertNotNull(GameMeta.readBannerScene(scene("features/tex-etc1.cbmd")))
        assertEquals(1, s.models)
        assert(s.meshes >= 1)
        assert(s.textures.isNotEmpty())
        for ((_, image) in s.textures) assertEquals(image.width * image.height * 4, image.pixels.size)
        assertEquals(emptySet(), s.unsupported)
        // The scene JSON is the original implementation dump's scene, byte for byte.
        val golden = String(scene("features/tex-etc1.json")).trim()
        assert(golden.endsWith(",\"scene\":" + s.json + "}")) { "scene JSON differs from the original implementation dump" }
    }

    @Test
    fun `lists unsupported features, and files without a banner are null`() {
        assertEquals(setOf("dot3-combiner"), assertNotNull(GameMeta.readBannerScene(scene("cases/combiner-dot3.cgfx"))).unsupported)
        assertNull(GameMeta.readBannerScene(sample))
        assertNull(GameMeta.readBannerScene(ByteArray(0)))
        assertFailsWith<IOException> { GameMeta.readBannerScene(Int.MAX_VALUE) }
    }

    /** The original renderer's frame (fixtures/render), as top-down RGBA. */
    private fun golden(name: String): ByteArray {
        val image = javax.imageio.ImageIO.read(File(System.getProperty("gamemeta.fixtures"), "render/$name.png"))
        val out = ByteArray(image.width * image.height * 4)
        for (y in 0 until image.height) for (x in 0 until image.width) {
            val argb = image.getRGB(x, y)
            val o = (y * image.width + x) * 4
            out[o] = (argb shr 16).toByte(); out[o + 1] = (argb shr 8).toByte(); out[o + 2] = argb.toByte(); out[o + 3] = (argb ushr 24).toByte()
        }
        return out
    }

    @Test
    fun `renders a banner frame on the CPU like the original renderer`() {
        BannerRenderer.open(scene("features/bone-anim.cbmd"))!!.use { banner ->
            assertEquals(120f, banner.frames)
            val image = banner.render(60f, 400, 240)
            assertEquals(400 * 240 * 4, image.pixels.size)
            assert(image.pixels.contentEquals(golden("bone-anim-1"))) { "the CPU frame differs from the original implementation" }
            assertFailsWith<IllegalArgumentException> { banner.render(0f, 0, 240) }
        }
        BannerRenderer.open(scene("features/alpha-test.cbmd"))!!.use { assertEquals(0f, it.frames) }
        assertNull(BannerRenderer.open(sample))
    }

    @Test
    fun `the GL renderer is null without a current context`() {
        BannerRenderer.open(scene("features/bone-anim.cbmd"))!!.use { banner ->
            assertNull(banner.createGles())
            assertNull(banner.createGles("libno-such-gl.so"))
        }
    }

    private fun idFixture(name: String) = fixture("ids/$name")

    private fun md5(bytes: ByteArray) = java.security.MessageDigest.getInstance("MD5").digest(bytes).joinToString("") { "%02x".format(it) }

    private fun source(bytes: ByteArray) = object : FileSource {
        override val size = bytes.size.toLong()
        override fun read(offset: Long, length: Int): ByteArray? =
            if (offset < 0 || offset + length > size) null else bytes.copyOfRange(offset.toInt(), offset.toInt() + length)
    }

    @Test
    fun `reads PlayStation, PSP and GameCube ids from disc images`() {
        assertEquals("SLUS00003", GameMeta.readIds(idFixture("sample-ps2.iso"), "game.iso")?.serial)
        val psp = assertNotNull(GameMeta.readIds(idFixture("sample-psp.iso"), "psp.iso"))
        assertEquals(Triple("Sample Portable", "ULUS00001", "ULUS00001"), Triple(psp.pspTitle, psp.pspDiscId, psp.serial))
        assertEquals("ULUS00001", GameMeta.readIds(idFixture("sample.pbp"), "EBOOT.PBP")?.pspDiscId)
        val gc = assertNotNull(GameMeta.readIds(idFixture("sample-gc.iso"), "gc.iso"))
        assertEquals("GSMP01", gc.gameId)
        assertEquals("GSMP01", gc.primaryId)
    }

    @Test
    fun `reads CHD, zip, NSP and Xbox ids`() {
        assertEquals("0123456789abcdef0123456789abcdef01234567", GameMeta.readIds(idFixture("sample.chd"), "disc.chd")?.chdSha1)
        val zip = assertNotNull(GameMeta.readIds(idFixture("sample-rom.zip"), "rom.zip"))
        val crc = java.util.zip.CRC32().also { c -> c.update(ByteArray(4096) { (it * 31).toByte() }) }.value
        assertEquals("%08x".format(crc), zip.zipCrc)
        assertEquals(4096L, zip.zipSize)
        assertEquals("0100000000001000", GameMeta.readIds(idFixture("sample.nsp"), "game.nsp")?.titleId)
        assertEquals("4D530004", GameMeta.readIds(idFixture("sample.xbe"), "default.xbe")?.xboxId)
        assertNull(GameMeta.readIds(ByteArray(64), "thing.xyz"))
    }

    @Test
    fun `a cue sheet reads its first track through the opener, which is closed and may fail`() {
        val bin = idFixture("sample.bin")
        val opened = mutableListOf<String>()
        var closed = 0
        val opener = FileOpener { path ->
            opened += path
            if (path != "sample.bin") null else object : FileSource {
                override val size = bin.size.toLong()
                override fun read(offset: Long, length: Int) = source(bin).read(offset, length)
                override fun close() { closed++ }
            }
        }
        assertEquals("SLUS00003", GameMeta.readIds(idFixture("sample.cue"), "sample.cue", opener)?.serial)
        assertEquals(listOf("sample.bin"), opened)
        assertEquals(1, closed)
        assertNull(GameMeta.readIds(idFixture("sample.cue"), "sample.cue"))
        assertNull(GameMeta.readIds(idFixture("sample.cue"), "sample.cue", FileOpener { null }))
        val broken = FileOpener { object : FileSource { override val size = 1L shl 20; override fun read(offset: Long, length: Int): ByteArray = throw IOException("gone") } }
        assertNull(GameMeta.readIds(idFixture("sample.cue"), "sample.cue", broken))
    }

    @Test
    fun `the RA hasher streams whole-file platforms, SNES and NES, and refuses the rest`() {
        fun hash(name: String, bytes: ByteArray, chunk: Int): String? {
            val h = assertNotNull(GameMeta.raHasher(name, bytes.size.toLong()))
            bytes.asList().chunked(chunk).forEach { h.update(it.toByteArray()) }
            return h.finish()
        }
        val rom = idFixture("sample.sfc")
        assertEquals(md5(rom), hash("a.sfc", rom, 1000))
        assertEquals(md5(rom), hash("a.smc", ByteArray(512) + rom, 333)) // the 512-byte copier header is not hashed
        val nes = idFixture("sample.nes")
        assertEquals(md5(nes.copyOfRange(16, nes.size)), hash("a.nes", nes, 7777))
        assertNull(GameMeta.raHasher("a.iso", 10))
        assertFailsWith<IllegalStateException> { assertNotNull(GameMeta.raHasher("a.gb", 1)).also { it.finish() }.update(ByteArray(1)) }
    }

    @Test
    fun `reads a disc id from a header and refuses with a reason`() {
        assertEquals(DiscIdResult.Ok("GSMP01", DiscRegion.Pal, DiscSystem.GameCube), GameMeta.discId(idFixture("sample-gc.iso")))
        assertEquals(DiscIdResult.Refused(DiscIdRefusal.TooShort), GameMeta.discId(ByteArray(10)))
        assertEquals(DiscIdResult.Refused(DiscIdRefusal.BadMagic), GameMeta.discId(ByteArray(0x440)))
        val xbox = GameMeta.discId(idFixture("sample.xbe"))
        assertEquals(DiscSystem.Xbox, (xbox as DiscIdResult.Ok).system)
        assertEquals("4D530004", xbox.id)
        assertFailsWith<IOException> { GameMeta.discId(Int.MAX_VALUE) }
    }

    @Test
    fun `reads cartridge headers with version and the typed header as JSON`() {
        val cases = listOf(
            Triple("carts/sample.gb", Platform.GameBoy, 2),
            Triple("carts/sample.gbc", Platform.GameBoyColor, 2),
            Triple("carts/sample.gba", Platform.GameBoyAdvance, 1),
            Triple("carts/sample.z64", Platform.Nintendo64, 1),
            Triple("carts/sample.sfc", Platform.Snes, 0),
            Triple("carts/sample.md", Platform.MegaDrive, 2),
            Triple("carts/sample.sms", Platform.MasterSystem, 1),
            Triple("carts/sample.gg", Platform.GameGear, 1),
            Triple("carts/sample-gb.zip", Platform.GameBoy, 2),
        )
        for ((path, platform, version) in cases) {
            val info = assertNotNull(GameMeta.read(fixture(path), path.substringAfter('/')), path)
            assertEquals(platform, info.platform, path)
            assertEquals(version, info.version, path)
            assertNotNull(info.cartHeaderJson, path)
        }
        val gb = assertNotNull(GameMeta.read(fixture("carts/sample.gb"), "sample.gb"))
        assertEquals("SYNTH TEST", gb.title)
        assertEquals("01", gb.makerCode)
        assertEquals(setOf(Region.NorthAmerica, Region.Europe), gb.regions)
        assert(gb.cartHeaderJson!!.startsWith("{\"family\":\"gb\",\"title\":\"SYNTH TEST\"")) { gb.cartHeaderJson!! }
        val nes = assertNotNull(GameMeta.read(fixture("carts/sample.nes"), "sample.nes"))
        assertEquals(Platform.Nes, nes.platform)
        assertNull(nes.version)
        assertNull(assertNotNull(GameMeta.read(sample, "sample.3ds")).cartHeaderJson)
    }

    private fun wiiBanner(name: String) = File(System.getProperty("gamemeta.fixtures"), "wii-banner/$name").readBytes()

    private fun wiiGolden(name: String): ByteArray {
        val image = javax.imageio.ImageIO.read(File(System.getProperty("gamemeta.fixtures"), "wii-banner/$name.png"))
        val out = ByteArray(image.width * image.height * 4)
        for (y in 0 until image.height) for (x in 0 until image.width) {
            val argb = image.getRGB(x, y)
            val o = (y * image.width + x) * 4
            out[o] = (argb shr 16).toByte(); out[o + 1] = (argb shr 8).toByte(); out[o + 2] = argb.toByte(); out[o + 3] = (argb ushr 24).toByte()
        }
        return out
    }

    @Test
    fun `draws a Wii channel banner like the original renderer`() {
        WiiBanner.open(wiiBanner("banner608.bin"))!!.use { banner ->
            assertEquals(40, banner.startFrames)
            assertEquals(120, banner.loopFrames)
            assertEquals(mapOf("wnd1" to 1, "txt1" to 1), banner.skipped)
            assertEquals(emptyList(), banner.missingTextures)
            assertEquals(608 to 456, banner.size())
            assert(banner.poster().pixels.contentEquals(wiiGolden("banner608-poster"))) { "the poster differs from the original implementation" }
            assert(banner.render(100f, 0.5f).pixels.contentEquals(wiiGolden("banner608-4"))) { "a half scale loop frame differs from the original implementation" }
            assertFailsWith<IllegalArgumentException> { banner.render(0f, 0f) }
        }
        WiiBanner.open(wiiBanner("missing-texture.bin"))!!.use { assertEquals(listOf("gone.tpl"), it.missingTextures) }
        assertNull(WiiBanner.open(sample))
    }
}
