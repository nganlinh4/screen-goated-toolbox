package dev.screengoated.toolbox.mobile.phonecontrol.result

import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal data class TargetBounds(
    val left: Int,
    val top: Int,
    val right: Int,
    val bottom: Int,
) {
    init {
        require(right >= left) { "target bounds must have non-negative width" }
        require(bottom >= top) { "target bounds must have non-negative height" }
    }

    fun toWireJson(): JsonObject = buildJsonObject {
        put("left", left)
        put("top", top)
        put("right", right)
        put("bottom", bottom)
    }
}

/** Immutable target lease. Platform node objects never belong in this contract. */
internal data class PhoneControlTargetIdentity(
    val snapshotGeneration: Long,
    val displayId: Int,
    val windowId: Long,
    val packageOrSurface: String,
    val nodeOrDocumentIdentity: String,
    val bounds: TargetBounds,
    val observationTimestampMs: Long,
) {
    init {
        require(snapshotGeneration > 0) { "snapshot generation must be positive" }
        require(displayId >= 0) { "display id must be non-negative" }
        require(windowId >= 0) { "window id must be non-negative" }
        require(packageOrSurface.isNotBlank()) { "package or surface must not be blank" }
        require(nodeOrDocumentIdentity.isNotBlank()) {
            "node or document identity must not be blank"
        }
        require(observationTimestampMs >= 0) {
            "observation timestamp must be non-negative"
        }
    }

    fun toWireJson(): JsonObject = buildJsonObject {
        put("snapshot_generation", snapshotGeneration)
        put("display_id", displayId)
        put("window_id", windowId)
        put("package_or_surface", packageOrSurface)
        put("node_or_document_identity", nodeOrDocumentIdentity)
        put("bounds", bounds.toWireJson())
        put("observation_timestamp", observationTimestampMs)
    }
}

internal data class ResultScope(
    val displayId: Int? = null,
    val userId: Int? = null,
    val profileId: String? = null,
    val surface: String? = null,
) {
    init {
        require(displayId == null || displayId >= 0) { "display id must be non-negative" }
        require(userId == null || userId >= 0) { "user id must be non-negative" }
        require(profileId == null || profileId.isNotBlank()) {
            "profile id must be absent or non-blank"
        }
        require(surface == null || surface.isNotBlank()) {
            "surface must be absent or non-blank"
        }
        require(displayId != null || userId != null || profileId != null || surface != null) {
            "result scope must contain at least one identity field"
        }
    }

    fun toWireJson(): JsonObject = buildJsonObject {
        displayId?.let { put("display_id", it) }
        userId?.let { put("user_id", it) }
        profileId?.let { put("profile_id", it) }
        surface?.let { put("surface", it) }
    }
}

internal data class RequiredUserStep(
    val code: String,
    val message: String? = null,
) {
    init {
        require(code.isNotBlank()) { "required user-step code must not be blank" }
        require(message == null || message.isNotBlank()) {
            "required user-step message must be absent or non-blank"
        }
    }

    fun toWireJson(): JsonObject = buildJsonObject {
        put("code", code)
        message?.let { put("message", it) }
    }
}

/**
 * Provider-neutral tool result. The wire representation keeps execution,
 * effect certainty, target invalidation, and recovery instructions separate.
 */
internal data class PhoneControlResultEnvelope(
    val code: String,
    val capability: String,
    val requestedTool: String,
    val turnId: Long,
    val jobId: String,
    val provider: String,
    val providerState: CapabilityState,
    val observationGeneration: Long,
    val effect: EffectCertainty,
    val snapshotInvalidated: Boolean,
    val retryable: Boolean,
    val requiredUserStep: RequiredUserStep? = null,
    val freshObservationRequired: Boolean? = null,
    val scope: ResultScope? = null,
    val target: PhoneControlTargetIdentity? = null,
) {
    init {
        require(code.isNotBlank()) { "result code must not be blank" }
        require(capability.isNotBlank()) { "result capability must not be blank" }
        require(requestedTool.isNotBlank()) { "requested tool must not be blank" }
        require(turnId > 0) { "turn id must be positive" }
        require(jobId.isNotBlank()) { "job id must not be blank" }
        require(provider.isNotBlank()) { "provider must not be blank" }
        require(observationGeneration >= 0) {
            "observation generation must be non-negative"
        }
        require(target == null || target.snapshotGeneration == observationGeneration) {
            "target and result observation generations must match"
        }
    }

    fun toWireJson(): JsonObject = buildJsonObject {
        put("code", code)
        put("capability", capability)
        put("requested_tool", requestedTool)
        put("turn_id", turnId)
        put("job_id", jobId)
        put("provider", provider)
        put("provider_state", providerState.wireName)
        put("observation_generation", observationGeneration)
        put("effect_status", effect.wireName)
        val effectMayHaveOccurred = effect.effectMayHaveOccurred
        if (effectMayHaveOccurred == null) {
            put("effect_may_have_occurred", JsonNull)
        } else {
            put("effect_may_have_occurred", effectMayHaveOccurred)
        }
        put("effect_verified", effect.effectVerified)
        val executed = effect.executed
        if (executed == null) {
            put("executed", JsonNull)
        } else {
            put("executed", executed)
        }
        put("snapshot_invalidated", snapshotInvalidated)
        put("retryable", retryable)
        requiredUserStep?.let { put("required_user_step", it.toWireJson()) }
        freshObservationRequired?.let { put("fresh_observation_required", it) }
        scope?.let { put("scope", it.toWireJson()) }
        target?.let { put("target", it.toWireJson()) }
    }
}
