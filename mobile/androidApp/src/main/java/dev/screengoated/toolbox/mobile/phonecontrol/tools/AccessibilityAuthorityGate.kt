package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull

internal fun JsonObject.confirmationOrNull(): Boolean? {
    val value = get("confirm") ?: return false
    val primitive = value as? JsonPrimitive ?: return null
    if (primitive.isString) return null
    return primitive.booleanOrNull
}
