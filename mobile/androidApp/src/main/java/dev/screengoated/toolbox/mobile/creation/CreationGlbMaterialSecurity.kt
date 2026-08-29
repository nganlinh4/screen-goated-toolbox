package dev.screengoated.toolbox.mobile.creation

import org.json.JSONArray
import org.json.JSONObject

internal fun validateGlbMaterialTextures(
    document: JSONObject,
    texturePixels: List<Long>,
    initialReferencedPixels: Long,
) {
    val materials = document.optionalArray("materials")
    var referencedPixels = initialReferencedPixels
    repeat(materials.length()) { index ->
        val material = materials.requiredObject(index)
        validateGlbMaterialNumericValues(material)
        listOf("normalTexture", "occlusionTexture", "emissiveTexture").forEach { key ->
            referencedPixels = chargeGlbTextureInfo(
                material.opt(key),
                1,
                texturePixels,
                referencedPixels,
            )
        }
        material.optJSONObject("pbrMetallicRoughness")?.let { pbr ->
            referencedPixels = chargeGlbTextureInfo(
                pbr.opt("baseColorTexture"),
                1,
                texturePixels,
                referencedPixels,
            )
            referencedPixels = chargeGlbTextureInfo(
                pbr.opt("metallicRoughnessTexture"),
                2,
                texturePixels,
                referencedPixels,
            )
        }
        val extensions = material.optJSONObject("extensions") ?: return@repeat
        extensions.keys().forEach { name ->
            val body = requireNotNull(extensions.optJSONObject(name)) {
                "The model result has invalid material metadata"
            }
            val slots = when (name) {
                "KHR_materials_clearcoat" -> listOf(
                    "clearcoatTexture" to 1,
                    "clearcoatRoughnessTexture" to 1,
                    "clearcoatNormalTexture" to 1,
                )
                "KHR_materials_iridescence" -> listOf(
                    "iridescenceTexture" to 1,
                    "iridescenceThicknessTexture" to 1,
                )
                "KHR_materials_sheen" -> listOf(
                    "sheenColorTexture" to 1,
                    "sheenRoughnessTexture" to 1,
                )
                "KHR_materials_specular" -> listOf(
                    "specularTexture" to 1,
                    "specularColorTexture" to 1,
                )
                "KHR_materials_transmission" -> listOf("transmissionTexture" to 1)
                "KHR_materials_volume" -> listOf("thicknessTexture" to 1)
                "KHR_materials_anisotropy" -> listOf("anisotropyTexture" to 1)
                "EXT_materials_bump" -> listOf("bumpTexture" to 1)
                "KHR_materials_dispersion",
                "KHR_materials_emissive_strength",
                "KHR_materials_ior",
                "KHR_materials_unlit",
                -> emptyList()
                else -> error("The model result has invalid material metadata")
            }
            slots.forEach { (key, assignments) ->
                referencedPixels = chargeGlbTextureInfo(
                    body.opt(key),
                    assignments,
                    texturePixels,
                    referencedPixels,
                )
            }
        }
    }
}

internal fun validateGlbMaterialNumericValues(value: Any?) {
    validateGlbMaterialValue(value, null)
}

private fun validateGlbMaterialValue(value: Any?, field: String?) {
    require(field !in GLB_MATERIAL_NUMBER_FIELDS || value is Number) {
        "The model result has invalid material values"
    }
    val expectedArrayLength = GLB_MATERIAL_ARRAY_LENGTHS[field]
    require(expectedArrayLength == null || value is JSONArray) {
        "The model result has invalid material values"
    }
    require(field != "scale" || value is Number || value is JSONArray) {
        "The model result has invalid material values"
    }
    require(field !in setOf("name", "alphaMode") || value is String) {
        "The model result has invalid material values"
    }
    require(field != "doubleSided" || value is Boolean) {
        "The model result has invalid material values"
    }
    when (value) {
        is JSONObject -> value.keys().forEach { name ->
            if (name != "extras") validateGlbMaterialValue(value.opt(name), name)
        }
        is JSONArray -> {
            val expected = expectedArrayLength ?: if (field == "scale") 2 else null
            require(expected != null && value.length() == expected) {
                "The model result has invalid material values"
            }
            repeat(value.length()) { index ->
                require(value.opt(index) is Number) {
                    "The model result has invalid material values"
                }
                validateGlbMaterialValue(value.opt(index), null)
            }
        }
        is Number -> require(
            value.toDouble().isFinite() &&
                kotlin.math.abs(value.toDouble()) <=
                CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE
        ) { "The model result has invalid material values" }
        is String -> require(
            field == "name" ||
                field == "alphaMode" && value in setOf("OPAQUE", "MASK", "BLEND")
        ) { "The model result has invalid material values" }
        is Boolean -> require(field == "doubleSided") {
            "The model result has invalid material values"
        }
        null, JSONObject.NULL -> error("The model result has invalid material values")
    }
}

private fun chargeGlbTextureInfo(
    raw: Any?,
    assignments: Int,
    texturePixels: List<Long>,
    currentPixels: Long,
): Long {
    if (raw == null) return currentPixels
    require(raw != JSONObject.NULL) { "The model result has invalid texture metadata" }
    val info = raw as? JSONObject ?: error("The model result has invalid texture metadata")
    val index = info.requiredMaterialIndex("index", texturePixels.size)
    val texCoord = info.optionalTextureCoordinate("texCoord", 0)
    var cloneCount = if (texCoord > 0) 1 else 0
    if (info.has("extensions")) {
        val extensions = requireNotNull(info.optJSONObject("extensions")) {
            "The model result has invalid texture metadata"
        }
        extensions.keys().forEach { name ->
            require(name == "KHR_texture_transform") {
                "The model result has invalid texture metadata"
            }
            val transform = requireNotNull(extensions.optJSONObject(name)) {
                "The model result has invalid texture metadata"
            }
            require(
                transform.keys().asSequence().all {
                    it in setOf("offset", "rotation", "scale", "texCoord")
                }
            ) { "The model result has invalid texture transform" }
            var transformClones = false
            if (transform.has("texCoord")) {
                transformClones = transform.optionalTextureCoordinate("texCoord", texCoord) != texCoord
            }
            listOf("offset", "scale").forEach { key ->
                if (transform.has(key)) {
                    validateTextureTransformArray(transform, key)
                    transformClones = true
                }
            }
            if (transform.has("rotation")) {
                validateTextureTransformNumber(transform.opt("rotation"))
                transformClones = true
            }
            if (transformClones) cloneCount += 1
        }
    }
    if (cloneCount == 0) return currentPixels
    val clonePixels = checkedMaterialMultiply(
        texturePixels[index],
        Math.multiplyExact(assignments, cloneCount).toLong(),
    )
    return checkedMaterialAdd(currentPixels, clonePixels).also {
        require(it <= CREATION_GLB_MAXIMUM_REFERENCED_TEXTURE_PIXELS) {
            "The model result contains too much texture data"
        }
    }
}

private fun JSONObject.requiredMaterialIndex(name: String, size: Int): Int {
    require(size > 0)
    val value = opt(name)
    require(value is Number) { "The model result has invalid texture metadata" }
    return value.toString().toIntOrNull()?.also { require(it in 0 until size) }
        ?: error("The model result has invalid texture metadata")
}

private fun JSONObject.optionalTextureCoordinate(name: String, fallback: Int): Int {
    if (!has(name)) return fallback
    val value = opt(name)
    require(value is Number) { "The model result has invalid texture metadata" }
    return value.toString().toIntOrNull()?.also { require(it in 0..3) }
        ?: error("The model result has invalid texture metadata")
}

private fun validateTextureTransformArray(value: JSONObject, name: String) {
    val values = requireNotNull(value.optJSONArray(name)) {
        "The model result has invalid texture transform"
    }
    require(values.length() == 2) { "The model result has invalid texture transform" }
    repeat(2) { validateTextureTransformNumber(values.opt(it)) }
}

private fun validateTextureTransformNumber(value: Any?) {
    require(
        value is Number &&
            value.toDouble().isFinite() &&
            kotlin.math.abs(value.toDouble()) <= CREATION_GLB_MAXIMUM_ABSOLUTE_RENDERER_VALUE
    ) { "The model result has invalid texture transform" }
}

private fun checkedMaterialAdd(left: Long, right: Long): Long =
    runCatching { Math.addExact(left, right) }
        .getOrElse { error("The model result material metadata is too large") }

private fun checkedMaterialMultiply(left: Long, right: Long): Long =
    runCatching { Math.multiplyExact(left, right) }
        .getOrElse { error("The model result material metadata is too large") }

private val GLB_MATERIAL_NUMBER_FIELDS = setOf(
    "metallicFactor", "roughnessFactor", "strength", "alphaCutoff", "emissiveStrength",
    "clearcoatFactor", "clearcoatRoughnessFactor", "dispersion", "iridescenceFactor",
    "iridescenceIor", "iridescenceThicknessMinimum", "iridescenceThicknessMaximum",
    "sheenRoughnessFactor", "transmissionFactor", "thicknessFactor", "attenuationDistance",
    "ior", "specularFactor", "bumpFactor", "anisotropyStrength", "anisotropyRotation",
    "index", "texCoord", "rotation",
)
private val GLB_MATERIAL_ARRAY_LENGTHS = mapOf(
    "baseColorFactor" to 4,
    "emissiveFactor" to 3,
    "sheenColorFactor" to 3,
    "specularColorFactor" to 3,
    "attenuationColor" to 3,
    "offset" to 2,
)
