package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveFunctionCall
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull

internal fun GeminiLiveFunctionCall.structuralDispatchLog(generation: Long): String =
    "tool_dispatched name=$name id=$id generation=$generation"

internal fun PhoneControlCompletedTool.structuralReceiptLog(): String {
    val response = result.response
    val code = (response["code"] as? JsonPrimitive)?.contentOrNull ?: "unknown"
    return "tool_receipt name=${request.name} id=${request.id} " +
        "generation=${request.generation} code=$code certainty=${result.certainty.name.lowercase()} " +
        "state_reconciled=${response.stateReconciled()}"
}

internal fun JsonObject.stateReconciled(): Boolean =
    (get("state_reconciled") as? JsonPrimitive)?.booleanOrNull == true
