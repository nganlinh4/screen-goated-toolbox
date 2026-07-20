package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlGenerationId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlOutputChunk
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlSnapshotGeneration
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTargetId
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTargetIdentity
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.longOrNull

internal fun JsonObject.outputChunk(): PhoneControlOutputChunk = PhoneControlOutputChunk(
    generation = requiredGeneration("generation"),
    sequence = optionalLong("sequence"),
)

internal fun JsonObject.targetIdentity(): PhoneControlTargetIdentity = PhoneControlTargetIdentity(
    id = PhoneControlTargetId(requiredString("targetId")),
    snapshotGeneration = PhoneControlSnapshotGeneration(requiredLong("snapshotGeneration")),
)

internal fun JsonObject.certainty(): PhoneControlEffectCertainty = when {
    optionalBoolean("effectVerified") == true -> PhoneControlEffectCertainty.VERIFIED
    optionalBoolean("effectMayHaveOccurred") == true ->
        PhoneControlEffectCertainty.MAY_HAVE_OCCURRED
    else -> PhoneControlEffectCertainty.PROVEN_NO_EFFECT
}

internal fun JsonObject.requiredGeneration(field: String): PhoneControlGenerationId =
    PhoneControlGenerationId(requiredLong(field))

internal fun JsonObject.optionalGeneration(field: String): PhoneControlGenerationId? =
    optionalLong(field)?.let(::PhoneControlGenerationId)

internal fun contractElement(value: Any?): JsonElement = when (value) {
    null -> JsonNull
    is Boolean -> JsonPrimitive(value)
    is Int -> JsonPrimitive(value)
    is Long -> JsonPrimitive(value)
    is String -> JsonPrimitive(value)
    else -> error("Unsupported fixture value: $value")
}

internal fun JsonObject.requiredString(field: String): String =
    getValue(field).jsonPrimitive.content

internal fun JsonObject.optionalString(field: String): String? =
    (get(field) as? JsonPrimitive)?.contentOrNull

internal fun JsonObject.requiredLong(field: String): Long = getValue(field).jsonPrimitive.long

internal fun JsonObject.optionalLong(field: String): Long? =
    (get(field) as? JsonPrimitive)?.longOrNull

internal fun JsonObject.requiredBoolean(field: String): Boolean =
    getValue(field).jsonPrimitive.boolean

internal fun JsonObject.optionalBoolean(field: String): Boolean? =
    (get(field) as? JsonPrimitive)?.booleanOrNull
