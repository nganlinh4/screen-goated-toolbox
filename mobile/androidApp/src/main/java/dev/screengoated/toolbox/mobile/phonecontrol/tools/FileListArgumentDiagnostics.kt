package dev.screengoated.toolbox.mobile.phonecontrol.tools

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.intOrNull

internal fun invalidFileListRequest(
    job: PhoneControlToolJobContext,
    args: JsonObject,
    fileKinds: Set<String>,
    sortFields: Set<String>,
    orders: Set<String>,
    maxLimit: Int,
): PhoneControlToolExecution {
    val failure = when {
        args.string("path").isNullOrBlank() ->
            ArgumentFailure("path", "missing_or_invalid", "list_files requires a non-empty path")
        "kind" in args && args.string("kind") !in fileKinds ->
            ArgumentFailure("kind", "unsupported_value", "kind is not supported")
        !args.hasStringArray("extensions") ->
            ArgumentFailure("extensions", "invalid_type", "extensions must contain only strings")
        "sort_by" in args && args.string("sort_by") !in sortFields ->
            ArgumentFailure("sort_by", "unsupported_value", "sort_by is not supported")
        "order" in args && args.string("order") !in orders ->
            ArgumentFailure("order", "unsupported_value", "order is not supported")
        "limit" in args && (args["limit"] as? JsonPrimitive)?.intOrNull !in 1..maxLimit ->
            ArgumentFailure("limit", "out_of_range", "limit must be between 1 and $maxLimit")
        else -> ArgumentFailure(
            "arguments",
            "missing_or_invalid",
            "list_files arguments are invalid",
        )
    }
    return invalidArgs(
        job = job,
        tool = "list_files",
        message = failure.message,
        argumentField = failure.field,
        contractReason = failure.reason,
    )
}

private fun JsonObject.hasStringArray(name: String): Boolean {
    val value = get(name) ?: return true
    val array = value as? JsonArray ?: return false
    return array.all { element -> (element as? JsonPrimitive)?.isString == true }
}

private data class ArgumentFailure(
    val field: String,
    val reason: String,
    val message: String,
)
