// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

/** The platform a file belongs to; [code] matches the C API's `GmPlatform`. */
enum class Platform(val code: Int) {
    DS(1), DSi(2), ThreeDS(3), GameCube(4), Wii(5), WiiU(6),
    GameBoy(7), GameBoyColor(8), GameBoyAdvance(9), Nintendo64(10), Snes(11), Nes(12), MegaDrive(13), Sega32x(14),
    MasterSystem(15), GameGear(16),
}

/** Title languages, in 3DS SMDH slot order. */
enum class Language { Japanese, English, French, German, Italian, Spanish, ChineseSimplified, Korean, Dutch, Portuguese, Russian, ChineseTraditional }

/** Regions, in 3DS SMDH region-lockout bit order. */
enum class Region { Japan, NorthAmerica, Europe, Australia, China, Korea, Taiwan }

/** Age-rating boards by their slot in the 3DS SMDH ratings table. */
enum class RatingBoard(val slot: Int) { CERO(0), ESRB(1), USK(3), PEGI(4), PEGI_PRT(6), BBFC(7), COB(8), GRB(9), CGSRR(10) }

/** The pictures a GameCube, Wii or Wii U file can hold; [bit] is the position in the C API's `GmImage` mask. */
enum class ImageKind(val bit: Int) {
    /** The Wii U `iconTex.tga` (128x128). */ Icon(0),
    /** The GameCube `opening.bnr` banner (96x32). */ Banner(1),
    /** A Wii disc's title logo (the wide picture with transparency in its `icon.bin`). */ Logo(2),
    /** The Wii U TV boot splash (1280x720). */ TvSplash(3),
    /** The Wii U GamePad boot splash (854x480). */ GamePadSplash(4),
    /** The Wii U `bootLogoTex.tga`. */ BootLogo(5),
}

/** One step of a DSi animated icon: show [frame] for [durationFrames] sixtieths of a second. */
data class IconStep(val frame: RgbaImage, val durationFrames: Int)

/** One language's title: [name] is what a list shows (the long title, else the short one, lines joined with a space). */
data class Title(val name: String, val shortName: String, val publisher: String?)

/** A byte range of the file. */
data class Extent(val offset: Long, val size: Long)

/** Top-down, 4 bytes per pixel in R, G, B, A order. */
class RgbaImage(val width: Int, val height: Int, val pixels: ByteArray) {
    init {
        require(pixels.size == width * height * 4) { "pixel buffer does not match ${width}x$height" }
    }

    override fun equals(other: Any?): Boolean =
        other is RgbaImage && other.width == width && other.height == height && other.pixels.contentEquals(pixels)

    override fun hashCode(): Int = 31 * (31 * width + height) + pixels.contentHashCode()

    override fun toString(): String = "RgbaImage(${width}x$height)"
}

/** What [GameMeta.read] found in one file. Every field but [platform] may be empty. */
data class GameInfo(
    val platform: Platform,
    val titles: Map<Language, Title>,
    /** The main icon (DS 32x32, 3DS 48x48; the GameCube banner, Wii title logo or Wii U icon for those). */
    val icon: RgbaImage?,
    /** The small icon (3DS 24x24). */
    val smallIcon: RgbaImage?,
    /** 3DS, Wii U, Wii WAD: the title id, 16 upper-case hex digits. */
    val titleId: String?,
    /** 3DS `CTR-P-XXXX`, Wii U `WUP-P-XXXX`. */
    val productCode: String?,
    /** DS 4-character game code; GameCube and Wii 6-character game id; a Wii WAD's 4 ASCII characters. */
    val gameId: String?,
    /** DS 2-character maker code; GameCube and Wii: the game id's last two characters. */
    val makerCode: String?,
    /** The regions the file is locked to; empty when it names none or is region free. */
    val regions: Set<Region>,
    val regionFree: Boolean,
    /** Active age ratings: board to minimum age (0 for no age restriction). */
    val ratings: Map<RatingBoard, Int>,
    /** 3DS: where the ExeFS banner sits (located, never read); DS: the banner block. */
    val banner: Extent?,
    /** A DSi animated icon's steps; empty for a still icon. */
    val animatedIcon: List<IconStep>,
    /** GameCube, Wii, Wii U: the pictures the file holds ([icon] is the small one read with the titles). */
    val images: Set<ImageKind>,
    /** Wii U: the size of `bootMovie.h264`, when the title has one. */
    val bootMovieSize: Long?,
    /** Wii U: the size of `bootSound.btsnd`, when the title has one. */
    val bootSoundSize: Long?,
    /** GameCube and Wii: the disc header's own title, for a disc with no banner text. */
    val headerTitle: String?,
    /** Cartridge consoles (Game Boy to Game Gear): the header's version byte or revision, when it has one. */
    val version: Int?,
    /**
     * Cartridge consoles: the typed header as one JSON object (`{"family":"gb","title":"...",...}`; every decoded field,
     * raw codes included, `null` for one the header lacks). Null for any other file.
     */
    val cartHeaderJson: String?,
) {
    /** English if present, else the first language that has one; null when blank. */
    val title: String? get() = (titles[Language.English] ?: titles.entries.minByOrNull { it.key }?.value)?.name?.takeIf { it.isNotBlank() }
}
