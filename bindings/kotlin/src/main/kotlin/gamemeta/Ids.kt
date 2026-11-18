// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

import java.io.Closeable
import java.io.IOException

/**
 * What [GameMeta.readIds] found by a file's name and contents; each field is null when the file has no such id. The same ids
 * a launcher uses to match a game in a database: PlayStation serials, GameCube/Wii and Xbox ids, CHD and zip checksums.
 * The RetroAchievements MD5 reads the whole file, so it is not here: feed a [RaHasher] ([GameMeta.raHasher]).
 */
data class Ids(
    /** PlayStation 1/2 serial (`SLUS-00001` style, from `SYSTEM.CNF` or a cue sheet's first data track), or a PSP disc id from an `EBOOT.PBP`. */
    val serial: String?,
    /** The GameCube/Wii 6-character game id, read from a disc image or its container header. */
    val gameId: String?,
    /** The title id of a WAD, 3DS, Wii U or Switch (NSP) title. */
    val titleId: String?,
    /** The raw SHA-1 (hex) a CHD v5 header records. */
    val chdSha1: String?,
    /** The CRC-32 (8 hex digits) of the single ROM in a zip, from its central directory. */
    val zipCrc: String?,
    /** The uncompressed size of that ROM. */
    val zipSize: Long?,
    /** `TITLE` of a PSP game's `PARAM.SFO`. */
    val pspTitle: String?,
    /** `DISC_ID` of a PSP game's `PARAM.SFO`. */
    val pspDiscId: String?,
    /** The Xbox id from an XBE's certificate (title id and region). */
    val xboxId: String?,
    /** The one id a launcher shows: [serial], else [gameId], else [titleId]. */
    val primaryId: String?,
)

/** A file beside the one being read, for formats that point at siblings (a cue sheet's `.bin`, a Wii U title's `meta.xml`). */
interface FileSource : Closeable {
    /** The file's size in bytes. */
    val size: Long

    /** Exactly [length] bytes from [offset], or null when that range is not inside the file. Throw an `IOException` on a failed read. */
    fun read(offset: Long, length: Int): ByteArray?

    override fun close() {}
}

/**
 * Opens files beside the game file by path relative to its folder (`game (Track 1).bin`, `../meta/meta.xml`); null when there
 * is none. The reader closes each [FileSource] it is handed.
 */
fun interface FileOpener {
    /** The file at [path], or null. */
    fun open(path: String): FileSource?
}

/** Why [GameMeta.discId] gave no id. */
enum class DiscIdRefusal { TooShort, BadMagic, UnsupportedContainer, Unreadable }

/** The region a disc id's last letter says. */
enum class DiscRegion { NtscU, NtscJ, Pal, NtscK, Unknown }

/** The console of a disc. */
enum class DiscSystem { GameCube, Wii, Xbox }

/** The result of [GameMeta.discId]: the id with its region and console, or why there is none. */
sealed interface DiscIdResult {
    /** A disc id. */
    data class Ok(val id: String, val region: DiscRegion, val system: DiscSystem) : DiscIdResult

    /** No id: [reason]. */
    data class Refused(val reason: DiscIdRefusal) : DiscIdResult
}

/**
 * The RetroAchievements MD5 of a file fed in chunks, so a large ROM is never held in memory. Create one with [GameMeta.raHasher]
 * (null for a file this does not hash), call [update] for each chunk, then [finish].
 */
class RaHasher internal constructor(private var handle: Long) : Closeable {
    /** Feeds the first [length] bytes of [bytes]. */
    fun update(bytes: ByteArray, length: Int = bytes.size) {
        check(handle != 0L) { "game-metadata: hasher closed" }
        GameMeta.raUpdate(handle, bytes, length)
    }

    /** The lower-case hex MD5, or null when this file is not hashed this way (an NES file with no content, say). Closes the hasher. */
    fun finish(): String? {
        check(handle != 0L) { "game-metadata: hasher closed" }
        return try {
            GameMeta.raFinish(handle)
        } finally {
            close()
        }
    }

    override fun close() {
        if (handle != 0L) GameMeta.raFree(handle)
        handle = 0
    }
}

internal fun unpackDiscId(numbers: LongArray): DiscIdResult {
    if (numbers[0] != 0L) {
        return DiscIdResult.Refused(
            when (numbers[0].toInt()) {
                1 -> DiscIdRefusal.TooShort
                2 -> DiscIdRefusal.BadMagic
                3 -> DiscIdRefusal.UnsupportedContainer
                else -> DiscIdRefusal.Unreadable
            },
        )
    }
    val id = buildString { for (shift in 56 downTo 0 step 8) ((numbers[3] ushr shift) and 0xff).toInt().takeIf { it != 0 }?.let { append(it.toChar()) } }
    return DiscIdResult.Ok(id, DiscRegion.entries[numbers[1].toInt()], DiscSystem.entries[numbers[2].toInt()])
}

internal fun ioError(): Nothing = throw IOException("game-metadata: the file could not be read")
