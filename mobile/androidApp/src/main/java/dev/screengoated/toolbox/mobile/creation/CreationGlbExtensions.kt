package dev.screengoated.toolbox.mobile.creation

import org.json.JSONArray
import org.json.JSONObject

internal fun validateCreationGlbExtensions(document: JSONObject) {
    val used = creationGlbExtensionDeclarations(document, "extensionsUsed")
    val required = creationGlbExtensionDeclarations(document, "extensionsRequired")
    require(required.all(used::contains)) {
        "The model result has undeclared required extensions"
    }
    require(used.all(CREATION_GLB_ALLOWED_EXTENSIONS::contains)) {
        "The model result uses an unsupported extension"
    }
    val observed = mutableSetOf<String>()
    validateCreationGlbNestedExtensions(document, used, observed)
    require(observed == used) { "The model result has unused extension declarations" }
}

private fun creationGlbExtensionDeclarations(
    document: JSONObject,
    name: String,
): Set<String> {
    if (!document.has(name)) return emptySet()
    val values = requireNotNull(document.optJSONArray(name)) {
        "The model result has invalid extension declarations"
    }
    val declarations = mutableSetOf<String>()
    for (index in 0 until values.length()) {
        val value = values.opt(index)
        require(value is String && value.isNotBlank() && declarations.add(value)) {
            "The model result has invalid extension declarations"
        }
    }
    return declarations
}

private fun validateCreationGlbNestedExtensions(
    value: Any?,
    declared: Set<String>,
    observed: MutableSet<String>,
) {
    when (value) {
        is JSONObject -> value.keys().forEach { key ->
            val child = value.opt(key)
            if (key == "extensions") {
                require(child is JSONObject) { "The model result has invalid extensions" }
                child.keys().forEach { extension ->
                    require(
                        extension in declared &&
                            extension in CREATION_GLB_ALLOWED_EXTENSIONS &&
                            child.opt(extension) is JSONObject
                    ) { "The model result uses an undeclared or unsupported extension" }
                    observed += extension
                    validateCreationGlbNestedExtensions(
                        child.opt(extension),
                        declared,
                        observed,
                    )
                }
            } else if (key != "extras") {
                validateCreationGlbNestedExtensions(child, declared, observed)
            }
        }
        is JSONArray -> {
            for (index in 0 until value.length()) {
                validateCreationGlbNestedExtensions(value.opt(index), declared, observed)
            }
        }
    }
}

internal val CREATION_GLB_ALLOWED_EXTENSIONS = setOf(
    "EXT_materials_bump",
    "EXT_texture_webp",
    "KHR_materials_anisotropy",
    "KHR_materials_clearcoat",
    "KHR_materials_dispersion",
    "KHR_materials_emissive_strength",
    "KHR_materials_ior",
    "KHR_materials_iridescence",
    "KHR_materials_sheen",
    "KHR_materials_specular",
    "KHR_materials_transmission",
    "KHR_materials_unlit",
    "KHR_materials_volume",
    "KHR_texture_transform",
)
