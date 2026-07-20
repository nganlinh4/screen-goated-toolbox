package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlEffectCertainty
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

internal data class PhoneControlToolRequest(
    val id: String,
    val name: String,
    val arguments: JsonElement,
    val turnId: Long,
    val generation: Long,
) {
    init {
        require(id.isNotBlank()) { "tool call id must not be blank" }
        require(name.isNotBlank()) { "tool call name must not be blank" }
        require(turnId >= 0L) { "turnId must be non-negative" }
        require(generation >= 0L) { "generation must be non-negative" }
    }
}

internal data class PhoneControlToolExecutionResult(
    val response: JsonObject,
    val certainty: PhoneControlEffectCertainty,
    val terminalSummary: String? = null,
    val refreshScreenFrame: Boolean = false,
    val screenFramePayload: String? = null,
)

internal fun interface PhoneControlToolCompletion {
    fun complete(result: PhoneControlToolExecutionResult)
}

internal fun interface PhoneControlToolJob {
    /** Returns the best known effect certainty at the cancellation boundary. */
    fun cancel(): PhoneControlEffectCertainty
}

/**
 * Narrow dispatcher boundary. Implementations start admitted work promptly and invoke completion
 * at most once. The runtime admits at most one call session-wide.
 */
internal fun interface PhoneControlToolExecutor {
    fun execute(
        request: PhoneControlToolRequest,
        completion: PhoneControlToolCompletion,
    ): PhoneControlToolJob
}
