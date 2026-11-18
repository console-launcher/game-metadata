// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

import java.io.File
import java.net.URL
import java.net.URLClassLoader
import kotlin.test.Test
import kotlin.test.assertEquals

/**
 * A host jar carries the library inside it, and one JVM can hold several class loaders that each load these classes
 * (Robolectric sandboxes, test runners, plugin hosts). Each loader must get its own copy of the library: a second
 * extraction over the first one's mapped file crashes the JVM.
 */
class GameMetaLoadTest {
    private fun location(c: Class<*>): URL = c.protectionDomain.codeSource.location

    @Test
    fun `class loaders that each load the classes from a host jar do not disturb one another`() {
        val library = File(System.getProperty("gamemeta.library"))
        val os = System.getProperty("os.name").lowercase().let { if (it.startsWith("mac")) "macos" else if (it.startsWith("win")) "windows" else "linux" }
        val arch = System.getProperty("os.arch").let { if (it == "aarch64" || it == "arm64") "aarch64" else "x86_64" }
        val resources = File.createTempFile("gamemeta-res", "").also { it.delete(); it.mkdirs() }
        val inJar = File(resources, "gamemeta/native/$os-$arch/${library.name}").also { it.parentFile.mkdirs() }
        library.copyTo(inJar)
        val urls = arrayOf(resources.toURI().toURL(), location(GameMeta::class.java), location(Unit::class.java))
        fun newLoader() = URLClassLoader(urls, ClassLoader.getPlatformClassLoader())
        fun discId(loader: ClassLoader): Any? {
            val type = loader.loadClass("gamemeta.GameMeta")
            return type.getMethod("discId", ByteArray::class.java).invoke(type.getField("INSTANCE").get(null), ByteArray(64))?.toString()
        }
        val saved = System.getProperty("gamemeta.library")
        System.clearProperty("gamemeta.library")
        try {
            val first = newLoader()
            val second = newLoader()
            val expected = discId(first)
            assertEquals(expected, discId(second))
            assertEquals(expected, discId(first))
            assertEquals(expected, discId(second))
        } finally {
            System.setProperty("gamemeta.library", saved)
            inJar.delete()
            inJar.parentFile.delete()
            inJar.parentFile.parentFile.delete()
            inJar.parentFile.parentFile.parentFile.delete()
            resources.delete()
        }
    }
}
