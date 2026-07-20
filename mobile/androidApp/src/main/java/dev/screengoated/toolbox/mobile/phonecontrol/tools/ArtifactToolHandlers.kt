package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidFileProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.AndroidProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.PhoneControlArtifactStore
import java.io.File
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import kotlinx.serialization.json.JsonObject

internal class ArtifactToolHandlers(
    private val artifacts: PhoneControlArtifactStore,
    private val files: AndroidFileProvider,
) {
    fun info(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val id = args.string("id")
            ?: return invalidArgs(job, "artifact_info", "artifact_info requires id")
        if (isAbsolute(id)) return unavailableArtifactPath(job, "artifact_info")
        val artifact = artifacts.get(id)
            ?: return providerResult(
                job,
                "artifact_info",
                ARTIFACT_CAPABILITY,
                APP_PROVIDER,
                mutating = false,
                result = AndroidProviderResult.Failure(
                    "artifact_not_found",
                    "The artifact ID is unknown.",
                ),
            )
        return providerResult(
            job,
            "artifact_info",
            ARTIFACT_CAPABILITY,
            APP_PROVIDER,
            mutating = false,
            result = AndroidProviderResult.Success(artifact.info()),
        )
    }

    fun extract(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val id = args.string("id")
            ?: return invalidArgs(job, "extract_artifact", "extract_artifact requires id")
        val artifact = artifacts.get(id)
            ?: return artifactFailure(job, "extract_artifact", "artifact_not_found", "The artifact ID is unknown.")
        val text = decodeUtf8(artifact.bytes)
            ?: return artifactFailure(job, "extract_artifact", "not_utf8", "The artifact is not UTF-8 text.")
        val startText = args.string("start_text")
        val endText = args.string("end_text")
        if (startText != null && startText.length !in 1..MAX_ANCHOR_CHARS) {
            return invalidArgs(job, "extract_artifact", "start_text must contain 1 to $MAX_ANCHOR_CHARS characters")
        }
        if (endText != null && endText.length !in 1..MAX_ANCHOR_CHARS) {
            return invalidArgs(job, "extract_artifact", "end_text must contain 1 to $MAX_ANCHOR_CHARS characters")
        }
        val startOccurrence = args.optionalPositiveInt("start_occurrence")
            ?: if ("start_occurrence" in args) {
                return invalidArgs(job, "extract_artifact", "start_occurrence must be positive")
            } else {
                null
            }
        val endOccurrence = args.optionalPositiveInt("end_occurrence")
            ?: if ("end_occurrence" in args) {
                return invalidArgs(job, "extract_artifact", "end_occurrence must be positive")
            } else {
                null
            }
        if (startText == null && startOccurrence != null) {
            return invalidArgs(job, "extract_artifact", "start_occurrence requires start_text")
        }
        if (endText == null && endOccurrence != null) {
            return invalidArgs(job, "extract_artifact", "end_occurrence requires end_text")
        }
        val includeStart = args.optionalBoolean("include_start", true)
            ?: return invalidArgs(job, "extract_artifact", "include_start must be boolean")
        val includeEnd = args.optionalBoolean("include_end", true)
            ?: return invalidArgs(job, "extract_artifact", "include_end must be boolean")
        val start = if (startText == null) {
            0
        } else {
            when (val resolved = resolveAnchor(text, startText, startOccurrence)) {
                is AnchorResolution.Failure -> return artifactFailure(
                    job,
                    "extract_artifact",
                    resolved.code,
                    "The start anchor ${resolved.message}",
                )
                is AnchorResolution.Match -> resolved.index + if (includeStart) 0 else resolved.length
            }
        }
        val end = if (endText == null) {
            text.length
        } else {
            when (val resolved = resolveAnchor(text, endText, endOccurrence)) {
                is AnchorResolution.Failure -> return artifactFailure(
                    job,
                    "extract_artifact",
                    resolved.code,
                    "The end anchor ${resolved.message}",
                )
                is AnchorResolution.Match -> resolved.index + if (includeEnd) resolved.length else 0
            }
        }
        if (start > end) {
            return artifactFailure(
                job,
                "extract_artifact",
                "invalid_range",
                "The resolved start anchor occurs after the end anchor.",
            )
        }
        val extracted = text.substring(start, end)
        val created = artifacts.put(
            extracted.toByteArray(Charsets.UTF_8),
            artifact.mimeType,
            artifact.name?.let { "extract-$it" },
        )
        return providerResult(
            job,
            "extract_artifact",
            ARTIFACT_CAPABILITY,
            APP_PROVIDER,
            mutating = true,
            result = AndroidProviderResult.Success(
                JsonObject(
                    created.info() + mapOf(
                        "source_artifact_id" to kotlinx.serialization.json.JsonPrimitive(id),
                        "characters" to kotlinx.serialization.json.JsonPrimitive(extracted.length),
                    ),
                ),
                effectMayHaveOccurred = true,
                effectVerified = true,
            ),
        )
    }

    fun save(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val id = args.string("id")
            ?: return invalidArgs(job, "save_artifact", "save_artifact requires id")
        val path = args.string("path")
            ?: return unavailableArtifactPath(job, "save_artifact")
        if (!isAbsolute(path)) return unavailableArtifactPath(job, "save_artifact")
        val overwrite = args.optionalBoolean("overwrite", false)
            ?: return invalidArgs(job, "save_artifact", "overwrite must be boolean")
        return providerResult(
            job,
            "save_artifact",
            ARTIFACT_CAPABILITY,
            APP_PROVIDER,
            mutating = true,
            result = files.saveArtifact(id, path, overwrite),
        )
    }

    private fun artifactFailure(
        job: PhoneControlToolJobContext,
        tool: String,
        code: String,
        message: String,
    ): PhoneControlToolExecution = providerResult(
        job,
        tool,
        ARTIFACT_CAPABILITY,
        APP_PROVIDER,
        mutating = false,
        result = AndroidProviderResult.Failure(code, message),
    )
}

private sealed interface AnchorResolution {
    data class Match(val index: Int, val length: Int) : AnchorResolution
    data class Failure(val code: String, val message: String) : AnchorResolution
}

private fun resolveAnchor(
    text: String,
    anchor: String,
    requestedOccurrence: Int?,
): AnchorResolution {
    val positions = buildList {
        var offset = 0
        while (offset <= text.length - anchor.length) {
            val index = text.indexOf(anchor, offset)
            if (index < 0) break
            add(index)
            offset = index + anchor.length
        }
    }
    val index = when {
        requestedOccurrence != null -> positions.getOrNull(requestedOccurrence - 1)
        positions.size == 1 -> positions.single()
        positions.isEmpty() -> null
        else -> return AnchorResolution.Failure("ambiguous_anchor", "is ambiguous; supply its occurrence.")
    } ?: return AnchorResolution.Failure("anchor_not_found", "was not found.")
    return AnchorResolution.Match(index, anchor.length)
}

private fun decodeUtf8(bytes: ByteArray): String? = runCatching {
    Charsets.UTF_8.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(bytes))
        .toString()
}.getOrNull()

private fun JsonObject.optionalPositiveInt(name: String): Int? =
    if (name !in this) null else int(name)?.takeIf { it > 0 }

private fun JsonObject.optionalBoolean(name: String, default: Boolean): Boolean? =
    if (name !in this) default else boolean(name)

private fun unavailableArtifactPath(
    job: PhoneControlToolJobContext,
    tool: String,
): PhoneControlToolExecution = unavailableToolResponse(
    job,
    tool,
    ARTIFACT_CAPABILITY,
    APP_PROVIDER,
    CapabilityState.NEEDS_USER_STEP,
    "choose_artifact_destination",
)

private fun isAbsolute(path: String): Boolean = runCatching { File(path.trim()).isAbsolute }
    .getOrDefault(false)

private const val APP_PROVIDER = "android_app_api"
private const val ARTIFACT_CAPABILITY = "file_resource_access"
private const val MAX_ANCHOR_CHARS = 2_000
