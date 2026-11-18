// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The Kotlin API of game-metadata. One source set serves both artifacts scripts/release builds: the Android AAR
// (classes + per-ABI .so) and the desktop host jar (classes + the host .so under gamemeta/native/<os>-<arch>/).
plugins {
    kotlin("jvm") version "2.3.0"
}

group = "gamemeta"
version = "0.7.1"

kotlin {
    jvmToolchain(17)
}

dependencies {
    testImplementation(kotlin("test"))
    testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher:1.11.4")
}

// The host library and the fixtures directory come from the Rust build: scripts/kotlin-test passes their paths.
tasks.test {
    useJUnitPlatform()
    systemProperty("gamemeta.library", providers.gradleProperty("gamemetaLibrary").getOrElse(""))
    systemProperty("gamemeta.fixtures", providers.gradleProperty("gamemetaFixtures").getOrElse(""))
}

tasks.jar {
    archiveBaseName.set("game-metadata-classes")
    // MPL-2.0 text and third-party notices travel inside the classes jar, hence inside the AAR and the host jar too.
    into("META-INF") { from(rootDir.resolve("../../LICENSE"), rootDir.resolve("../../NOTICE.md")) }
}
