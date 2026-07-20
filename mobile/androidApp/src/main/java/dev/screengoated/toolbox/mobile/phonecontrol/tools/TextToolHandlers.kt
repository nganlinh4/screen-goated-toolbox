package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.PhoneControlArtifactStore
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityProviderResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AccessibilityTextOutcome
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceIdentity
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.AndroidSurfaceTargetParseResult
import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import java.io.File
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal sealed interface TextArtifactResolution {
    data class Success(
        val id: String,
        val text: String,
        val sha256: String,
        val bytes: Int,
    ) : TextArtifactResolution

    data class Failure(val code: String, val message: String) : TextArtifactResolution
}

internal fun interface TextArtifactResolver {
    fun resolve(idOrPath: String): TextArtifactResolution
}

internal class ArtifactStoreTextResolver(
    private val artifacts: PhoneControlArtifactStore,
) : TextArtifactResolver {
    override fun resolve(idOrPath: String): TextArtifactResolution {
        val artifact = artifacts.get(idOrPath)
        val bytes = if (artifact != null) {
            artifact.bytes
        } else {
            val requestedFile = File(idOrPath.trim())
            if (!requestedFile.isAbsolute) {
                return TextArtifactResolution.Failure("artifact_not_found", "The artifact is unknown.")
            }
            val file = runCatching { requestedFile.canonicalFile }.getOrNull()
                ?: return TextArtifactResolution.Failure("artifact_not_found", "The artifact is unknown.")
            if (!file.isFile || !file.canRead()) {
                return TextArtifactResolution.Failure("artifact_not_found", "The artifact is unknown.")
            }
            if (file.length() > MAX_TEXT_ARTIFACT_BYTES) {
                return TextArtifactResolution.Failure("artifact_too_large", "The text artifact is too large.")
            }
            runCatching { file.readBytes() }.getOrElse {
                return TextArtifactResolution.Failure("artifact_read_failed", "The text artifact could not be read.")
            }
        }
        val text = decodeArtifactUtf8(bytes)
            ?: return TextArtifactResolution.Failure("not_utf8", "The artifact is not valid UTF-8 text.")
        val identity = artifact?.id ?: idOrPath
        val sha = artifact?.sha256 ?: bytes.sha256ForTextTool()
        return TextArtifactResolution.Success(identity, text, sha, bytes.size)
    }
}

internal class TextToolHandlers(
    private val artifacts: TextArtifactResolver,
    private val backend: TextToolBackend = AndroidTextToolBackend,
) {
    suspend fun typeText(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val surface = parseAndroidTextTarget(args.string("target"))
            ?: return invalidArgs(job, "type_text", ANDROID_TEXT_TARGET_MESSAGE)
        val text = args.string("text")
            ?: return invalidArgs(job, "type_text", "type_text requires literal text")
        val slow = strictOptionalBoolean(args, "slow", false)
            ?: return invalidArgs(job, "type_text", "slow must be boolean")
        val pressEnter = strictOptionalBoolean(args, "press_enter", false)
            ?: return invalidArgs(job, "type_text", "press_enter must be boolean")
        val target = when (val focused = backend.focusedTarget(surface)) {
            is AccessibilityProviderResult.Failure -> return textFailure(job, "type_text", focused)
            is AccessibilityProviderResult.Success -> focused.value
        }
        return editResult(
            job,
            "type_text",
            backend.typeText(target, text, slow, pressEnter),
            failureProvider = backend.textInputProviderId,
        )
    }

    suspend fun pasteArtifact(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val id = args.string("id")
            ?: return invalidArgs(job, "paste_artifact", "paste_artifact requires id")
        val artifact = when (val resolved = artifacts.resolve(id)) {
            is TextArtifactResolution.Failure -> return artifactFailure(job, resolved)
            is TextArtifactResolution.Success -> resolved
        }
        val target = when (val focused = backend.focusedTarget()) {
            is AccessibilityProviderResult.Failure -> {
                return textFailure(job, "paste_artifact", focused)
            }
            is AccessibilityProviderResult.Success -> focused.value
        }
        return editResult(
            job = job,
            tool = "paste_artifact",
            result = backend.typeText(target, artifact.text, slow = false, pressEnter = false),
            extra = buildJsonObject {
                put("artifact_id", artifact.id)
                put("source_sha256", artifact.sha256)
                put("source_bytes", artifact.bytes)
                put("requested_code_points", artifact.text.codePointCount(0, artifact.text.length))
            },
            failureProvider = backend.textInputProviderId,
        )
    }

    suspend fun keyCombination(
        job: PhoneControlToolJobContext,
        args: JsonObject,
    ): PhoneControlToolExecution {
        val surface = parseAndroidTextTarget(args.string("target"))
            ?: return invalidArgs(job, "key_combination", ANDROID_TEXT_TARGET_MESSAGE)
        val keys = args.string("keys")
            ?: return invalidArgs(job, "key_combination", "key_combination requires keys")
        val groups = when (val parsed = parseAndroidKeySequence(keys)) {
            is AndroidKeySequenceParseResult.Invalid -> {
                return invalidArgs(job, "key_combination", parsed.message)
            }
            is AndroidKeySequenceParseResult.Unsupported -> {
                return unsupportedKey(job, parsed.token)
            }
            is AndroidKeySequenceParseResult.Success -> parsed.groups
        }
        val holdSeconds = args.number("hold_seconds") ?: if ("hold_seconds" in args) {
            return invalidArgs(job, "key_combination", "hold_seconds must be numeric")
        } else {
            0.0
        }
        if (!holdSeconds.isFinite() || holdSeconds !in 0.0..MAX_HOLD_SECONDS) {
            return invalidArgs(job, "key_combination", "hold_seconds must be between 0 and 10")
        }
        val holdMs = (holdSeconds * 1_000.0).toLong().coerceAtLeast(DEFAULT_KEY_HOLD_MS)
        val target = when (val focused = backend.focusedTarget(surface)) {
            is AccessibilityProviderResult.Failure -> return textFailure(job, "key_combination", focused)
            is AccessibilityProviderResult.Success -> focused.value
        }
        return editResult(
            job,
            "key_combination",
            backend.sendKeys(target, groups, holdMs),
            buildJsonObject {
                put("keys", keys)
                put("held_ms", holdMs)
                put("sequence_groups", groups.size)
            },
            failureProvider = INPUT_METHOD_PROVIDER,
            capability = KEY_ACTION_CAPABILITY,
        )
    }

    private fun artifactFailure(
        job: PhoneControlToolJobContext,
        failure: TextArtifactResolution.Failure,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = "paste_artifact",
            capability = TEXT_EDIT_CAPABILITY,
            provider = APP_PROVIDER,
            providerState = CapabilityState.READY,
            code = failure.code,
            observationGeneration = backend.observationGeneration,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            data = buildJsonObject {
                put("message", failure.message)
                put("provider_role", "dependency")
            },
        ),
        mutating = false,
    )

    private fun editResult(
        job: PhoneControlToolJobContext,
        tool: String,
        result: AccessibilityProviderResult<AccessibilityTextOutcome>,
        extra: JsonObject = JsonObject(emptyMap()),
        failureProvider: String = ACCESSIBILITY_PROVIDER,
        capability: String = TEXT_EDIT_CAPABILITY,
    ): PhoneControlToolExecution = when (result) {
        is AccessibilityProviderResult.Failure ->
            textFailure(job, tool, result, failureProvider, capability)
        is AccessibilityProviderResult.Success -> {
            val outcome = result.value
            PhoneControlToolExecution(
                response = toolResponse(
                    job = job,
                    requestedTool = tool,
                    capability = capability,
                    provider = outcome.provider,
                    providerState = CapabilityState.READY,
                    code = outcome.code,
                    observationGeneration = outcome.generation,
                    effect = outcome.effect,
                    snapshotInvalidated = outcome.snapshotInvalidated,
                    freshObservationRequired = outcome.freshObservationRequired,
                    data = JsonObject(
                        extra + buildJsonObject {
                            put("dispatched_code_points", outcome.insertedCodePoints)
                            put("dispatched_key_groups", outcome.completedKeyGroups)
                            put("enter_dispatched", outcome.submitted)
                            put("text_postcondition_verified", outcome.effect == EffectCertainty.VERIFIED)
                            outcome.message?.let { put("message", it) }
                        },
                    ),
                ),
                mutating = outcome.effect.effectMayHaveOccurred == true,
                refreshScreenFrame = outcome.snapshotInvalidated,
            )
        }
    }

    private fun textFailure(
        job: PhoneControlToolJobContext,
        tool: String,
        failure: AccessibilityProviderResult.Failure,
        primaryProvider: String = ACCESSIBILITY_PROVIDER,
        capability: String = TEXT_EDIT_CAPABILITY,
    ): PhoneControlToolExecution {
        val serviceMissing = !backend.isReady || failure.code == "capability_unavailable"
        val unsupported = failure.code == "unsupported_on_surface"
        val uncertain = failure.effect != EffectCertainty.PROVEN_NO_EFFECT
        val providerState = when {
            serviceMissing -> CapabilityState.NEEDS_USER_STEP
            unsupported -> CapabilityState.UNSUPPORTED
            failure.requiredUserStep != null -> CapabilityState.READY
            else -> CapabilityState.DEGRADED
        }
        return PhoneControlToolExecution(
            response = toolResponse(
                job = job,
                requestedTool = tool,
                capability = capability,
                provider = if (unsupported) INPUT_METHOD_PROVIDER else primaryProvider,
                providerState = providerState,
                code = failure.code,
                observationGeneration = backend.observationGeneration,
                effect = failure.effect,
                snapshotInvalidated = uncertain,
                retryable = failure.retryable,
                requiredUserStep = failure.requiredUserStep
                    ?: if (serviceMissing) "enable_accessibility" else null,
                freshObservationRequired = failure.freshObservationRequired || uncertain,
                data = buildJsonObject {
                    put("message", failure.message)
                },
            ),
            mutating = uncertain,
            refreshScreenFrame = uncertain,
        )
    }

    private fun unsupportedKey(
        job: PhoneControlToolJobContext,
        token: String,
    ): PhoneControlToolExecution = PhoneControlToolExecution(
        response = toolResponse(
            job = job,
            requestedTool = "key_combination",
            capability = TEXT_EDIT_CAPABILITY,
            provider = INPUT_METHOD_PROVIDER,
            providerState = CapabilityState.UNSUPPORTED,
            code = "unsupported_on_android",
            observationGeneration = backend.observationGeneration,
            effect = EffectCertainty.PROVEN_NO_EFFECT,
            snapshotInvalidated = false,
            data = buildJsonObject {
                put("message", "The desktop key '$token' has no exact Android equivalent.")
            },
        ),
        mutating = false,
    )
}

internal fun parseAndroidTextTarget(value: String?): AndroidSurfaceIdentity? {
    val target = value ?: return null
    return when (val parsed = AndroidSurfaceIdentity.parseStableTarget(target)) {
        is AndroidSurfaceTargetParseResult.Stable -> parsed.identity
        else -> null
    }
}

private fun strictOptionalBoolean(args: JsonObject, name: String, default: Boolean): Boolean? =
    if (name !in args) default else args.boolean(name)

private fun decodeArtifactUtf8(bytes: ByteArray): String? = runCatching {
    Charsets.UTF_8.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(bytes))
        .toString()
}.getOrNull()

private fun ByteArray.sha256ForTextTool(): String = java.security.MessageDigest.getInstance("SHA-256")
    .digest(this)
    .joinToString("") { "%02x".format(it) }

private const val ANDROID_TEXT_TARGET_MESSAGE =
    "Android text tools require a current stable surface target from list_windows"
private const val ACCESSIBILITY_PROVIDER = TextProviderIds.ACCESSIBILITY
private const val INPUT_METHOD_PROVIDER = TextProviderIds.INPUT_METHOD
private const val APP_PROVIDER = "android_app_api"
private const val TEXT_EDIT_CAPABILITY = "ui.text_edit"
private const val KEY_ACTION_CAPABILITY = "ui.key_action"
private const val MAX_TEXT_ARTIFACT_BYTES = 8L * 1024L * 1024L
private const val MAX_HOLD_SECONDS = 10.0
private const val DEFAULT_KEY_HOLD_MS = 45L
