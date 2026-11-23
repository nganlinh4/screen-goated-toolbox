package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.result.EffectCertainty
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlToolResponse
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

internal class PhoneControlTurnOutbound(
    private val sink: PhoneControlTurnSink,
) {
    var refused: Boolean = false
        private set

    fun sendPayload(payload: String): Boolean = send { sink.sendPayload(payload) }

    fun sendEvidence(payload: String): Boolean = send { sink.sendScreenEvidence(payload) }

    fun sendRejectedTool(id: String, name: String, code: String, message: String): Boolean =
        sendPayload(
            buildPhoneControlToolResponse(id = id, name = name, response = provenNoEffectRejection(code, message)),
        )

    /** The tool response first, then its tool-owned screen evidence, then any requested refresh. */
    fun deliverToolReceipt(completed: PhoneControlCompletedTool) {
        val response = buildPhoneControlToolResponse(
            id = completed.request.id,
            name = completed.request.name,
            response = completed.result.response,
        )
        if (!sendPayload(response)) return
        completed.result.screenFramePayload?.let { payload ->
            if (!sendEvidence(payload)) return
        }
        if (completed.result.refreshScreenFrame) sink.requestScreenRefresh()
    }

    fun refuse() {
        if (refused) return
        refused = true
        sink.abortProtocolSession()
    }

    fun block() {
        refused = true
    }

    fun reset() {
        refused = false
    }

    private inline fun send(send: () -> Boolean): Boolean {
        if (refused) return false
        if (send()) return true
        refuse()
        return false
    }
}

/** Receipt for a call refused before any side effect, in the shared `effect_status` envelope key. */
internal fun provenNoEffectRejection(code: String, message: String): JsonObject = buildJsonObject {
    put("code", code)
    put("message", message)
    put("effect_status", EffectCertainty.PROVEN_NO_EFFECT.wireName)
}
