// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

package gamemeta

/**
 * A 3DS banner parsed into a renderer-neutral scene ([GameMeta.readBannerScene]). [json] is the whole scene (models, meshes, materials,
 * lights, cameras, animations; textures as size and SHA-256 of their pixels) as deterministic JSON with sorted keys; the counts and
 * [textures] are the parts a host reads without parsing it. [unsupported] lists features the file uses that the scene does not model.
 */
data class BannerSceneInfo(
    val json: String,
    val models: Int,
    val meshes: Int,
    /** By name; top-down RGBA. */
    val textures: Map<String, RgbaImage>,
    val lights: Int,
    val cameras: Int,
    val skeletalAnimations: Int,
    val materialAnimations: Int,
    val visibilityAnimations: Int,
    val unsupported: Set<String>,
)
