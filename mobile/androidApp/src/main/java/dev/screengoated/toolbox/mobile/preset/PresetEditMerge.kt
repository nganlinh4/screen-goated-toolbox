package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.shared.preset.Preset
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

private val editJson = Json { encodeDefaults = true }

/** Preserve current fields that this particular editor action did not change. */
fun mergePresetEdit(current: Preset, previous: Preset, updated: Preset): Preset {
    require(current.id == previous.id && previous.id == updated.id)
    val serializer = Preset.serializer()
    val old = editJson.encodeToJsonElement(serializer, previous).jsonObject
    val next = editJson.encodeToJsonElement(serializer, updated).jsonObject
    val merged = editJson.encodeToJsonElement(serializer, current).jsonObject.toMutableMap()
    for ((key, value) in next) {
        if (old[key] != value) merged[key] = value
    }
    return editJson.decodeFromJsonElement(serializer, JsonObject(merged))
}
