package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive

internal fun JsonObject.string(name: String): String? =
    (get(name) as? JsonPrimitive)?.takeIf(JsonPrimitive::isString)?.contentOrNull

internal fun JsonObject.int(name: String): Int? = get(name)?.jsonPrimitive?.intOrNull

internal fun JsonObject.number(name: String): Double? =
    get(name)?.jsonPrimitive?.contentOrNull?.toDoubleOrNull()

internal fun JsonObject.boolean(name: String): Boolean? = get(name)?.jsonPrimitive?.booleanOrNull
