package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull

internal fun GeminiLiveFunctionCall.structuralDispatchLog(
    generation: Long,
    turnId: Long,
): String {
    val argumentKeys = (args as? JsonObject)?.keys?.sorted().orEmpty()
    val argumentCount = argumentKeys.size
    val argumentBytes = args.toString().toByteArray(Charsets.UTF_8).size
    return "tool_dispatched turn_id=$turnId job_id=${id.diagnosticIdentity()} name=$name " +
        "generation=$generation argument_fields=$argumentCount " +
        "argument_keys=${argumentKeys.joinToString(":").ifEmpty { "none" }} " +
        "argument_bytes=$argumentBytes"
}

internal fun PhoneControlCompletedTool.structuralReceiptLog(): String {
    val response = result.response
    return buildString {
        append("tool_receipt")
        field("turn_id", request.turnId)
        field("job_id", request.id.diagnosticIdentity())
        field("name", request.name)
        field("generation", request.generation)
        field("elapsed_ms", elapsedMs)
        field("code", response.string("code") ?: "unknown")
        field("certainty", result.certainty.name.lowercase())
        OPTIONAL_STRING_FIELDS.forEach { key -> response.string(key)?.let { field(key, it) } }
        OPTIONAL_LONG_FIELDS.forEach { key -> response.long(key)?.let { field(key, it) } }
        OPTIONAL_BOOLEAN_FIELDS.forEach { key -> response.boolean(key)?.let { field(key, it) } }
        (response["required_user_step"] as? JsonObject)
            ?.string("code")
            ?.let { field("required_user_step", it) }
        (response["elements"] as? JsonArray)?.size?.let { field("element_count", it) }
        (response["windows"] as? JsonArray)?.size?.let { field("window_count", it) }
    }
}

internal fun JsonObject.stateReconciled(): Boolean =
    (get("state_reconciled") as? JsonPrimitive)?.booleanOrNull == true

private fun StringBuilder.field(name: String, value: Any) {
    append(' ').append(name).append('=').append(value)
}

private fun JsonObject.string(name: String): String? =
    (get(name) as? JsonPrimitive)?.contentOrNull

private fun JsonObject.long(name: String): Long? =
    (get(name) as? JsonPrimitive)?.longOrNull

private fun JsonObject.boolean(name: String): Boolean? =
    (get(name) as? JsonPrimitive)?.booleanOrNull

private fun String.diagnosticIdentity(): String {
    if (
        firstOrNull()?.isLetter() == true &&
        length <= MAX_DIAGNOSTIC_IDENTITY &&
        all { it.isLetterOrDigit() || it in ID_PUNCTUATION }
    ) {
        return this
    }
    return "opaque_${hashCode().toUInt().toString(16)}"
}

private val OPTIONAL_STRING_FIELDS = listOf(
    "capability",
    "provider",
    "provider_state",
    "failure_class",
    "provider_route_error",
    "argument_field",
    "contract_reason",
    "effect_status",
)
private val OPTIONAL_LONG_FIELDS = listOf(
    "observation_generation",
    "attempted_observation_generation",
    "attempted_visual_revision",
    "current_visual_revision",
    "attempted_target_id",
    "target_snapshot_generation",
    "target_display_id",
    "target_window_id",
)
private val OPTIONAL_BOOLEAN_FIELDS = listOf(
    "effect_verified",
    "snapshot_invalidated",
    "retryable",
    "fresh_observation_required",
    "fresh_observation_attached",
    "state_reconciled",
)
private val ID_PUNCTUATION = setOf('_', '-', '.', ':')
private const val MAX_DIAGNOSTIC_IDENTITY = 128
