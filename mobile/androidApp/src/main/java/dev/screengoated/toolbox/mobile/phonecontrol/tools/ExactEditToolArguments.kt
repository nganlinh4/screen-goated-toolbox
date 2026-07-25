package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.ExactReplacement
import java.io.File
import java.net.URI
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

internal data class ExactEditRequest(
    val path: String,
    val expectedSha256: String,
    val replacements: List<ExactReplacement>,
)

internal sealed interface ExactEditArguments {
    data class Valid(val request: ExactEditRequest) : ExactEditArguments
    data class Invalid(val response: PhoneControlToolExecution) : ExactEditArguments
}

internal fun parseExactEditArguments(
    job: PhoneControlToolJobContext,
    args: JsonObject,
    tool: String,
): ExactEditArguments {
    val path = args.string("path")
        ?: return ExactEditArguments.Invalid(invalidArgs(job, tool, "$tool requires path"))
    if (isContentUriForExactEdit(path)) {
        return ExactEditArguments.Invalid(
            unavailableToolResponse(
                job,
                tool,
                "file_resource_access",
                "android_app_api",
                dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState.UNSUPPORTED,
            ),
        )
    }
    if (!isAbsolutePathForExactEdit(path)) {
        return ExactEditArguments.Invalid(
            unavailableToolResponse(
                job,
                tool,
                "file_resource_access",
                "android_app_api",
                dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState.NEEDS_USER_STEP,
                "grant_storage_access",
            ),
        )
    }
    val expectedSha = args.string("expected_sha256")?.takeIf(String::isNotBlank)
        ?: return ExactEditArguments.Invalid(
            invalidArgs(job, tool, "expected_sha256 is required"),
        )
    val rawReplacements = args["replacements"] as? JsonArray
        ?: return ExactEditArguments.Invalid(
            invalidArgs(job, tool, "replacements must be an array"),
        )
    if (rawReplacements.size !in 1..MAX_EXACT_REPLACEMENTS) {
        return ExactEditArguments.Invalid(
            invalidArgs(
                job,
                tool,
                "replacements must contain 1 to $MAX_EXACT_REPLACEMENTS items",
            ),
        )
    }
    val replacements = mutableListOf<ExactReplacement>()
    rawReplacements.forEachIndexed { index, element ->
        val replacement = element as? JsonObject
            ?: return ExactEditArguments.Invalid(
                invalidArgs(job, tool, "replacement ${index + 1} is not an object"),
            )
        val oldText = replacement.string("old_text")?.takeIf(String::isNotEmpty)
            ?: return ExactEditArguments.Invalid(
                invalidArgs(job, tool, "replacement ${index + 1} needs old_text"),
            )
        val newText = replacement.string("new_text")
            ?: return ExactEditArguments.Invalid(
                invalidArgs(job, tool, "replacement ${index + 1} needs new_text"),
            )
        val expectedCount = replacement.int("expected_count")?.takeIf { it > 0 }
            ?: return ExactEditArguments.Invalid(
                invalidArgs(
                    job,
                    tool,
                    "replacement ${index + 1} needs positive expected_count",
                ),
            )
        replacements += ExactReplacement(oldText, newText, expectedCount)
    }
    return ExactEditArguments.Valid(ExactEditRequest(path, expectedSha, replacements))
}

private fun isAbsolutePathForExactEdit(path: String): Boolean =
    runCatching { File(path.trim()).isAbsolute }.getOrDefault(false)

private fun isContentUriForExactEdit(path: String): Boolean = runCatching {
    URI(path.trim()).scheme.equals("content", ignoreCase = true)
}.getOrDefault(false)

private const val MAX_EXACT_REPLACEMENTS = 64
