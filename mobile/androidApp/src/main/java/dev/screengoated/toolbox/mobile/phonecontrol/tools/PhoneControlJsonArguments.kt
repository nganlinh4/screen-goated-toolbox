package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull

internal fun JsonObject.string(name: String): String? =
    (get(name) as? JsonPrimitive)?.takeIf(JsonPrimitive::isString)?.contentOrNull

internal fun JsonObject.int(name: String): Int? = primitive(name)?.intOrNull

internal fun JsonObject.number(name: String): Double? =
    primitive(name)?.contentOrNull?.toDoubleOrNull()

internal fun JsonObject.boolean(name: String): Boolean? = primitive(name)?.booleanOrNull

// A wrong-shaped value (object or array) is an absent argument, never a throw:
// a throw would surface as an uncertain effect for mutating handlers.
private fun JsonObject.primitive(name: String): JsonPrimitive? = get(name) as? JsonPrimitive
